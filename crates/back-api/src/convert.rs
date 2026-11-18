//! The WIT `request`/`response` (`wit/types.wit`) to and from the `http` types.
//!
//! The rules are those of the host (`crates/app/src/api/http.rs`): the version is a string like
//! `HTTP/1.1`, an unknown one is HTTP/1.1. What `http` cannot hold is dropped: a header with an
//! invalid name or value, a URI that does not parse (the request gets `/`).

use crate::bindings::exports::laplace::lapp::types::{Header, Method, Request, Response, Uri};

pub fn from_wit_method(method: Method) -> http::Method {
    match method {
        Method::Options => http::Method::OPTIONS,
        Method::Get => http::Method::GET,
        Method::Post => http::Method::POST,
        Method::Put => http::Method::PUT,
        Method::Delete => http::Method::DELETE,
        Method::Head => http::Method::HEAD,
        Method::Trace => http::Method::TRACE,
        Method::Connect => http::Method::CONNECT,
        Method::Patch => http::Method::PATCH,
    }
}

pub fn into_wit_method(method: &http::Method) -> Method {
    match *method {
        http::Method::OPTIONS => Method::Options,
        http::Method::POST => Method::Post,
        http::Method::PUT => Method::Put,
        http::Method::DELETE => Method::Delete,
        http::Method::HEAD => Method::Head,
        http::Method::TRACE => Method::Trace,
        http::Method::CONNECT => Method::Connect,
        http::Method::PATCH => Method::Patch,
        _ => Method::Get,
    }
}

pub fn from_wit_version(version: &str) -> http::Version {
    match version {
        "HTTP/0.9" => http::Version::HTTP_09,
        "HTTP/1.0" => http::Version::HTTP_10,
        "HTTP/2.0" => http::Version::HTTP_2,
        "HTTP/3.0" => http::Version::HTTP_3,
        _ => http::Version::HTTP_11,
    }
}

pub fn into_wit_version(version: http::Version) -> String {
    format!("{version:?}")
}

pub fn from_wit_uri(uri: &Uri) -> http::Uri {
    let path_and_query = if uri.path_and_query.is_empty() {
        "/"
    } else {
        &uri.path_and_query
    };
    let parsed = if uri.scheme.is_empty() || uri.authority.is_empty() {
        path_and_query.parse()
    } else {
        http::Uri::builder()
            .scheme(uri.scheme.as_str())
            .authority(uri.authority.as_str())
            .path_and_query(path_and_query)
            .build()
            .or_else(|_| path_and_query.parse())
    };
    parsed.unwrap_or_else(|_| http::Uri::from_static("/"))
}

pub fn into_wit_uri(uri: &http::Uri) -> Uri {
    Uri {
        scheme: uri.scheme_str().unwrap_or_default().into(),
        authority: uri.authority().map(ToString::to_string).unwrap_or_default(),
        path_and_query: uri.path_and_query().map(ToString::to_string).unwrap_or_default(),
    }
}

pub fn from_wit_headers(headers: Vec<Header>) -> http::HeaderMap {
    headers
        .into_iter()
        .filter_map(|Header { name, value }| {
            Some((
                http::HeaderName::try_from(name).ok()?,
                http::HeaderValue::from_bytes(&value).ok()?,
            ))
        })
        .collect()
}

pub fn into_wit_headers(headers: &http::HeaderMap) -> Vec<Header> {
    headers
        .iter()
        .map(|(name, value)| Header {
            name: name.as_str().into(),
            value: value.as_bytes().to_vec(),
        })
        .collect()
}

pub fn from_wit_request(request: Request) -> http::Request<Vec<u8>> {
    let mut converted = http::Request::new(request.body);
    *converted.method_mut() = from_wit_method(request.method);
    *converted.uri_mut() = from_wit_uri(&request.uri);
    *converted.version_mut() = from_wit_version(&request.version);
    *converted.headers_mut() = from_wit_headers(request.headers);
    converted
}

pub fn into_wit_request(request: &http::Request<Vec<u8>>) -> Request {
    Request {
        method: into_wit_method(request.method()),
        uri: into_wit_uri(request.uri()),
        version: into_wit_version(request.version()),
        headers: into_wit_headers(request.headers()),
        body: request.body().clone(),
    }
}

pub fn from_wit_response(response: Response) -> http::Response<Vec<u8>> {
    let mut converted = http::Response::new(response.body);
    *converted.status_mut() =
        http::StatusCode::from_u16(response.status).unwrap_or(http::StatusCode::INTERNAL_SERVER_ERROR);
    *converted.version_mut() = from_wit_version(&response.version);
    *converted.headers_mut() = from_wit_headers(response.headers);
    converted
}

pub fn into_wit_response(response: http::Response<Vec<u8>>) -> Response {
    let (parts, body) = response.into_parts();
    Response {
        status: parts.status.as_u16(),
        version: into_wit_version(parts.version),
        headers: into_wit_headers(&parts.headers),
        body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wit_request() -> Request {
        Request {
            method: Method::Post,
            uri: Uri {
                scheme: String::new(),
                authority: String::new(),
                path_and_query: "/echo/api/hello?x=1".into(),
            },
            version: "HTTP/2.0".into(),
            headers: vec![
                Header {
                    name: "content-type".into(),
                    value: b"text/plain".to_vec(),
                },
                Header {
                    name: "bad name".into(),
                    value: b"dropped".to_vec(),
                },
            ],
            body: b"body".to_vec(),
        }
    }

    #[test]
    fn request_from_wit() {
        let request = from_wit_request(wit_request());

        assert_eq!(request.method(), http::Method::POST);
        assert_eq!(request.uri(), "/echo/api/hello?x=1");
        assert_eq!(request.version(), http::Version::HTTP_2);
        assert_eq!(request.headers().len(), 1);
        assert_eq!(request.headers()[http::header::CONTENT_TYPE], "text/plain");
        assert_eq!(request.body(), b"body");
    }

    #[test]
    fn request_round_trip() {
        let request = into_wit_request(&from_wit_request(wit_request()));

        assert!(matches!(request.method, Method::Post));
        assert_eq!(request.uri.path_and_query, "/echo/api/hello?x=1");
        assert_eq!(request.version, "HTTP/2.0");
        assert_eq!(request.headers.len(), 1);
        assert_eq!(request.body, b"body");
    }

    #[test]
    fn absolute_uri_from_wit() {
        let uri = from_wit_uri(&Uri {
            scheme: "https".into(),
            authority: "example.org".into(),
            path_and_query: String::new(),
        });

        assert_eq!(uri, "https://example.org/");
        assert_eq!(into_wit_uri(&uri).authority, "example.org");
        assert_eq!(
            from_wit_uri(&Uri {
                scheme: String::new(),
                authority: String::new(),
                path_and_query: "not a uri".into(),
            }),
            "/"
        );
    }

    #[test]
    fn response_round_trip() {
        let response = http::Response::builder()
            .status(http::StatusCode::NOT_FOUND)
            .version(http::Version::HTTP_10)
            .header("x-lapp", "echo")
            .body(b"missing".to_vec())
            .unwrap();

        let wit = into_wit_response(response);
        assert_eq!((wit.status, wit.version.as_str()), (404, "HTTP/1.0"));

        let response = from_wit_response(wit);
        assert_eq!(response.status(), http::StatusCode::NOT_FOUND);
        assert_eq!(response.headers()["x-lapp"], "echo");
        assert_eq!(response.body(), b"missing");
        assert_eq!(from_wit_version("HTTP/9"), http::Version::HTTP_11);
    }
}
