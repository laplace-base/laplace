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
    log::info!("Open lapp: {name}");

    let _wapp_window = WebviewWindowBuilder::new(&handle, name, WebviewUrl::App(Path::new(name).join("index.html")))
        .on_download(|_view, event| {
            match event {
                DownloadEvent::Requested { url, destination } => {
                    log::info!("Download: Requested {url:?}, {destination:?}")
                },
                DownloadEvent::Finished { url, path, success: _ } => {
                    log::info!("Download: Finished {url:?}, {path:?}")
                },
                _ => todo!(),
            }
            true
        })
        .on_page_load(|_window, payload| match payload.event() {
            PageLoadEvent::Started => {
                log::info!("{} starting loading", payload.url());
            },
            PageLoadEvent::Finished => {
                log::info!("{} finished loading", payload.url());
            },
        })
        .on_navigation(|url| {
            log::info!("Navigation: {url}");
            true
        })
        .on_web_resource_request(|request, _response| {
            log::info!("Request: {}", request.uri());
        })
        .build()
        .expect("Failed to open lapp");
}
