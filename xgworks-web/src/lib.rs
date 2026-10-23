//! wasm-bindgen entry points of the browser build. The app is
//! `xgworks-native` itself; the page (`web/index.html`) first puts the
//! XGworks data files into the virtual file system, then starts it.

use wasm_bindgen::prelude::*;

/// Add a file to the app's virtual file system (`"XG4/ws.ini"`, `"song/x.xws"`).
#[wasm_bindgen]
pub fn add_file(path: &str, bytes: Vec<u8>) {
    xgworks_native::platform::add_file(path, bytes);
}

/// Start the app in canvas `canvas_id`. `args` are the native command-line
/// options (`song/x.xws --list 3 --mixer …`).
#[wasm_bindgen]
pub async fn start(canvas_id: String, args: Vec<String>) -> Result<(), JsValue> {
    xgworks_native::web_start(&canvas_id, args).await
}

/// Version text (name, version, commits, build date) for the page.
#[wasm_bindgen]
pub fn version() -> String {
    xgworks_native::version::about()
}
