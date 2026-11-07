//! The XGM Editor's drum editor (after Midifile Optimizer's): the drum
//! instruments a part plays in the song, their velocities, and their drum
//! setup as NRPN (GM / GM2 / GS: NRPN MSB = parameter, LSB = note, then Data
//! Entry). XG drum setup goes through the XG Parameter Changes (`3n rr pp`).

use std::collections::HashMap;

use crate::xgtbl::{first_notes, is_reset, part_track};
use crate::{EditTrack, Event};

/// What the drum editor's sliders set.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DrumParam {
    Velo,
    Level,
    Pitch,
    Reverb,
    Chorus,
    Pan,
    Cutoff,
}

impl DrumParam {
    pub const ALL: [DrumParam; 7] = [DrumParam::Velo, DrumParam::Level, DrumParam::Pitch, DrumParam::Reverb, DrumParam::Chorus, DrumParam::Pan, DrumParam::Cutoff];

    pub fn label(self) -> &'static str {
        match self {
            DrumParam::Velo => "Velo",
            DrumParam::Level => "Lev",
            DrumParam::Pitch => "Pitch",
            DrumParam::Reverb => "Rev",
            DrumParam::Chorus => "Cho",
            DrumParam::Pan => "Pan",
            DrumParam::Cutoff => "LP",
        }
    }

    /// The XG Drum Setup parameter (`3n rr <lo>`).
    pub fn xg_lo(self) -> Option<u8> {
        Some(match self {
            DrumParam::Pitch => 0x00,
            DrumParam::Level => 0x02,
            DrumParam::Pan => 0x04,
            DrumParam::Reverb => 0x05,
            DrumParam::Chorus => 0x06,
            DrumParam::Cutoff => 0x0B,
            DrumParam::Velo => return None,
        })
    }

    /// The drum NRPN MSB (GS / XG; LSB = the note).
    pub fn nrpn(self) -> Option<u8> {
        Some(match self {
            DrumParam::Cutoff => 0x14,
            DrumParam::Pitch => 0x18,
            DrumParam::Level => 0x1A,
            DrumParam::Pan => 0x1C,
            DrumParam::Reverb => 0x1D,
            DrumParam::Chorus => 0x1E,
            DrumParam::Velo => return None,
        })
    }

    /// The value shown while the song does not set one.
    pub fn default(self) -> u8 {
        match self {
            DrumParam::Level => 127,
            DrumParam::Reverb => 40,
            DrumParam::Chorus => 0,
            _ => 64,
        }
    }
}

/// A drum instrument the part plays: its note, how often, the mean velocity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DrumKey {
    pub note: u8,
    pub count: usize,
    pub velocity: u8,
}

fn on_part(t: &EditTrack, part: u8) -> bool {
    t.port().saturating_sub(1).min(3) == part / 16
}

/// Every velocity of `note` on the part, mutable (track notes and the notes
/// of other channels kept as events in a format-0 track).
fn velocities<'a>(tracks: &'a mut [EditTrack], part: u8, note: Option<u8>) -> Vec<(u8, &'a mut u8)> {
    let ch = part & 0x0F;
    let mut out = Vec::new();
    for t in tracks.iter_mut().filter(|t| on_part(t, part)) {
        let own = t.channel & 0x0F == ch;
        if own {
            out.extend(t.notes.iter_mut().filter(|n| note.is_none_or(|k| n.pitch == k)).map(|n| (n.pitch, &mut n.vel)));
        }
        for e in t.others.iter_mut() {
            if let Event::Note { ch: c, note: k, vel, .. } = e {
                if *c & 0x0F == ch && note.is_none_or(|w| *k == w) {
                    out.push((*k, vel));
                }
            }
        }
    }
    out
}

/// The drum instruments the part plays, by note.
pub fn used_keys(tracks: &[EditTrack], part: u8) -> Vec<DrumKey> {
    let mut tracks = tracks.to_vec();
    let mut by: HashMap<u8, (usize, u32)> = HashMap::new();
    for (k, v) in velocities(&mut tracks, part, None) {
        let e = by.entry(k).or_default();
        e.0 += 1;
        e.1 += *v as u32;
    }
    let mut keys: Vec<DrumKey> = by.into_iter().map(|(note, (n, sum))| DrumKey { note, count: n, velocity: (sum / n as u32) as u8 }).collect();
    keys.sort_by_key(|k| k.note);
    keys
}

/// Move the velocities of one drum instrument so their mean is `target`
/// (each note by the same amount, kept in 1..127).
pub fn set_key_velocity(tracks: &mut [EditTrack], part: u8, note: u8, target: u8) {
    let mut vs = velocities(tracks, part, Some(note));
    if vs.is_empty() {
        return;
    }
    let mean = vs.iter().map(|v| *v.1 as i32).sum::<i32>() / vs.len() as i32;
    let d = target as i32 - mean;
    for (_, v) in vs.iter_mut() {
        **v = (**v as i32 + d).clamp(1, 127) as u8;
    }
}

/// Where the part's drum NRPNs are: (MSB, note) → (track, event index of the
/// Data Entry), the last one in time order.
fn nrpn_writers(tracks: &[EditTrack], part: u8) -> HashMap<(u8, u8), (usize, usize)> {
    let ch = part & 0x0F;
    let mut evs: Vec<(u32, usize, usize)> = Vec::new();
    for (ti, t) in tracks.iter().enumerate().filter(|(_, t)| on_part(t, part)) {
        for (ei, e) in t.others.iter().enumerate() {
            if let Event::Chan { t, data } = e {
                if data.len() >= 3 && data[0] == 0xB0 | ch {
                    evs.push((*t, ti, ei));
                }
            }
        }
    }
    evs.sort();
    let mut out = HashMap::new();
    let (mut msb, mut lsb) = (None, None);
    for (_, ti, ei) in evs {
        let Event::Chan { data, .. } = &tracks[ti].others[ei] else { continue };
        match data[1] {
            99 => msb = Some(data[2]),
            98 => lsb = Some(data[2]),
            100 | 101 => (msb, lsb) = (None, None),
            6 => {
                if let (Some(m), Some(l)) = (msb, lsb) {
                    out.insert((m, l), (ti, ei));
                }
            }
            _ => {}
        }
    }
    out
}

/// The part's drum NRPN values: (MSB, note) → value.
pub fn nrpn_values(tracks: &[EditTrack], part: u8) -> HashMap<(u8, u8), u8> {
    nrpn_writers(tracks, part)
        .into_iter()
        .filter_map(|(k, (ti, ei))| match &tracks[ti].others[ei] {
            Event::Chan { data, .. } => Some((k, data[2])),
            _ => None,
        })
        .collect()
}

/// Set a drum NRPN: the song's own Data Entry gets the value, else NRPN MSB /
/// LSB / Data Entry go into the part's track before its first note (after
/// the song's last reset).
pub fn set_nrpn(tracks: &mut [EditTrack], part: u8, msb: u8, note: u8, v: u8) {
    if let Some((ti, ei)) = nrpn_writers(tracks, part).get(&(msb, note)).copied() {
        if let Event::Chan { data, .. } = &mut tracks[ti].others[ei] {
            data[2] = v & 0x7F;
        }
        return;
    }
    let Some(ti) = part_track(tracks, part) else { return };
    let ch = part & 0x0F;
    let first = first_notes(&tracks[ti])[ch as usize];
    let reset = tracks
        .iter()
        .flat_map(|t| t.others.iter())
        .filter_map(|e| match e {
            Event::Sysex { t, data, .. } if is_reset(data) && *t <= first => Some(*t),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    let t = &mut tracks[ti];
    let mut pos = t.others.iter().position(|e| e.t() > reset).unwrap_or(t.others.len());
    for (cc, d) in [(99, msb), (98, note), (6, v & 0x7F)] {
        t.others.insert(pos, Event::Chan { t: reset, data: vec![0xB0 | ch, cc, d] });
        pos += 1;
    }
}

/// The MIDI messages of one drum NRPN.
pub fn nrpn_msgs(ch: u8, msb: u8, note: u8, v: u8) -> [Vec<u8>; 3] {
    [vec![0xB0 | ch, 99, msb], vec![0xB0 | ch, 98, note], vec![0xB0 | ch, 6, v & 0x7F]]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song() -> Vec<EditTrack> {
        let n = |tick, pitch, vel| crate::EditNote { tick, dur: 10, pitch, vel };
        let drums = EditTrack {
            channel: 9,
            notes: vec![n(0, 36, 100), n(480, 38, 90), n(960, 36, 120), n(1440, 42, 126)],
            others: vec![Event::Sysex { t: 0, status: 0xF0, data: vec![0x41, 0x10, 0x42, 0x12, 0x40, 0x00, 0x7F, 0x00, 0x41, 0xF7] }],
            ..Default::default()
        };
        vec![drums]
    }

    #[test]
    fn keys_and_velocity() {
        let mut s = song();
        let keys = used_keys(&s, 9);
        assert_eq!(keys.iter().map(|k| (k.note, k.count, k.velocity)).collect::<Vec<_>>(), vec![(36, 2, 110), (38, 1, 90), (42, 1, 126)]);
        set_key_velocity(&mut s, 9, 36, 80);
        assert_eq!(s[0].notes.iter().filter(|n| n.pitch == 36).map(|n| n.vel).collect::<Vec<_>>(), vec![70, 90]);
        set_key_velocity(&mut s, 9, 42, 127);
        assert_eq!(s[0].notes[3].vel, 127);
        assert!(used_keys(&s, 0).is_empty());
    }

    #[test]
    fn nrpn_written_then_changed() {
        let mut s = song();
        set_nrpn(&mut s, 9, 0x1A, 38, 90);
        assert_eq!(s[0].others.len(), 4, "after the GS reset");
        assert_eq!(s[0].others[1], Event::Chan { t: 0, data: vec![0xB9, 99, 0x1A] });
        assert_eq!(nrpn_values(&s, 9).get(&(0x1A, 38)), Some(&90));
        set_nrpn(&mut s, 9, 0x1A, 38, 70);
        assert_eq!(s[0].others.len(), 4, "the same Data Entry changed");
        assert_eq!(nrpn_values(&s, 9).get(&(0x1A, 38)), Some(&70));
        set_nrpn(&mut s, 9, 0x18, 36, 60);
        assert_eq!(nrpn_values(&s, 9).len(), 2);
    }
}
