use laplace_common_api::http;
use tauri::{AppHandle, Manager as _};
use wasmtime::component::bindgen;

pub use self::exports::laplace::http::types::{Header, Method, Request, Response, Uri};
use crate::State;

bindgen!({
    path: "../../wit/http.wit",
    world: "http"
});

mod convert {
    use super::http::StatusCode;
    use super::*;

    pub fn into_wit_http_method(method: http::Method) -> Method {
        match method {
            method if method == http::Method::GET => Method::Get,
            method if method == http::Method::POST => Method::Post,
            method if method == http::Method::PUT => Method::Put,
            method if method == http::Method::DELETE => Method::Delete,
            method if method == http::Method::HEAD => Method::Head,
            method if method == http::Method::OPTIONS => Method::Options,
            method if method == http::Method::CONNECT => Method::Connect,
            method if method == http::Method::PATCH => Method::Patch,
            method if method == http::Method::TRACE => Method::Trace,
            _ => Method::Get,
        }
    }

    pub fn into_wit_http_uri(uri: http::Uri) -> Uri {
        Uri {
            scheme: uri.scheme_str().unwrap_or_default().into(),
            authority: uri
                .authority()
                .map(|authority| authority.to_string())
                .unwrap_or_default(),
            path_and_query: uri
                .path_and_query()
                .map(|path_and_query| path_and_query.to_string())
                .unwrap_or_default(),
        }
    }

    pub fn into_wit_http_header(header: http::Header) -> Header {
        Header {
            name: header.name,
            value: header.value,
        }
    }

    pub fn into_wit_http_request(request: http::Request) -> Request {
        let http::Request {
            method,
            uri,
            version,
            headers,
            body,
        } = request;

        Request {
            method: into_wit_http_method(method),
            uri: into_wit_http_uri(uri),
            version: format!("{version:?}"),
            headers: headers.into_iter().map(into_wit_http_header).collect(),
            body,
        }
    }

    pub fn from_wit_http_version(version: &str) -> http::Version {
        match version {
            "HTTP/0.9" => http::Version::HTTP_09,
            "HTTP/1.0" => http::Version::HTTP_10,
            "HTTP/1.1" => http::Version::HTTP_11,
            "HTTP/2.0" => http::Version::HTTP_2,
            "HTTP/3.0" => http::Version::HTTP_3,
            _ => http::Version::HTTP_11,
        }
    }

    pub fn from_wit_http_response(response: Response) -> Result<http::Response, http::status::InvalidStatusCode> {
        Ok(http::Response {
            status: StatusCode::from_u16(response.status)?,
            version: from_wit_http_version(&response.version),
            headers: response
                .headers
                .into_iter()
                .map(|header| http::Header {
                    name: header.name,
                    value: header.value,
                })
                .collect(),
            body: response.body,
        })
    }
}

/// Hands the request of a lapp frontend to the backend of the same lapp — the one named by the
/// label of the calling window.
///
/// A lapp that is not registered, may not be called or fails to handle the request is reported by
/// an error, which rejects the `invoke` promise of the frontend.
#[tauri::command]
pub async fn handle_http(
    app: AppHandle,
    webview_window: tauri::WebviewWindow,
    request: http::Request,
) -> Result<http::Response, String> {
    let lapp_name = webview_window.label().to_owned();

    tauri::async_runtime::spawn_blocking(move || process_http(&app.state::<State>(), &lapp_name, request))
        .await
        .unwrap_or_else(|err| Err(format!("The request handling task has failed: {err}")))
        .inspect_err(|err| log::error!("{err}"))
}

pub fn process_http(state: &State, lapp_name: &str, request: http::Request) -> Result<http::Response, String> {
    let response = state
        .lapps
        .handle_http(lapp_name, &convert::into_wit_http_request(request))?;

    convert::from_wit_http_response(response)
        .map_err(|err| format!("Lapp '{lapp_name}' returned an invalid response: {err}"))
}
