use std::borrow::Cow;
use std::fmt::Display;
use std::path::Path;

use laplace_settings::lapp::update::UpdateQuery;
use laplace_settings::lapp::{self, LappInfo};
use tauri::http::StatusCode;
use tauri::webview::{DownloadEvent, NewWindowResponse, PageLoadEvent};
use tauri::{AppHandle, Manager as _, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

use crate::State;
#[cfg(target_os = "android")]
use crate::android;
use crate::assets::is_asset_allowed;
use crate::lapps::{LappError, LappResult};

#[tauri::command]
pub fn get_lang(state: tauri::State<State>) -> String {
    state.lang.clone()
}

/// Lets a command of platform management through only from the launcher window.
///
/// Every such command lives in this module and calls it first.
pub fn require_launcher(window_label: &str, command: &'static str) -> LappResult<()> {
    if lapp::is_main(window_label) {
        Ok(())
    } else {
        Err(LappError::LauncherOnly {
            command,
            window: window_label.into(),
        })
    }
}

fn reject(err: impl Display) -> String {
    let message = err.to_string();
    log::error!("{message}");
    message
}

#[tauri::command]
pub fn get_lapps(webview_window: WebviewWindow, state: tauri::State<State>) -> Result<Vec<LappInfo>, String> {
    require_launcher(webview_window.label(), "get_lapps").map_err(reject)?;
    Ok(state.lapps.lapps())
}

/// Runs off the main thread: it writes the user settings file and waits for a call in progress
/// to stop the backend.
#[tauri::command]
pub async fn update_lapp(
    app: AppHandle,
    webview_window: WebviewWindow,
    query: UpdateQuery,
) -> Result<UpdateQuery, String> {
    require_launcher(webview_window.label(), "update_lapp").map_err(reject)?;
    tauri::async_runtime::spawn_blocking(move || app.state::<State>().lapps.update_lapp(query))
        .await
        .map_err(|err| reject(format!("The lapp update task has failed: {err}")))?
        .map_err(reject)
}

/// Async, since building a window in a synchronous command deadlocks on Windows.
#[tauri::command]
pub async fn open_lapp(
    handle: AppHandle,
    webview_window: WebviewWindow,
    state: tauri::State<'_, State>,
    name: &str,
) -> Result<(), String> {
    require_launcher(webview_window.label(), "open_lapp").map_err(reject)?;
    log::debug!("Open lapp: {name}");
    let title = state.lapps.check_open(name).map_err(reject)?.title(name);

    // The window label is the lapp name — that is what `handle_http` routes by — so a lapp has a
    // single window, and opening it again raises the one that is there instead of failing on the
    // label that is taken.
    if let Some(window) = handle.get_webview_window(name) {
        log::debug!("Lapp '{name}' is already open");
        raise(&window, name);
        return Ok(());
    }

    let builder = WebviewWindowBuilder::new(&handle, name, WebviewUrl::App(Path::new(name).join("index.html")))
        .title(title)
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
        .on_web_resource_request({
            let lapp_name = name.to_owned();

            move |request, response| {
                log::debug!("Request: {}", request.uri());

                if !is_asset_allowed(&lapp_name, request.uri().path()) {
                    log::warn!("Lapp '{lapp_name}' is denied the asset '{}'", request.uri());
                    *response.status_mut() = StatusCode::FORBIDDEN;
                    *response.body_mut() = Cow::Borrowed(&[]);
                }
            }
        })
        // A window opened by the lapp would be neither filtered nor routed to it.
        .on_new_window({
            let lapp_name = name.to_owned();

            move |url, _features| {
                log::warn!("Lapp '{lapp_name}' is denied opening a new window for '{url}'");
                NewWindowResponse::Deny
            }
        });

    #[cfg(target_os = "android")]
    let builder = builder.activity_name(android::LAPP_ACTIVITY);

    builder
        .build()
        .map(drop)
        .map_err(|err| reject(format!("Failed to open the window of lapp '{name}': {err}")))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_launcher_window_passes() {
        assert!(require_launcher(lapp::MAIN_NAME, "get_lapps").is_ok());

        let err = require_launcher("echo", "update_lapp").unwrap_err();
        assert!(matches!(
            &err,
            LappError::LauncherOnly { command: "update_lapp", window } if window == "echo"
        ));
        assert_eq!(
            err.to_string(),
            "Command 'update_lapp' is only available to the launcher, not to the window 'echo'"
        );
    }
}
