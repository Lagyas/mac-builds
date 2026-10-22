//! MIDI input: `midir` natively (every input port is opened), the Web MIDI
//! API in the browser (the page asks for access and keeps it in
//! `globalThis.hs2Midi`; messages are queued in JS and polled each frame).

use std::sync::{Arc, Mutex};

pub type Queue = Arc<Mutex<Vec<Vec<u8>>>>;

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::Queue;
    use midir::{Ignore, MidiInput, MidiInputConnection};

    pub struct Inputs {
        _conns: Vec<MidiInputConnection<()>>,
        pub names: Vec<String>,
    }

    /// Open all MIDI inputs; incoming messages are applied by `on_msg`.
    pub fn open(on_msg: impl Fn(&[u8]) + Send + Sync + Clone + 'static, _q: &Queue) -> Inputs {
        let mut conns = Vec::new();
        let mut names = Vec::new();
        let Ok(probe) = MidiInput::new("hs2-list") else { return Inputs { _conns: conns, names } };
        let ports = probe.ports();
        for p in ports.iter() {
            let name = probe.port_name(p).unwrap_or_else(|_| "?".into());
            let Ok(mut inp) = MidiInput::new("hs2") else { continue };
            inp.ignore(Ignore::SysexAndTime);
            let f = on_msg.clone();
            if let Ok(c) = inp.connect(p, "hs2-in", move |_, msg, _| f(msg), ()) {
                conns.push(c);
                names.push(name);
            }
        }
        Inputs { _conns: conns, names }
    }

    pub fn poll(_q: &Queue) -> Vec<Vec<u8>> {
        Vec::new()
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::Queue;
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(inline_js = r#"
        const q = [];
        let hooked = false;
        function hook() {
            const a = globalThis.hs2Midi;
            if (!a || hooked) return;
            hooked = true;
            const attach = () => { for (const i of a.inputs.values()) i.onmidimessage = e => q.push(Array.from(e.data)); };
            attach();
            a.onstatechange = attach;
        }
        export function midi_in_names() { hook(); const a = globalThis.hs2Midi; return a ? [...a.inputs.values()].map(i => i.name) : []; }
        export function midi_take() { hook(); const out = q.splice(0, q.length); return out.map(m => new Uint8Array(m)); }
    "#)]
    extern "C" {
        fn midi_in_names() -> Vec<String>;
        fn midi_take() -> Vec<js_sys::Uint8Array>;
    }

    pub struct Inputs {
        pub names: Vec<String>,
    }

    pub fn open(_on_msg: impl Fn(&[u8]) + Send + Sync + Clone + 'static, _q: &Queue) -> Inputs {
        Inputs { names: midi_in_names() }
    }

    pub fn poll(_q: &Queue) -> Vec<Vec<u8>> {
        midi_take().into_iter().map(|a| a.to_vec()).collect()
    }
}

pub use imp::*;
