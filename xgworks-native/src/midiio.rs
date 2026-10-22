//! MIDI output: `midir` natively, the Web MIDI API in the browser (the page
//! requests access, with SysEx, before the app starts and keeps it in
//! `globalThis.xgwMidi`). Devices are addressed by their index in
//! `output_names()`.

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use midir::{MidiOutput, MidiOutputConnection};

    pub fn output_names() -> Vec<String> {
        let Ok(out) = MidiOutput::new("xgworks-list") else { return Vec::new() };
        out.ports().iter().map(|p| out.port_name(p).unwrap_or_else(|_| "?".into())).collect()
    }

    pub struct Conn(MidiOutputConnection);

    pub fn connect(dev: usize) -> Option<Conn> {
        let out = MidiOutput::new("xgworks").ok()?;
        let ports = out.ports();
        let port = ports.get(dev)?;
        out.connect(port, "xgworks").ok().map(Conn)
    }

    impl Conn {
        pub fn send(&mut self, bytes: &[u8]) {
            let _ = self.0.send(bytes);
        }

        /// Send at `_at_ms` (the player's clock); natively the player thread
        /// already waits for the right moment, so this sends now.
        pub fn send_at(&mut self, bytes: &[u8], _at_ms: f64) {
            self.send(bytes);
        }

        /// Drop messages queued for later (nothing is queued natively).
        #[allow(dead_code)]
        pub fn clear(&mut self) {}
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use wasm_bindgen::prelude::*;

    #[wasm_bindgen(inline_js = r#"
        function outs() { const a = globalThis.xgwMidi; return a ? [...a.outputs.values()] : []; }
        export function midi_names() { return outs().map(o => o.name); }
        export function midi_send(i, bytes, at) {
            const o = outs()[i];
            if (!o) return;
            try { if (at > 0) o.send(bytes.slice(), at); else o.send(bytes.slice()); } catch (e) { console.warn('MIDI send', e); }
        }
        export function midi_clear(i) { const o = outs()[i]; if (o && o.clear) try { o.clear(); } catch (e) {} }
        export function perf_now() { return performance.now(); }
    "#)]
    extern "C" {
        fn midi_names() -> Vec<String>;
        fn midi_send(i: usize, bytes: &[u8], at: f64);
        fn midi_clear(i: usize);
        fn perf_now() -> f64;
    }

    pub fn output_names() -> Vec<String> {
        midi_names()
    }

    pub struct Conn(usize);

    pub fn connect(dev: usize) -> Option<Conn> {
        (dev < midi_names().len()).then_some(Conn(dev))
    }

    impl Conn {
        pub fn send(&mut self, bytes: &[u8]) {
            midi_send(self.0, bytes, 0.0);
        }

        /// Send at `at_ms` on the page clock (`performance.now()`); the browser
        /// keeps the timing, so the player can schedule a little ahead.
        pub fn send_at(&mut self, bytes: &[u8], at_ms: f64) {
            midi_send(self.0, bytes, if at_ms > perf_now() { at_ms } else { 0.0 });
        }

        /// Drop messages scheduled for later (where the browser supports it).
        pub fn clear(&mut self) {
            midi_clear(self.0);
        }
    }

    pub fn now_ms() -> f64 {
        perf_now()
    }
}

pub use imp::*;
