use laplace_common_api::http;
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

    pub fn from_wit_http_response(response: Response) -> http::Response {
        http::Response {
            status: StatusCode::from_u16(response.status).expect("Invalid status code"),
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
        }
    }
}

#[tauri::command]
pub fn handle_http(
    webview_window: tauri::WebviewWindow,
    state: tauri::State<State>,
    request: http::Request,
) -> http::Response {
    let lapp_name = webview_window.label();
    let mut lapps = state.lapps.lock().expect("Lapps lock failed");
    let response = lapps
        .get_mut(lapp_name)
        .expect("Lapp not found")
        .http_handle(&convert::into_wit_http_request(request))
        .expect("Failed to call http::handle function");

    convert::from_wit_http_response(response)
}
