use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard};

use laplace_settings::laplace::LappsSettings;
use laplace_settings::lapp::access::FrontPermission;
use laplace_settings::lapp::{self, ApplicationSettings, LappSettings, LappUserSettings};
use wasmtime::component::{Component, Linker};
use wasmtime::error::Context;
use wasmtime::{Engine, Store};

use crate::api::http::{Http, Request, Response};
use crate::wasi::WasiStates;

/// The registry of the lapps: what their manifests require and what the user has decided, apart
/// from the instances of their backends, which are started on demand.
#[derive(Default)]
pub struct LappsManager {
    engine: Engine,
    lapps: HashMap<String, Lapp>,
}

impl LappsManager {
    /// Registers the founded lapps and starts the backends of those that are enabled and autoloaded.
    ///
    /// A lapp whose manifest or user settings cannot be read is skipped with an error in the log.
    pub fn load(settings: &LappsSettings) -> Self {
        let mut manager = Self::default();
        manager.lapps.insert(lapp::MAIN_NAME.into(), Lapp::launcher());

        let Ok(read_dir) = fs::read_dir(&settings.path).inspect_err(|err| {
            log::error!(
                "Failed to read the lapps directory '{}': {err}",
                settings.path.display()
            )
        }) else {
            return manager;
        };

        for entry in read_dir {
            let Ok(entry) = entry.inspect_err(|err| {
                log::error!(
                    "Failed to read an entry of the lapps directory '{}': {err}",
                    settings.path.display()
                )
            }) else {
                continue;
            };

            let Ok(lapp_name) = entry.file_name().into_string() else {
                log::error!("Lapp name {:?} is not valid UTF-8", entry.file_name());
                continue;
            };

            // Hidden entries are the service directories of a lapp update.
            if lapp_name.starts_with('.') {
                continue;
            }

            let lapp_dir = entry.path();
            if !lapp_dir.is_dir() {
                log::debug!("Skip '{}': not a lapp directory", lapp_dir.display());
                continue;
            }

            if lapp::is_main(&lapp_name) {
                log::debug!("Skip '{}': the launcher is built in", lapp_dir.display());
                continue;
            }

            if !lapp::is_valid_name(&lapp_name) {
                log::error!("Lapp name '{lapp_name}' is invalid: only a-z, 0-9 and '-' are allowed");
                continue;
            }

            if let Some(allowed_lapps) = &settings.allowed
                && !allowed_lapps.contains(&lapp_name)
            {
                continue;
            }

            let Ok(lapp) = Lapp::read(settings, &lapp_name, &lapp_dir)
                .inspect_err(|err| log::error!("Failed to register lapp '{lapp_name}': {err:#}"))
            else {
                continue;
            };

            manager.lapps.insert(lapp_name, lapp);
        }

        for (name, lapp) in &manager.lapps {
            if lapp.is_startup_active()
                && let Err(err) = manager.instance(name, lapp)
            {
                log::error!("{err}");
            }
        }
        log::info!("Registered {} lapps", manager.lapps.len());

        manager
    }

    pub fn lapp(&self, name: &str) -> Option<&Lapp> {
        self.lapps.get(name)
    }

    /// Whether the backend of the lapp `name` has an instance.
    pub fn is_running(&self, name: &str) -> bool {
        self.lapp(name).is_some_and(|lapp| lapp.lock_instance().is_some())
    }

    /// Hands a request of the frontend of the lapp `name` to its backend, starting it if needed.
    pub fn handle_http(&self, name: &str, request: &Request) -> Result<Response, String> {
        let lapp = self.lapp(name).ok_or_else(|| format!("Lapp '{name}' is not found"))?;
        lapp.check_invoke_back(name)?;

        let mut instance = self.instance(name, lapp)?;
        let state = instance.as_mut().expect("the instance was started");

        state
            .http_handle(request)
            .map_err(|err| format!("Lapp '{name}' failed to handle the request: {err:#}"))
    }

    // The instance of the lapp backend, started if it is not running yet.
    fn instance<'a>(&self, name: &str, lapp: &'a Lapp) -> Result<MutexGuard<'a, Option<LappState>>, String> {
        let back_path = lapp.back_path.as_deref().ok_or_else(|| no_backend(name))?;

        // Only this lapp waits while its component compiles.
        let mut instance = lapp.lock_instance();
        if instance.is_none() {
            log::debug!("Start lapp '{name}'");
            let state = LappState::load(&self.engine, back_path, name)
                .map_err(|err| format!("Failed to start lapp '{name}': {err:#}"))?;
            *instance = Some(state);
        }
        Ok(instance)
    }
}

fn no_backend(name: &str) -> String {
    format!("Lapp '{name}' has no backend")
}

/// A registered lapp.
pub struct Lapp {
    manifest_settings: LappSettings,
    user_settings: RwLock<LappUserSettings>,
    back_path: Option<PathBuf>,
    instance: Mutex<Option<LappState>>,
}

impl Lapp {
    pub fn new(manifest_settings: LappSettings, user_settings: LappUserSettings, back_path: Option<PathBuf>) -> Self {
        Self {
            manifest_settings,
            user_settings: RwLock::new(user_settings),
            back_path,
            instance: Mutex::new(None),
        }
    }

    pub fn read(settings: &LappsSettings, name: &str, lapp_dir: &Path) -> anyhow::Result<Self> {
        let manifest_path = settings.manifest_path(name);
        let manifest_settings = LappSettings::load(name, &manifest_path)
            .with_context(|| format!("Failed to read the manifest '{}'", manifest_path.display()))?;

        let user_config_path = settings.user_config_path(name);
        let user_settings = LappUserSettings::load(&user_config_path, &manifest_settings)
            .with_context(|| format!("Failed to read the user settings '{}'", user_config_path.display()))?;

        let back_path = LappState::back_path(lapp_dir, name);

        Ok(Self::new(
            manifest_settings,
            user_settings,
            back_path.exists().then_some(back_path),
        ))
    }

    /// The launcher: built into the platform, with no manifest file and no backend.
    pub fn launcher() -> Self {
        let manifest_settings = LappSettings {
            lapp_name: lapp::MAIN_NAME.into(),
            application: ApplicationSettings {
                title: "Laplace".into(),
                ..Default::default()
            },
            ..Default::default()
        };
        let user_settings = LappUserSettings::new(&manifest_settings);
        Self::new(manifest_settings, user_settings, None)
    }

    pub fn user_settings(&self) -> RwLockReadGuard<'_, LappUserSettings> {
        self.user_settings.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn is_startup_active(&self) -> bool {
        self.manifest_settings.is_lapp_startup_active(&self.user_settings())
    }

    // Whether the frontend of the lapp may call its backend.
    fn check_invoke_back(&self, name: &str) -> Result<(), String> {
        let user_settings = self.user_settings();

        if !user_settings.enabled() {
            return Err(format!("Lapp '{name}' is not enabled"));
        }

        if self.back_path.is_none() {
            return Err(no_backend(name));
        }

        if !user_settings
            .effective_permissions(&self.manifest_settings)
            .front
            .contains(&FrontPermission::InvokeBack)
        {
            return Err(format!(
                "Lapp '{name}' is denied the permission '{}'",
                FrontPermission::InvokeBack.as_str()
            ));
        }
        Ok(())
    }

    // A poisoned lock only means that a call has panicked while holding it, which must not make
    // the lapp unreachable for good: its instance reports by itself whether it is still usable.
    fn lock_instance(&self) -> MutexGuard<'_, Option<LappState>> {
        self.instance.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[allow(dead_code)]
pub struct LappState {
    name: String,
    store: Store<WasiStates>,
    http_instance: Http,
}

impl LappState {
    pub fn back_path(root_path: impl AsRef<Path>, name: &str) -> PathBuf {
        root_path.as_ref().join(format!("{name}-back.wasm"))
    }

    /// Loads the component from `path`; the lapp settings are not its concern.
    pub fn load(engine: &Engine, path: impl AsRef<Path>, name: impl Into<String>) -> anyhow::Result<Self> {
        let path = path.as_ref();
        let component = Component::from_file(engine, path)
            .with_context(|| format!("Component file '{}' not found", path.display()))?;

        Self::new(engine, name.into(), component)
    }

    pub fn new(engine: &Engine, name: String, component: Component) -> anyhow::Result<Self> {
        let wasi_states = WasiStates::new();
        let mut store = Store::new(engine, wasi_states);

        let mut linker = Linker::new(engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;

        let http_instance =
            Http::instantiate(&mut store, &component, &linker).context("Failed to instantiate the Http world")?;

        Ok(Self {
            name,
            store,
            http_instance,
        })
    }

    pub fn http_handle(&mut self, request: &Request) -> wasmtime::Result<Response> {
        self.http_instance
            .laplace_http_by_client_handler()
            .call_handle(&mut self.store, request)
            .context("Failed to call http::handle function")
    }
}
