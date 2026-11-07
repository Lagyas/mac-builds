//! Vocal Harmony for Yamaha arrangers (PSR-SX900 / SX700; after the user's
//! midi-file-editor): the VH SysEx block (SPECIAL EFFECT 04 00 xx, A/D PART
//! 10 00 xx and the Clavinova-ID vocoder part message), the harmony channel's
//! silent control notes (velocity 1, CC 7 = 1) and harmony notes computed
//! from the XF chords. The VH type names are not here: the app reads them from
//! midi-file-editor.html at run time.

use std::collections::HashSet;

use crate::listev::TimeMap;
use crate::xgtbl::is_reset;
use crate::{EditNote, EditTrack, Event};

/// The VH settings the block carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VhSettings {
    pub type_msb: u8,
    pub type_lsb: u8,
    pub part_number: u8,
    /// 0 Off, 1 Upper, 2 Lower.
    pub vocoder_part: u8,
    pub lead_dry: u8,
    pub harm_dry: u8,
    pub lead_send: u8,
    pub harm_send: u8,
    pub mic_chorus: u8,
    pub mic_reverb: u8,
    pub vfx_msb: u8,
    pub vfx_lsb: u8,
    pub vfx_on: bool,
}

/// The defaults of midi-file-editor (a reference PSR-SX900 file).
impl Default for VhSettings {
    fn default() -> Self {
        VhSettings {
            type_msb: 12,
            type_lsb: 1,
            part_number: 0,
            vocoder_part: 0,
            lead_dry: 125,
            harm_dry: 125,
            lead_send: 24,
            harm_send: 24,
            mic_chorus: 0,
            mic_reverb: 18,
            vfx_msb: 21,
            vfx_lsb: 0,
            vfx_on: false,
        }
    }
}

/// The harmony channel's default (channel 16).
pub const DEFAULT_CH: u8 = 15;
const MANAGED: [u8; 11] = [0x00, 0x01, 0x0C, 0x14, 0x18, 0x19, 0x1A, 0x1B, 0x60, 0x61, 0x76];
const AD_MANAGED: [u8; 2] = [0x12, 0x13];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Vh,
    Ad,
    Vocoder,
}

/// One VH-related SysEx (without F0): its kind and (address, value) pairs.
fn classify(d: &[u8]) -> Option<(Kind, Vec<(u8, u8)>)> {
    let d = d.strip_prefix(&[0xF0]).unwrap_or(d);
    let body = d.strip_suffix(&[0xF7]).unwrap_or(d);
    if body.len() >= 7 && body[0] == 0x43 && body[1] & 0xF0 == 0x10 && body[2] == 0x4C {
        let kind = match (body[3], body[4]) {
            (0x04, 0x00) => Kind::Vh,
            (0x10, 0x00) => Kind::Ad,
            _ => return None,
        };
        return Some((kind, body[6..].iter().enumerate().map(|(i, &v)| (body[5] + i as u8, v)).collect()));
    }
    if body.len() == 8 && body[..4] == [0x43, 0x73, 0x01, 0x11] && body[5] == 0x50 && body[6] == 0x10 {
        return Some((Kind::Vocoder, vec![(0, body[7])]));
    }
    None
}

fn managed(kind: Kind, params: &[(u8, u8)]) -> bool {
    match kind {
        Kind::Vocoder => true,
        Kind::Vh => params.iter().all(|p| MANAGED.contains(&p.0)),
        Kind::Ad => params.iter().all(|p| AD_MANAGED.contains(&p.0)),
    }
}

/// What the song has: the settings (defaults where it sets none), the
/// harmony channel it names (None = none / off), and whether it has VH data.
pub struct VhFound {
    pub settings: VhSettings,
    pub channel: Option<u8>,
    pub has_any: bool,
    /// (track, event) of the messages this editor writes again.
    managed: Vec<(usize, usize, u32)>,
}

pub fn analyze(tracks: &[EditTrack]) -> VhFound {
    let mut s = VhSettings::default();
    let mut ch = None;
    let mut managed_ev = Vec::new();
    let mut any = false;
    let mut evs: Vec<(u32, usize, usize)> = Vec::new();
    for (ti, t) in tracks.iter().enumerate() {
        for (ei, e) in t.others.iter().enumerate() {
            if let Event::Sysex { t, status: 0xF0, .. } = e {
                evs.push((*t, ti, ei));
            }
        }
    }
    evs.sort();
    for (t, ti, ei) in evs {
        let Event::Sysex { data, .. } = &tracks[ti].others[ei] else { continue };
        let Some((kind, params)) = classify(data) else { continue };
        any = true;
        if managed(kind, &params) {
            managed_ev.push((ti, ei, t));
        }
        for (a, v) in params {
            match (kind, a) {
                (Kind::Vh, 0x00) => s.type_msb = v,
                (Kind::Vh, 0x01) => s.type_lsb = v,
                (Kind::Vh, 0x0C) => s.part_number = v,
                (Kind::Vh, 0x14) => ch = (v < 16).then_some(v),
                (Kind::Vh, 0x18) => s.lead_dry = v,
                (Kind::Vh, 0x19) => s.harm_dry = v,
                (Kind::Vh, 0x1A) => s.lead_send = v,
                (Kind::Vh, 0x1B) => s.harm_send = v,
                (Kind::Vh, 0x60) => s.vfx_msb = v,
                (Kind::Vh, 0x61) => s.vfx_lsb = v,
                (Kind::Vh, 0x76) => s.vfx_on = v != 0,
                (Kind::Ad, 0x12) => s.mic_chorus = v,
                (Kind::Ad, 0x13) => s.mic_reverb = v,
                (Kind::Vocoder, _) => s.vocoder_part = v,
                _ => {}
            }
        }
    }
    VhFound { settings: s, channel: ch, has_any: any, managed: managed_ev }
}

/// The VH block, in the order of the reference file: (label, SysEx without F0).
pub fn build_msgs(s: &VhSettings, ch: u8) -> Vec<(String, Vec<u8>)> {
    let xg = |hh: u8, ll: u8, d: &[u8]| {
        let mut v = vec![0x43, 0x10, 0x4C, hh, 0x00, ll];
        v.extend(d.iter().map(|b| b & 0x7F));
        v.push(0xF7);
        v
    };
    vec![
        (format!("VH type {}/{}", s.type_msb, s.type_lsb), xg(0x04, 0x00, &[s.type_msb, s.type_lsb])),
        (format!("VH Part Number {}", s.part_number), xg(0x04, 0x0C, &[s.part_number])),
        (format!("Vocoder Part {}", ["Off", "Upper", "Lower"].get(s.vocoder_part as usize).unwrap_or(&"?")), vec![0x43, 0x73, 0x01, 0x11, 0x00, 0x50, 0x10, s.vocoder_part & 0x7F, 0xF7]),
        (format!("Harmony Channel {}", ch + 1), xg(0x04, 0x14, &[ch])),
        (format!("Lead Vocal Dry {}", s.lead_dry), xg(0x04, 0x18, &[s.lead_dry])),
        (format!("Harmony Vocal Dry {}", s.harm_dry), xg(0x04, 0x19, &[s.harm_dry])),
        (format!("Lead Vocal Send {}", s.lead_send), xg(0x04, 0x1A, &[s.lead_send])),
        (format!("Harmony Vocal Send {}", s.harm_send), xg(0x04, 0x1B, &[s.harm_send])),
        (format!("Mic Chorus Send {}", s.mic_chorus), xg(0x10, 0x12, &[s.mic_chorus])),
        (format!("Mic Reverb Send {}", s.mic_reverb), xg(0x10, 0x13, &[s.mic_reverb])),
        (format!("Vocal Effect type {}/{}", s.vfx_msb, s.vfx_lsb), xg(0x04, 0x60, &[s.vfx_msb, s.vfx_lsb])),
        (format!("Vocal Effect {}", if s.vfx_on { "on" } else { "off" }), xg(0x04, 0x76, &[s.vfx_on as u8])),
    ]
}

/// The tick where the block goes when the song has none: 100 ms after its
/// last reset (or tick 4).
fn after_resets(tracks: &[EditTrack], tm: &TimeMap) -> u32 {
    let last = tracks
        .iter()
        .flat_map(|t| t.others.iter())
        .filter_map(|e| match e {
            Event::Sysex { t, data, .. } if is_reset(data) || crate::fxsysex::is_gs_reset(data) => Some(*t),
            _ => None,
        })
        .max();
    match last {
        Some(t) => tm.from_ms(tm.to_ms(t) + 100.0).max(t + 1),
        None => 4,
    }
}

/// Write the VH block into the first track: in place of the VH messages this
/// editor manages (when they come after the resets), else 100 ms after the last
/// reset; other VH messages stay. Returns (messages, start tick, replaced).
pub fn insert_sysex(tracks: &mut [EditTrack], tm: &TimeMap, s: &VhSettings, ch: u8) -> (usize, u32, usize) {
    let found = analyze(tracks);
    let min_base = after_resets(tracks, tm);
    let first = found.managed.iter().map(|m| m.2).min();
    let base = match first {
        Some(t) if t >= min_base => t,
        _ => min_base,
    };
    remove_events(tracks, found.managed.iter().map(|m| (m.0, m.1)).collect());
    let msgs = build_msgs(s, ch);
    let n = msgs.len();
    if let Some(t0) = tracks.first_mut() {
        for (k, (_, data)) in msgs.into_iter().enumerate() {
            t0.others.push(Event::Sysex { t: base + 4 * k as u32, status: 0xF0, data });
        }
    }
    (n, base, found.managed.len())
}

fn remove_events(tracks: &mut [EditTrack], gone: HashSet<(usize, usize)>) {
    for (ti, t) in tracks.iter_mut().enumerate() {
        let mut k = 0;
        t.others.retain(|_| {
            let keep = !gone.contains(&(ti, k));
            k += 1;
            keep
        });
    }
}

/// The harmony channel's state: notes, their velocities, the CC 7 there.
pub struct ChannelCheck {
    pub notes: usize,
    pub first_note: Option<u32>,
    pub velocities: Vec<u8>,
    pub cc7: Vec<(u32, u8)>,
}

impl ChannelCheck {
    pub fn vel_ok(&self) -> bool {
        self.velocities.iter().all(|&v| v == 1)
    }
    /// One CC 7 = 1, at or before the first note.
    pub fn vol_ok(&self) -> bool {
        self.cc7.len() == 1 && self.cc7[0].1 == 1 && self.first_note.is_none_or(|f| self.cc7[0].0 <= f)
    }
}

fn on_ch(t: &EditTrack, ch: u8) -> bool {
    t.channel & 0x0F == ch
}

pub fn check_channel(tracks: &[EditTrack], ch: u8) -> ChannelCheck {
    let mut ticks = Vec::new();
    let mut vels = Vec::new();
    let mut cc7 = Vec::new();
    for t in tracks {
        if on_ch(t, ch) {
            for n in &t.notes {
                ticks.push(n.tick);
                vels.push(n.vel);
            }
        }
        for e in &t.others {
            match e {
                Event::Note { t, ch: c, vel, .. } if c & 0x0F == ch => {
                    ticks.push(*t);
                    vels.push(*vel);
                }
                Event::Chan { t, data } if data.len() >= 3 && data[0] == 0xB0 | ch && data[1] == 7 => cc7.push((*t, data[2])),
                _ => {}
            }
        }
    }
    cc7.sort();
    vels.sort();
    vels.dedup();
    ChannelCheck { notes: ticks.len(), first_note: ticks.iter().copied().min(), velocities: vels, cc7 }
}

/// Velocity 1 on every note of the channel; one CC 7 = 1 just before the
/// first note (in place of the others). Returns how many velocities changed.
pub fn fix_channel(tracks: &mut [EditTrack], ch: u8) -> usize {
    let mut n = 0;
    let mut first: Option<(u32, usize)> = None;
    for (ti, t) in tracks.iter_mut().enumerate() {
        let own = on_ch(t, ch);
        if own {
            for x in t.notes.iter_mut() {
                if x.vel != 1 {
                    x.vel = 1;
                    n += 1;
                }
                if first.is_none_or(|f| x.tick < f.0) {
                    first = Some((x.tick, ti));
                }
            }
        }
        for e in t.others.iter_mut() {
            if let Event::Note { t: tk, ch: c, vel, .. } = e {
                if *c & 0x0F == ch {
                    if *vel != 1 {
                        *vel = 1;
                        n += 1;
                    }
                    if first.is_none_or(|f| *tk < f.0) {
                        first = Some((*tk, ti));
                    }
                }
            }
        }
        t.others.retain(|e| !matches!(e, Event::Chan { data, .. } if data.len() >= 3 && data[0] == 0xB0 | ch && data[1] == 7));
    }
    let (at, ti) = first.map(|(t, ti)| (t, ti)).unwrap_or((0, 0));
    if let Some(t) = tracks.get_mut(ti) {
        t.others.push(Event::Chan { t: at, data: vec![0xB0 | ch, 7, 1] });
    }
    n
}

/// Chord tones (semitones above the root) of the 34 XF chord types.
const XF_INTERVALS: [&[i32]; 34] = [
    &[0, 4, 7], &[0, 4, 7, 9], &[0, 4, 7, 11], &[0, 4, 7, 11], &[0, 4, 7, 2], &[0, 4, 11, 2], &[0, 4, 9, 2], &[0, 4, 8],
    &[0, 3, 7], &[0, 3, 7, 9], &[0, 3, 7, 10], &[0, 3, 6, 10], &[0, 3, 7, 2], &[0, 3, 10, 2], &[0, 3, 10, 5], &[0, 3, 7, 11], &[0, 3, 11, 2],
    &[0, 3, 6], &[0, 3, 6, 9], &[0, 4, 7, 10], &[0, 5, 7, 10], &[0, 4, 6, 10], &[0, 4, 10, 2], &[0, 4, 10, 6], &[0, 4, 10, 9], &[0, 4, 10, 1],
    &[0, 4, 10, 8], &[0, 4, 10, 3], &[0, 4, 8, 11], &[0, 4, 8, 10], &[0], &[0, 7], &[0, 5, 7], &[0, 2, 7],
];

fn root_pc(b: u8) -> Option<i32> {
    let (n, f) = ((b & 0x0F) as i32, ((b >> 4) & 7) as i32);
    if !(1..=7).contains(&n) || f > 6 {
        return None;
    }
    Some(([0, 2, 4, 5, 7, 9, 11][n as usize - 1] + f - 3).rem_euclid(12))
}

/// A chord line: (tick, pitch classes; None = N.C.), in time order.
pub type ChordLine = Vec<(u32, Option<Vec<i32>>)>;

/// Pitch classes of XF chord data (`43 7B 01 cr ct bn bt`): Some(None) = N.C.,
/// None = not XF chord data.
pub fn xf_pcs(data: &[u8]) -> Option<Option<Vec<i32>>> {
    if data.len() != 7 || data[..3] != [0x43, 0x7B, 0x01] {
        return None;
    }
    Some(root_pc(data[3]).and_then(|r| XF_INTERVALS.get(data[4] as usize).map(|iv| iv.iter().map(|i| (r + i).rem_euclid(12)).collect())))
}

/// Sorted, repeats merged.
pub fn tidy(mut v: ChordLine) -> ChordLine {
    v.sort_by_key(|c| c.0);
    v.dedup_by(|b, a| a.1 == b.1);
    v
}

/// The song's XF chords (FF 7F 43 7B 01 …).
pub fn xf_chords(tracks: &[EditTrack]) -> ChordLine {
    tidy(
        tracks
            .iter()
            .flat_map(|t| t.others.iter())
            .filter_map(|e| match e {
                Event::Meta { t, mtype: 0x7F, data } => xf_pcs(data).map(|p| (*t, p)),
                _ => None,
            })
            .collect(),
    )
}

/// Chord kinds the note detection knows: (name suffix, tones above the root).
pub const DETECT_KINDS: [(&str, &[i32]); 8] =
    [("", &[0, 4, 7]), ("m", &[0, 3, 7]), ("7", &[0, 4, 7, 10]), ("m7", &[0, 3, 7, 10]), ("M7", &[0, 4, 7, 11]), ("sus4", &[0, 5, 7]), ("dim", &[0, 3, 6]), ("aug", &[0, 4, 8])];

/// Chords found in the notes, one per half bar: every channel except drums
/// (10) and `skip_ch`, each pitch class weighted by how long it sounds there
/// (the lowest note counts more, as the likely root). Returns (tick, root pc,
/// kind index into DETECT_KINDS), repeats merged; a half bar without notes
/// keeps the chord before it.
pub fn detect_chords(tracks: &[EditTrack], tm: &TimeMap, skip_ch: u8) -> Vec<(u32, i32, usize)> {
    let mut notes: Vec<(u32, u32, u8)> = Vec::new();
    for t in tracks {
        if !on_ch(t, 9) && !on_ch(t, skip_ch) {
            notes.extend(t.notes.iter().map(|n| (n.tick, n.tick + n.dur.max(1), n.pitch)));
        }
        notes.extend(t.others.iter().filter_map(|e| match e {
            Event::Note { t, ch, note, gate, .. } if ch & 0x0F != 9 && ch & 0x0F != skip_ch => Some((*t, *t + (*gate).max(1), *note)),
            _ => None,
        }));
    }
    let end = notes.iter().map(|n| n.1).max().unwrap_or(0);
    notes.sort_by_key(|n| n.0);
    let mut out: Vec<(u32, i32, usize)> = Vec::new();
    let mut s = notes.first().map_or(0, |n| tm.from_mbc(tm.to_mbc(n.0).0, 1, 0));
    while s < end {
        let len = (tm.bar_len(s) / 2).max(1);
        let e = s + len;
        let mut w = [0f32; 12];
        let mut low: Option<u8> = None;
        for &(a, b, p) in notes.iter().take_while(|n| n.0 < e) {
            let o = b.min(e).saturating_sub(a.max(s));
            if o > 0 {
                w[(p % 12) as usize] += o as f32;
                low = Some(low.map_or(p, |l| l.min(p)));
            }
        }
        if let Some(l) = low {
            w[(l % 12) as usize] += len as f32 * 0.5;
            let total: f32 = w.iter().sum();
            let mut best = (f32::MIN, 0, 0);
            for root in 0..12 {
                for (k, (_, iv)) in DETECT_KINDS.iter().enumerate() {
                    let inside: f32 = iv.iter().map(|i| w[((root + i) % 12) as usize]).sum();
                    // all tones should sound; a smaller chord wins a tie
                    let missing = iv.iter().filter(|i| w[((root + **i) % 12) as usize] == 0.0).count() as f32;
                    let score = inside - (total - inside) - missing * total * 0.3 - iv.len() as f32 * 0.01 * total + if root == (l % 12) as i32 { total * 0.05 } else { 0.0 };
                    if score > best.0 {
                        best = (score, root, k);
                    }
                }
            }
            if out.last().is_none_or(|x| (x.1, x.2) != (best.1, best.2)) {
                out.push((s, best.1, best.2));
            }
        }
        s = e;
    }
    out
}

/// Close voicing: each chord tone at the octave nearest `center`.
fn voice(pcs: &[i32], center: i32, max: usize) -> Vec<u8> {
    let mut v: Vec<u8> = pcs
        .iter()
        .take(max)
        .map(|&pc| {
            let mut best = pc;
            let mut n = pc;
            while n < 128 {
                if (n - center).abs() < (best - center).abs() {
                    best = n;
                }
                n += 12;
            }
            best as u8
        })
        .collect();
    v.sort();
    v.dedup();
    v
}

/// How computed notes go in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenMode {
    /// Replace the channel's notes in the bars.
    Replace,
    /// Only where the channel has no notes.
    Fill,
}

/// Harmony notes (velocity 1) from `chords` on the harmony channel, in
/// `bars` (inclusive ranges; empty = the whole song), `lead` ticks before each
/// chord change, `voices` notes near `center`. They go into the channel's
/// track, else a new "Vocal Harmony" track. Returns (notes, segments skipped).
#[allow(clippy::too_many_arguments)]
pub fn generate(tracks: &mut Vec<EditTrack>, tm: &TimeMap, chords: &[(u32, Option<Vec<i32>>)], ch: u8, bars: &[(u32, u32)], mode: GenMode, lead: u32, voices: usize, center: i32) -> (usize, usize) {
    let song_end = tracks.iter().flat_map(|t| t.notes.iter().map(|n| n.tick + n.dur).chain(t.others.iter().map(|e| e.t()))).max().unwrap_or(0);
    let ranges: Vec<(u32, u32)> = if bars.is_empty() { vec![(0, song_end)] } else { bars.iter().map(|&(a, b)| (tm.from_mbc(a.max(1), 1, 0), tm.from_mbc(b + 1, 1, 0))).collect() };
    let in_ranges = |t: u32| ranges.iter().any(|&(s, e)| t >= s && t < e);
    // Where the channel's notes are: a track on that channel, or (format 0)
    // notes of that channel kept as events in another track. New notes go
    // there too; a song with neither gets a "Vocal Harmony" track.
    let as_event = tracks.iter().position(|t| t.others.iter().any(|e| matches!(e, Event::Note { ch: c, .. } if c & 0x0F == ch)));
    let own = tracks.iter().position(|t| on_ch(t, ch) && !t.notes.is_empty());
    if mode == GenMode::Replace {
        for t in tracks.iter_mut() {
            if on_ch(t, ch) {
                t.notes.retain(|n| !in_ranges(n.tick));
            }
            t.others.retain(|e| !matches!(e, Event::Note { t, ch: c, .. } if c & 0x0F == ch && in_ranges(*t)));
        }
    }
    let mut kept: Vec<(u32, u32)> = Vec::new();
    for t in tracks.iter() {
        if on_ch(t, ch) {
            kept.extend(t.notes.iter().map(|n| (n.tick, n.tick + n.dur)));
        }
        kept.extend(t.others.iter().filter_map(|e| match e {
            Event::Note { t, ch: c, gate, .. } if c & 0x0F == ch => Some((*t, *t + gate)),
            _ => None,
        }));
    }
    let timeline = tidy(chords.to_vec());
    let mut new_notes: Vec<(u32, u32, u8)> = Vec::new();
    let mut skipped = 0;
    let mut c = center;
    for &(rs, re) in &ranges {
        let mut segs: Vec<(u32, Option<Vec<i32>>)> = Vec::new();
        if let Some(b) = timeline.iter().rev().find(|x| x.0 <= rs) {
            segs.push((rs, b.1.clone()));
        }
        segs.extend(timeline.iter().filter(|x| x.0 > rs && x.0 < re).cloned());
        for (k, (start, pcs)) in segs.iter().enumerate() {
            let end = segs.get(k + 1).map_or(re, |s| s.0);
            let Some(pcs) = pcs else { continue };
            if end <= *start {
                continue;
            }
            let on = if *start == rs { *start } else { start.saturating_sub(lead) };
            let off = (if end == re { end } else { end.saturating_sub(lead) }).max(on + 1);
            if mode == GenMode::Fill && kept.iter().any(|&(s, e)| s < off && e > on) {
                skipped += 1;
                continue;
            }
            let notes = voice(pcs, c, voices.clamp(1, 4));
            c = notes.iter().map(|&n| n as i32).sum::<i32>() / notes.len().max(1) as i32;
            new_notes.extend(notes.into_iter().map(|p| (on, off - on, p)));
        }
    }
    let made = new_notes.len();
    match (as_event, own) {
        (Some(ti), None) => {
            for (t, gate, note) in new_notes {
                tracks[ti].others.push(Event::Note { t, ch, note, vel: 1, gate });
            }
        }
        _ => {
            let ti = own.unwrap_or_else(|| {
                tracks.push(EditTrack { name: "Vocal Harmony".into(), channel: ch, ..Default::default() });
                tracks.len() - 1
            });
            tracks[ti].notes.extend(new_notes.into_iter().map(|(tick, dur, pitch)| EditNote { tick, dur, pitch, vel: 1 }));
            tracks[ti].notes.sort_by_key(|n| (n.tick, n.pitch));
        }
    }
    (made, skipped)
}

/// "54-77, 94-117" → [(54, 77), (94, 117)]; None when not understood.
pub fn parse_bars(s: &str) -> Option<Vec<(u32, u32)>> {
    let mut out = Vec::new();
    for part in s.split([',', ';', ' ']).filter(|p| !p.is_empty()) {
        let (a, b) = match part.split_once('-') {
            Some((a, b)) => (a.trim().parse().ok()?, b.trim().parse().ok()?),
            None => {
                let a: u32 = part.trim().parse().ok()?;
                (a, a)
            }
        };
        if a < 1 || b < a {
            return None;
        }
        out.push((a, b));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song() -> Vec<EditTrack> {
        let reset = Event::Sysex { t: 0, status: 0xF0, data: vec![0x43, 0x10, 0x4C, 0, 0, 0x7E, 0, 0xF7] };
        // C (bar 1), F (bar 2), G7 (bar 3)
        let chord = |t, cr, ct| Event::Meta { t, mtype: 0x7F, data: vec![0x43, 0x7B, 0x01, cr, ct, 0x7F, 0x7F] };
        let master = EditTrack { others: vec![reset, chord(0, 0x31, 0), chord(1920, 0x34, 0), chord(3840, 0x35, 19)], ..Default::default() };
        let mel = EditTrack { channel: 0, notes: vec![EditNote { tick: 0, dur: 5760, pitch: 60, vel: 100 }], ..Default::default() };
        vec![master, mel]
    }

    #[test]
    fn block_written_and_read_back() {
        let mut s = song();
        let tm = TimeMap::new(&s, 480);
        let mut set = VhSettings::default();
        set.type_lsb = 3;
        let (n, base, replaced) = insert_sysex(&mut s, &tm, &set, 15);
        assert_eq!((n, replaced), (12, 0));
        assert!(base > 0, "after the XG System On");
        let f = analyze(&s);
        assert_eq!(f.settings, set);
        assert_eq!(f.channel, Some(15));
        // again: replaced in place
        set.lead_dry = 100;
        let (_, base2, replaced) = insert_sysex(&mut s, &tm, &set, 15);
        assert_eq!((base2, replaced), (base, 12));
        assert_eq!(analyze(&s).settings.lead_dry, 100);
        assert_eq!(s[0].others.len(), 4 + 12);
    }

    #[test]
    fn harmony_from_chords_and_channel_fix() {
        let mut s = song();
        let tm = TimeMap::new(&s, 480);
        let xf = xf_chords(&s);
        let (made, _) = generate(&mut s, &tm, &xf, 15, &[], GenMode::Replace, 10, 4, 64);
        assert_eq!(made, 3 + 3 + 4, "C, F, G7");
        let h = s.iter().find(|t| t.channel == 15).unwrap();
        assert!(h.notes.iter().all(|n| n.vel == 1));
        // F starts 10 ticks early, the first chord at its own tick
        assert!(h.notes.iter().any(|n| n.tick == 1910));
        assert!(h.notes.iter().any(|n| n.tick == 0));
        let c = check_channel(&s, 15);
        assert!(c.vel_ok() && !c.vol_ok());
        fix_channel(&mut s, 15);
        assert!(check_channel(&s, 15).vol_ok());
        // fill: nothing new where notes are
        let xf = xf_chords(&s);
        let (made, skipped) = generate(&mut s, &tm, &xf, 15, &[], GenMode::Fill, 10, 4, 64);
        assert_eq!((made, skipped), (0, 3));
        assert_eq!(parse_bars("54-77, 94-117").unwrap(), vec![(54, 77), (94, 117)]);
        assert!(parse_bars("5-3").is_none());
    }
}
