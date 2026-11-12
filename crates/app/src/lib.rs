use laplace_settings::LaplaceSettings;
use tauri::{Assets, Wry};

use crate::assets::FsStoredAssets;
pub use crate::lapps::{Lapp, LappError, LappResult, LappState, LappsManager};

#[cfg(target_os = "android")]
pub mod android;
pub mod api;
pub mod assets;
pub mod lapps;
pub mod logging;
pub mod wasi;

pub struct State {
    pub lang: String,
    pub lapps: LappsManager,
}

pub fn run_desktop(settings: LaplaceSettings) {
    log::info!("Run");
    let lapps = LappsManager::load(&settings.lapps);

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
    settings.lapps.back_root = data.path.join(&settings.lapps.back_root);
    settings.lapps.config_root = data.path.join(&settings.lapps.config_root);
    settings.lapps.data_root = data.path.join(&settings.lapps.data_root);

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

    let lapps = LappsManager::load(&settings.lapps);

    start(
        settings.platform.lang,
        lapps,
        Box::new(FsStoredAssets::new(settings.platform.frontend_assets_root)),
    )
}

/// Builds and runs the Tauri application.
///
/// `assets` replaces the assets embedded at compile time by [`tauri::generate_context`].
fn start(lang: String, lapps: LappsManager, assets: Box<dyn Assets<Wry>>) {
    let mut context = tauri::generate_context!();
    context.set_assets(assets);

    tauri::Builder::default()
        .manage(State { lang, lapps })
        .invoke_handler(tauri::generate_handler![
            api::laplace::get_lang,
            api::laplace::get_lapps,
            api::laplace::update_lapp,
            api::laplace::open_lapp,
            api::http::handle_http,
        ])
        .run(context)
        .expect("error while running tauri application");
}
