//! wasm-bindgen entry points of the browser build. The app is `hs2-native`
//! itself; the page (`web/index.html`) feeds content files chosen by the user.

use wasm_bindgen::prelude::*;

/// Add a Hypersonic 2 content file (relative path, bytes) chosen in the page.
#[wasm_bindgen]
pub fn add_file(path: &str, bytes: Vec<u8>) {
    hs2_native::platform::add_file(path, bytes);
}

/// Start the app in canvas `canvas_id`.
#[wasm_bindgen]
pub async fn start(canvas_id: String) -> Result<(), JsValue> {
    hs2_native::web_start(&canvas_id).await
}

/// Version text (name, version, commits, build date) for the page.
#[wasm_bindgen]
pub fn version() -> String {
    hs2_native::version::about()
}
