use crate::bindings::exports::laplace::http::by_client_handler::{Guest, Request, Response};

#[allow(warnings)]
#[rustfmt::skip]
mod bindings;

struct HttpHandler;

impl Guest for HttpHandler {
    fn handle(request: Request) -> Response {
        let path = request.uri.path_and_query.as_str();
        if path.ends_with("/loop") {
            #[allow(clippy::empty_loop)]
            loop {}
        }
        if path.ends_with("/alloc") {
            let memory = vec![1u8; 1 << 30];
            std::hint::black_box(&memory);
        }

        Response {
            status: 200,
            version: request.version,
            headers: Vec::new(),
            body: b"ok".to_vec(),
        }
    }
}

bindings::export!(HttpHandler with_types_in bindings);
