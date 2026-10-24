//! Event list of XGworks' List window (List.hlp): one row per event with the
//! columns L1/L2/L3 (measure / beat / clock, or min / sec / msec), Type and
//! Value1..3, formatted like the original (compared with its List window).
//!
//! RE notes (XGworks.exe): note names `"%-2s%2d"` table @0x36b2d0 ("C -2" ..
//! "G  8", C3 = 60), dynamics `char[4][128]` @0x36b6d0 (ppp 0..15 … fff
//! 112..127), formats `"%02d:%03d"` gate, `" [%3d]"`, `"[%5d]"` pitch bend,
//! `"%04d"/"%02d"/"%03d"` location, `"%d:%02d:%03d"` time; CC / NRPN / bank
//! names from `ws.ini` (`[XG Controllers]`, `[XG Nrpn1/2]`, `[XG Rpn1/2]`,
//! `[voice list]`, `[bank lsb]`); voice names from the `[voice list]` files
//! (`Melody.ini`: `[Melody] prog=name`, then `[name] lsb=voice`); SysEx
//! parameter names from per-block tables (`listnames_data`), built as
//! `"XG " + "Prm "/"Prm Req "/"Dmp "/"Dmp Req " + block + name`.

use std::collections::HashMap;

use crate::listnames_data::*;
use crate::xgtbl::{FxSlot, Ini, XgTable};
use crate::xgunits::{fx_display, fx_slot_desc, param_display, Enums};
use crate::{EditNote, EditTrack, Event};

// ---------------------------------------------------------------------------
// rows
// ---------------------------------------------------------------------------
/// Where a list row's event lives in its track.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Src {
    Note(usize),
    Other(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Row {
    pub track: usize,
    pub src: Src,
    pub tick: u32,
}

/// The rows of the given (track, tick range) spans, in time order; at equal
/// times non-note events come first, each kind in stored order.
pub fn collect_rows(tracks: &[EditTrack], spans: &[(usize, u32, u32)]) -> Vec<Row> {
    let mut rows = Vec::new();
    for (k, &(ti, lo, hi)) in spans.iter().enumerate() {
        let Some(t) = tracks.get(ti) else { continue };
        let inside = |tick: u32| tick >= lo && tick < hi;
        for (i, e) in t.others.iter().enumerate() {
            if inside(e.t()) {
                rows.push((e.t(), k, 0u8, i, Row { track: ti, src: Src::Other(i), tick: e.t() }));
            }
        }
        for (i, n) in t.notes.iter().enumerate() {
            if inside(n.tick) {
                rows.push((n.tick, k, 1u8, i, Row { track: ti, src: Src::Note(i), tick: n.tick }));
            }
        }
    }
    rows.sort_by_key(|r| (r.0, r.1, r.2, r.3));
    rows.into_iter().map(|r| r.4).collect()
}

/// The event behind a row (notes as `Event::Note` on the track channel).
pub fn row_event(tracks: &[EditTrack], r: &Row) -> Option<Event> {
    let t = tracks.get(r.track)?;
    match r.src {
        Src::Note(i) => t.notes.get(i).map(|n| Event::Note { t: n.tick, ch: t.channel & 0x0F, note: n.pitch, vel: n.vel, gate: n.dur }),
        Src::Other(i) => t.others.get(i).cloned(),
    }
}

/// Store an edited event back into its row's place.
pub fn set_row_event(tracks: &mut [EditTrack], r: &Row, e: Event) {
    let Some(t) = tracks.get_mut(r.track) else { return };
    match (r.src, e) {
        (Src::Note(i), Event::Note { t: tk, note, vel, gate, .. }) => {
            if let Some(n) = t.notes.get_mut(i) {
                *n = EditNote { tick: tk, dur: gate, pitch: note, vel };
            }
        }
        (Src::Other(i), e) => {
            if let Some(o) = t.others.get_mut(i) {
                *o = e;
            }
        }
        _ => {}
    }
}

/// Add an event to a track (notes into `notes`, everything else into `others`).
pub fn push_event(t: &mut EditTrack, e: Event) -> Src {
    match e {
        Event::Note { t: tk, note, vel, gate, .. } => {
            t.notes.push(EditNote { tick: tk, dur: gate, pitch: note, vel });
            Src::Note(t.notes.len() - 1)
        }
        e => {
            t.others.push(e);
            Src::Other(t.others.len() - 1)
        }
    }
}

/// Remove the rows' events (indices are invalid afterwards).
pub fn remove_rows(tracks: &mut [EditTrack], rows: &[Row]) {
    let mut by: HashMap<usize, (Vec<usize>, Vec<usize>)> = HashMap::new();
    for r in rows {
        let e = by.entry(r.track).or_default();
        match r.src {
            Src::Note(i) => e.0.push(i),
            Src::Other(i) => e.1.push(i),
        }
    }
    for (ti, (mut n, mut o)) in by {
        let Some(t) = tracks.get_mut(ti) else { continue };
        n.sort_unstable();
        n.dedup();
        o.sort_unstable();
        o.dedup();
        for i in n.into_iter().rev() {
            if i < t.notes.len() {
                t.notes.remove(i);
            }
        }
        for i in o.into_iter().rev() {
            if i < t.others.len() {
                t.others.remove(i);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// time: measure / beat / clock and min / sec / msec
// ---------------------------------------------------------------------------
#[derive(Clone, Debug)]
pub struct TimeMap {
    pub ppqn: u32,
    /// (tick, first measure index, numerator, denominator) — at bar starts.
    sigs: Vec<(u32, u32, u32, u32)>,
    /// (tick, µs per quarter, ms at tick).
    tempos: Vec<(u32, f64, f64)>,
}

impl TimeMap {
    pub fn new(tracks: &[EditTrack], ppqn: u16) -> TimeMap {
        let ppqn = ppqn.max(1) as u32;
        let mut sig_ev: Vec<(u32, u32, u32)> = Vec::new();
        let mut tempo_ev: Vec<(u32, f64)> = Vec::new();
        for t in tracks {
            for e in &t.others {
                if let Event::Meta { t: tk, mtype, data } = e {
                    match (*mtype, data.len()) {
                        (0x58, 2..) if data[0] > 0 => sig_ev.push((*tk, data[0] as u32, 1u32 << data[1].min(6))),
                        (0x51, 3) => tempo_ev.push((*tk, ((data[0] as u32) << 16 | (data[1] as u32) << 8 | data[2] as u32) as f64)),
                        _ => {}
                    }
                }
            }
        }
        sig_ev.sort_by_key(|s| s.0);
        tempo_ev.sort_by(|a, b| a.0.cmp(&b.0));
        let mut sigs = vec![(0u32, 0u32, 4u32, 4u32)];
        for (tk, n, d) in sig_ev {
            let &(t0, m0, n0, d0) = sigs.last().unwrap();
            let bar = ppqn * 4 * n0 / d0;
            // a signature takes effect at the bar it falls in (rounded up)
            let bars = (tk.saturating_sub(t0) + bar - 1) / bar;
            let at = t0 + bars * bar;
            if at == t0 {
                *sigs.last_mut().unwrap() = (t0, m0, n, d);
            } else {
                sigs.push((at, m0 + bars, n, d));
            }
        }
        let mut tempos = vec![(0u32, 500_000.0f64, 0.0f64)];
        for (tk, us) in tempo_ev {
            let &(t0, us0, ms0) = tempos.last().unwrap();
            let ms = ms0 + (tk - t0) as f64 * us0 / ppqn as f64 / 1000.0;
            if tk == t0 {
                *tempos.last_mut().unwrap() = (t0, us, ms0);
            } else {
                tempos.push((tk, us, ms));
            }
        }
        TimeMap { ppqn, sigs, tempos }
    }

    fn sig_at(&self, tick: u32) -> (u32, u32, u32, u32) {
        *self.sigs.iter().rev().find(|s| s.0 <= tick).unwrap_or(&self.sigs[0])
    }

    /// Clocks per beat at `tick` (a beat is the signature's denominator).
    pub fn beat_len(&self, tick: u32) -> u32 {
        let (_, _, _, d) = self.sig_at(tick);
        self.ppqn * 4 / d
    }

    pub fn bar_len(&self, tick: u32) -> u32 {
        let (_, _, n, d) = self.sig_at(tick);
        self.ppqn * 4 * n / d
    }

    /// (measure, beat, clock), measure and beat 1-based.
    pub fn to_mbc(&self, tick: u32) -> (u32, u32, u32) {
        let (t0, m0, n, d) = self.sig_at(tick);
        let beat = self.ppqn * 4 / d;
        let bar = beat * n;
        let rel = tick - t0;
        (m0 + rel / bar + 1, rel % bar / beat + 1, rel % beat)
    }

    /// Tick of (measure, beat, clock); beat / clock may overflow into the next unit.
    pub fn from_mbc(&self, m: u32, b: u32, c: u32) -> u32 {
        let m = m.max(1) - 1;
        let &(t0, m0, n, d) = self.sigs.iter().rev().find(|s| s.1 <= m).unwrap_or(&self.sigs[0]);
        let beat = self.ppqn * 4 / d;
        t0 + (m - m0.min(m)) * beat * n + b.max(1).saturating_sub(1) * beat + c
    }

    pub fn to_ms(&self, tick: u32) -> f64 {
        let &(t0, us, ms0) = self.tempos.iter().rev().find(|x| x.0 <= tick).unwrap_or(&self.tempos[0]);
        ms0 + (tick - t0) as f64 * us / self.ppqn as f64 / 1000.0
    }

    pub fn from_ms(&self, ms: f64) -> u32 {
        let &(t0, us, ms0) = self.tempos.iter().rev().find(|x| x.2 <= ms).unwrap_or(&self.tempos[0]);
        t0 + ((ms - ms0).max(0.0) * 1000.0 * self.ppqn as f64 / us).round() as u32
    }

    /// The three location cells as text.
    pub fn loc_text(&self, tick: u32, time_mode: bool) -> [String; 3] {
        if time_mode {
            let ms = self.to_ms(tick).round() as u64;
            [format!("{}", ms / 60_000), format!("{:02}", ms / 1000 % 60), format!("{:03}", ms % 1000)]
        } else {
            let (m, b, c) = self.to_mbc(tick);
            [format!("{m:04}"), format!("{b:02}"), format!("{c:03}")]
        }
    }

    /// Tick from the three location values (as typed: measure/beat/clock or min/sec/msec).
    pub fn loc_tick(&self, v: [u32; 3], time_mode: bool) -> u32 {
        if time_mode {
            self.from_ms(v[0] as f64 * 60_000.0 + v[1] as f64 * 1000.0 + v[2] as f64)
        } else {
            self.from_mbc(v[0], v[1], v[2])
        }
    }
}

// ---------------------------------------------------------------------------
// names (ws.ini + voice lists)
// ---------------------------------------------------------------------------
pub const NOTE_LETTERS: [&str; 12] = ["C ", "C#", "D ", "D#", "E ", "F ", "F#", "G ", "G#", "A ", "A#", "B "];

/// "D  4" — XGworks' list note name (C3 = 60).
pub fn note_name(n: u8) -> String {
    format!("{}{:2}", NOTE_LETTERS[(n % 12) as usize], n as i32 / 12 - 2)
}

/// Dynamics mark of a velocity ("ppp" … "fff", right-aligned to 3).
pub fn dynamic(vel: u8) -> &'static str {
    ["ppp", " pp", "  p", " mp", " mf", "  f", " ff", "fff"][(vel.min(127) / 16) as usize]
}

/// Names the list needs from `ws.ini` and the voice list files.
#[derive(Clone, Debug, Default)]
pub struct ListNames {
    pub cc: HashMap<u8, String>,
    pub bank_msb: HashMap<u8, String>,
    pub bank_lsb: HashMap<u8, String>,
    pub nrpn1: HashMap<u8, String>,
    pub nrpn2: HashMap<u8, String>,
    pub rpn1: HashMap<u8, String>,
    pub rpn2: HashMap<u8, String>,
    pub xf_meta: HashMap<String, String>,
    /// MSB → (program → name, program name → (LSB → voice)).
    voices: HashMap<u8, (HashMap<u8, String>, HashMap<String, HashMap<u8, String>>)>,
    /// Bank MSBs of drum kits (ws.ini [kit list]); their voice files map
    /// kit program → kit name, and per kit note → instrument name.
    kits: Vec<u8>,
    pub enums: Enums,
}

/// List window "Display XG" settings (dialog "XG Settings"): which events are
/// shown with XG names; off = the plain MIDI view (numbers, "Sysex").
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct XgDisp {
    pub sysex: bool,
    pub note: bool,
    pub cc: bool,
    pub prog: bool,
}

impl Default for XgDisp {
    fn default() -> Self {
        XgDisp { sysex: true, note: true, cc: true, prog: true }
    }
}

fn num_map(ini: &Ini, sec: &str) -> HashMap<u8, String> {
    ini.section(sec)
        .map(|s| s.iter().filter_map(|(k, v)| Some((k.parse::<u8>().ok()?, v.clone()))).collect())
        .unwrap_or_default()
}

impl ListNames {
    /// `ws` = ws.ini bytes; `read` loads a voice list file by name ("Melody.ini").
    pub fn load(ws: &[u8], read: impl Fn(&str) -> Option<Vec<u8>>) -> ListNames {
        let ini = Ini::parse(ws);
        let mut n = ListNames {
            cc: num_map(&ini, "XG Controllers"),
            bank_msb: num_map(&ini, "voice list"),
            bank_lsb: num_map(&ini, "bank lsb"),
            nrpn1: num_map(&ini, "XG Nrpn1"),
            nrpn2: num_map(&ini, "XG Nrpn2"),
            rpn1: num_map(&ini, "XG Rpn1"),
            rpn2: num_map(&ini, "XG Rpn2"),
            xf_meta: ini.section("XF Meta Seq").map(|s| s.iter().cloned().collect()).unwrap_or_default(),
            voices: HashMap::new(),
            kits: num_map(&ini, "kit list").into_keys().collect(),
            enums: Enums::parse(ws),
        };
        for (&msb, file) in &n.bank_msb.clone() {
            let Some(bytes) = read(&format!("{file}.ini")) else { continue };
            let vi = Ini::parse(&bytes);
            let progs = vi.sections.first().map(|s| s.1.iter().filter_map(|(k, v)| Some((k.parse::<u8>().ok()?, v.clone()))).collect()).unwrap_or_default();
            let subs = vi.sections.iter().skip(1).map(|(name, kv)| (name.clone(), kv.iter().filter_map(|(k, v)| Some((k.parse::<u8>().ok()?, v.clone()))).collect())).collect();
            n.voices.insert(msb, (progs, subs));
        }
        n
    }

    /// Drum instrument name of `note` in the kit at bank MSB / program (0-based),
    /// e.g. "Hi-HatClosed" for note 42 of Standard Kit (Drum Kit.ini).
    pub fn drum(&self, msb: u8, prog: u8, note: u8) -> Option<&str> {
        if !self.kits.contains(&msb) {
            return None;
        }
        let (progs, subs) = self.voices.get(&msb)?;
        subs.get(progs.get(&prog)?)?.get(&note).map(|s| s.as_str())
    }

    /// Voice name for bank MSB/LSB and program (0-based), like the original:
    /// the program's name in the MSB's list, then its LSB variation.
    pub fn voice(&self, msb: u8, lsb: u8, prog: u8) -> Option<String> {
        let (progs, subs) = self.voices.get(&msb)?;
        let base = progs.get(&prog)?;
        if let Some(v) = subs.get(base).and_then(|m| m.get(&lsb)) {
            return Some(v.clone());
        }
        // drum kits / SFX: no LSB variations
        if subs.get(base).map_or(true, |m| m.is_empty()) || lsb == 0 { Some(base.clone()) } else { None }
    }
}

// ---------------------------------------------------------------------------
// formatting
// ---------------------------------------------------------------------------
/// The Type / Value1..3 texts of one row.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RowText {
    pub ty: String,
    pub v: [String; 3],
}

/// Running state while formatting rows in time order (bank select per
/// channel, NRPN/RPN MSB, current effect types for parameter names).
#[derive(Clone, Debug)]
pub struct FmtState {
    bank: [(u8, u8); 16],
    prog: [u8; 16],
    nrpn_msb: [Option<u8>; 16],
    rpn_msb: [Option<u8>; 16],
}

impl Default for FmtState {
    /// XG power-on state: part 10 plays drums (bank MSB 127).
    fn default() -> Self {
        let mut bank = [(0u8, 0u8); 16];
        bank[9].0 = 127;
        FmtState { bank, prog: [0; 16], nrpn_msb: [None; 16], rpn_msb: [None; 16] }
    }
}

pub struct Fmt<'a> {
    pub names: &'a ListNames,
    /// Which events get XG names.
    pub xg: XgDisp,
    pub tbl: Option<&'a XgTable>,
    pub ppqn: u32,
    /// Effect type changes in the whole song: (tick, slot key, (MSB, LSB)).
    pub fx_hist: Vec<(u32, [u8; 2], (u8, u8))>,
}

fn fx_key(slot: FxSlot) -> [u8; 2] {
    let [h, m, l] = slot.type_addr();
    [h, m.wrapping_add(l)]
}

/// Effect type changes found in the song, in time order.
pub fn song_fx_types(tracks: &[EditTrack]) -> Vec<(u32, [u8; 2], (u8, u8))> {
    let mut out = Vec::new();
    for t in tracks {
        for e in &t.others {
            if let Event::Sysex { t: tk, status: 0xF0, data } = e {
                if let Some(([h, m, l], d)) = crate::mixer::parse_xg_change(data) {
                    if d.len() >= 2 && matches!((h, m, l), (2, 1, 0x00 | 0x20 | 0x40) | (3, _, 0)) {
                        out.push((*tk, [h, m.wrapping_add(l)], (d[0], d[1])));
                    }
                }
            }
        }
    }
    out.sort_by_key(|x| x.0);
    out
}

impl<'a> Fmt<'a> {
    /// Effect type of `slot` at `tick` (XG defaults before any change).
    fn fx_at(&self, slot: FxSlot, tick: u32) -> (u8, u8) {
        let key = fx_key(slot);
        self.fx_hist.iter().rev().find(|x| x.0 <= tick && x.1 == key).map(|x| x.2).unwrap_or(match slot {
            FxSlot::Reverb => (0x01, 0x00),
            FxSlot::Chorus => (0x41, 0x00),
            FxSlot::Variation => (0x05, 0x00),
            FxSlot::Insertion(_) => (0x48, 0x00),
        })
    }

    pub fn row(&self, e: &Event, st: &mut FmtState) -> RowText {
        match e {
            Event::Note { ch, note, vel, gate, .. } => {
                // drum parts show the kit's instrument names (XG display)
                let c = (*ch & 0x0F) as usize;
                let drum = if self.xg.note { self.names.drum(st.bank[c].0, st.prog[c], *note) } else { None };
                RowText {
                    ty: "Note".into(),
                    v: [format!("{} [{:3}]", drum.map_or_else(|| note_name(*note), str::to_string), note), gate_text(*gate, self.ppqn), format!("{} [{:3}]", dynamic(*vel), vel)],
                }
            }
            Event::Chan { data, .. } => self.chan(data, st),
            Event::Meta { mtype, data, .. } => meta_text(*mtype, data, self.names),
            Event::Sysex { t, status, data } => self.sysex(*t, *status, data),
        }
    }

    fn chan(&self, d: &[u8], st: &mut FmtState) -> RowText {
        let ch = (d[0] & 0x0F) as usize;
        let a = d.get(1).copied().unwrap_or(0);
        let b = d.get(2).copied().unwrap_or(0);
        let cc_names = self.xg.cc;
        let named = |name: Option<&String>, v: u8| format!("{} [{v:3}]", name.filter(|_| cc_names).map_or("", |s| s.as_str()));
        match d[0] & 0xF0 {
            0xA0 => RowText { ty: "Poly After".into(), v: [format!("{} [{a:3}]", note_name(a)), format!(" [{b:3}]"), String::new()] },
            0xB0 => {
                let v2 = match a {
                    0 => {
                        st.bank[ch].0 = b;
                        named(self.names.bank_msb.get(&b), b)
                    }
                    32 => {
                        st.bank[ch].1 = b;
                        named(self.names.bank_lsb.get(&b), b)
                    }
                    99 => {
                        st.nrpn_msb[ch] = Some(b);
                        named(self.names.nrpn1.get(&b).filter(|s| *s != "XG Nrpn2"), b)
                    }
                    98 => named(if st.nrpn_msb[ch] == Some(1) { self.names.nrpn2.get(&b) } else { None }, b),
                    101 => {
                        st.rpn_msb[ch] = Some(b);
                        named(self.names.rpn1.get(&b).filter(|s| *s != "XG Rpn2"), b)
                    }
                    100 => named(if st.rpn_msb[ch].is_some() { self.names.rpn2.get(&b) } else { None }, b),
                    _ => format!(" [{b:3}]"),
                };
                RowText { ty: "Control".into(), v: [named(self.names.cc.get(&a), a), v2, String::new()] }
            }
            0xC0 => {
                let (msb, lsb) = st.bank[ch];
                st.prog[ch] = a;
                let name = if self.xg.prog { self.names.voice(msb, lsb, a).unwrap_or_default() } else { String::new() };
                RowText { ty: "Program".into(), v: [format!("{name} [{:3}]", a as u32 + 1), String::new(), String::new()] }
            }
            0xD0 => RowText { ty: "Ch After".into(), v: [format!("[{a:3}]"), String::new(), String::new()] },
            0xE0 => {
                let v = ((b as i32) << 7 | a as i32) - 8192;
                RowText { ty: "Pitch Bend".into(), v: [format!("[{v:5}]"), String::new(), String::new()] }
            }
            _ => RowText { ty: format!("{:02X}", d[0]), v: Default::default() },
        }
    }

    fn sysex(&self, tick: u32, status: u8, d: &[u8]) -> RowText {
        let hex = d.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ");
        let len = (d.len() + (status == 0xF0) as usize).to_string();
        let (ty, v1) = if !self.xg.sysex {
            ("Sysex".into(), String::new())
        } else if status == 0xF0 {
            self.sysex_name(tick, d)
        } else {
            ("System Exclusive".into(), String::new())
        };
        RowText { ty, v: [v1, hex, len] }
    }

    /// (Type, Value1) of a SysEx (data after F0, F7 included).
    fn sysex_name(&self, tick: u32, d: &[u8]) -> (String, String) {
        let body = if d.last() == Some(&0xF7) { &d[..d.len() - 1] } else { d };
        match body {
            [0x7E, _, 0x09, 0x01] => return ("GM System On".into(), String::new()),
            [0x7E, _, 0x09, 0x03] => return ("GM2 System On".into(), String::new()),
            [0x7E, _, 0x09, 0x02] => return ("GM System Off".into(), String::new()),
            [0x7F, _, 0x04, 0x01, l, m] => return ("Master Volume".into(), ((*m as u32) << 7 | *l as u32).to_string()),
            [0x7E, ..] => return ("Universal Non-Realtime Message".into(), String::new()),
            [0x7F, ..] => return ("Universal Realtime Message".into(), String::new()),
            _ => {}
        }
        if body.len() >= 6 && body[0] == 0x43 && (body[2] == 0x4C || body[2] == 0x49) {
            let kind = match body[1] >> 4 {
                1 => "Prm ",
                3 => "Prm Req ",
                0 => "Dmp ",
                2 => "Dmp Req ",
                _ => return ("System Exclusive".into(), String::new()),
            };
            let (h, m, l) = (body[3], body[4], body[5]);
            let data = &body[6..];
            if body[2] == 0x49 {
                // MU blocks (XGworks.exe 0x61b84e and the type builders around 0x6144bc)
                let li = l as usize & 0x7F;
                let (block, name): (String, &str) = match (h, m) {
                    (0x00, _) => ("Sys ".into(), MU_SYS[li]),
                    (0x09, 0) => ("Cur Prf ".into(), MU_PRF[li]),
                    (0x09, 1) => (format!("Cur Prf Prt{m} "), MU_PRF_PRT[li]),
                    (0x0A, _) => ("Remote ".into(), MU_REMOTE[li]),
                    (0x30, p) => (format!("Int Prf{:03} ", p as u32 + 1), MU_INT_PRF[li]),
                    (0x31..=0x34, p) => (format!("Int Prf{} Prt{:03} ", h & 0x0F, p as u32 + 1), MU_INT_PRF_PRT[li]),
                    _ => (String::new(), ""),
                };
                let v1 = self.enum_or_num(name, data);
                return (format!("Mu {kind}{block}{name}").trim_end().to_string(), v1);
            }
            return self.xg_name(tick, kind, h, m, l, data);
        }
        ("System Exclusive".into(), String::new())
    }

    fn enum_or_num(&self, name: &str, data: &[u8]) -> String {
        let raw = data.iter().fold(0u32, |a, &b| a << 7 | b as u32);
        self.names.enums.0.get(name.trim()).and_then(|l| l.get(raw as usize)).cloned().unwrap_or_else(|| if data.is_empty() { String::new() } else { raw.to_string() })
    }

    fn xg_name(&self, tick: u32, kind: &str, h: u8, m: u8, l: u8, data: &[u8]) -> (String, String) {
        let li = l as usize & 0x7F;
        let raw = data.iter().fold(0u32, |a, &b| a << 7 | b as u32);
        let (block, name): (String, &str) = match (h, m) {
            (0x00, 0x00) => ("Sys ".into(), XG_SYS[li]),
            (0x00, 0x01) => ("Sys Inf ".into(), XG_SYS_INF[li]),
            (0x02, 0x01) => ("Efct1 ".into(), XG_EFCT1[li]),
            (0x02, 0x40) => ("Eq ".into(), XG_EQ[li]),
            (0x03, n) => (format!("Efct2 Ins{} ", n as u32 + 1), XG_EFCT2[li]),
            (0x08, n) => (format!("Prt{:03} ", n as u32 + 1), XG_PART[li]),
            (0x09, n) => (format!("Prt{:03} ", n as u32 + 1), XG_VL_PART[li]),
            (0x0A, n) => (format!("Prt{:03} ", n as u32 + 1), XG_PART2[li]),
            (0x10, n) => (format!("Audio{} ", n as u32 + 1), XG_AUDIO[li]),
            (0x11, _) => ("Audio ".into(), XG_AUDIO_CFG[li]),
            (0x30..=0x3F, n) => (format!("Drm S{} ({}) ", h - 0x2F, note_name(n).replace("  ", " ")), XG_DRUM[li]),
            _ => (String::new(), ""),
        };
        let mut ty = format!("XG {kind}{block}{name}");
        // effect slots: types name the effect, parameters get the effect's param name
        let slot = match (h, m, l) {
            (0x02, 0x01, 0x00..=0x1F) => Some(FxSlot::Reverb),
            (0x02, 0x01, 0x20..=0x3F) => Some(FxSlot::Chorus),
            (0x02, 0x01, 0x40..=0x7F) => Some(FxSlot::Variation),
            (0x03, n, _) => Some(FxSlot::Insertion(n)),
            _ => None,
        };
        let mut v1 = String::new();
        if let Some(slot) = slot {
            let [_, _, tl] = slot.type_addr();
            if l == tl && data.len() >= 2 {
                v1 = fx_type_name(data[0], data[1])
                    .map(str::to_string)
                    .or_else(|| self.tbl.map(|t| t.fx_name(data[0], data[1])))
                    .unwrap_or_else(|| format!("{:02X}{:02X}", data[0], data[1]));
            } else if let Some(k) = (0..16).find(|&k| (0..2).any(|w| slot.param_addr(k, w == 1).0 == [h, m, l])) {
                if let Some(tbl) = self.tbl {
                    let (msb, lsb) = self.fx_at(slot, tick);
                    let list = tbl.types(slot).iter().find(|x| x.0 == msb && x.1 == lsb).and_then(|x| tbl.fx_params.get(&x.2));
                    if let Some(d) = list.and_then(|p| fx_slot_desc(p, k)) {
                        ty = format!("{ty}{}", d.0);
                        v1 = fx_display(d, raw, &self.names.enums);
                    }
                }
            }
        }
        if v1.is_empty() {
            v1 = match (h, l, name.trim()) {
                (0x00, 0x7E, _) => "XG System On".into(),
                (0x00, 0x7F, _) => "All Prm Reset".into(),
                (0x00, 0x7D, _) => format!("Drum S{}", raw + 1),
                (_, _, "Var Prt" | "Prt") => if raw < 64 { format!("Part{}", raw + 1) } else { "Off".into() },
                (_, _, n) if self.names.enums.0.contains_key(n) => self.enum_or_num(n, data),
                _ => self.tbl_value(h, m, l, data).unwrap_or_else(|| if data.is_empty() { String::new() } else { raw.to_string() }),
            };
        }
        (ty.trim_end().to_string(), v1)
    }

    /// Value text from the model's parameter table (units, offsets).
    fn tbl_value(&self, h: u8, m: u8, l: u8, data: &[u8]) -> Option<String> {
        let tbl = self.tbl?;
        let blk = tbl.blocks.iter().find(|b| (b.h.0..=b.h.1).contains(&h) && (b.m.0..=b.m.1).contains(&m) && b.params.iter().any(|p| p.lo == l))?;
        let p = blk.params.iter().find(|p| p.lo == l && !p.is_effect_slot())?;
        Some(param_display(p, p.combine(data)))
    }
}

/// The original's effect type name ("Phaser 1"); an unknown LSB falls back
/// to the type's base (LSB 0), as XG tone generators do.
pub fn fx_type_name(msb: u8, lsb: u8) -> Option<&'static str> {
    let find = |c: u16| FX_TYPE_NAMES.iter().find(|x| x.0 == c).map(|x| x.1);
    find((msb as u16) << 7 | lsb as u16).or_else(|| find((msb as u16) << 7))
}

/// Note gate "[bb:ccc]" (quarter beats : clocks).
pub fn gate_text(gate: u32, ppqn: u32) -> String {
    format!("[{:02}:{:03}]", gate / ppqn.max(1), gate % ppqn.max(1))
}

/// Note value of a gate for the list's note symbol: (base 1 = whole … 64,
/// dotted, triplet); `None` when no value is close.
pub fn gate_symbol(gate: u32, ppqn: u32) -> Option<(u32, bool, bool)> {
    let whole = ppqn as f64 * 4.0;
    let mut best: Option<((u32, bool, bool), f64)> = None;
    for base in [1u32, 2, 4, 8, 16, 32, 64] {
        for (dot, tri, mul) in [(false, false, 1.0), (true, false, 1.5), (false, true, 2.0 / 3.0)] {
            let len = whole / base as f64 * mul;
            let err = (gate as f64 - len).abs() / len;
            if best.map_or(true, |b| err < b.1) {
                best = Some(((base, dot, tri), err));
            }
        }
    }
    // shorter than any symbol: the smallest one (as the original shows [00:005])
    if (gate as f64) < whole / 64.0 * 2.0 / 3.0 {
        return Some((64, false, true));
    }
    best.filter(|b| b.1 < 0.5).map(|b| b.0)
}

const KEYS_MAJ: [&str; 15] = [
    "Cb Maj (7Flats)", "Gb Maj (6Flats)", "Db Maj (5Flats)", "Ab Maj (4Flats)", "Eb Maj (3Flats)", "Bb Maj (2Flats)",
    "F Maj (1Flat)", "C Maj", "G Maj (1Sharp)", "D Maj (2Sharps)", "A Maj (3Sharps)", "E Maj (4Sharps)",
    "B Maj (5Sharps)", "F# Maj (6Sharps)", "C# Maj (7Sharps)",
];
const KEYS_MIN: [&str; 15] = [
    "Ab Min (7Flats)", "Eb Min (6Flats)", "Bb Min (5Flats)", "F Min (4Flats)", "C Min (3Flats)", "G Min (2Flats)",
    "D Min (1Flat)", "A Min", "E Min (1Sharp)", "B Min (2Sharp)", "F# Min (3Sharp)", "C# Min (4Sharp)",
    "G# Min (5Sharp)", "D# Min (6Sharp)", "A# Min (7Sharp)",
];

/// Meta event type names, as the List shows them.
pub fn meta_name(mtype: u8) -> &'static str {
    match mtype {
        0x00 => "Sequence Number",
        0x01 => "Text Event",
        0x02 => "Copyright Notice",
        0x03 => "Sequence Name",
        0x04 => "Instrument Name",
        0x05 => "Lyric",
        0x06 => "Marker",
        0x07 => "Cue Point",
        0x21 => "Port",
        0x2F => "End of Track",
        0x51 => "Tempo",
        0x54 => "SMPTE Offset",
        0x58 => "Time Signature",
        0x59 => "Key Signature",
        0x7F => "Sequencer Specific-Meta Event",
        _ => "Meta Event",
    }
}

fn meta_text(mtype: u8, d: &[u8], names: &ListNames) -> RowText {
    let text = |d: &[u8]| d.iter().map(|&b| if b >= 0x20 { b as char } else { ' ' }).collect::<String>();
    let hex = |d: &[u8]| d.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" ");
    let (ty, v1) = match (mtype, d) {
        (0x01..=0x07, _) => (meta_name(mtype).to_string(), text(d)),
        (0x51, [a, b, c]) => {
            let us = ((*a as u32) << 16 | (*b as u32) << 8 | *c as u32).max(1);
            ("Tempo".into(), format!("{:.2}", 60_000_000.0 / us as f64))
        }
        (0x58, [n, dd, ..]) => ("Time Signature".into(), format!("{:02}/{:02}", n, 1u32 << (*dd).min(6))),
        (0x59, [sf, mi, ..]) => {
            let i = ((*sf as i8) as i32 + 7).clamp(0, 14) as usize;
            ("Key Signature".into(), if *mi != 0 { KEYS_MIN[i] } else { KEYS_MAJ[i] }.to_string())
        }
        (0x54, [h, m, s, f, ff, ..]) => ("SMPTE Offset".into(), format!("{h:02}:{m:02}:{s:02}:{f:02}:{ff:02}")),
        (0x7F, _) => {
            let key = format!("{:X}{}", d.len(), d.iter().take(3).map(|b| format!("{b:02X}")).collect::<String>());
            match names.xf_meta.get(&key) {
                Some(n) => (n.clone(), hex(&d[3.min(d.len())..])),
                None => (meta_name(mtype).into(), hex(d)),
            }
        }
        _ => (meta_name(mtype).into(), hex(d)),
    };
    RowText { ty, v: [v1, String::new(), String::new()] }
}

/// Texts of all rows in order (state carried through, so bank / effect
/// context comes from earlier rows of the list).
pub fn format_rows(tracks: &[EditTrack], rows: &[Row], fmt: &Fmt) -> Vec<RowText> {
    let mut st = FmtState::default();
    rows.iter().map(|r| row_event(tracks, r).map(|e| fmt.row(&e, &mut st)).unwrap_or_default()).collect()
}

// ---------------------------------------------------------------------------
// editing a cell
// ---------------------------------------------------------------------------
/// Value columns (Value1..3) that can be typed into for an event.
pub fn editable(e: &Event, col: usize) -> bool {
    match e {
        Event::Note { .. } => col < 3,
        Event::Chan { data, .. } => match data[0] & 0xF0 {
            0xA0 | 0xB0 => col < 2,
            _ => col == 0,
        },
        Event::Meta { mtype, .. } => col == 0 && matches!(mtype, 0x01..=0x07 | 0x51 | 0x58 | 0x59),
        Event::Sysex { .. } => col == 1,
    }
}

/// Raw editable number of Value column `col` (for +/- keys).
fn value_of(e: &Event, col: usize) -> Option<i32> {
    match (e, col) {
        (Event::Note { note, .. }, 0) => Some(*note as i32),
        (Event::Note { gate, .. }, 1) => Some(*gate as i32),
        (Event::Note { vel, .. }, 2) => Some(*vel as i32),
        (Event::Chan { data, .. }, c) => match (data[0] & 0xF0, c) {
            (0xE0, 0) => Some(((data[2] as i32) << 7 | data[1] as i32) - 8192),
            (0xC0, 0) => Some(data[1] as i32 + 1),
            (_, 0) => data.get(1).map(|&v| v as i32),
            (_, 1) => data.get(2).map(|&v| v as i32),
            _ => None,
        },
        (Event::Meta { mtype: 0x51, data, .. }, 0) if data.len() == 3 => {
            let us = ((data[0] as u32) << 16 | (data[1] as u32) << 8 | data[2] as u32).max(1);
            Some((60_000_000.0 / us as f64).round() as i32)
        }
        _ => None,
    }
}

fn with_value(e: &Event, col: usize, v: i32) -> Option<Event> {
    let c7 = |v: i32| v.clamp(0, 127) as u8;
    let mut e = e.clone();
    match (&mut e, col) {
        (Event::Note { note, .. }, 0) => *note = c7(v),
        (Event::Note { gate, .. }, 1) => *gate = v.max(1) as u32,
        (Event::Note { vel, .. }, 2) => *vel = v.clamp(1, 127) as u8,
        (Event::Chan { data, .. }, c) => match (data[0] & 0xF0, c) {
            (0xE0, 0) => {
                let x = (v.clamp(-8192, 8191) + 8192) as u16;
                data[1] = (x & 0x7F) as u8;
                data[2] = (x >> 7) as u8;
            }
            (0xC0, 0) => data[1] = (v.clamp(1, 128) - 1) as u8,
            (_, 0) if data.len() > 1 => data[1] = c7(v),
            (_, 1) if data.len() > 2 => data[2] = c7(v),
            _ => return None,
        },
        (Event::Meta { mtype: 0x51, data, .. }, 0) => {
            let us = (60_000_000.0 / (v.clamp(1, 999) as f64)).round() as u32;
            *data = vec![(us >> 16) as u8, (us >> 8) as u8, us as u8];
        }
        _ => return None,
    }
    Some(e)
}

/// The event with Value column `col` stepped by `delta` (PgUp/PgDn ±1, Home/End ±10).
pub fn bump_value(e: &Event, col: usize, delta: i32, ppqn: u32) -> Option<Event> {
    let v = value_of(e, col)?;
    // a gate steps by clocks; ±10 jumps by a 16th
    let d = if matches!(e, Event::Note { .. }) && col == 1 && delta.abs() >= 10 { delta.signum() * (ppqn as i32 / 4) } else { delta };
    with_value(e, col, v + d)
}

/// Parse a note name ("D4", "F#-1", "C 3") or a number.
pub fn parse_note(s: &str) -> Option<u8> {
    let s = s.trim();
    if let Ok(n) = s.parse::<i32>() {
        return (0..=127).contains(&n).then_some(n as u8);
    }
    let up = s.to_ascii_uppercase().replace(' ', "");
    let (l, rest) = up.split_at(1.min(up.len()));
    let base = match l {
        "C" => 0,
        "D" => 2,
        "E" => 4,
        "F" => 5,
        "G" => 7,
        "A" => 9,
        "B" => 11,
        _ => return None,
    };
    let (acc, oct) = if let Some(r) = rest.strip_prefix('#') { (1, r) } else if let Some(r) = rest.strip_prefix('B') { (-1, r) } else { (0, rest) };
    let o: i32 = oct.parse().ok()?;
    let n = (o + 2) * 12 + base + acc;
    (0..=127).contains(&n).then_some(n as u8)
}

/// The event with Value column `col` set from typed text.
pub fn set_value(e: &Event, col: usize, text: &str, ppqn: u32) -> Option<Event> {
    let t = text.trim();
    let num = || t.trim_matches(|c| c == '[' || c == ']').trim().parse::<i32>().ok();
    match (e, col) {
        (Event::Note { .. }, 0) | (Event::Chan { .. }, 0) if matches!(e, Event::Note { .. }) || matches!(e, Event::Chan { data, .. } if data[0] & 0xF0 == 0xA0) => {
            with_value(e, col, parse_note(t.trim_matches(|c| c == '[' || c == ']'))? as i32)
        }
        (Event::Note { .. }, 1) => {
            let g = t.trim_matches(|c| c == '[' || c == ']');
            let v = match g.split_once(':') {
                Some((b, c)) => b.trim().parse::<i32>().ok()? * ppqn as i32 + c.trim().parse::<i32>().ok()?,
                None => g.trim().parse().ok()?,
            };
            with_value(e, 1, v)
        }
        (Event::Meta { mtype: m @ 0x01..=0x07, t: tk, .. }, 0) => Some(Event::Meta { t: *tk, mtype: *m, data: t.as_bytes().to_vec() }),
        (Event::Meta { mtype: 0x51, t: tk, .. }, 0) => {
            let bpm: f64 = t.parse().ok()?;
            let us = (60_000_000.0 / bpm.clamp(1.0, 999.0)).round() as u32;
            Some(Event::Meta { t: *tk, mtype: 0x51, data: vec![(us >> 16) as u8, (us >> 8) as u8, us as u8] })
        }
        (Event::Meta { mtype: 0x59, t: tk, .. }, 0) => {
            let norm = |x: &str| x.to_lowercase().replace(' ', "");
            let want = norm(t);
            let hit = |keys: &[&str; 15]| keys.iter().position(|k| norm(k) == want || norm(k.split('(').next().unwrap_or(k)) == want);
            let (i, minor) = match (hit(&KEYS_MAJ), hit(&KEYS_MIN)) {
                (Some(i), _) => (i, 0u8),
                (None, Some(i)) => (i, 1u8),
                _ => return None,
            };
            Some(Event::Meta { t: *tk, mtype: 0x59, data: vec![(i as i32 - 7) as i8 as u8, minor] })
        }
        (Event::Meta { mtype: 0x58, data, t: tk, .. }, 0) => {
            let (n, d) = t.split_once('/')?;
            let (n, d): (u8, u32) = (n.trim().parse().ok()?, d.trim().parse().ok()?);
            if n == 0 || !d.is_power_of_two() {
                return None;
            }
            let mut nd = data.clone();
            nd.resize(4, 0);
            nd[0] = n;
            nd[1] = d.trailing_zeros() as u8;
            if nd[2] == 0 {
                nd[2] = 24;
                nd[3] = 8;
            }
            Some(Event::Meta { t: *tk, mtype: 0x58, data: nd })
        }
        (Event::Sysex { status, t: tk, .. }, 1) => {
            let mut bytes = Vec::new();
            for h in t.split(|c: char| c.is_whitespace() || c == ',') {
                if h.is_empty() {
                    continue;
                }
                bytes.push(u8::from_str_radix(h, 16).ok()?);
            }
            if bytes.first() == Some(&0xF0) {
                bytes.remove(0);
            }
            if bytes.last() != Some(&0xF7) {
                bytes.push(0xF7);
            }
            Some(Event::Sysex { t: *tk, status: *status, data: bytes })
        }
        _ => with_value(e, col, num()?),
    }
}

/// Move an event to another tick.
pub fn with_tick(e: &Event, tick: u32) -> Event {
    let mut e = e.clone();
    match &mut e {
        Event::Note { t, .. } | Event::Chan { t, .. } | Event::Meta { t, .. } | Event::Sysex { t, .. } => *t = tick,
    }
    e
}

// ---------------------------------------------------------------------------
// new events (Insert Event submenu / Insert dialog)
// ---------------------------------------------------------------------------
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewKind {
    Note,
    PolyAfter,
    Control,
    Program,
    ChAfter,
    PitchBend,
    Nrpn,
    Rpn,
    Voice,
    SysEx,
    Text,
    SeqName,
    InstName,
    Lyric,
    Marker,
    // master track (Master.hlp)
    Tempo,
    TimeSig,
    KeySig,
    Copyright,
    CuePoint,
}

impl NewKind {
    pub const ALL: [NewKind; 15] = [
        NewKind::Note, NewKind::PolyAfter, NewKind::Control, NewKind::Program, NewKind::ChAfter, NewKind::PitchBend,
        NewKind::Nrpn, NewKind::Rpn, NewKind::Voice, NewKind::SysEx, NewKind::Text, NewKind::SeqName,
        NewKind::InstName, NewKind::Lyric, NewKind::Marker,
    ];
    /// The events of the Master Track window.
    pub const MASTER: [NewKind; 10] = [
        NewKind::Tempo, NewKind::TimeSig, NewKind::KeySig, NewKind::Marker, NewKind::Text, NewKind::SeqName,
        NewKind::Copyright, NewKind::InstName, NewKind::Lyric, NewKind::CuePoint,
    ];
    pub fn label(self) -> &'static str {
        match self {
            NewKind::Note => "Note",
            NewKind::PolyAfter => "Poly After Touch",
            NewKind::Control => "Control Change",
            NewKind::Program => "Program Change",
            NewKind::ChAfter => "After Touch",
            NewKind::PitchBend => "Pitch Bend",
            NewKind::Nrpn => "Packed Nrpn",
            NewKind::Rpn => "Packed Rpn",
            NewKind::Voice => "Packed Voice",
            NewKind::SysEx => "System Exclusive",
            NewKind::Text => "Text Event",
            NewKind::SeqName => "Sequence Name",
            NewKind::InstName => "Instrument Name",
            NewKind::Lyric => "Lyric",
            NewKind::Marker => "Marker",
            NewKind::Tempo => "Tempo",
            NewKind::TimeSig => "Time Signature",
            NewKind::KeySig => "Key Signature",
            NewKind::Copyright => "Copyright Notice",
            NewKind::CuePoint => "Cue Point",
        }
    }
    /// (label, min, max, default) of the dialog's value fields.
    pub fn fields(self) -> &'static [(&'static str, i32, i32, i32)] {
        match self {
            NewKind::Note => &[("Note", 0, 127, 60), ("Velocity", 1, 127, 100), ("Gate", 1, 99999, 480)],
            NewKind::PolyAfter => &[("Note", 0, 127, 60), ("Value", 0, 127, 0)],
            NewKind::Control => &[("Control", 0, 127, 7), ("Value", 0, 127, 100)],
            NewKind::Program => &[("Value", 1, 128, 1)],
            NewKind::ChAfter => &[("Value", 0, 127, 0)],
            NewKind::PitchBend => &[("Value", -8192, 8191, 0)],
            NewKind::Nrpn => &[("MSB", 0, 127, 1), ("LSB", 0, 127, 8), ("Data MSB", 0, 127, 64), ("Data LSB", -1, 127, -1)],
            NewKind::Rpn => &[("MSB", 0, 127, 0), ("LSB", 0, 127, 0), ("Data MSB", 0, 127, 2), ("Data LSB", -1, 127, -1)],
            NewKind::Voice => &[("Bank MSB", 0, 127, 0), ("Bank LSB", 0, 127, 0), ("Program", 1, 128, 1)],
            NewKind::Tempo => &[("BPM", 20, 300, 120)],
            NewKind::TimeSig => &[("Beats", 1, 32, 4), ("Note value", 1, 32, 4)],
            NewKind::KeySig => &[("Sharps + / flats -", -7, 7, 0), ("Minor (1)", 0, 1, 0)],
            _ => &[],
        }
    }
    /// Auto Step default: notes advance by their gate, others stay (0 clocks).
    pub fn has_text(self) -> bool {
        matches!(self, NewKind::SysEx | NewKind::Text | NewKind::SeqName | NewKind::InstName | NewKind::Lyric | NewKind::Marker | NewKind::Copyright | NewKind::CuePoint)
    }

    /// The events this insert creates at `tick` on channel `ch`.
    pub fn build(self, tick: u32, ch: u8, v: &[i32], text: &str) -> Vec<Event> {
        let g = |i: usize| v.get(i).copied().unwrap_or(0);
        let c7 = |x: i32| x.clamp(0, 127) as u8;
        let cc = |n: u8, x: i32| Event::Chan { t: tick, data: vec![0xB0 | ch, n, c7(x)] };
        let meta = |m: u8| vec![Event::Meta { t: tick, mtype: m, data: text.as_bytes().to_vec() }];
        match self {
            NewKind::Note => vec![Event::Note { t: tick, ch, note: c7(g(0)), vel: g(1).clamp(1, 127) as u8, gate: g(2).max(1) as u32 }],
            NewKind::PolyAfter => vec![Event::Chan { t: tick, data: vec![0xA0 | ch, c7(g(0)), c7(g(1))] }],
            NewKind::Control => vec![cc(c7(g(0)), g(1))],
            NewKind::Program => vec![Event::Chan { t: tick, data: vec![0xC0 | ch, (g(0).clamp(1, 128) - 1) as u8] }],
            NewKind::ChAfter => vec![Event::Chan { t: tick, data: vec![0xD0 | ch, c7(g(0))] }],
            NewKind::PitchBend => {
                let x = (g(0).clamp(-8192, 8191) + 8192) as u16;
                vec![Event::Chan { t: tick, data: vec![0xE0 | ch, (x & 0x7F) as u8, (x >> 7) as u8] }]
            }
            NewKind::Nrpn | NewKind::Rpn => {
                let (m, l) = if self == NewKind::Nrpn { (99, 98) } else { (101, 100) };
                let mut out = vec![cc(m, g(0)), cc(l, g(1)), cc(6, g(2))];
                if g(3) >= 0 {
                    out.push(cc(38, g(3)));
                }
                out
            }
            NewKind::Voice => vec![cc(0, g(0)), cc(32, g(1)), Event::Chan { t: tick, data: vec![0xC0 | ch, (g(2).clamp(1, 128) - 1) as u8] }],
            NewKind::SysEx => {
                let ev = Event::Sysex { t: tick, status: 0xF0, data: vec![0xF7] };
                set_value(&ev, 1, text, 480).map(|e| vec![e]).unwrap_or_default()
            }
            NewKind::Text => meta(0x01),
            NewKind::SeqName => meta(0x03),
            NewKind::InstName => meta(0x04),
            NewKind::Lyric => meta(0x05),
            NewKind::Marker => meta(0x06),
            NewKind::Copyright => meta(0x02),
            NewKind::CuePoint => meta(0x07),
            NewKind::Tempo => {
                let us = (60_000_000.0 / (g(0).clamp(20, 300) as f64)).round() as u32;
                vec![Event::Meta { t: tick, mtype: 0x51, data: vec![(us >> 16) as u8, (us >> 8) as u8, us as u8] }]
            }
            NewKind::TimeSig => {
                let d = (g(1).clamp(1, 32) as u32).next_power_of_two().min(32);
                vec![Event::Meta { t: tick, mtype: 0x58, data: vec![g(0).clamp(1, 32) as u8, d.trailing_zeros() as u8, 24, 8] }]
            }
            NewKind::KeySig => vec![Event::Meta { t: tick, mtype: 0x59, data: vec![g(0).clamp(-7, 7) as i8 as u8, (g(1) != 0) as u8] }],
        }
    }
}

/// Event kinds of the display filter (List.hlp "表示フィルター").
pub const FILTER_KINDS: [&str; 9] =
    ["Note", "Poly After", "Control Change", "Program Change", "Ch After", "Pitch Bend", "System Exclusive", "Meta Event", "Others"];

/// Filter slot of an event.
pub fn filter_kind(e: &Event) -> usize {
    match e {
        Event::Note { .. } => 0,
        Event::Chan { data, .. } => match data[0] & 0xF0 {
            0xA0 => 1,
            0xB0 => 2,
            0xC0 => 3,
            0xD0 => 4,
            0xE0 => 5,
            _ => 8,
        },
        Event::Sysex { .. } => 6,
        Event::Meta { .. } => 7,
    }
}

#[cfg(test)]
mod key_tests {
    use super::*;

    #[test]
    fn master_events() {
        let e = NewKind::KeySig.build(0, 0, &[-3, 0], "");
        let e2 = set_value(&e[0], 0, "D Maj", 480).unwrap();
        assert!(matches!(e2, Event::Meta { mtype: 0x59, ref data, .. } if data[0] == 2 && data[1] == 0));
        let t = NewKind::Tempo.build(0, 0, &[100], "");
        assert!(matches!(t[0], Event::Meta { mtype: 0x51, ref data, .. } if data == &vec![0x09, 0x27, 0xC0]));
        let ts = NewKind::TimeSig.build(0, 0, &[3, 8], "");
        assert!(matches!(ts[0], Event::Meta { mtype: 0x58, ref data, .. } if data[0] == 3 && data[1] == 3));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notes_and_dynamics() {
        assert_eq!(note_name(74), "D  4");
        assert_eq!(note_name(0), "C -2");
        assert_eq!(note_name(78), "F# 4");
        assert_eq!(dynamic(93), "  f");
        assert_eq!(dynamic(101), " ff");
        assert_eq!(gate_text(44, 480), "[00:044]");
        assert_eq!(parse_note("D4"), Some(74));
        assert_eq!(parse_note("F#-1"), Some(18));
        assert_eq!(parse_note("74"), Some(74));
    }

    #[test]
    fn sysex_names_like_original() {
        let names = ListNames::default();
        let f = Fmt { names: &names, xg: XgDisp::default(), tbl: None, ppqn: 480, fx_hist: Vec::new() };
        let mut st = FmtState::default();
        let ev = |d: &[u8]| Event::Sysex { t: 0, status: 0xF0, data: d.to_vec() };
        let r = f.row(&ev(&[0x43, 0x10, 0x4C, 0, 0, 0x7E, 0, 0xF7]), &mut st);
        assert_eq!(r.ty, "XG Prm Sys XG System On");
        assert_eq!(r.v, ["XG System On".to_string(), "43 10 4C 00 00 7E 00 F7".into(), "9".into()]);
        let r = f.row(&ev(&[0x43, 0x10, 0x4C, 0x08, 0x0A, 0x11, 0, 0xF7]), &mut st);
        assert_eq!(r.ty, "XG Prm Prt011 Dry Lvl");
        assert_eq!(r.v[0], "0");
        // MU blocks beyond System (current / internal performance, remote)
        let r = f.row(&ev(&[0x43, 0x10, 0x49, 0x09, 0x00, 0x0C, 0x64, 0xF7]), &mut st);
        assert_eq!(r.ty, "Mu Prm Cur Prf Mstr Vol");
        let r = f.row(&ev(&[0x43, 0x10, 0x49, 0x32, 0x04, 0x02, 0x64, 0xF7]), &mut st);
        assert_eq!(r.ty, "Mu Prm Int Prf2 Prt005 Vol");
        let r = f.row(&ev(&[0x43, 0x10, 0x49, 0x0A, 0x00, 0x00, 0x01, 0xF7]), &mut st);
        assert_eq!(r.ty, "Mu Prm Remote Play");
        let r = f.row(&ev(&[0x43, 0x10, 0x4C, 0x03, 0x00, 0x0C, 0x02, 0xF7]), &mut st);
        assert_eq!(r.ty, "XG Prm Efct2 Ins1 Prt");
        assert_eq!(r.v[0], "Part3");
        let r = f.row(&ev(&[0x43, 0x10, 0x49, 0, 0, 0x12, 0x01, 0xF7]), &mut st);
        assert_eq!(r.ty, "Mu Prm Sys Vce Map");
    }

    #[test]
    fn time_map_mbc() {
        let tm = TimeMap::new(&[], 480);
        assert_eq!(tm.to_mbc(0), (1, 1, 0));
        assert_eq!(tm.to_mbc(18 * 1920 + 480 + 240), (19, 2, 240));
        assert_eq!(tm.from_mbc(19, 2, 240), 18 * 1920 + 480 + 240);
        let mut t = EditTrack::default();
        t.others.push(Event::Meta { t: 1920, mtype: 0x58, data: vec![3, 2, 24, 8] });
        let tm = TimeMap::new(&[t], 480);
        assert_eq!(tm.to_mbc(1920 + 1440), (3, 1, 0));
        assert_eq!(tm.from_mbc(3, 1, 0), 1920 + 1440);
    }
}
