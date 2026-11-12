use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError, RwLock, RwLockReadGuard, RwLockWriteGuard, TryLockError};

use laplace_settings::laplace::LappsSettings;
use laplace_settings::lapp::access::{FrontPermission, Permission};
use laplace_settings::lapp::file::LappSettingsError;
use laplace_settings::lapp::update::UpdateQuery;
use laplace_settings::lapp::{self, LappInfo, LappSettings, LappUserSettings};
use thiserror::Error;
use wasmtime::component::{Component, Linker};
use wasmtime::error::Context;
use wasmtime::{Engine, Store};

use crate::api::http::{Http, Request, Response};
use crate::wasi::WasiStates;

/// The causes go into the messages, as that is all the frontend gets of an error.
#[derive(Debug, Error)]
pub enum LappError {
    #[error("Lapp '{0}' is not found")]
    NotFound(String),

    #[error("Lapp '{0}' is not enabled")]
    NotEnabled(String),

    #[error("Lapp '{0}' has no backend")]
    NoBackend(String),

    #[error("Lapp '{name}' is denied the permission '{}'", permission.as_str())]
    PermissionDenied { name: String, permission: Permission },

    #[error("Failed to read the manifest '{}': {error}", path.display())]
    Manifest { path: PathBuf, error: LappSettingsError },

    #[error("Failed to read the user settings '{}': {error}", path.display())]
    UserSettings { path: PathBuf, error: LappSettingsError },

    #[error("Failed to save the user settings '{}': {error}", path.display())]
    SaveUserSettings { path: PathBuf, error: LappSettingsError },

    #[error("Failed to start lapp '{name}': {error:#}")]
    Start { name: String, error: anyhow::Error },

    #[error("Lapp '{name}' failed to handle the request: {error:#}")]
    Handle { name: String, error: wasmtime::Error },
}

pub type LappResult<T> = Result<T, LappError>;

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

        let Ok(read_dir) = fs::read_dir(&settings.back_root).inspect_err(|err| {
            log::error!(
                "Failed to read the lapps directory '{}': {err}",
                settings.back_root.display()
            )
        }) else {
            return manager;
        };

        for entry in read_dir {
            let Ok(entry) = entry.inspect_err(|err| {
                log::error!(
                    "Failed to read an entry of the lapps directory '{}': {err}",
                    settings.back_root.display()
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
                log::debug!("Skip '{}': the name is reserved for the launcher", lapp_dir.display());
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
                .inspect_err(|err| log::error!("Failed to register lapp '{lapp_name}': {err}"))
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
        self.lapp(name).is_some_and(Lapp::is_running)
    }

    /// The registered lapps, sorted by name.
    pub fn lapps(&self) -> Vec<LappInfo> {
        let mut lapps: Vec<_> = self.lapps.iter().map(|(name, lapp)| lapp.info(name)).collect();
        lapps.sort_by(|a, b| a.name.cmp(&b.name));
        lapps
    }

    /// Applies `query` to the user settings of its lapp, saves them and returns the applied part.
    ///
    /// Disabling the lapp or changing its permissions stops its running backend; the next one is
    /// started as usual, on the first request.
    pub fn update_lapp(&self, query: UpdateQuery) -> LappResult<UpdateQuery> {
        let name = &query.lapp_name;
        let lapp = self.lapp(&name).ok_or_else(|| LappError::NotFound(name.clone()))?;

        let updated = {
            let mut user_settings = lapp.user_settings_write();

            // Changed in a copy, so that a failed save leaves the registry as the file is.
            let mut changed = user_settings.clone();
            let updated = changed
                .update(query, &lapp.manifest_settings, &lapp.user_config_path)
                .map_err(|error| LappError::SaveUserSettings {
                    path: lapp.user_config_path.clone(),
                    error,
                })?;
            *user_settings = changed;
            updated
        };

        let stops =
            updated.enabled == Some(false) || updated.allow_permission.is_some() || updated.deny_permission.is_some();

        if stops && lapp.lock_instance().take().is_some() {
            log::debug!("Stop lapp '{}'", lapp.manifest_settings.lapp_name);
        }

        Ok(updated)
    }

    /// Whether a window may be opened for the lapp `name`.
    pub fn check_open(&self, name: &str) -> LappResult<()> {
        let lapp = self.lapp(name).ok_or_else(|| LappError::NotFound(name.into()))?;

        if lapp.user_settings_read().enabled() {
            Ok(())
        } else {
            Err(LappError::NotEnabled(name.into()))
        }
    }

    /// Hands a request of the frontend of the lapp `name` to its backend, starting it if needed.
    pub fn handle_http(&self, name: &str, request: &Request) -> LappResult<Response> {
        let lapp = self.lapp(name).ok_or_else(|| LappError::NotFound(name.into()))?;
        lapp.check_invoke_back(name)?;

        let mut instance = self.instance(name, lapp)?;
        let state = instance.as_mut().expect("the instance was started");

        state.http_handle(request).map_err(|error| LappError::Handle {
            name: name.into(),
            error,
        })
    }

    // The instance of the lapp backend, started if it is not running yet.
    fn instance<'a>(&self, name: &str, lapp: &'a Lapp) -> LappResult<MutexGuard<'a, Option<LappState>>> {
        let back_path = lapp
            .back_path
            .as_deref()
            .ok_or_else(|| LappError::NoBackend(name.into()))?;

        // Only this lapp waits while its component compiles.
        let mut instance = lapp.lock_instance();
        if instance.is_none() {
            log::debug!("Start lapp '{name}'");
            let state = LappState::load(&self.engine, back_path, name).map_err(|error| LappError::Start {
                name: name.into(),
                error,
            })?;
            *instance = Some(state);
        }
        Ok(instance)
    }
}

/// A registered lapp.
pub struct Lapp {
    manifest_settings: LappSettings,
    user_settings: RwLock<LappUserSettings>,
    user_config_path: PathBuf,
    back_path: Option<PathBuf>,
    instance: Mutex<Option<LappState>>,
}

impl Lapp {
    pub fn new(
        manifest_settings: LappSettings,
        user_settings: LappUserSettings,
        user_config_path: PathBuf,
        back_path: Option<PathBuf>,
    ) -> Self {
        Self {
            manifest_settings,
            user_settings: RwLock::new(user_settings),
            user_config_path,
            back_path,
            instance: Mutex::new(None),
        }
    }

    pub fn read(settings: &LappsSettings, name: &str, lapp_dir: &Path) -> LappResult<Self> {
        let path = settings.manifest_path(name);
        let manifest_settings = LappSettings::load(name, &path).map_err(|error| LappError::Manifest { path, error })?;

        let path = settings.user_config_path(name);
        match LappUserSettings::load(&path, &manifest_settings) {
            Ok(user_settings) => {
                let back_path = LappState::back_path(lapp_dir, name);
                Ok(Self::new(
                    manifest_settings,
                    user_settings,
                    path,
                    back_path.exists().then_some(back_path),
                ))
            },
            Err(error) => Err(LappError::UserSettings { path, error }),
        }
    }

    pub fn user_settings_read(&self) -> RwLockReadGuard<'_, LappUserSettings> {
        self.user_settings.read().unwrap_or_else(PoisonError::into_inner)
    }

    fn user_settings_write(&self) -> RwLockWriteGuard<'_, LappUserSettings> {
        self.user_settings.write().unwrap_or_else(PoisonError::into_inner)
    }

    /// Does not wait for a call in progress: a busy instance is running.
    pub fn is_running(&self) -> bool {
        match self.instance.try_lock() {
            Ok(instance) => instance.is_some(),
            Err(TryLockError::Poisoned(err)) => err.into_inner().is_some(),
            Err(TryLockError::WouldBlock) => true,
        }
    }

    pub fn info(&self, name: &str) -> LappInfo {
        let user_settings = self.user_settings_read().clone();
        LappInfo {
            name: name.into(),
            effective_permissions: user_settings.effective_permissions(&self.manifest_settings),
            manifest: self.manifest_settings.clone(),
            user_settings,
            is_running: self.is_running(),
        }
    }

    fn is_startup_active(&self) -> bool {
        self.manifest_settings
            .is_lapp_startup_active(&self.user_settings_read())
    }

    // Whether the frontend of the lapp may call its backend.
    fn check_invoke_back(&self, name: &str) -> LappResult<()> {
        let user_settings = self.user_settings_read();

        if !user_settings.enabled() {
            return Err(LappError::NotEnabled(name.into()));
        }

        if self.back_path.is_none() {
            return Err(LappError::NoBackend(name.into()));
        }

        if !user_settings
            .effective_permissions(&self.manifest_settings)
            .front
            .contains(&FrontPermission::InvokeBack)
        {
            return Err(LappError::PermissionDenied {
                name: name.into(),
                permission: FrontPermission::InvokeBack.into(),
            });
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
