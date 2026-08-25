pub use http::{
    self, HeaderMap, HeaderName, HeaderValue, Method, StatusCode, Uri, Version, header, method, status, uri, version,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Header {
    pub name: String,
    pub value: Vec<u8>,
}

#[derive(Clone, Default, Debug, Deserialize, Serialize)]
pub struct Request {
    #[serde(with = "http_serde::method")]
    pub method: Method,

    #[serde(with = "http_serde::uri")]
    pub uri: Uri,

    #[serde(with = "http_serde::version")]
    pub version: Version,

    pub headers: Vec<Header>,

    pub body: Vec<u8>,
}

impl Request {
    pub fn builder() -> RequestBuilder {
        RequestBuilder::default()
    }

    pub fn get<T>(uri: T) -> RequestBuilder
    where
        T: TryInto<Uri>,
        <T as TryInto<Uri>>::Error: Into<http::Error>,
    {
        Request::builder().method(Method::GET).uri(uri)
    }
}

pub struct RequestBuilder {
    request: Result<Request, http::Error>,
}

impl Default for RequestBuilder {
    #[inline]
    fn default() -> Self {
        Self {
            request: Ok(Request::default()),
        }
    }
}

impl RequestBuilder {
    pub fn uri<T>(self, uri: T) -> Self
    where
        T: TryInto<Uri>,
        <T as TryInto<Uri>>::Error: Into<http::Error>,
    {
        self.and_then(move |mut request| {
            request.uri = uri.try_into().map_err(Into::into)?;
            Ok(request)
        })
    }

    pub fn method<T>(self, method: T) -> Self
    where
        T: TryInto<Method>,
        <T as TryInto<Method>>::Error: Into<http::Error>,
    {
        self.and_then(move |mut request| {
            let method = method.try_into().map_err(Into::into)?;
            request.method = method;
            Ok(request)
        })
    }

    pub fn body(self, body: Vec<u8>) -> Self {
        self.and_then(move |mut request| {
            request.body = body;
            Ok(request)
        })
    }

    pub fn build(self) -> Result<Request, http::Error> {
        self.request
    }

    fn and_then(self, func: impl FnOnce(Request) -> Result<Request, http::Error>) -> Self {
        Self {
            request: self.request.and_then(func),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Response {
    #[serde(with = "http_serde::status_code")]
    pub status: StatusCode,

    #[serde(with = "http_serde::version")]
    pub version: Version,

    pub headers: Vec<Header>,

    pub body: Vec<u8>,
}
