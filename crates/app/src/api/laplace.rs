use std::path::Path;

use tauri::webview::{DownloadEvent, PageLoadEvent};
use tauri::{AppHandle, WebviewUrl, WebviewWindowBuilder};

use crate::State;

#[tauri::command]
pub fn get_lang(state: tauri::State<State>) -> String {
    state.lang.clone()
}

#[tauri::command]
pub fn open_lapp(handle: AppHandle, name: &str) {
    log::debug!("Open lapp: {name}");

    let _wapp_window = WebviewWindowBuilder::new(&handle, name, WebviewUrl::App(Path::new(name).join("index.html")))
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
        })
        .build()
        .expect("Failed to open lapp");
}
