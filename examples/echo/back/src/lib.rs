use laplace_back_api::http::{Request, Response};
use laplace_back_api::{Lapp, Router};

struct Echo;

impl Lapp for Echo {
    fn handle(request: Request<Vec<u8>>) -> Response<Vec<u8>> {
        Router::new().fallback(echo).handle(request)
    }
}

/// Answers with the URI of the request and its headers.
fn echo(request: Request<Vec<u8>>) -> Response<Vec<u8>> {
    let uri = request.uri();
    let body = format!(
        "Echo {}://{}/{}",
        uri.scheme_str().unwrap_or_default(),
        uri.authority().map(ToString::to_string).unwrap_or_default(),
        uri.path_and_query().map(ToString::to_string).unwrap_or_default(),
    );

    let (parts, _) = request.into_parts();
    let mut response = Response::new(body.into_bytes());
    *response.version_mut() = parts.version;
    *response.headers_mut() = parts.headers;
    response
}

laplace_back_api::export!(Echo);
