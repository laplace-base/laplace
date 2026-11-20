//! Outgoing HTTP of the backend over `wasi:http`, blocking.
//!
//! The host lets a request through only with the `http` permission of the back and to its effective
//! hosts, by the methods and within the timeout of the manifest; otherwise the request fails with
//! [`Error::is_denied`].
//!
//! ```ignore
//! let request = http::Request::get("https://api.example.org/items").body(Vec::new())?;
//! let response = http_client::send(request)?;
//! ```

use http::Uri;
use wasip2::http::outgoing_handler;
use wasip2::http::types::{
    ErrorCode, Fields, HeaderError, IncomingBody, Method, OutgoingBody, OutgoingRequest, Scheme,
};
use wasip2::io::streams::StreamError;

/// How much of a body is written or read at once.
const CHUNK: usize = 4096;

/// An error of a request: a refusal of the host, an error of `wasi:http`, or an invalid request.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid header {0}: {1:?}")]
    InvalidHeader(String, HeaderError),

    #[error("invalid method {0:?}")]
    InvalidMethod(Method),

    #[error("The URI of the request is not absolute: {0:?}")]
    InvalidUri(Uri),

    #[error("invalid scheme {0:?}")]
    InvalidScheme(Scheme),

    #[error("invalid authority {0}")]
    InvalidAuthority(String),

    #[error("invalid path {0}")]
    InvalidPath(String),

    #[error("invalid response: {0}")]
    InvalidResponse(http::Error),

    #[error("The request body is taken")]
    BodyIsTaken,

    #[error("The request body stream is taken")]
    BodyStreamIsTaken,

    #[error("No response after waiting for it")]
    NoResponse,

    #[error("The response is taken")]
    ResponseIsTaken,

    #[error("The response body is taken")]
    ResponseBodyIsTaken,

    #[error("The response body stream is taken")]
    ResponseBodyStreamIsTaken,

    #[error("{0:?}")]
    Code(#[from] ErrorCode),

    #[error("{0}")]
    StreamLastOperationFailed(String),

    #[error("the stream is closed")]
    StreamClosed,
}

impl Error {
    /// Whether the host has refused the request: no `http` permission, a host, method or scheme that
    /// is not allowed.
    pub fn is_denied(&self) -> bool {
        matches!(self, Self::Code(ErrorCode::HttpRequestDenied))
    }

    pub fn message(&self) -> String {
        self.to_string()
    }
}

impl From<StreamError> for Error {
    fn from(err: StreamError) -> Self {
        match err {
            StreamError::LastOperationFailed(err) => Self::StreamLastOperationFailed(err.to_debug_string()),
            StreamError::Closed => Self::StreamClosed, //new("the stream is closed"),
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

/// Sends `request` and waits for the whole response. The URI must be absolute.
pub fn send(request: http::Request<Vec<u8>>) -> Result<http::Response<Vec<u8>>> {
    let (parts, body) = request.into_parts();

    let headers = Fields::new();
    for (name, value) in &parts.headers {
        headers
            .append(name.as_str(), value.as_bytes())
            .map_err(|err| Error::InvalidHeader(name.to_string(), err))?;
    }

    let outgoing = OutgoingRequest::new(headers);

    let method = method(&parts.method);
    outgoing
        .set_method(&method)
        .map_err(|()| Error::InvalidMethod(method))?;

    let scheme = match parts.uri.scheme_str() {
        Some("http") => Scheme::Http,
        Some("https") => Scheme::Https,
        Some(other) => Scheme::Other(other.into()),
        None => return Err(Error::InvalidUri(parts.uri)),
    };
    outgoing
        .set_scheme(Some(&scheme))
        .map_err(|()| Error::InvalidScheme(scheme))?;

    let authority = parts.uri.authority().map(|authority| authority.as_str());
    outgoing
        .set_authority(authority)
        .map_err(|()| Error::InvalidAuthority(format!("{authority:?}")))?;

    let path = parts.uri.path_and_query().map(|path| path.as_str());
    outgoing
        .set_path_with_query(path)
        .map_err(|()| Error::InvalidPath(format!("{path:?}")))?;

    let outgoing_body = outgoing.body().map_err(|()| Error::BodyIsTaken)?;
    let future = outgoing_handler::handle(outgoing, None)?;
    {
        let stream = outgoing_body.write().map_err(|()| Error::BodyStreamIsTaken)?;
        for chunk in body.chunks(CHUNK) {
            stream.blocking_write_and_flush(chunk)?;
        }
    }
    OutgoingBody::finish(outgoing_body, None)?;

    future.subscribe().block();
    let incoming = future
        .get()
        .ok_or_else(|| Error::NoResponse)?
        .map_err(|()| Error::ResponseIsTaken)??;

    let mut response = http::Response::builder().status(incoming.status());
    for (name, value) in incoming.headers().entries() {
        response = response.header(name, value);
    }

    let incoming_body = incoming.consume().map_err(|()| Error::ResponseBodyIsTaken)?;
    let mut body = Vec::new();
    {
        let stream = incoming_body.stream().map_err(|()| Error::ResponseBodyStreamIsTaken)?;
        loop {
            match stream.blocking_read(CHUNK as u64) {
                Ok(chunk) => body.extend(chunk),
                Err(StreamError::Closed) => break,
                Err(err) => return Err(err.into()),
            }
        }
    }
    drop(IncomingBody::finish(incoming_body));

    response.body(body).map_err(Error::InvalidResponse)
}

fn method(method: &http::Method) -> Method {
    match *method {
        http::Method::GET => Method::Get,
        http::Method::HEAD => Method::Head,
        http::Method::POST => Method::Post,
        http::Method::PUT => Method::Put,
        http::Method::DELETE => Method::Delete,
        http::Method::CONNECT => Method::Connect,
        http::Method::OPTIONS => Method::Options,
        http::Method::TRACE => Method::Trace,
        http::Method::PATCH => Method::Patch,
        ref other => Method::Other(other.as_str().into()),
    }
}
