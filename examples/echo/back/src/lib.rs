use crate::bindings::exports::laplace::http::by_client_handler::{Guest, Request, Response};

#[allow(warnings)]
#[rustfmt::skip]
mod bindings;

struct HttpHandler;

impl Guest for HttpHandler {
    fn handle(request: Request) -> Response {
        let body = format!(
            "Echo {}://{}/{}",
            request.uri.scheme, request.uri.authority, request.uri.path_and_query
        );

        Response {
            status: 200,
            version: request.version,
            headers: request.headers,
            body: body.into_bytes(),
        }
    }
}

bindings::export!(HttpHandler with_types_in bindings);
