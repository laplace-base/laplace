use std::path::PathBuf;
use std::sync::Mutex;

use laplace_app_lib::api::http::process_http;
use laplace_app_lib::{LappState, State};
use laplace_common_api::http::{Request, StatusCode};
use wasmtime::Engine;

// Host state with the given lapp backends.
fn state_with(names: &[&str]) -> State {
    let backs_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../lapps/back");
    let engine = Engine::default();
    let lapps = names
        .iter()
        .map(|&name| {
            let lapp = LappState::load(&engine, backs_dir.join(name), name)
                .unwrap_or_else(|err| panic!("Lapp '{name}' is not built, run `cargo make test`: {err:#}"));
            (name.to_owned(), Mutex::new(lapp))
        })
        .collect();

    State {
        lang: "en".into(),
        lapps,
    }
}

#[test]
fn echo_responds_to_get() {
    let state = state_with(&["echo"]);
    let request = Request::get("/echo/api/hello").build().unwrap();

    let response = process_http(&state, "echo", request).unwrap();

    assert_eq!(response.status, StatusCode::OK);
    assert_eq!(String::from_utf8(response.body).unwrap(), "Echo :////echo/api/hello");
}

#[test]
fn request_to_missing_lapp_fails() {
    let state = state_with(&[]);
    let request = Request::get("/missing/api/").build().unwrap();

    let err = process_http(&state, "missing", request).unwrap_err();

    assert_eq!(err, "Lapp 'missing' is not loaded");
}
