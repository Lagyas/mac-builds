//! Effect settings of GS and GM2 instruments for the XGM Editor: GS (Roland
//! Data Set 1, `F0 41 dev 42 12 aa aa aa dd… cs F7`) Reverb / Chorus / Delay
//! and the SC-88Pro Insertion Effect (EFX), GM2 Global Parameter Control
//! (`F0 7F dev 04 05 01 01 01 01 01 pp vv F7`) Reverb / Chorus. Read from the
//! song and written back into it (the song's own message when it has one).

use std::collections::HashMap;

use crate::xgtbl::is_reset;
use crate::{EditTrack, Event};

/// One GS effect parameter: address, label, range, default.
#[derive(Clone, Copy, Debug)]
pub struct GsParam {
    pub addr: [u8; 3],
    pub name: &'static str,
    pub max: u8,
    pub default: u8,
}

const fn p(lo: u8, name: &'static str, max: u8, default: u8) -> GsParam {
    GsParam { addr: [0x40, 0x01, lo], name, max, default }
}

pub const GS_REVERB_MACROS: [&str; 8] = ["Room 1", "Room 2", "Room 3", "Hall 1", "Hall 2", "Plate", "Delay", "Panning Delay"];
pub const GS_CHORUS_MACROS: [&str; 8] = ["Chorus 1", "Chorus 2", "Chorus 3", "Chorus 4", "Feedback Chorus", "Flanger", "Short Delay", "Short Delay (FB)"];
pub const GS_DELAY_MACROS: [&str; 10] = ["Delay 1", "Delay 2", "Delay 3", "Delay 4", "Pan Delay 1", "Pan Delay 2", "Pan Delay 3", "Pan Delay 4", "Delay to Reverb", "Pan Repeat"];

/// GS Reverb: the macro first, then its parameters.
pub const GS_REVERB: [GsParam; 7] = [
    p(0x30, "Type", 7, 4),
    p(0x31, "Character", 7, 4),
    p(0x32, "Pre-LPF", 7, 0),
    p(0x33, "Level", 127, 64),
    p(0x34, "Time", 127, 64),
    p(0x35, "Delay FB", 127, 0),
    p(0x37, "Pre-Delay", 127, 0),
];
pub const GS_CHORUS: [GsParam; 9] = [
    p(0x38, "Type", 7, 2),
    p(0x39, "Pre-LPF", 7, 0),
    p(0x3A, "Level", 127, 64),
    p(0x3B, "Feedback", 127, 8),
    p(0x3C, "Delay", 127, 80),
    p(0x3D, "Rate", 127, 3),
    p(0x3E, "Depth", 127, 19),
    p(0x3F, "To Reverb", 127, 0),
    p(0x40, "To Delay", 127, 0),
];
pub const GS_DELAY: [GsParam; 11] = [
    p(0x50, "Type", 9, 0),
    p(0x51, "Pre-LPF", 7, 0),
    p(0x52, "Time C", 115, 97),
    p(0x53, "Ratio L", 120, 1),
    p(0x54, "Ratio R", 120, 1),
    p(0x55, "Level C", 127, 127),
    p(0x56, "Level L", 127, 0),
    p(0x57, "Level R", 127, 0),
    p(0x58, "Level", 127, 64),
    p(0x59, "Feedback", 127, 80),
    p(0x5A, "To Reverb", 127, 0),
];

/// SC-88Pro Insertion Effect: type (MSB, LSB at 40 03 00), 20 parameters
/// (40 03 03…), sends (40 03 17…19).
pub const GS_EFX_TYPE: [u8; 3] = [0x40, 0x03, 0x00];
pub fn gs_efx_param(k: usize) -> [u8; 3] {
    [0x40, 0x03, 0x03 + k as u8]
}
pub const GS_EFX_SENDS: [(u8, &str); 3] = [(0x17, "To Reverb"), (0x18, "To Chorus"), (0x19, "To Delay")];

/// The GS address of a part's EFX switch (40 4x 22; part 10 = block 0,
/// 1–9 = 1–9, 11–16 = A–F).
pub fn gs_part_efx(part: u8) -> [u8; 3] {
    let n = part & 0x0F;
    let block = match n {
        0..=8 => n + 1,
        9 => 0,
        _ => n,
    };
    [0x40, 0x40 | block, 0x22]
}

/// A GS Data Set 1 message (with F0 / F7) for `addr` and `data`.
pub fn gs_dt1(dev: u8, addr: [u8; 3], data: &[u8]) -> Vec<u8> {
    let mut m = vec![0xF0, 0x41, 0x10 | (dev & 0x0F), 0x42, 0x12, addr[0], addr[1], addr[2]];
    m.extend(data.iter().map(|b| b & 0x7F));
    let sum: u32 = m[5..].iter().map(|&b| b as u32).sum();
    m.push(((128 - sum % 128) % 128) as u8);
    m.push(0xF7);
    m
}

/// (address, data) of a GS Data Set 1 (with or without the F0).
pub fn parse_gs(d: &[u8]) -> Option<([u8; 3], &[u8])> {
    let d = d.strip_prefix(&[0xF0]).unwrap_or(d);
    if d.len() < 8 || d[0] != 0x41 || d[2] != 0x42 || d[3] != 0x12 {
        return None;
    }
    let end = d.iter().position(|&b| b == 0xF7)?;
    if end < 8 {
        return None;
    }
    Some(([d[4], d[5], d[6]], &d[7..end - 1]))
}

/// GM2 Global Parameter Control slot: Reverb (1) or Chorus (2).
pub const GM2_REVERB: u8 = 1;
pub const GM2_CHORUS: u8 = 2;
pub const GM2_REVERB_TYPES: [(u8, &str); 6] = [(0, "Small Room"), (1, "Medium Room"), (2, "Large Room"), (3, "Medium Hall"), (4, "Large Hall"), (8, "Plate")];
pub const GM2_CHORUS_TYPES: [&str; 6] = ["Chorus 1", "Chorus 2", "Chorus 3", "Chorus 4", "FB Chorus", "Flanger"];
/// (slot, parameter, label, default): parameter 0 is the type.
pub const GM2_PARAMS: [(u8, u8, &str, u8); 7] = [
    (GM2_REVERB, 0, "Type", 4),
    (GM2_REVERB, 1, "Time", 64),
    (GM2_CHORUS, 0, "Type", 2),
    (GM2_CHORUS, 1, "Rate", 3),
    (GM2_CHORUS, 2, "Depth", 19),
    (GM2_CHORUS, 3, "Feedback", 8),
    (GM2_CHORUS, 4, "To Reverb", 0),
];

pub fn gm2_global(dev: u8, slot: u8, param: u8, v: u8) -> Vec<u8> {
    vec![0xF0, 0x7F, dev & 0x7F, 0x04, 0x05, 0x01, 0x01, 0x01, 0x01, slot, param & 0x7F, v & 0x7F, 0xF7]
}

/// (slot, parameter, value) of a GM2 Reverb / Chorus Global Parameter Control.
pub fn parse_gm2(d: &[u8]) -> Option<(u8, u8, u8)> {
    let d = d.strip_prefix(&[0xF0]).unwrap_or(d);
    match d {
        [0x7F, _, 0x04, 0x05, 0x01, 0x01, 0x01, 0x01, slot @ (1 | 2), pp, vv, ..] => Some((*slot, *pp, *vv)),
        _ => None,
    }
}

/// Where a value sits in the song: (track, event, data index).
type Writer = (usize, usize, usize);

fn sysex_order(tracks: &[EditTrack]) -> Vec<(u32, usize, usize)> {
    let mut v: Vec<(u32, usize, usize)> = Vec::new();
    for (ti, t) in tracks.iter().enumerate() {
        for (ei, e) in t.others.iter().enumerate() {
            if let Event::Sysex { t, status: 0xF0, .. } = e {
                v.push((*t, ti, ei));
            }
        }
    }
    v.sort();
    v
}

/// The GS effect values the song sets (last one wins; a reset clears them).
fn gs_writers(tracks: &[EditTrack]) -> HashMap<[u8; 3], Writer> {
    let mut w = HashMap::new();
    for (_, ti, ei) in sysex_order(tracks) {
        let Event::Sysex { data, .. } = &tracks[ti].others[ei] else { continue };
        if is_reset(data) || is_gs_reset(data) {
            w.clear();
        } else if let Some((a, v)) = parse_gs(data) {
            for i in 0..v.len() {
                let lo = a[2] as usize + i;
                w.insert([a[0], a[1], lo as u8], (ti, ei, 7 + i));
            }
        }
    }
    w
}

/// GS Reset (`40 00 7F 00`).
pub fn is_gs_reset(d: &[u8]) -> bool {
    parse_gs(d).is_some_and(|(a, _)| a == [0x40, 0x00, 0x7F])
}

pub fn gs_values(tracks: &[EditTrack]) -> HashMap<[u8; 3], u8> {
    gs_writers(tracks)
        .into_iter()
        .filter_map(|(a, (ti, ei, k))| match &tracks[ti].others[ei] {
            Event::Sysex { data, .. } => data.get(k).map(|&v| (a, v)),
            _ => None,
        })
        .collect()
}

fn gm2_writers(tracks: &[EditTrack]) -> HashMap<(u8, u8), Writer> {
    let mut w = HashMap::new();
    for (_, ti, ei) in sysex_order(tracks) {
        let Event::Sysex { data, .. } = &tracks[ti].others[ei] else { continue };
        if is_reset(data) || is_gs_reset(data) {
            w.clear();
        } else if let Some((slot, pp, _)) = parse_gm2(data) {
            w.insert((slot, pp), (ti, ei, 10));
        }
    }
    w
}

pub fn gm2_values(tracks: &[EditTrack]) -> HashMap<(u8, u8), u8> {
    gm2_writers(tracks)
        .into_iter()
        .filter_map(|(k, (ti, ei, i))| match &tracks[ti].others[ei] {
            Event::Sysex { data, .. } => data.get(i).map(|&v| (k, v)),
            _ => None,
        })
        .collect()
}

/// Insert a setup SysEx (without F0) into the first track after the song's
/// last reset (System On / GS Reset), or at its start.
fn insert_setup(tracks: &mut [EditTrack], body: Vec<u8>) {
    let reset = tracks
        .iter()
        .flat_map(|t| t.others.iter())
        .filter_map(|e| match e {
            Event::Sysex { t, data, .. } if is_reset(data) || is_gs_reset(data) => Some(*t),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    let Some(t0) = tracks.first_mut() else { return };
    let pos = t0.others.iter().position(|e| e.t() > reset).unwrap_or(t0.others.len());
    t0.others.insert(pos, Event::Sysex { t: reset, status: 0xF0, data: body });
}

/// Set a GS value in the song: the song's own message (checksum redone), else a new one.
pub fn set_gs(tracks: &mut [EditTrack], dev: u8, addr: [u8; 3], data: &[u8]) {
    let w = gs_writers(tracks);
    let mut rest = Vec::new();
    for (i, &v) in data.iter().enumerate() {
        let a = [addr[0], addr[1], addr[2] + i as u8];
        match w.get(&a) {
            Some(&(ti, ei, k)) => {
                if let Event::Sysex { data: d, .. } = &mut tracks[ti].others[ei] {
                    d[k] = v & 0x7F;
                    if let Some(end) = d.iter().position(|&b| b == 0xF7) {
                        let sum: u32 = d[4..end - 1].iter().map(|&b| b as u32).sum();
                        d[end - 1] = ((128 - sum % 128) % 128) as u8;
                    }
                }
            }
            None => rest.push((a, v)),
        }
    }
    for (a, v) in rest {
        insert_setup(tracks, gs_dt1(dev, a, &[v])[1..].to_vec());
    }
}

pub fn set_gm2(tracks: &mut [EditTrack], dev: u8, slot: u8, param: u8, v: u8) {
    match gm2_writers(tracks).get(&(slot, param)) {
        Some(&(ti, ei, k)) => {
            if let Event::Sysex { data, .. } = &mut tracks[ti].others[ei] {
                data[k] = v & 0x7F;
            }
        }
        None => insert_setup(tracks, gm2_global(dev, slot, param, v)[1..].to_vec()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gs_message_and_checksum() {
        // the classic "reverb macro Hall 2": F0 41 10 42 12 40 01 30 04 0B F7
        assert_eq!(gs_dt1(0, [0x40, 0x01, 0x30], &[4]), vec![0xF0, 0x41, 0x10, 0x42, 0x12, 0x40, 0x01, 0x30, 0x04, 0x0B, 0xF7]);
        assert_eq!(gs_part_efx(9), [0x40, 0x40, 0x22]);
        assert_eq!(gs_part_efx(0), [0x40, 0x41, 0x22]);
        assert_eq!(gs_part_efx(10), [0x40, 0x4A, 0x22]);
    }

    #[test]
    fn gs_values_in_the_song() {
        let reset = gs_dt1(0, [0x40, 0x00, 0x7F], &[0])[1..].to_vec();
        let mut song = vec![EditTrack { others: vec![Event::Sysex { t: 0, status: 0xF0, data: reset }], ..Default::default() }];
        set_gs(&mut song, 0, [0x40, 0x01, 0x30], &[5]);
        assert_eq!(song[0].others.len(), 2);
        assert_eq!(gs_values(&song).get(&[0x40, 0x01, 0x30]), Some(&5));
        set_gs(&mut song, 0, [0x40, 0x01, 0x30], &[1]);
        assert_eq!(song[0].others.len(), 2, "changed in place");
        let Event::Sysex { data, .. } = &song[0].others[1] else { panic!() };
        assert_eq!(data, &gs_dt1(0, [0x40, 0x01, 0x30], &[1])[1..].to_vec(), "checksum redone");
        // GM2
        set_gm2(&mut song, 0x7F, GM2_REVERB, 0, 8);
        set_gm2(&mut song, 0x7F, GM2_REVERB, 0, 3);
        assert_eq!(gm2_values(&song).get(&(GM2_REVERB, 0)), Some(&3));
        assert_eq!(song[0].others.len(), 3);
    }
}
