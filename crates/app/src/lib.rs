use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::{fs, io};

use laplace_settings::LaplaceSettings;
use laplace_settings::laplace::LappsSettings;
use tauri::http;
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
                LappState::new(&self.engine, entry.path(), lapp_name)?,
            );
        }

        Ok(lapps)
    }
}

#[allow(dead_code)]
pub struct LappState {
    name: String,
    path: PathBuf,
    store: Store<WasiStates>,
    http_instance: Http,
}

impl LappState {
    pub fn new(engine: &Engine, root_path: impl AsRef<Path>, name: impl Into<String>) -> anyhow::Result<Self> {
        let name = name.into();
        let path = root_path.as_ref().join(format!("{name}-back.wasm"));
        let component = Component::from_file(engine, &path)
            .with_context(|| format!("Component file '{}' not found", path.display()))?;

        let wasi_states = WasiStates::new();
        let mut store = Store::new(engine, wasi_states);

        let mut linker = Linker::new(engine);
        wasmtime_wasi::p2::add_to_linker_sync(&mut linker)?;

        let http_instance =
            Http::instantiate(&mut store, &component, &linker).context("Failed to instantiate the Http world")?;

        Ok(Self {
            name,
            path,
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

pub fn run(settings: LaplaceSettings) {
    println!("Run");
    let manager = LappsManager::new();
    let lapps = manager.load_lapps(settings.lapps).expect("Failed load lapps");

    let mut context = tauri::generate_context!();
    context.set_assets(Box::new(FsStoredAssets::new(settings.platform.frontend_assets_root)));

    tauri::Builder::default()
        .manage(State {
            lang: settings.platform.lang,
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
