//! The Importer plug-in (Importer.dll, Importer.hlp): part or all of another
//! song goes into the open one. Checked against the original: the imported
//! tracks are appended as NEW tracks (name, channel and type kept), placed at
//! the chosen location.
//!
//! - Import type "All" takes the whole song, "Marker" the span from a marker
//!   up to the next one.
//! - An XWS source gives the blocks that START in the span (whole, not cut);
//!   an SMF / XF source the events starting in it (notes not cut).
//! - Marker Offset cuts that many clocks off the head of the span.
//! - Master track events (Marker, Tempo, Time Signature, XF Chord Name) in the
//!   span come along when chosen; the song's own ones stay.

use crate::{EditBlock, EditTrack, Event};

/// The song's markers (FF 06) in time order: (tick, text).
pub fn markers(tracks: &[EditTrack]) -> Vec<(u32, String)> {
    let mut m: Vec<(u32, String)> = tracks
        .iter()
        .flat_map(|t| t.others.iter())
        .filter_map(|e| match e {
            Event::Meta { t, mtype: 0x06, data } => Some((*t, data.iter().map(|&b| b as char).collect::<String>().trim().to_string())),
            _ => None,
        })
        .collect();
    m.sort_by_key(|x| x.0);
    m
}

#[derive(Clone, Copy, Debug)]
pub struct ImportOpts {
    /// Source span [from, to) in source ticks; None = the whole song.
    pub span: Option<(u32, u32)>,
    /// Marker Offset: clocks cut off the head of the span.
    pub offset: u32,
    /// Destination tick (Meas / Beat / Clock).
    pub at: u32,
    /// Master track events: marker, tempo, time signature, XF chord name.
    pub master: [bool; 4],
}

fn block_owner(blocks: &[EditBlock], t: u32) -> usize {
    blocks.iter().rposition(|b| b.start <= t).unwrap_or(0)
}

fn is_master(e: &Event, which: [bool; 4]) -> bool {
    match e {
        Event::Meta { mtype: 0x06, .. } => which[0],
        Event::Meta { mtype: 0x51, .. } => which[1],
        Event::Meta { mtype: 0x58, .. } => which[2],
        Event::Meta { mtype: 0x7F, data, .. } => which[3] && data.starts_with(&[0x43, 0x7B, 0x01]),
        _ => false,
    }
}

/// Import `src` (at `src_ppqn`) into `dest` (at `dst_ppqn`); returns the
/// number of tracks added.
pub fn import(dest: &mut Vec<EditTrack>, src: &[EditTrack], src_ppqn: u16, dst_ppqn: u16, o: &ImportOpts) -> usize {
    let (from, to) = o.span.unwrap_or((0, u32::MAX));
    let head = from.saturating_add(o.offset);
    let scale = |t: u32| (t as u64 * dst_ppqn.max(1) as u64 / src_ppqn.max(1) as u64) as u32;
    // source tick → destination tick (before the head: clamped to the location)
    let place = |t: u32| o.at + scale(t.saturating_sub(head));
    let mut added = 0;
    for t in src.iter().filter(|t| !t.conductor) {
        let mut nt = EditTrack { name: t.name.clone(), channel: t.channel, header: t.header.clone(), ..Default::default() };
        if !t.blocks.is_empty() {
            // XWS: whole blocks that start in the span
            let picked: Vec<usize> = (0..t.blocks.len()).filter(|&i| t.blocks[i].start >= from && t.blocks[i].start < to).collect();
            if picked.is_empty() {
                continue;
            }
            for &bi in &picked {
                let b = &t.blocks[bi];
                nt.blocks.push(EditBlock { name: b.name.clone(), start: place(b.start), end: place(b.end.max(b.start)), raw: Vec::new() });
            }
            for n in t.notes.iter().filter(|n| picked.contains(&block_owner(&t.blocks, n.tick))) {
                nt.notes.push(crate::EditNote { tick: place(n.tick), dur: scale(n.dur).max(1), ..n.clone() });
            }
            for e in t.others.iter().filter(|e| picked.contains(&block_owner(&t.blocks, e.t()))) {
                nt.others.push(crate::listev::with_tick(e, place(e.t())));
            }
        } else {
            // SMF / XF: the events starting in the span (from the head on)
            for n in t.notes.iter().filter(|n| n.tick >= head && n.tick < to) {
                nt.notes.push(crate::EditNote { tick: place(n.tick), dur: scale(n.dur).max(1), ..n.clone() });
            }
            for e in t.others.iter().filter(|e| e.t() >= head && e.t() < to && !matches!(e, Event::Meta { mtype: 0x06 | 0x51 | 0x58 | 0x2F, .. })) {
                nt.others.push(crate::listev::with_tick(e, place(e.t())));
            }
            if nt.notes.is_empty() && nt.others.is_empty() {
                continue;
            }
        }
        nt.notes.sort_by_key(|n| n.tick);
        nt.others.sort_by_key(|e| e.t());
        dest.push(nt);
        added += 1;
    }
    // master track events in the span
    if o.master.iter().any(|&m| m) {
        let evs: Vec<Event> = src
            .iter()
            .flat_map(|t| t.others.iter())
            .filter(|e| e.t() >= head && e.t() < to && is_master(e, o.master))
            .map(|e| crate::listev::with_tick(e, place(e.t())))
            .collect();
        if !evs.is_empty() {
            let ci = match dest.iter().position(|t| t.conductor) {
                Some(i) => i,
                None => {
                    dest.insert(0, EditTrack { conductor: true, ..Default::default() });
                    0
                }
            };
            dest[ci].others.extend(evs);
            dest[ci].others.sort_by_key(|e| e.t());
        }
    }
    added
}

/// AutoPlay song list (.ATP, `[Juke Box List]`) / Importer file list (.IMP,
/// `[File List]`): INI with keys 0..127, empty values after the list.
pub fn write_list(section: &str, files: &[String]) -> String {
    let mut s = format!("[{section}]\r\n");
    for i in 0..128 {
        s += &format!("{i}={}\r\n", files.get(i).map_or("", |f| f.as_str()));
    }
    s
}

pub fn read_list(text: &str) -> Vec<String> {
    let mut v: Vec<(usize, String)> = text
        .lines()
        .filter_map(|l| {
            let (k, val) = l.split_once('=')?;
            Some((k.trim().parse().ok()?, val.trim().to_string()))
        })
        .filter(|(_, v)| !v.is_empty())
        .collect();
    v.sort_by_key(|x| x.0);
    v.into_iter().map(|x| x.1).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::EditNote;

    #[test]
    fn imports_blocks_of_a_marker_span_as_new_tracks() {
        let src = vec![
            EditTrack { conductor: true, others: vec![
                Event::Meta { t: 1920, mtype: 0x06, data: b"Intro".to_vec() },
                Event::Meta { t: 3840, mtype: 0x06, data: b"A".to_vec() },
            ], ..Default::default() },
            EditTrack {
                name: "Bass".into(),
                channel: 0,
                blocks: vec![
                    EditBlock { name: "Setup".into(), start: 0, end: 1920, raw: vec![] },
                    EditBlock { name: "Bass".into(), start: 1920, end: 9600, raw: vec![] },
                ],
                notes: vec![EditNote { tick: 100, dur: 10, pitch: 40, vel: 1 }, EditNote { tick: 2000, dur: 10, pitch: 41, vel: 1 }, EditNote { tick: 9000, dur: 10, pitch: 42, vel: 1 }],
                ..Default::default()
            },
        ];
        let mut dest = vec![EditTrack { name: "x".into(), ..Default::default() }];
        let m = markers(&src);
        let o = ImportOpts { span: Some((m[0].0, m[1].0)), offset: 0, at: 19 * 1920, master: [true, false, false, false] };
        assert_eq!(import(&mut dest, &src, 480, 480, &o), 1);
        let t = dest.last().unwrap();
        assert_eq!(t.name, "Bass");
        assert_eq!(t.blocks.len(), 1); // only the block starting in Intro, whole
        assert_eq!(t.notes.iter().map(|n| n.tick).collect::<Vec<_>>(), [19 * 1920 + 80, 19 * 1920 + 7080]);
        assert!(dest.iter().any(|t| t.conductor && t.others.iter().any(|e| e.t() == 19 * 1920)));
        assert_eq!(read_list(&write_list("Juke Box List", &["a.mid".into(), "b.xws".into()])), ["a.mid", "b.xws"]);
    }
}
