//! Realtime recording (XGworks.hlp "リアルタイムレコーディング"): with REC on,
//! Play records the MIDI input into the selected track (merged with what is
//! there), on the track's channel; Stop writes it in. MIDI Thru sends the
//! input to the track's port / channel while playing or not. Metronome
//! (Ctrl+M in the original) clicks on channel 10 — accent on the bar —
//! during playback and / or recording, with 0–2 bars count-in when recording.

use super::App;
use crate::platform::Instant;
use crate::player::{open_device, send_dev};
use crate::midiio;
use eframe::egui;
use std::sync::{Arc, Mutex};
use xgworks_core::listev::TimeMap;
use xgworks_core::{EditNote, Event};

/// Messages from the MIDI input with their arrival time.
pub type InBuf = Arc<Mutex<Vec<(Instant, Vec<u8>)>>>;
/// Where MIDI Thru goes: (device, channel), None = off.
pub type ThruTarget = Arc<Mutex<Option<(usize, u8)>>>;

#[derive(Clone, Copy)]
pub struct Metronome {
    pub on_play: bool,
    pub on_rec: bool,
    pub count_in: u32,
    pub accent_note: u8,
    pub beat_note: u8,
    pub vel: u8,
}

impl Default for Metronome {
    fn default() -> Self {
        // GM Hi / Low Wood Block on channel 10
        Metronome { on_play: false, on_rec: true, count_in: 1, accent_note: 76, beat_note: 77, vel: 100 }
    }
}

/// Click messages `(ms, track = usize::MAX, bytes)` for every beat from
/// `from` to `to` (ticks), `offset` ms added; `count_in` bars of clicks
/// before `from` are placed in 0..offset.
pub fn clicks(tm: &TimeMap, from: u32, to: u32, offset: f64, count_in: u32, m: &Metronome) -> Vec<(f64, usize, Vec<u8>)> {
    let mut out = Vec::new();
    let base = tm.to_ms(from);
    let mut push = |ms: f64, accent: bool| {
        let n = if accent { m.accent_note } else { m.beat_note };
        let v = if accent { m.vel } else { (m.vel as u32 * 3 / 4) as u8 };
        out.push((ms, usize::MAX, vec![0x99, n, v.max(1)]));
        out.push((ms + 60.0, usize::MAX, vec![0x99, n, 0]));
    };
    if count_in > 0 && offset > 0.0 {
        let beats = tm.bar_len(from) / tm.beat_len(from).max(1);
        let total = beats * count_in;
        for k in 0..total {
            push(offset * k as f64 / total as f64, k % beats == 0);
        }
    }
    // first beat at / after `from`
    let (m0, b0, c0) = tm.to_mbc(from);
    let mut t = if c0 == 0 { from } else { tm.from_mbc(m0, b0 + 1, 0) };
    while t < to {
        let (_, b, _) = tm.to_mbc(t);
        push(offset + tm.to_ms(t) - base, b == 1);
        t += tm.beat_len(t).max(1);
    }
    out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// Turn timed input messages into notes and channel events at ticks.
/// `to_tick(ms since the take started)`; messages before 0 ms (count-in) are dropped.
pub fn take_to_events(msgs: &[(f64, Vec<u8>)], ch: u8, to_tick: impl Fn(f64) -> u32) -> (Vec<EditNote>, Vec<Event>) {
    let mut notes = Vec::new();
    let mut others = Vec::new();
    let mut open: Vec<(u8, u32, u8)> = Vec::new(); // (pitch, tick, vel)
    let mut last_tick = 0;
    for (ms, b) in msgs {
        if *ms < 0.0 || b.is_empty() {
            continue;
        }
        let t = to_tick(*ms);
        last_tick = last_tick.max(t);
        match (b[0] & 0xF0, b.len()) {
            (0x90, 3) if b[2] > 0 => open.push((b[1], t, b[2])),
            (0x80, 3) | (0x90, 3) => {
                if let Some(i) = open.iter().position(|o| o.0 == b[1]) {
                    let (p, st, v) = open.remove(i);
                    notes.push(EditNote { tick: st, dur: (t - st).max(1), pitch: p, vel: v });
                }
            }
            (0xA0..=0xE0, _) if b[0] < 0xF0 => {
                let mut d = b.clone();
                d[0] = (d[0] & 0xF0) | (ch & 0x0F);
                others.push(Event::Chan { t, data: d });
            }
            _ => {}
        }
    }
    for (p, st, v) in open {
        notes.push(EditNote { tick: st, dur: (last_tick.max(st + 1) - st).max(1), pitch: p, vel: v });
    }
    (notes, others)
}

impl App {
    /// Open / switch the MIDI input (None = off).
    pub(super) fn set_input(&mut self, dev: Option<usize>) {
        self.midi_in = None;
        self.in_dev = dev;
        let Some(d) = dev else { return };
        let (buf, thru, midi) = (self.in_buf.clone(), self.thru_target.clone(), self.midi.clone());
        self.midi_in = midiio::connect_input(d, move |msg| {
            if let Ok(mut b) = buf.lock() {
                if b.len() < 200_000 {
                    b.push((Instant::now(), msg.to_vec()));
                }
            }
            // MIDI Thru: channel messages to the record track's port / channel
            if let Some((dev, ch)) = *thru.lock().unwrap() {
                if !msg.is_empty() && msg[0] >= 0x80 && msg[0] < 0xF0 {
                    let mut m = msg.to_vec();
                    m[0] = (m[0] & 0xF0) | ch;
                    if open_device(&midi, dev) {
                        send_dev(&midi, dev, &m);
                    }
                }
            }
        });
        if self.midi_in.is_none() {
            self.status = "Could not open the MIDI input.".into();
        }
    }

    /// Keep MIDI Thru pointed at the selected track; drop input while idle.
    pub(super) fn record_tick(&mut self) {
        let target = (self.thru && self.sel_track < self.edit.len()).then(|| {
            let t = &self.edit[self.sel_track];
            (self.port_device(t.port()), t.channel & 0x0F)
        });
        *self.thru_target.lock().unwrap() = target;
        if !self.recording {
            self.in_buf.lock().unwrap().clear();
        }
    }

    /// Stop of a take: write the recorded input into the record track.
    pub(super) fn finish_take(&mut self) {
        if !self.recording {
            return;
        }
        self.recording = false;
        let (Some(st), Some(tm)) = (self.take_start, self.play_tm.clone()) else { return };
        let ti = self.rec_track;
        let msgs: Vec<(f64, Vec<u8>)> = std::mem::take(&mut *self.in_buf.lock().unwrap())
            .into_iter()
            .map(|(at, b)| ((at.duration_since(st).as_secs_f64() * 1000.0) - self.count_in_ms, b))
            .collect();
        if ti >= self.edit.len() {
            return;
        }
        let ch = self.edit[ti].channel & 0x0F;
        let from_ms = tm.to_ms(self.play_from);
        let (notes, others) = take_to_events(&msgs, ch, |ms| tm.from_ms(from_ms + ms));
        if notes.is_empty() && others.is_empty() {
            self.status = "Nothing was recorded.".into();
            return;
        }
        self.tv_snapshot();
        let (n, o) = (notes.len(), others.len());
        let tr = &mut self.edit[ti];
        tr.notes.extend(notes);
        tr.notes.sort_by_key(|x| (x.tick, x.pitch));
        tr.others.extend(others);
        self.extend_blocks(ti);
        self.song_edited();
        self.status = format!("Recorded {n} notes and {o} other events into track {}.", self.track_number(ti));
    }

    pub(super) fn track_number(&self, ti: usize) -> usize {
        self.visible_tracks().iter().position(|&v| v == ti).map_or(ti + 1, |k| k + 1)
    }

    /// Setup menu: MIDI input, Thru, metronome.
    pub(super) fn setup_menu(&mut self, ui: &mut egui::Ui) {
        ui.menu_button("Setup", |ui| {
            ui.label(egui::RichText::new("MIDI input (recording)").strong());
            let names = midiio::input_names();
            let cur = self.in_dev;
            if ui.radio(cur.is_none(), "Off").clicked() {
                self.set_input(None);
            }
            for (i, n) in names.iter().enumerate() {
                if ui.radio(cur == Some(i), n).clicked() {
                    self.set_input(Some(i));
                }
            }
            if names.is_empty() {
                ui.label(egui::RichText::new(if cfg!(target_arch = "wasm32") { "(not in the browser version)" } else { "(no MIDI input found)" }).small());
            }
            ui.checkbox(&mut self.thru, "MIDI Thru to the selected track");
            ui.separator();
            ui.label(egui::RichText::new("Metronome").strong());
            ui.checkbox(&mut self.metro.on_play, "Click while playing");
            ui.checkbox(&mut self.metro.on_rec, "Click while recording");
            ui.horizontal(|ui| {
                ui.label("Count-in");
                ui.add(egui::Slider::new(&mut self.metro.count_in, 0..=2).suffix(" bars"));
            });
            ui.horizontal(|ui| {
                ui.label("Notes (ch 10)");
                ui.add(egui::DragValue::new(&mut self.metro.accent_note).range(0..=127)).on_hover_text("Bar (accent)");
                ui.add(egui::DragValue::new(&mut self.metro.beat_note).range(0..=127)).on_hover_text("Beat");
                ui.label("Vel");
                ui.add(egui::DragValue::new(&mut self.metro.vel).range(1..=127));
            });
            ui.separator();
            if ui.button("Preferences…").clicked() {
                self.show_prefs = true;
                ui.close_menu();
            }
        });
    }

    /// Transport: REC toggle (numpad + like the original) and the metronome.
    pub(super) fn record_buttons(&mut self, ui: &mut egui::Ui) {
        let rec = egui::Button::new(egui::RichText::new("REC").color(if self.rec_armed { egui::Color32::WHITE } else { egui::Color32::from_rgb(200, 30, 30) }))
            .fill(if self.rec_armed { egui::Color32::from_rgb(200, 30, 30) } else { ui.visuals().widgets.inactive.weak_bg_fill });
        let tip = if self.in_dev.is_some() { "Record the MIDI input into the selected track on Play (numpad +)" } else { "Choose the MIDI input in Setup first (numpad +)" };
        if ui.add_enabled(!self.edit.is_empty() && !self.recording, rec).on_hover_text(tip).clicked() {
            self.rec_armed = !self.rec_armed;
            if self.rec_armed && self.in_dev.is_none() {
                self.status = "Choose a MIDI input in Setup ▸ MIDI input.".into();
            }
        }
        let mut on = self.metro.on_play;
        if ui.toggle_value(&mut on, "♩").on_hover_text("Metronome while playing (Setup ▸ Metronome)").changed() {
            self.metro.on_play = on;
        }
    }

    /// The play plan additions: clicks and the count-in shift.
    pub(super) fn add_clicks(&mut self, msgs: Vec<(f64, usize, Vec<u8>)>, tm: &TimeMap, from: u32) -> Vec<(f64, usize, Vec<u8>)> {
        let rec = self.rec_armed && !self.monitoring;
        let click = !self.monitoring && if rec { self.metro.on_rec } else { self.metro.on_play };
        let count_in = if rec && click { self.metro.count_in } else { 0 };
        self.count_in_ms = if count_in > 0 {
            let bar = tm.bar_len(from) * count_in;
            tm.to_ms(from + bar) - tm.to_ms(from)
        } else {
            0.0
        };
        let mut out: Vec<(f64, usize, Vec<u8>)> = msgs.into_iter().map(|(t, k, b)| (t + self.count_in_ms, k, b)).collect();
        if click {
            let end = if rec { self.total_ticks + tm.bar_len(self.total_ticks) * 64 } else { self.total_ticks };
            out.extend(clicks(tm, from, end, self.count_in_ms, count_in, &self.metro));
            out.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        }
        if rec {
            self.recording = true;
            self.rec_track = self.sel_track;
            self.in_buf.lock().unwrap().clear();
            self.take_start = Some(Instant::now());
            self.status = format!("Recording into track {}…", self.track_number(self.sel_track));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn take_becomes_notes_and_events() {
        let msgs = vec![
            (-10.0, vec![0x90, 1, 1]),
            (0.0, vec![0x90, 60, 100]),
            (100.0, vec![0xB3, 64, 127]),
            (500.0, vec![0x80, 60, 0]),
            (600.0, vec![0x90, 62, 90]),
        ];
        let (n, o) = take_to_events(&msgs, 2, |ms| (ms * 0.96) as u32);
        assert_eq!(n.len(), 2);
        assert_eq!((n[0].tick, n[0].dur, n[0].pitch), (0, 480, 60));
        assert_eq!(o.len(), 1);
        assert!(matches!(&o[0], Event::Chan { data, .. } if data[0] == 0xB2));
    }

    #[test]
    fn clicks_accent_bars() {
        let tm = TimeMap::new(&[], 480);
        let c = clicks(&tm, 0, 1920 * 2, 0.0, 0, &Metronome::default());
        let ons: Vec<&Vec<u8>> = c.iter().filter(|x| x.2[2] > 0).map(|x| &x.2).collect();
        assert_eq!(ons.len(), 8);
        assert_eq!(ons[0][1], 76);
        assert_eq!(ons[1][1], 77);
        assert_eq!(ons[4][1], 76);
    }
}

/// Needs a loopback MIDI port (loopMIDI): `cargo test -- --ignored loopback`.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod loopback {
    use crate::midiio;
    use std::sync::{Arc, Mutex};

    #[test]
    #[ignore]
    fn loopback_input_receives_output() {
        let ins = midiio::input_names();
        let outs = midiio::output_names();
        let Some(i) = ins.iter().position(|n| n.to_lowercase().contains("loopmidi")) else { panic!("no loopMIDI input in {ins:?}") };
        let o = outs.iter().position(|n| n == &ins[i]).or_else(|| outs.iter().position(|n| n.to_lowercase().contains("loopmidi"))).unwrap();
        let got = Arc::new(Mutex::new(Vec::new()));
        let g = got.clone();
        let _inp = midiio::connect_input(i, move |m| g.lock().unwrap().push(m.to_vec())).expect("input");
        let mut out = midiio::connect(o).expect("output");
        out.send(&[0x90, 60, 100]);
        out.send(&[0x80, 60, 0]);
        std::thread::sleep(std::time::Duration::from_millis(300));
        let v = got.lock().unwrap().clone();
        assert!(v.contains(&vec![0x90, 60, 100]), "{v:?}");
    }
}
