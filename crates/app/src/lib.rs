use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;
use std::{fs, io};

use laplace_settings::LaplaceSettings;
use laplace_settings::laplace::LappsSettings;
use tauri::{Assets, Wry, http};
use wasmtime::component::{Component, Linker};
use wasmtime::error::Context;
use wasmtime::{Engine, Store};

use crate::api::http::{Http, Request, Response};
use crate::assets::FsStoredAssets;
use crate::wasi::WasiStates;

pub mod api;
pub mod assets;
pub mod logging;
pub mod wasi;

/// The lapp backends embedded into the binary by `build.rs`. Empty on desktop targets.
#[cfg(mobile)]
pub mod bundled_lapps {
    include!(concat!(env!("OUT_DIR"), "/bundled_lapps.rs"));
}

#[derive(Default)]
pub struct LappsManager {
    engine: Engine,
}

impl LappsManager {
    pub fn new() -> Self {
        let engine = Engine::default();

        Self { engine }
    }

    pub fn load_lapps(&self, settings: LappsSettings) -> anyhow::Result<Lapps> {
        let mut lapps = HashMap::new();
        let read_dir = fs::read_dir(&settings.path)?;

        for entry in read_dir {
            let entry = entry?;
            let lapp_name = entry.file_name().into_string().map_err(|invalid_name| {
                log::error!("Lapp name '{invalid_name:?}' is not valid UTF-8");
                io::Error::from(io::ErrorKind::InvalidData)
            })?;

            if let Some(allowed_lapps) = &settings.allowed
                && !allowed_lapps.contains(&lapp_name)
            {
                continue;
            }

            lapps.insert(
                lapp_name.clone(),
                LappState::from_file(&self.engine, entry.path(), lapp_name)?,
            );
        }

        Ok(lapps)
    }

    /// Loads the lapps embedded into the binary at build time.
    ///
    /// Used on mobile, where there is no lapps directory to scan next to the binary.
    pub fn load_bundled_lapps(&self, bundled: &[(&str, &[u8])]) -> anyhow::Result<Lapps> {
        bundled
            .iter()
            .map(|(name, component)| {
                Ok((
                    name.to_string(),
                    LappState::from_binary(&self.engine, *name, component)?,
                ))
            })
            .collect()
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
    pub fn from_file(engine: &Engine, root_path: impl AsRef<Path>, name: impl Into<String>) -> anyhow::Result<Self> {
        let name = name.into();
        let path = root_path.as_ref().join(format!("{name}-back.wasm"));
        let component = Component::from_file(engine, &path)
            .with_context(|| format!("Component file '{}' not found", path.display()))?;

        Self::new(engine, name, component)
    }

    /// Loads a component from bytes embedded into the binary.
    pub fn from_binary(engine: &Engine, name: impl Into<String>, component: &[u8]) -> anyhow::Result<Self> {
        let name = name.into();
        let component = Component::from_binary(engine, component)
            .with_context(|| format!("Invalid bundled component of the lapp '{name}'"))?;

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
    pub lapps: Mutex<Lapps>,
}

pub fn run_desktop(settings: LaplaceSettings) {
    log::info!("Run");
    let manager = LappsManager::new();
    let lapps = manager.load_lapps(settings.lapps).expect("Failed load lapps");

    start(
        settings.platform.lang,
        lapps,
        Some(Box::new(FsStoredAssets::new(settings.platform.frontend_assets_root))),
    )
}

/// Mobile entry point called by the generated Android/iOS projects.
///
/// A mobile package has no CLI, no `Laplace.toml` next to the binary and no writable lapps
/// directory, so everything is taken from the package itself: the frontend from the assets embedded
/// by [`tauri::generate_context`] and the lapp backends from [`bundled_lapps`].
#[cfg(mobile)]
#[tauri::mobile_entry_point]
pub fn run_mobile() {
    let settings = LaplaceSettings::default();
    logging::init_mobile(&settings.log);

    log::info!("Run mobile");
    let manager = LappsManager::new();
    let lapps = manager
        .load_bundled_lapps(bundled_lapps::LAPPS)
        .expect("Failed load bundled lapps");

    start(settings.platform.lang, lapps, None)
}

/// Builds and runs the Tauri application.
///
/// `assets` replaces the assets embedded at compile time; passing [`None`] keeps the embedded ones.
fn start(lang: String, lapps: Lapps, assets: Option<Box<dyn Assets<Wry>>>) {
    let mut context = tauri::generate_context!();
    if let Some(assets) = assets {
        context.set_assets(assets);
    }

    tauri::Builder::default()
        .manage(State {
            lang,
            lapps: Mutex::new(lapps),
        })
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            api::laplace::get_lang,
            api::laplace::open_lapp,
            api::http::handle_http,
        ])
        .register_uri_scheme_protocol("app", move |ctx, request| {
            log::info!("Request: {request:?}, label: {}", ctx.webview_label());
            let asset_path = request.uri().path();

            let content = match fs::read(asset_path) {
                Ok(content) => content,
                Err(err) => {
                    return http::Response::builder()
                        .status(404)
                        .body(err.to_string().into_bytes())
                        .unwrap();
                },
            };

            let mime_type = if asset_path.ends_with(".html") {
                mime::TEXT_HTML
            } else if asset_path.ends_with(".css") {
                mime::TEXT_CSS
            } else if asset_path.ends_with(".js") {
                mime::TEXT_JAVASCRIPT
            } else {
                mime::APPLICATION_OCTET_STREAM
            };

            http::Response::builder()
                .header("Content-Type", mime_type.as_ref())
                .body(content)
                .unwrap()
        })
        .run(context)
        .expect("error while running tauri application");
}
