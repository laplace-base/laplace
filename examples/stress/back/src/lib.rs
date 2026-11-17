//! A test backend:
//! - loops forever on /loop
//! - runs out of memory on /alloc
//! - tells the last init version on /previous
//! - prints on /print
//! - panics on /panic

use std::sync::Mutex;

use crate::bindings::exports::laplace::http::by_client_handler::{Guest, Request, Response};
use crate::bindings::exports::laplace::http::lifecycle::Guest as LifecycleGuest;

#[allow(warnings)]
#[rustfmt::skip]
mod bindings;

/// The `previous-version` the last `init` was given.
static PREVIOUS_VERSION: Mutex<Option<String>> = Mutex::new(None);

struct HttpHandler;

impl LifecycleGuest for HttpHandler {
    fn init(previous_version: Option<String>) -> Result<(), String> {
        if previous_version.as_deref() == Some("fail") {
            return Err("init is told to fail".into());
        }
        *PREVIOUS_VERSION.lock().unwrap() = previous_version;
        Ok(())
    }
}

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
        if path.ends_with("/print") {
            println!("stress stdout line");
            eprint!("stress stderr ");
            eprintln!("line");
            // Unbuffered, unlike stdout: the host gets the tail without a line break.
            eprint!("stress stderr tail");
        }
        if path.ends_with("/panic") {
            panic!("stress is told to panic");
        }
        let body = if path.ends_with("/previous") {
            PREVIOUS_VERSION
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "none".into())
        } else {
            "ok".into()
        };

        Response {
            status: 200,
            version: request.version,
            headers: Vec::new(),
            body: body.into_bytes(),
        }
    }
}

bindings::export!(HttpHandler with_types_in bindings);
