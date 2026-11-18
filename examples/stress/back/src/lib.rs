//! A test backend:
//! - loops forever on /loop
//! - runs out of memory on /alloc
//! - tells the last init version on /previous
//! - prints on /print
//! - panics on /panic

use std::hint::black_box;
use std::sync::Mutex;

use laplace_back_api::http::{Request, Response};
use laplace_back_api::{Lapp, api_path};

/// The `previous_version` the last `init` was given.
static PREVIOUS_VERSION: Mutex<Option<String>> = Mutex::new(None);

struct Stress;

impl Lapp for Stress {
    fn init(previous_version: Option<String>) -> Result<(), String> {
        if previous_version.as_deref() == Some("fail") {
            return Err("init is told to fail".into());
        }
        *PREVIOUS_VERSION.lock().unwrap() = previous_version;
        Ok(())
    }

    fn handle(request: Request<Vec<u8>>) -> Response<Vec<u8>> {
        let body = match api_path(request.uri().path()) {
            Some("/loop") =>
            {
                #[allow(clippy::empty_loop)]
                loop {}
            },
            Some("/alloc") => {
                let memory = vec![1u8; 1 << 30];
                black_box(&memory);
                "ok".into()
            },
            Some("/previous") => PREVIOUS_VERSION
                .lock()
                .unwrap()
                .clone()
                .unwrap_or_else(|| "none".into()),
            Some("/print") => {
                println!("stress stdout line");
                eprint!("stress stderr ");
                eprintln!("line");
                // Unbuffered, unlike stdout: the host gets the tail without a line break.
                eprint!("stress stderr tail");
                "ok".into()
            },
            Some("/panic") => panic!("stress is told to panic"),
            _ => "ok".into(),
        };

        Response::new(body.into_bytes())
    }
}

laplace_back_api::export!(Stress);
