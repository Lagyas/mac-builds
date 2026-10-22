//! Synthesizer in the style of Steinberg / Wizoo Hypersonic 2.
//!
//! * `bank`, `patch` — reads Hypersonic 2 content banks and patches (plain data);
//!   sample / wavetable audio of the original is protected and never decoded.
//! * `synth` — 16-part engine with Sample (SoundFont / built-in), Analog,
//!   Wavetable, FM and Drawbar elements, mod matrix, Hyperknobs, FX.
//! * `library`, `factory` — patch library and built-in patches.

pub mod arp;
pub mod bank;
pub mod combi;
pub mod dsp;
pub mod factory;
pub mod fx;
pub mod library;
pub mod params;
pub mod patch;
pub mod phrase;
pub mod sf2;
pub mod state;
pub mod synth;
pub mod wavetable;

pub use patch::{ElemType, Element, Patch};
pub use synth::Synth;
