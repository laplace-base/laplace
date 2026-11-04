use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::Mutex;

use laplace_settings::LaplaceSettings;
use laplace_settings::laplace::LappsSettings;
use tauri::{Assets, Wry, http};
use wasmtime::component::{Component, Linker};
use wasmtime::error::Context;
use wasmtime::{Engine, Store};

use crate::api::http::{Http, Request, Response};
use crate::assets::FsStoredAssets;
use crate::wasi::WasiStates;

#[cfg(target_os = "android")]
pub mod android;
pub mod api;
pub mod assets;
pub mod logging;
pub mod wasi;

#[derive(Default)]
pub struct LappsManager {
    engine: Engine,
}

impl LappsManager {
    pub fn new() -> Self {
        let engine = Engine::default();

        Self { engine }
    }

    /// Loads the backends of the lapps found in `settings.path`.
    ///
    /// A lapp that fails to load is skipped with an error in the log.
    pub fn load_lapps(&self, settings: LappsSettings) -> Lapps {
        let mut lapps = HashMap::new();
        let Ok(read_dir) = fs::read_dir(&settings.path).inspect_err(|err| {
            log::error!(
                "Failed to read the lapps directory '{}': {err}",
                settings.path.display()
            )
        }) else {
            return lapps;
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

            if !entry.path().is_dir() {
                log::debug!("Skip '{}': not a lapp directory", entry.path().display());
                continue;
            }

            if let Some(allowed_lapps) = &settings.allowed
                && !allowed_lapps.contains(&lapp_name)
            {
                continue;
            }

            let Ok(lapp) = LappState::load(&self.engine, entry.path(), lapp_name.clone())
                .inspect_err(|err| log::error!("Failed to load lapp '{lapp_name}': {err:#}"))
            else {
                continue;
            };

            lapps.insert(lapp_name, lapp);
        }

        lapps
    }
}

#[allow(dead_code)]
pub struct LappState {
    name: String,
    store: Store<WasiStates>,
    http_instance: Http,
}

impl LappState {
    /// Loads `<root_path>/<name>-back.wasm` from the filesystem.
    pub fn load(engine: &Engine, root_path: impl AsRef<Path>, name: impl Into<String>) -> anyhow::Result<Self> {
        let name = name.into();
        let path = root_path.as_ref().join(format!("{name}-back.wasm"));
        let component = Component::from_file(engine, &path)
            .with_context(|| format!("Component file '{}' not found", path.display()))?;

        Self::new(engine, name, component)
    }

    fn new(engine: &Engine, name: String, component: Component) -> anyhow::Result<Self> {
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

type Lapps = HashMap<String, LappState>;

pub struct State {
    pub lang: String,
    pub lapps: HashMap<String, Mutex<LappState>>,
}

pub fn run_desktop(settings: LaplaceSettings) {
    log::info!("Run");
    let manager = LappsManager::new();
    let lapps = manager.load_lapps(settings.lapps);

    start(
        settings.platform.lang,
        lapps,
        Box::new(FsStoredAssets::new(settings.platform.frontend_assets_root)),
    )
}

/// Mobile entry point called by the generated Android project.
///
/// A mobile package has no CLI and nothing readable as files next to the binary: `Laplace.toml`,
/// the lapp frontends and the lapp backends live in the data directory that
/// [`android::prepare_data_dir`] picks and fills from the package assets. Everything the settings
/// point at is therefore relative to that directory.
#[cfg(target_os = "android")]
#[tauri::mobile_entry_point]
pub fn run_mobile() {
    // Logging is set up from the settings in the data directory, so until then failures are only
    // reported by the panic itself, which `tao` forwards to logcat under the `RustStdoutStderr` tag.
    let data = android::prepare_data_dir().expect("Failed to prepare the data directory");

    let mut settings = LaplaceSettings::new(data.config_path()).expect("Settings should be configured");
    settings.platform.frontend_assets_root = data.path.join(&settings.platform.frontend_assets_root);
    settings.lapps.path = data.path.join(&settings.lapps.path);

    logging::init_mobile(&settings.log);
    log::info!(
        "Run mobile: {} data directory '{}', unpacked {} files ({} bytes)",
        if data.shared { "shared" } else { "sandbox" },
        data.path.display(),
        data.unpacked_files,
        data.unpacked_bytes,
    );
    for warning in &data.warnings {
        log::warn!("{warning}");
    }

    let manager = LappsManager::new();
    let lapps = manager.load_lapps(settings.lapps);

    start(
        settings.platform.lang,
        lapps,
        Box::new(FsStoredAssets::new(settings.platform.frontend_assets_root)),
    )
}

/// Builds and runs the Tauri application.
///
/// `assets` replaces the assets embedded at compile time by [`tauri::generate_context`].
fn start(lang: String, lapps: Lapps, assets: Box<dyn Assets<Wry>>) {
    let mut context = tauri::generate_context!();
    context.set_assets(assets);

    tauri::Builder::default()
        .manage(State {
            lang,
            lapps: lapps.into_iter().map(|(name, lapp)| (name, Mutex::new(lapp))).collect(),
        })
        .invoke_handler(tauri::generate_handler![
            api::laplace::get_lang,
            api::laplace::open_lapp,
            api::http::handle_http,
        ])
        .run(context)
        .expect("error while running tauri application");
}
