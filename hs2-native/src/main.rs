//! Native desktop entry point; the app itself lives in the library so the web
//! build (`../hs2-web`) runs the same code.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(target_arch = "wasm32"))]
fn main() -> eframe::Result<()> {
    hs2_native::native_main()
}

#[cfg(target_arch = "wasm32")]
fn main() {}
