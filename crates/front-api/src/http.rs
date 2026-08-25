use std::future::Future;

use futures::FutureExt;
pub use laplace_common_api::http::*;
use serde::Serialize;
pub use serde_wasm_bindgen::Error;

use crate::invoke;

#[derive(Serialize)]
struct HandleHttpArgs<'a> {
    request: &'a Request,
}

pub fn handle_http(request: &Request) -> Result<impl Future<Output = Result<Response, Error>> + 'static, Error> {
    let args = serde_wasm_bindgen::to_value(&HandleHttpArgs { request })?;
    Ok(invoke("handle_http", args).map(serde_wasm_bindgen::from_value))
}
