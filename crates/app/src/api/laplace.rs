use std::path::Path;

use laplace_settings::lapp::LappInfo;
use laplace_settings::lapp::update::UpdateQuery;
use tauri::webview::{DownloadEvent, PageLoadEvent};
use tauri::{AppHandle, Manager as _, WebviewUrl, WebviewWindowBuilder};

use crate::State;
#[cfg(target_os = "android")]
use crate::android;

#[tauri::command]
pub fn get_lang(state: tauri::State<State>) -> String {
    state.lang.clone()
}

#[tauri::command]
pub fn get_lapps(state: tauri::State<State>) -> Vec<LappInfo> {
    state.lapps.lapps()
}

/// Runs off the main thread: it writes the user settings file and waits for a call in progress
/// to stop the backend.
#[tauri::command]
pub async fn update_lapp(app: AppHandle, query: UpdateQuery) -> Result<UpdateQuery, String> {
    tauri::async_runtime::spawn_blocking(move || app.state::<State>().lapps.update_lapp(query))
        .await
        .map_err(|err| format!("The lapp update task has failed: {err}"))
        .and_then(|result| result.map_err(|err| err.to_string()))
        .inspect_err(|err| log::error!("{err}"))
}

/// Async, since building a window in a synchronous command deadlocks on Windows.
#[tauri::command]
pub async fn open_lapp(handle: AppHandle, state: tauri::State<'_, State>, name: &str) -> Result<(), String> {
    log::debug!("Open lapp: {name}");
    state
        .lapps
        .check_open(name)
        .map_err(|err| err.to_string())
        .inspect_err(|err| log::error!("{err}"))?;

    // The window label is the lapp name — that is what `handle_http` routes by — so a lapp has a
    // single window, and opening it again raises the one that is there instead of failing on the
    // label that is taken.
    if let Some(window) = handle.get_webview_window(name) {
        log::debug!("Lapp '{name}' is already open");
        raise(&window, name);
        return Ok(());
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
                _ => log::debug!("Download: other event"),
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
    let builder = builder.activity_name(android::LAPP_ACTIVITY);

    builder
        .build()
        .map(drop)
        .map_err(|err| format!("Failed to open the window of lapp '{name}': {err}"))
        .inspect_err(|err| log::error!("{err}"))
}

/// Brings the window that is already open back in front of the user.
#[cfg(not(target_os = "android"))]
fn raise(window: &tauri::WebviewWindow, name: &str) {
    if let Err(err) = window.set_focus() {
        log::warn!("Failed to focus the window of lapp '{name}': {err:#}");
    }
}

/// The window is an activity in a task of its own, which `set_focus` does not raise — it is a no-op
/// on Android — so the task is moved to the front through the activity instead.
#[cfg(target_os = "android")]
fn raise(_window: &tauri::WebviewWindow, name: &str) {
    match android::move_lapp_window_to_front(name) {
        Ok(true) => {},
        Ok(false) => log::warn!("No window of lapp '{name}' to bring to the front"),
        Err(err) => log::warn!("Failed to bring the window of lapp '{name}' to the front: {err:#}"),
    }
}
