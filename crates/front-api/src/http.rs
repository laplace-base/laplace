use std::future::Future;

pub use laplace_common_api::http::*;
use serde::Serialize;
use wasm_bindgen::JsValue;

use crate::try_invoke;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The request or the response could not be converted to or from a JS value.
    #[error("Value conversion error: {0}")]
    Serde(#[from] serde_wasm_bindgen::Error),

    /// The host has failed to handle the request, e.g. the lapp is not loaded or its backend has
    /// failed.
    #[error("Invoke handling error: {0}")]
    Invoke(String),
}

impl From<JsValue> for Error {
    fn from(rejection: JsValue) -> Self {
        Self::Invoke(rejection.as_string().unwrap_or_else(|| format!("{rejection:?}")))
    }
}

#[derive(Serialize)]
struct HandleHttpArgs<'a> {
    request: &'a Request,
}

pub fn handle_http(request: &Request) -> Result<impl Future<Output = Result<Response, Error>> + 'static, Error> {
    let args = serde_wasm_bindgen::to_value(&HandleHttpArgs { request })?;

    Ok(async move {
        let response = try_invoke("handle_http", args).await?;
        Ok(serde_wasm_bindgen::from_value(response)?)
    })
}
