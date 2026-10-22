//! Song playback over the shared MIDI connections.
//!
//! Natively a thread waits for each message; in the browser (no threads) a
//! timer sends everything due within the next `LOOKAHEAD_MS` with a Web MIDI
//! timestamp, so the browser keeps the timing. Both use the same `Player`
//! (live mute / solo per track and per mixer channel).

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use crate::midiio;

/// Open MIDI output connections by device index, plus the default device
/// (the toolbar's choice) that editor messages (mixer, XG editor) go to.
/// Devices stay open once used; Windows lets a device be opened only once,
/// so everything shares these connections.
#[derive(Default)]
pub struct Outs {
    pub conns: HashMap<usize, midiio::Conn>,
    pub default: usize,
}

pub type SharedOut = Arc<Mutex<Outs>>;

pub fn open_device(midi: &SharedOut, dev: usize) -> bool {
    let mut g = midi.lock().unwrap();
    if g.conns.contains_key(&dev) {
        return true;
    }
    match midiio::connect(dev) {
        Some(c) => {
            g.conns.insert(dev, c);
            true
        }
        None => false,
    }
}

/// Send to the default device.
pub fn send_shared(midi: &SharedOut, bytes: &[u8]) {
    let mut g = midi.lock().unwrap();
    let d = g.default;
    if let Some(c) = g.conns.get_mut(&d) {
        c.send(bytes);
    }
}

pub fn send_dev(midi: &SharedOut, dev: usize, bytes: &[u8]) {
    if let Some(c) = midi.lock().unwrap().conns.get_mut(&dev) {
        c.send(bytes);
    }
}

/// All Notes Off + All Sound Off on every open device (at `at_ms`, 0 = now).
fn all_off_at(midi: &SharedOut, at_ms: f64) {
    let mut g = midi.lock().unwrap();
    for conn in g.conns.values_mut() {
        for ch in 0..16u8 {
            conn.send_at(&[0xB0 | ch, 120, 0], at_ms);
            conn.send_at(&[0xB0 | ch, 123, 0], at_ms);
        }
    }
}

/// Plays a tagged plan. Muted tracks (Track View) and muted channels (Mixer)
/// drop their note-ons; when a track or channel gets muted mid-note, its
/// sounding notes are released (only that track's notes, so other tracks on
/// the same channel keep playing). Other messages (CC, program, SysEx) always
/// pass so the part state stays correct.
struct Player {
    msgs: Vec<(f64, usize, Vec<u8>)>,
    next: usize,
    devs: Vec<usize>,
    fallback: usize,
    aud: Arc<Vec<AtomicBool>>,
    chan: Arc<Vec<AtomicBool>>,
    /// Per track: on the mixer's port, so the mixer's channel mute applies.
    mixed: Arc<Vec<AtomicBool>>,
    held: Vec<Vec<(u8, u8)>>, // per track: (channel, pitch)
}

impl Player {
    fn dev(&self, k: usize) -> usize {
        self.devs.get(k).copied().unwrap_or(self.fallback)
    }

    fn audible(&self, k: usize, ch: u8) -> bool {
        self.aud.get(k).map_or(true, |a| a.load(Ordering::Relaxed))
            && (!self.mixed.get(k).map_or(true, |a| a.load(Ordering::Relaxed))
                || self.chan.get(ch as usize).map_or(true, |a| a.load(Ordering::Relaxed)))
    }

    /// Release the sounding notes of tracks / channels muted meanwhile.
    fn release_muted(&mut self, out: &mut Outs, at: f64) {
        for k in 0..self.held.len() {
            let mut h = std::mem::take(&mut self.held[k]);
            h.retain(|&(ch, p)| {
                let keep = self.audible(k, ch);
                if !keep {
                    if let Some(c) = out.conns.get_mut(&self.dev(k)) {
                        c.send_at(&[0x80 | ch, p, 0], at);
                    }
                }
                keep
            });
            self.held[k] = h;
        }
    }

    /// Song time (ms) of the next message.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    fn due(&self) -> Option<f64> {
        self.msgs.get(self.next).map(|m| m.0)
    }

    /// Send every message due by song time `upto`, stamped `base + time`.
    fn pump(&mut self, out: &mut Outs, upto: f64, base: f64) {
        while let Some((ms, k, bytes)) = self.msgs.get(self.next) {
            if *ms > upto {
                break;
            }
            let (ms, k) = (*ms, *k);
            self.next += 1;
            let kind = bytes[0] & 0xF0;
            let ch = bytes[0] & 0x0F;
            let is_on = kind == 0x90 && bytes.len() == 3 && bytes[2] > 0;
            let is_off = (kind == 0x80 || kind == 0x90) && bytes.len() == 3 && !is_on;
            let pitch = bytes.get(1).copied().unwrap_or(0);
            if is_on {
                if !self.audible(k, ch) {
                    continue;
                }
                if let Some(h) = self.held.get_mut(k) {
                    h.push((ch, pitch));
                }
            } else if is_off {
                if let Some(h) = self.held.get_mut(k) {
                    match h.iter().position(|&(c, p)| c == ch && p == pitch) {
                        Some(i) => {
                            h.swap_remove(i);
                        }
                        None => continue, // its note-on was muted (or already released)
                    }
                }
            }
            let dev = self.dev(k);
            if let Some(c) = out.conns.get_mut(&dev) {
                c.send_at(&self.msgs[self.next - 1].2, base + ms.max(0.0));
            }
        }
    }
}

pub fn start(
    midi: SharedOut,
    msgs: Vec<(f64, usize, Vec<u8>)>,
    devs: Vec<usize>,
    aud: Arc<Vec<AtomicBool>>,
    chan: Arc<Vec<AtomicBool>>,
    mixed: Arc<Vec<AtomicBool>>,
) -> PlayHandle {
    let fallback = midi.lock().unwrap().default;
    let held = vec![Vec::new(); aud.len()];
    let pl = Player { msgs, next: 0, devs, fallback, aud, chan, mixed, held };
    PlayHandle::spawn(midi, pl)
}

#[cfg(not(target_arch = "wasm32"))]
pub struct PlayHandle {
    stop: Arc<AtomicBool>,
    thread: Option<std::thread::JoinHandle<()>>,
}

#[cfg(not(target_arch = "wasm32"))]
impl PlayHandle {
    fn spawn(midi: SharedOut, mut pl: Player) -> Self {
        use crate::platform::{Duration, Instant};
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let thread = std::thread::spawn(move || {
            let start = Instant::now();
            loop {
                if flag.load(Ordering::Relaxed) {
                    all_off_at(&midi, 0.0);
                    return;
                }
                let now = start.elapsed().as_secs_f64() * 1000.0;
                {
                    let mut g = midi.lock().unwrap();
                    pl.release_muted(&mut g, 0.0);
                    pl.pump(&mut g, now, 0.0);
                }
                let Some(due) = pl.due() else { return };
                std::thread::sleep(Duration::from_secs_f64(((due - now) / 1000.0).clamp(0.0, 0.008)));
            }
        });
        PlayHandle { stop, thread: Some(thread) }
    }

    pub fn stop(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

/// How far ahead the web player hands messages to the browser.
#[cfg(target_arch = "wasm32")]
const LOOKAHEAD_MS: f64 = 250.0;

#[cfg(target_arch = "wasm32")]
pub struct PlayHandle {
    midi: SharedOut,
    interval: i32,
    _tick: wasm_bindgen::closure::Closure<dyn FnMut()>,
}

#[cfg(target_arch = "wasm32")]
impl PlayHandle {
    fn spawn(midi: SharedOut, mut pl: Player) -> Self {
        use wasm_bindgen::JsCast;
        let base = midiio::now_ms();
        let m = midi.clone();
        let mut tick = move || {
            let upto = midiio::now_ms() - base + LOOKAHEAD_MS;
            let mut g = m.lock().unwrap();
            // a release lands after anything already handed to the browser
            pl.release_muted(&mut g, base + upto);
            pl.pump(&mut g, upto, base);
        };
        tick();
        let tick = wasm_bindgen::closure::Closure::<dyn FnMut()>::new(tick);
        let interval = web_sys::window()
            .and_then(|w| w.set_interval_with_callback_and_timeout_and_arguments_0(tick.as_ref().unchecked_ref(), 25).ok())
            .unwrap_or(0);
        PlayHandle { midi, interval, _tick: tick }
    }

    pub fn stop(self) {
        if let Some(w) = web_sys::window() {
            w.clear_interval_with_handle(self.interval);
        }
        for c in self.midi.lock().unwrap().conns.values_mut() {
            c.clear();
        }
        // silence now, and again once the messages already queued have passed
        all_off_at(&self.midi, 0.0);
        all_off_at(&self.midi, midiio::now_ms() + LOOKAHEAD_MS + 30.0);
    }
}
