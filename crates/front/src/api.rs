use std::future::Future;

use laplace_front_api::{invoke, invoke_without_args};
use serde::{Deserialize, Serialize};
use wasm_bindgen::JsValue;
use wasm_dom::UnwrapThrowExt;

pub async fn get_lang() -> JsValue {
    invoke_without_args("get_lang").await
}

#[derive(Serialize, Deserialize)]
struct OpenLappArgs<'a> {
    name: &'a str,
}

pub fn open_lapp(name: impl AsRef<str>) -> impl Future<Output = JsValue> + 'static {
    let args = serde_wasm_bindgen::to_value(&OpenLappArgs { name: name.as_ref() }).unwrap_throw();
    invoke("open_lapp", args)
}
