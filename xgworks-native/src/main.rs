//! Native desktop entry point; the app itself lives in the library (`app.rs`)
//! so the web build (`../xgworks-web`) runs the very same code.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    xgworks_native::native_main()
}

#[cfg(target_arch = "wasm32")]
fn main() {}
