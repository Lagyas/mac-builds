//! Arpeggiator with the 16-step sequencer (manual pp. 64-67).

use crate::dsp::Rng;
use crate::patch::{enum_idx, pp, Patch};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Up,
    Down,
    UpDown,
    AsPlayed,
    Random,
    StepSeq,
    CtrlSeq,
    RandSeq,
    ElemSeq,
    Phrase,
}

impl Mode {
    /// Menu order; normalized value = index of 10 (factory phrase patches use 0.6).
    pub const ALL: [Mode; 10] = [
        Mode::Up,
        Mode::Down,
        Mode::UpDown,
        Mode::AsPlayed,
        Mode::Random,
        Mode::StepSeq,
        Mode::Phrase,
        Mode::CtrlSeq,
        Mode::RandSeq,
        Mode::ElemSeq,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Mode::Up => "Up",
            Mode::Down => "Down",
            Mode::UpDown => "Up+Down",
            Mode::AsPlayed => "As Played",
            Mode::Random => "Random",
            Mode::StepSeq => "Step Seq",
            Mode::CtrlSeq => "Ctrl Seq",
            Mode::RandSeq => "Rand Seq",
            Mode::ElemSeq => "Elem Seq",
            Mode::Phrase => "Phrase",
        }
    }
}

/// Rate menu (fractions of a whole note).
pub const RATES: [(&str, f32); 12] = [
    ("1/1", 1.0),
    ("1/2", 0.5),
    ("1/4.", 0.375),
    ("1/4", 0.25),
    ("1/4T", 1.0 / 6.0),
    ("1/8.", 0.1875),
    ("1/8", 0.125),
    ("1/8T", 1.0 / 12.0),
    ("1/16.", 0.09375),
    ("1/16", 0.0625),
    ("1/16T", 1.0 / 24.0),
    ("1/32", 0.03125),
];

pub const SYNC: [&str; 3] = ["1st Note", "Retrig", "Beat"];

#[derive(Clone, Copy, Debug)]
pub struct Settings {
    pub on: bool,
    pub latch: bool,
    pub mode: Mode,
    pub octaves: usize,
    pub steps: usize,
    pub sync: usize,
    pub rate: f32,
    pub gate: f32,
    pub swing: f32,
    /// per step: (note offset semitones, velocity 0..1, gate 0..2, seq1, seq2 -1..1)
    pub seq: [(i32, f32, f32, f32, f32); 16],
    /// Phrase reference (phrase bank number, phrase index).
    pub phrase: (u32, u32),
}

impl Settings {
    pub fn from_patch(p: &Patch) -> Settings {
        let v = &p.p;
        let mut seq = [(0, 1.0, 1.0, 0.0, 0.0); 16];
        for (s, st) in seq.iter_mut().enumerate() {
            let o = pp::STEP + 5 * s;
            *st = (((v[o] - 0.5) * 48.0).round() as i32, v[o + 1], v[o + 2] * 2.0, (v[o + 3] - 0.5) * 2.0, (v[o + 4] - 0.5) * 2.0);
        }
        Settings {
            on: v[pp::ARP_ON] >= 0.5,
            latch: v[pp::ARP_LATCH] >= 0.5,
            mode: Mode::ALL[enum_idx(v[pp::ARP_MODE], Mode::ALL.len())],
            octaves: enum_idx(v[pp::ARP_OCT], 4) + 1,
            steps: enum_idx(v[pp::ARP_STEPS], 16) + 1,
            sync: enum_idx(v[pp::ARP_SYNC], 3),
            rate: RATES[enum_idx(v[pp::ARP_RATE], RATES.len())].1,
            gate: 0.05 + v[pp::ARP_GATE] * 1.95,
            swing: v[pp::ARP_SWING],
            seq,
            phrase: ((v[pp::PHRASE_BANK] * 128.0) as u32, (v[pp::PHRASE_PROG] * 128.0) as u32),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
    On(u8, u8),
    Off(u8),
    Cc(u8, u8),
}

#[derive(Clone, Debug, Default)]
pub struct Arp {
    held: Vec<(u8, u8)>, // as played
    latched: Vec<(u8, u8)>,
    step: usize,
    pos: usize,
    dir_up: bool,
    /// time to next step in beats
    next: f64,
    sounding: Vec<(u8, f64)>, // note, beats left
    pub seq1: f32,
    pub seq2: f32,
    rng: Option<Rng>,
    pedal: bool,
    phrase_pos: f64,
    first_vel: u8,
}

impl Arp {
    pub fn note_on(&mut self, s: &Settings, note: u8, vel: u8) {
        if self.held.is_empty() && (s.sync == 0 || s.sync == 1) {
            self.restart();
        } else if s.sync == 1 {
            self.restart();
        }
        if (s.latch || self.pedal) && self.held.is_empty() {
            self.latched.clear();
        }
        self.held.retain(|n| n.0 != note);
        if self.held.is_empty() {
            self.first_vel = vel;
        }
        self.held.push((note, vel));
        if s.latch || self.pedal {
            self.latched.retain(|n| n.0 != note);
            self.latched.push((note, vel));
        }
    }
    pub fn note_off(&mut self, note: u8) {
        self.held.retain(|n| n.0 != note);
    }
    pub fn sustain(&mut self, on: bool) {
        self.pedal = on;
        if on {
            self.latched = self.held.clone();
        } else {
            self.latched.clear();
        }
    }
    pub fn restart(&mut self) {
        self.phrase_pos = 0.0;
        self.step = 0;
        self.pos = 0;
        self.dir_up = true;
        self.next = 0.0;
    }
    fn pool(&self, s: &Settings) -> Vec<(u8, u8)> {
        let base = if (s.latch || self.pedal) && self.held.is_empty() { &self.latched } else { &self.held };
        let mut v = base.clone();
        if !matches!(s.mode, Mode::AsPlayed) {
            v.sort_by_key(|n| n.0);
        }
        let mut out = Vec::new();
        for o in 0..s.octaves {
            for &(n, vel) in &v {
                let nn = n as i32 + 12 * o as i32;
                if nn <= 127 {
                    out.push((nn as u8, vel));
                }
            }
        }
        out
    }
    pub fn all_off(&mut self, out: &mut Vec<Event>) {
        for (n, _) in self.sounding.drain(..) {
            out.push(Event::Off(n));
        }
        self.held.clear();
        self.latched.clear();
    }
    /// Advance by `beats` (quarter notes); emit note events.
    pub fn advance(&mut self, s: &Settings, beats: f64, out: &mut Vec<Event>) {
        let mut rng = self.rng.unwrap_or(Rng::new(0xa5a5));
        // releases
        let mut i = 0;
        while i < self.sounding.len() {
            self.sounding[i].1 -= beats;
            if self.sounding[i].1 <= 0.0 {
                out.push(Event::Off(self.sounding[i].0));
                self.sounding.swap_remove(i);
            } else {
                i += 1;
            }
        }
        let step_beats = (s.rate * 4.0) as f64;
        self.next -= beats;
        let mut guard = 0;
        while self.next <= 0.0 && guard < 8 {
            guard += 1;
            let pool = self.pool(s);
            let swing = if self.step % 2 == 0 { 1.0 + s.swing as f64 * 0.33 } else { 1.0 - s.swing as f64 * 0.33 };
            self.next += step_beats * swing;
            if pool.is_empty() {
                continue;
            }
            let st = s.seq[self.step % 16];
            self.seq1 = st.3;
            self.seq2 = st.4;
            let pick = match s.mode {
                Mode::Up | Mode::AsPlayed => Some(pool[self.pos % pool.len()]),
                Mode::Down => Some(pool[pool.len() - 1 - self.pos % pool.len()]),
                Mode::UpDown => {
                    let n = pool.len();
                    let cyc = if n > 1 { 2 * n - 2 } else { 1 };
                    let k = self.pos % cyc;
                    Some(pool[if k < n { k } else { cyc - k }])
                }
                Mode::Random => Some(pool[(rng.next_u32() as usize) % pool.len()]),
                Mode::StepSeq | Mode::ElemSeq | Mode::Phrase => {
                    let (n, v) = pool[0];
                    let nn = (n as i32 + st.0).clamp(0, 127) as u8;
                    Some((nn, ((v as f32) * st.1).clamp(1.0, 127.0) as u8))
                }
                Mode::RandSeq => {
                    let k = (rng.next_u32() as usize) % s.steps.max(1);
                    let st2 = s.seq[k];
                    let (n, v) = pool[0];
                    Some(((n as i32 + st2.0).clamp(0, 127) as u8, ((v as f32) * st2.1).clamp(1.0, 127.0) as u8))
                }
                Mode::CtrlSeq => None,
            };
            if let Some((n, v)) = pick {
                if st.1 > 0.01 || !matches!(s.mode, Mode::StepSeq | Mode::RandSeq | Mode::ElemSeq) {
                    out.push(Event::On(n, v.max(1)));
                    let gate = step_beats * (s.gate as f64) * if matches!(s.mode, Mode::StepSeq) { st.2 as f64 } else { 1.0 };
                    self.sounding.push((n, gate.max(0.01)));
                }
            }
            self.pos += 1;
            self.step = (self.step + 1) % s.steps.max(1);
            if self.step == 0 && matches!(s.mode, Mode::StepSeq | Mode::RandSeq | Mode::ElemSeq) {
                self.pos = 0;
            }
        }
        self.rng = Some(rng);
    }
    /// Phrase mode: play `ph` looped, mapped onto the held notes.
    pub fn advance_phrase(&mut self, s: &Settings, ph: &crate::phrase::Phrase, beats: f64, out: &mut Vec<Event>) {
        let mut i = 0;
        while i < self.sounding.len() {
            self.sounding[i].1 -= beats;
            if self.sounding[i].1 <= 0.0 {
                out.push(Event::Off(self.sounding[i].0));
                self.sounding.swap_remove(i);
            } else {
                i += 1;
            }
        }
        let mut held: Vec<u8> = self.pool(s).iter().map(|n| n.0).collect();
        held.sort_unstable();
        held.dedup();
        if held.is_empty() {
            return;
        }
        let speed = 0.0625 / s.rate.max(1e-3) as f64; // 1/16 = original speed
        let from = self.phrase_pos;
        let to = from + beats * speed;
        let len = ph.length.max(0.25);
        for e in &ph.events {
            // event time within [from, to) modulo the loop
            let mut t = e.time;
            while t < from {
                t += len;
            }
            if t >= to {
                continue;
            }
            match e.status & 0xf0 {
                0x90 if e.d2 > 0 => {
                    if let Some(k) = ph.map_note(e, &held) {
                        let vel = ((e.d2 as f32) * (self.first_vel.max(1) as f32 / 100.0)).clamp(1.0, 127.0) as u8;
                        if let Some(pos) = self.sounding.iter().position(|x| x.0 == k) {
                            out.push(Event::Off(k));
                            self.sounding.swap_remove(pos);
                        }
                        out.push(Event::On(k, vel));
                        self.sounding.push((k, (e.dur.max(0.02) / speed * s.gate.max(0.05) as f64).max(0.01)));
                    }
                }
                0xb0 => out.push(Event::Cc(e.d1, e.d2)),
                _ => {}
            }
        }
        self.phrase_pos = to % len;
    }

    pub fn active(&self) -> bool {
        !self.held.is_empty() || !self.latched.is_empty() || !self.sounding.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn up_pattern() {
        let mut p = Patch::new("a");
        p.p[pp::ARP_ON] = 1.0;
        p.p[pp::ARP_MODE] = 0.0;
        p.p[pp::ARP_RATE] = crate::patch::enum_val(9, RATES.len()); // 1/16
        let s = Settings::from_patch(&p);
        let mut a = Arp::default();
        a.note_on(&s, 60, 100);
        a.note_on(&s, 64, 100);
        a.note_on(&s, 67, 100);
        let mut ev = Vec::new();
        for _ in 0..12 {
            a.advance(&s, 0.25, &mut ev);
        }
        let ons: Vec<u8> = ev.iter().filter_map(|e| if let Event::On(n, _) = e { Some(*n) } else { None }).collect();
        assert_eq!(&ons[..6], &[60, 64, 67, 60, 64, 67]);
    }
}
