use std::path::Path;

use tauri::webview::{DownloadEvent, PageLoadEvent};
use tauri::{AppHandle, Manager as _, WebviewUrl, WebviewWindowBuilder};

use crate::State;

#[tauri::command]
pub fn get_lang(state: tauri::State<State>) -> String {
    state.lang.clone()
}

/// The activity class a lapp window is a webview of, declared in `AndroidManifest.xml`.
///
/// A mobile window is an activity, and one of this class is started for every lapp: the manifest
/// makes it a document activity, so each lapp becomes an entry of the recents screen of its own
/// while the launcher window keeps running in the main task. Without it `tao` would put the webview
/// into the activity the launcher already occupies.
#[cfg(target_os = "android")]
const LAPP_ACTIVITY: &str = "LappActivity";

#[tauri::command]
pub fn open_lapp(handle: AppHandle, name: &str) {
    log::debug!("Open lapp: {name}");

    // The window label is the lapp name — that is what `handle_http` routes by — so a lapp has a
    // single window, and opening it again raises the one that is there instead of failing on the
    // label that is taken.
    if let Some(window) = handle.get_webview_window(name) {
        log::debug!("Lapp '{name}' is already open");
        if let Err(err) = window.set_focus() {
            log::warn!("Failed to focus the window of lapp '{name}': {err}");
        }
        return;
    }

    let builder = WebviewWindowBuilder::new(&handle, name, WebviewUrl::App(Path::new(name).join("index.html")))
        .on_download(|_view, event| {
            match event {
                DownloadEvent::Requested { url, destination } => {
                    log::debug!("Download: Requested {url:?}, {destination:?}")
                },
                DownloadEvent::Finished { url, path, success: _ } => {
                    log::debug!("Download: Finished {url:?}, {path:?}")
                },
                _ => todo!(),
            }
            true
        })
        .on_page_load(|_window, payload| match payload.event() {
            PageLoadEvent::Started => {
                log::debug!("{} starting loading", payload.url());
            },
            PageLoadEvent::Finished => {
                log::debug!("{} finished loading", payload.url());
            },
        })
        .on_navigation(|url| {
            log::debug!("Navigation: {url}");
            true
        })
        .on_web_resource_request(|request, _response| {
            log::debug!("Request: {}", request.uri());
        });

    #[cfg(target_os = "android")]
    let builder = builder.activity_name(LAPP_ACTIVITY);

    let _lapp_window = builder.build().expect("Failed to open lapp");
}
