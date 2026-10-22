//! XG mixer model, reimplementing what `SwMixer.dll` drives.
//!
//! RE notes (SwMixer.dll): per-part controls go out as channel messages; the
//! defaults come from `Mixer.ini [Default]` (CC7=100, CC10=64, CC11=127,
//! CC71..74=64, CC91=40). XG parameters go out as Parameter Change SysEx
//! `F0 43 1n 4C hh mm ll dd.. F7` (n = device number). The mixer also writes
//! its settings into the song as events, so they persist and replay.

use crate::{EditTrack, Event};

/// Per-part controllers the mixer exposes: (CC number, label, XG default).
pub const MIXER_CCS: [(u8, &str, u8); 10] = [
    (7, "Volume", 100),
    (10, "Pan", 64),
    (11, "Expression", 127),
    (91, "Reverb", 40),
    (93, "Chorus", 0),
    (94, "Variation", 0),
    (74, "Brightness", 64),
    (71, "Harmonic", 64),
    (73, "Attack", 64),
    (72, "Release", 64),
];

pub const CC_BANK_MSB: u8 = 0;
pub const CC_BANK_LSB: u8 = 32;

/// Index into `PartMix::cc` for a CC number, if the mixer controls it.
pub fn cc_slot(cc: u8) -> Option<usize> {
    MIXER_CCS.iter().position(|c| c.0 == cc)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PartMix {
    pub bank_msb: u8,
    pub bank_lsb: u8,
    pub program: u8,
    pub cc: [u8; MIXER_CCS.len()],
}

impl Default for PartMix {
    fn default() -> Self {
        let mut cc = [0u8; MIXER_CCS.len()];
        for (i, c) in MIXER_CCS.iter().enumerate() {
            cc[i] = c.2;
        }
        PartMix { bank_msb: 0, bank_lsb: 0, program: 0, cc }
    }
}

fn first_note_tick(t: &EditTrack) -> u32 {
    t.notes.iter().map(|n| n.tick).min().unwrap_or(u32::MAX)
}

/// What the part sounds like when its first note plays: for each control, the
/// first value set at or before the first note (XG default otherwise).
pub fn read_part_mix(t: &EditTrack) -> PartMix {
    let mut m = PartMix::default();
    let limit = first_note_tick(t);
    let mut seen = [false; MIXER_CCS.len() + 3];
    let mut evs: Vec<&Event> = t.others.iter().filter(|e| e.t() <= limit).collect();
    evs.sort_by_key(|e| e.t());
    for e in evs {
        if let Event::Chan { data, .. } = e {
            match (data[0] & 0xF0, data.get(1), data.get(2)) {
                (0xB0, Some(&CC_BANK_MSB), Some(&v)) if !seen[0] => (m.bank_msb, seen[0]) = (v, true),
                (0xB0, Some(&CC_BANK_LSB), Some(&v)) if !seen[1] => (m.bank_lsb, seen[1]) = (v, true),
                (0xC0, Some(&p), _) if !seen[2] => (m.program, seen[2]) = (p, true),
                (0xB0, Some(&cc), Some(&v)) => {
                    if let Some(i) = cc_slot(cc) {
                        if !seen[3 + i] {
                            m.cc[i] = v;
                            seen[3 + i] = true;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    m
}

/// Channel messages that set a part to `m` (bank, program, then controllers).
pub fn part_messages(ch: u8, m: &PartMix) -> Vec<Vec<u8>> {
    let ch = ch & 0x0F;
    let mut out = vec![
        vec![0xB0 | ch, CC_BANK_MSB, m.bank_msb & 0x7F],
        vec![0xB0 | ch, CC_BANK_LSB, m.bank_lsb & 0x7F],
        vec![0xC0 | ch, m.program & 0x7F],
    ];
    for (i, c) in MIXER_CCS.iter().enumerate() {
        out.push(vec![0xB0 | ch, c.0, m.cc[i] & 0x7F]);
    }
    out
}

/// Key identifying which mixer control a channel message sets, if any.
fn mix_key(data: &[u8]) -> Option<(u8, u8)> {
    match data[0] & 0xF0 {
        0xC0 => Some((0xC0, 0)),
        0xB0 => {
            let cc = *data.get(1)?;
            (cc == CC_BANK_MSB || cc == CC_BANK_LSB || cc_slot(cc).is_some()).then_some((0xB0, cc))
        }
        _ => None,
    }
}

/// Write `m` into the track's setup: the first occurrence of each control at or
/// before the first note is overwritten in place; missing ones are inserted at
/// tick 0. Later automation is left untouched.
pub fn apply_part_mix(t: &mut EditTrack, m: &PartMix) {
    let limit = first_note_tick(t);
    let ch = t.channel & 0x0F;
    for msg in part_messages(ch, m) {
        let key = mix_key(&msg).unwrap();
        let mut best: Option<usize> = None;
        for (i, e) in t.others.iter().enumerate() {
            if let Event::Chan { t: et, data } = e {
                if *et <= limit && mix_key(data) == Some(key) {
                    if best.map_or(true, |b| *et < t.others[b].t()) {
                        best = Some(i);
                    }
                }
            }
        }
        match best {
            Some(i) => {
                let et = t.others[i].t();
                t.others[i] = Event::Chan { t: et, data: msg };
            }
            None => t.others.push(Event::Chan { t: 0, data: msg }),
        }
    }
}

// ---------------------------------------------------------------------------
// XG system / effect parameters (SysEx)
// ---------------------------------------------------------------------------

/// XG Parameter Change: `F0 43 1n 4C hh mm ll data.. F7`.
pub fn xg_param(dev: u8, addr: [u8; 3], data: &[u8]) -> Vec<u8> {
    let mut v = vec![0xF0, 0x43, 0x10 | (dev & 0x0F), 0x4C, addr[0], addr[1], addr[2]];
    v.extend(data.iter().map(|b| b & 0x7F));
    v.push(0xF7);
    v
}

pub fn xg_reset(dev: u8) -> Vec<u8> {
    xg_param(dev, [0x00, 0x00, 0x7E], &[0x00])
}

pub fn master_volume(dev: u8, v: u8) -> Vec<u8> {
    xg_param(dev, [0x00, 0x00, 0x04], &[v])
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxBlock {
    Reverb,
    Chorus,
    Variation,
}

impl FxBlock {
    pub fn type_addr(self) -> [u8; 3] {
        match self {
            FxBlock::Reverb => [0x02, 0x01, 0x00],
            FxBlock::Chorus => [0x02, 0x01, 0x20],
            FxBlock::Variation => [0x02, 0x01, 0x40],
        }
    }
    /// (name, type MSB, type LSB) — XG basic effect types.
    pub fn types(self) -> &'static [(&'static str, u8, u8)] {
        match self {
            FxBlock::Reverb => REVERB_TYPES,
            FxBlock::Chorus => CHORUS_TYPES,
            FxBlock::Variation => VARIATION_TYPES,
        }
    }
}

pub fn fx_type(dev: u8, block: FxBlock, msb: u8, lsb: u8) -> Vec<u8> {
    xg_param(dev, block.type_addr(), &[msb, lsb])
}

/// Variation connection: false = insertion (XG default), true = system (send via CC94).
pub fn variation_connection(dev: u8, system: bool) -> Vec<u8> {
    xg_param(dev, [0x02, 0x01, 0x5A], &[system as u8])
}

pub const REVERB_TYPES: &[(&str, u8, u8)] = &[
    ("No Effect", 0x00, 0x00),
    ("Hall 1", 0x01, 0x00),
    ("Hall 2", 0x01, 0x01),
    ("Room 1", 0x02, 0x00),
    ("Room 2", 0x02, 0x01),
    ("Room 3", 0x02, 0x02),
    ("Stage 1", 0x03, 0x00),
    ("Stage 2", 0x03, 0x01),
    ("Plate", 0x04, 0x00),
    ("White Room", 0x10, 0x00),
    ("Tunnel", 0x11, 0x00),
    ("Basement", 0x13, 0x00),
];

pub const CHORUS_TYPES: &[(&str, u8, u8)] = &[
    ("No Effect", 0x00, 0x00),
    ("Chorus 1", 0x41, 0x00),
    ("Chorus 2", 0x41, 0x01),
    ("Chorus 3", 0x41, 0x02),
    ("Chorus 4", 0x41, 0x08),
    ("Celeste 1", 0x42, 0x00),
    ("Celeste 2", 0x42, 0x01),
    ("Celeste 3", 0x42, 0x02),
    ("Celeste 4", 0x42, 0x08),
    ("Flanger 1", 0x43, 0x00),
    ("Flanger 2", 0x43, 0x01),
    ("Flanger 3", 0x43, 0x08),
];

pub const VARIATION_TYPES: &[(&str, u8, u8)] = &[
    ("No Effect", 0x00, 0x00),
    ("Hall 1", 0x01, 0x00),
    ("Room 1", 0x02, 0x00),
    ("Stage 1", 0x03, 0x00),
    ("Plate", 0x04, 0x00),
    ("Delay L,C,R", 0x05, 0x00),
    ("Delay L,R", 0x06, 0x00),
    ("Echo", 0x07, 0x00),
    ("Cross Delay", 0x08, 0x00),
    ("Early Ref 1", 0x09, 0x00),
    ("Early Ref 2", 0x09, 0x01),
    ("Gate Reverb", 0x0A, 0x00),
    ("Reverse Gate", 0x0B, 0x00),
    ("Karaoke 1", 0x14, 0x00),
    ("Chorus 1", 0x41, 0x00),
    ("Celeste 1", 0x42, 0x00),
    ("Flanger 1", 0x43, 0x00),
    ("Symphonic", 0x44, 0x00),
    ("Rotary Speaker", 0x45, 0x00),
    ("Tremolo", 0x46, 0x00),
    ("Auto Pan", 0x47, 0x00),
    ("Phaser 1", 0x48, 0x00),
    ("Distortion", 0x49, 0x00),
    ("Overdrive", 0x4A, 0x00),
    ("Amp Simulator", 0x4B, 0x00),
    ("3-Band EQ", 0x4C, 0x00),
    ("2-Band EQ", 0x4D, 0x00),
    ("Auto Wah", 0x4E, 0x00),
];

/// Song-wide XG settings the mixer's Master section controls.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MasterMix {
    pub volume: u8,
    pub reverb: usize,    // index into REVERB_TYPES
    pub chorus: usize,    // index into CHORUS_TYPES
    pub variation: usize, // index into VARIATION_TYPES
    pub var_system: bool,
}

impl Default for MasterMix {
    fn default() -> Self {
        // XG power-on: Hall 1, Chorus 1, Delay L,C,R in insertion mode.
        MasterMix { volume: 127, reverb: 1, chorus: 1, variation: 5, var_system: false }
    }
}

fn fx_index(block: FxBlock, msb: u8, lsb: u8) -> Option<usize> {
    block.types().iter().position(|t| t.1 == msb && t.2 == lsb)
}

/// XG Parameter Change `(addr, data)` from SysEx data after the `F0`
/// (as stored in `Event::Sysex`, trailing `F7` included).
pub fn parse_xg_change(d: &[u8]) -> Option<([u8; 3], &[u8])> {
    if d.len() >= 6 && d[0] == 0x43 && d[1] & 0xF0 == 0x10 && d[2] == 0x4C {
        let end = if d.last() == Some(&0xF7) { d.len() - 1 } else { d.len() };
        Some(([d[3], d[4], d[5]], &d[6..end]))
    } else {
        None
    }
}

fn parse_xg(status: u8, data: &[u8]) -> Option<([u8; 3], &[u8])> {
    if status == 0xF0 { parse_xg_change(data) } else { None }
}

/// Master settings as set by XG SysEx anywhere before the first note of the song.
pub fn read_master_mix(tracks: &[EditTrack]) -> MasterMix {
    let mut m = MasterMix::default();
    let limit = tracks.iter().map(first_note_tick).min().unwrap_or(u32::MAX);
    let mut evs: Vec<&Event> = tracks.iter().flat_map(|t| t.others.iter()).filter(|e| e.t() <= limit).collect();
    evs.sort_by_key(|e| e.t());
    for e in evs {
        if let Event::Sysex { status, data, .. } = e {
            if let Some((addr, d)) = parse_xg(*status, data) {
                match (addr, d) {
                    ([0, 0, 0x7E], _) => m = MasterMix::default(),
                    ([0, 0, 0x04], [v, ..]) => m.volume = *v,
                    ([2, 1, 0x5A], [v, ..]) => m.var_system = *v == 1,
                    (a, [hi, lo, ..]) => {
                        for b in [FxBlock::Reverb, FxBlock::Chorus, FxBlock::Variation] {
                            if a == b.type_addr() {
                                if let Some(i) = fx_index(b, *hi, *lo) {
                                    match b {
                                        FxBlock::Reverb => m.reverb = i,
                                        FxBlock::Chorus => m.chorus = i,
                                        FxBlock::Variation => m.variation = i,
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    m
}

pub fn master_messages(dev: u8, m: &MasterMix) -> Vec<Vec<u8>> {
    let r = REVERB_TYPES[m.reverb];
    let c = CHORUS_TYPES[m.chorus];
    let v = VARIATION_TYPES[m.variation];
    vec![
        master_volume(dev, m.volume),
        fx_type(dev, FxBlock::Reverb, r.1, r.2),
        fx_type(dev, FxBlock::Chorus, c.1, c.2),
        fx_type(dev, FxBlock::Variation, v.1, v.2),
        variation_connection(dev, m.var_system),
    ]
}

/// Write the master settings into `track` (normally track 0) as SysEx at tick 0,
/// replacing earlier setup SysEx for the same XG addresses. If the song has an
/// XG System On at tick 0, the new events are placed right after it.
pub fn apply_master_mix(track: &mut EditTrack, dev: u8, m: &MasterMix) {
    let msgs = master_messages(dev, m);
    let addrs: Vec<[u8; 3]> = msgs.iter().map(|b| [b[4], b[5], b[6]]).collect();
    let limit = first_note_tick(track);
    track.others.retain(|e| match e {
        Event::Sysex { t, status, data } if *t <= limit => {
            !parse_xg(*status, data).map_or(false, |(a, _)| addrs.contains(&a))
        }
        _ => true,
    });
    // after an XG reset at tick 0 (if any), which would otherwise wipe them
    let pos = track
        .others
        .iter()
        .rposition(|e| matches!(e, Event::Sysex { t: 0, status, data }
            if parse_xg(*status, data).map_or(false, |(a, _)| a == [0, 0, 0x7E])))
        .map_or(0, |i| i + 1);
    for (k, b) in msgs.into_iter().enumerate() {
        track.others.insert(pos + k, Event::Sysex { t: 0, status: 0xF0, data: b[1..].to_vec() });
    }
}

// ---------------------------------------------------------------------------
// XGworks' built-in Mixer window (Mixer.hlp): 16 MIDI-channel strips on one
// output port. Per channel: Pgm (bank MSB/LSB + program), Eq (NRPN 01/31 Hi,
// 01/30 Lo; the Hi/Lo buttons pick which one the knobs edit), Rev CC91,
// Cho CC93, Var CC94, Pan CC10, and a fader assigned to any CC in Mixer Setup
// (Volume by default). The Master fader is the Universal Master Volume
// (template `F0 7F 7F 04 01 ..` in XGworks.exe). Moving a control only sends
// it to the device; the song changes only through "Insert Mixer Parameter".
// ---------------------------------------------------------------------------

pub const NRPN_EQ_LO: u8 = 0x30;
pub const NRPN_EQ_HI: u8 = 0x31;

/// `Mixer.ini [Default]`: the initial value of a controller (0 if not listed).
pub fn ini_default(cc: u8) -> u8 {
    match cc {
        7 => 100,
        10 => 64,
        11 => 127,
        71..=74 => 64,
        91 => 40,
        _ => 0,
    }
}

/// One channel strip. EQ gains are raw NRPN data (64 = 0 dB, shown as -64..63).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChanMix {
    pub bank_msb: u8,
    pub bank_lsb: u8,
    pub program: u8,
    pub eq_hi: u8,
    pub eq_lo: u8,
    pub rev: u8,
    pub cho: u8,
    pub var: u8,
    pub pan: u8,
    /// Value of the assignable controller on the fader.
    pub ac: u8,
}

impl ChanMix {
    /// "Initialize Channel Data": Pgm 1, Eq centre, Rev 40, Cho/Var 0, Pan
    /// centre, fader at the controller's Mixer.ini default (Volume 100).
    pub fn initial(ac_cc: u8) -> Self {
        ChanMix {
            bank_msb: 0,
            bank_lsb: 0,
            program: 0,
            eq_hi: 64,
            eq_lo: 64,
            rev: ini_default(91),
            cho: 0,
            var: 0,
            pan: 64,
            ac: ini_default(ac_cc),
        }
    }
}

/// NRPN parameter set: `B0 63 msb`, `B0 62 lsb`, `B0 06 v`.
pub fn nrpn(ch: u8, msb: u8, lsb: u8, v: u8) -> [[u8; 3]; 3] {
    let s = 0xB0 | (ch & 0x0F);
    [[s, 0x63, msb], [s, 0x62, lsb], [s, 0x06, v & 0x7F]]
}

/// Universal Real Time SysEx Master Volume (`F0 7F 7F 04 01 ll mm F7`).
pub fn universal_master_volume(v: u8) -> Vec<u8> {
    vec![0xF0, 0x7F, 0x7F, 0x04, 0x01, 0x00, v & 0x7F, 0xF7]
}

/// Everything a strip sends ("Transmit Mixer Parameter"), in order.
pub fn chan_messages(ch: u8, m: &ChanMix, ac_cc: u8) -> Vec<Vec<u8>> {
    let s = 0xB0 | (ch & 0x0F);
    let mut out = vec![
        vec![s, CC_BANK_MSB, m.bank_msb & 0x7F],
        vec![s, CC_BANK_LSB, m.bank_lsb & 0x7F],
        vec![0xC0 | (ch & 0x0F), m.program & 0x7F],
    ];
    for (lsb, v) in [(NRPN_EQ_HI, m.eq_hi), (NRPN_EQ_LO, m.eq_lo)] {
        out.extend(nrpn(ch, 0x01, lsb, v).iter().map(|b| b.to_vec()));
    }
    for (cc, v) in [(91, m.rev), (93, m.cho), (94, m.var), (10, m.pan), (ac_cc, m.ac)] {
        out.push(vec![s, cc, v & 0x7F]);
    }
    out
}

/// Follows channel messages into the 16 strips (Play Monitor), keeping the
/// NRPN address per channel so EQ data entries land on the right knob.
#[derive(Clone, Debug)]
pub struct ChanTracker {
    nrpn: [(u8, u8); 16],
}

impl Default for ChanTracker {
    fn default() -> Self {
        ChanTracker { nrpn: [(0x7F, 0x7F); 16] }
    }
}

impl ChanTracker {
    /// Apply one channel message; returns the channel if a strip changed.
    pub fn apply(&mut self, mix: &mut [ChanMix; 16], ac_cc: u8, data: &[u8]) -> Option<usize> {
        let ch = (data.first()? & 0x0F) as usize;
        let m = &mut mix[ch];
        match (data[0] & 0xF0, data.get(1).copied(), data.get(2).copied()) {
            (0xC0, Some(p), _) => m.program = p,
            (0xB0, Some(cc), Some(v)) => {
                match cc {
                    0x63 => self.nrpn[ch].0 = v,
                    0x62 => self.nrpn[ch].1 = v,
                    0x65 | 0x64 => self.nrpn[ch] = (0x7F, 0x7F), // RPN selected
                    0x06 => match self.nrpn[ch] {
                        (0x01, NRPN_EQ_HI) => m.eq_hi = v,
                        (0x01, NRPN_EQ_LO) => m.eq_lo = v,
                        _ => return None,
                    },
                    CC_BANK_MSB => m.bank_msb = v,
                    CC_BANK_LSB => m.bank_lsb = v,
                    _ => {}
                }
                // the fader's controller may also be one of the knobs' CCs
                let mut hit = matches!(cc, 0x06 | CC_BANK_MSB | CC_BANK_LSB);
                for (n, slot) in [(91, &mut m.rev), (93, &mut m.cho), (94, &mut m.var), (10, &mut m.pan)] {
                    if cc == n {
                        *slot = v;
                        hit = true;
                    }
                }
                if cc == ac_cc {
                    m.ac = v;
                    hit = true;
                }
                if !hit {
                    return None;
                }
            }
            _ => return None,
        }
        Some(ch)
    }
}

/// Channel messages of all tracks, time-sorted: `(tick, bytes)`.
pub fn song_chan_events(tracks: &[EditTrack]) -> Vec<(u32, Vec<u8>)> {
    let mut v: Vec<(u32, Vec<u8>)> = tracks
        .iter()
        .flat_map(|t| t.others.iter())
        .filter_map(|e| match e {
            Event::Chan { t, data } if !data.is_empty() => Some((*t, data.clone())),
            _ => None,
        })
        .collect();
    v.sort_by_key(|x| x.0);
    v
}

/// The strips as the song sets them up: each channel follows its messages up
/// to (and including) the tick of its first note.
pub fn song_chan_mix(tracks: &[EditTrack], ac_cc: u8) -> [ChanMix; 16] {
    let mut first = [u32::MAX; 16];
    for t in tracks {
        let ch = (t.channel & 0x0F) as usize;
        first[ch] = first[ch].min(first_note_tick(t));
        for e in &t.others {
            if let Event::Note { t, ch, .. } = e {
                let c = (*ch & 0x0F) as usize;
                first[c] = first[c].min(*t);
            }
        }
    }
    let mut mix = [ChanMix::initial(ac_cc); 16];
    let mut tr = ChanTracker::default();
    for (t, data) in song_chan_events(tracks) {
        if t <= first[(data[0] & 0x0F) as usize] {
            tr.apply(&mut mix, ac_cc, &data);
        }
    }
    mix
}

/// "Insert Mixer Parameter": write each strip's settings at the start of
/// track `start + ch` (channel 1 into `start`, …). Events go after anything
/// already at tick 0 so the mixer values win. The Master volume goes into
/// `start` as well.
pub fn insert_mixer_params(tracks: &mut [EditTrack], start: usize, mix: &[ChanMix; 16], ac_cc: u8, master: u8) {
    for ch in 0..16usize {
        let Some(t) = tracks.get_mut(start + ch) else { break };
        let pos = t.others.iter().rposition(|e| e.t() == 0).map_or(0, |i| i + 1);
        let mut evs: Vec<Event> = Vec::new();
        if ch == 0 {
            let mv = universal_master_volume(master);
            evs.push(Event::Sysex { t: 0, status: 0xF0, data: mv[1..].to_vec() });
        }
        evs.extend(chan_messages(ch as u8, &mix[ch], ac_cc).into_iter().map(|data| Event::Chan { t: 0, data }));
        t.others.splice(pos..pos, evs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EditNote;

    fn track(others: Vec<Event>) -> EditTrack {
        EditTrack {
            name: "t".into(),
            channel: 2,
            notes: vec![EditNote { tick: 480, dur: 100, pitch: 60, vel: 100 }],
            others,
            ..Default::default()
        }
    }

    #[test]
    fn sysex_bytes() {
        assert_eq!(xg_reset(0), vec![0xF0, 0x43, 0x10, 0x4C, 0, 0, 0x7E, 0, 0xF7]);
        assert_eq!(fx_type(1, FxBlock::Chorus, 0x41, 0x02), vec![0xF0, 0x43, 0x11, 0x4C, 2, 1, 0x20, 0x41, 0x02, 0xF7]);
    }

    #[test]
    fn part_roundtrip_keeps_automation() {
        let mut t = track(vec![
            Event::Chan { t: 0, data: vec![0xB2, 7, 90] },
            Event::Chan { t: 960, data: vec![0xB2, 7, 20] }, // automation after first note
        ]);
        let m0 = read_part_mix(&t);
        assert_eq!(m0.cc[cc_slot(7).unwrap()], 90);
        assert_eq!(m0.cc[cc_slot(10).unwrap()], 64);
        let mut m = m0.clone();
        m.cc[cc_slot(7).unwrap()] = 110;
        m.program = 24;
        apply_part_mix(&mut t, &m);
        assert_eq!(read_part_mix(&t), m);
        assert!(t.others.contains(&Event::Chan { t: 960, data: vec![0xB2, 7, 20] }));
        assert!(t.others.contains(&Event::Chan { t: 0, data: vec![0xB2, 7, 110] }));
        // applying twice adds nothing new
        let n = t.others.len();
        apply_part_mix(&mut t, &m);
        assert_eq!(t.others.len(), n);
    }

    #[test]
    fn master_roundtrip() {
        let reset = xg_reset(0);
        let mut t = track(vec![Event::Sysex { t: 0, status: 0xF0, data: reset[1..].to_vec() }]);
        assert_eq!(read_master_mix(std::slice::from_ref(&t)), MasterMix::default());
        let m = MasterMix { volume: 100, reverb: 3, chorus: 9, variation: 22, var_system: true };
        apply_master_mix(&mut t, 0, &m);
        apply_master_mix(&mut t, 0, &m);
        assert_eq!(t.others.len(), 1 + 5);
        assert_eq!(read_master_mix(std::slice::from_ref(&t)), m);
    }

    #[test]
    fn channel_mixer_follows_song_and_inserts() {
        let t = track(vec![
            Event::Chan { t: 0, data: vec![0xC2, 33] },
            Event::Chan { t: 0, data: vec![0xB2, 0x63, 0x01] },
            Event::Chan { t: 0, data: vec![0xB2, 0x62, NRPN_EQ_LO] },
            Event::Chan { t: 0, data: vec![0xB2, 0x06, 70] },
            Event::Chan { t: 10, data: vec![0xB2, 7, 90] },
            Event::Chan { t: 960, data: vec![0xB2, 7, 20] }, // after the first note
        ]);
        let mut tracks = vec![t];
        let mix = song_chan_mix(&tracks, 7);
        assert_eq!(mix[2].program, 33);
        assert_eq!((mix[2].eq_lo, mix[2].eq_hi), (70, 64));
        assert_eq!(mix[2].ac, 90);
        assert_eq!(mix[0], ChanMix::initial(7));
        assert_eq!(mix[0].rev, 40);

        // insert with channel 1 going to track 0, then follow the song again
        let mut m2 = mix;
        m2[0].pan = 10;
        m2[0].eq_hi = 80;
        insert_mixer_params(&mut tracks, 0, &m2, 11, 100);
        let ev = song_chan_events(&tracks);
        let mut tr = ChanTracker::default();
        let mut got = [ChanMix::initial(11); 16];
        for (_, d) in &ev {
            tr.apply(&mut got, 11, d);
        }
        assert_eq!((got[0].pan, got[0].eq_hi, got[0].ac), (10, 80, ini_default(7)));
        assert!(tracks[0].others.iter().any(|e| matches!(e, Event::Sysex { data, .. } if data[..4] == [0x7F, 0x7F, 0x04, 0x01])));
    }
}
