//! Synth in the style of Hypersonic 2: egui front panel on `hs2-core`.
//! The same code runs natively (cpal + midir) and in the browser
//! (Web Audio + Web MIDI); `audio`, `midiin` and `platform` hide the differences.

mod app;
mod audio;
mod mapwin;
mod midiin;
mod pages;
mod panel;
pub mod platform;
pub mod version;

#[cfg(not(target_arch = "wasm32"))]
pub use app::native_main;
#[cfg(target_arch = "wasm32")]
pub use app::web_start;
