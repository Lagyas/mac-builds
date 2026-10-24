//! XGworks editor/player (egui) on `xgworks-core`. The same code runs natively
//! (eframe + midir, separate OS windows) and in the browser (eframe web +
//! Web MIDI, sub-windows inside the page); `platform` / `midiio` hide the
//! differences.

mod app;
mod clip;
mod cursors;
mod icons;
mod jobwin;
mod listwin;
mod lyricwin;
mod markerwin;
mod midiio;
mod mixwin;
mod peres;
mod pianoroll;
mod pluginwin;
pub mod platform;
mod player;
mod xged;
mod xfwin;
pub mod version;

#[cfg(not(target_arch = "wasm32"))]
pub use app::native_main;
#[cfg(target_arch = "wasm32")]
pub use app::web_start;
