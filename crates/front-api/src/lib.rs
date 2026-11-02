use wasm_bindgen::JsValue;
use wasm_bindgen::prelude::wasm_bindgen;

pub mod http;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = invoke)]
    pub async fn invoke_without_args(cmd: &str) -> JsValue;

    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"])]
    pub async fn invoke(cmd: &str, args: JsValue) -> JsValue;

    /// The same as [`invoke`], but a command that fails rejects the promise with its error, which
    /// comes back as `Err` instead of an exception thrown into the wasm.
    #[wasm_bindgen(js_namespace = ["window", "__TAURI__", "core"], js_name = invoke, catch)]
    pub async fn try_invoke(cmd: &str, args: JsValue) -> Result<JsValue, JsValue>;
}
