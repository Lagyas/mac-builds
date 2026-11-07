//! Demix (not in XGworks; as XGW Next's Track menu): split tracks into one
//! track per MIDI channel, or into one track per note (drum instrument).
//!
//! A track's own notes are on its channel; a Standard MIDI File track may
//! also hold notes of other channels (`Event::Note` in `others`) and channel
//! messages of any channel. Meta and SysEx events never move.

use crate::{EditNote, EditTrack, Event, TRACK_POOL};

/// GM drum names (notes 35..=81), for drum tracks without a kit name.
pub const GM_DRUMS: [&str; 47] = [
    "Acoustic Bass Drum", "Bass Drum 1", "Side Stick", "Acoustic Snare", "Hand Clap", "Electric Snare",
    "Low Floor Tom", "Closed Hi-Hat", "High Floor Tom", "Pedal Hi-Hat", "Low Tom", "Open Hi-Hat",
    "Low-Mid Tom", "Hi-Mid Tom", "Crash Cymbal 1", "High Tom", "Ride Cymbal 1", "Chinese Cymbal",
    "Ride Bell", "Tambourine", "Splash Cymbal", "Cowbell", "Crash Cymbal 2", "Vibraslap",
    "Ride Cymbal 2", "Hi Bongo", "Low Bongo", "Mute Hi Conga", "Open Hi Conga", "Low Conga",
    "High Timbale", "Low Timbale", "High Agogo", "Low Agogo", "Cabasa", "Maracas", "Short Whistle",
    "Long Whistle", "Short Guiro", "Long Guiro", "Claves", "Hi Wood Block", "Low Wood Block",
    "Mute Cuica", "Open Cuica", "Mute Triangle", "Open Triangle",
];

pub fn gm_drum(note: u8) -> Option<&'static str> {
    GM_DRUMS.get((note as usize).checked_sub(35)?).copied()
}

/// Channel of a channel event (`None` for meta / SysEx).
fn ev_channel(e: &Event) -> Option<u8> {
    match e {
        Event::Note { ch, .. } => Some(ch & 0x0F),
        Event::Chan { data, .. } => data.first().filter(|&&s| (0x80..=0xEF).contains(&s)).map(|s| s & 0x0F),
        _ => None,
    }
}

/// Every note of a track (its own and `Event::Note`s in `others`) with its channel.
fn all_notes(t: &EditTrack) -> Vec<(u8, EditNote)> {
    let mut v: Vec<(u8, EditNote)> = t.notes.iter().map(|n| (t.channel & 0x0F, n.clone())).collect();
    v.extend(t.others.iter().filter_map(|e| match e {
        Event::Note { t, ch, note, vel, gate } => Some((ch & 0x0F, EditNote { tick: *t, dur: *gate, pitch: *note, vel: *vel })),
        _ => None,
    }));
    v
}

/// The channels a track uses (notes and channel messages), ascending.
pub fn channels_used(t: &EditTrack) -> Vec<u8> {
    let mut v: Vec<u8> = all_notes(t).iter().map(|(c, _)| *c).chain(t.others.iter().filter_map(ev_channel)).collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// A demix result: the new track list, for each new track the index of the
/// track it came from, and how many tracks were split / created.
pub struct Demixed {
    pub tracks: Vec<EditTrack>,
    pub origin: Vec<usize>,
    pub count: usize,
}

/// A new track like `src` (header, port, blocks) on channel `ch`, no events.
fn derived(src: &EditTrack, name: String, ch: u8) -> EditTrack {
    let mut t = EditTrack {
        name,
        channel: ch,
        notes: Vec::new(),
        others: Vec::new(),
        blocks: src.blocks.clone(),
        header: src.header.clone(),
        conductor: false,
    };
    if ch == 9 {
        t.set_transpose(None); // drums are not transposed
    }
    t
}

fn finish(tracks: Vec<EditTrack>, origin: Vec<usize>, count: usize) -> Result<Demixed, String> {
    // the S4WT pool holds the conductor track and 100 tracks
    if tracks.len() > TRACK_POOL {
        return Err(format!("The result would have {} tracks; XGworks allows {}.", tracks.len() - 1, TRACK_POOL - 1));
    }
    Ok(Demixed { tracks, origin, count })
}

/// Split each track in `which` that uses more than one channel into a track
/// of its meta / SysEx events (if any, keeping name and channel) and one
/// "name [ch N]" track per channel, right after it; `count` = tracks split.
pub fn by_channel(tracks: &[EditTrack], which: &[usize]) -> Result<Demixed, String> {
    let mut out = Vec::with_capacity(tracks.len());
    let mut origin = Vec::with_capacity(tracks.len());
    let mut split = 0;
    for (ti, src) in tracks.iter().enumerate() {
        let chans = channels_used(src);
        if src.conductor || !which.contains(&ti) || chans.len() <= 1 {
            out.push(src.clone());
            origin.push(ti);
            continue;
        }
        split += 1;
        let common: Vec<Event> = src.others.iter().filter(|e| ev_channel(e).is_none()).cloned().collect();
        if !common.is_empty() {
            let mut t = derived(src, src.name.clone(), src.channel);
            t.header = src.header.clone();
            t.others = common;
            out.push(t);
            origin.push(ti);
        }
        let notes = all_notes(src);
        let base = if src.name.is_empty() { format!("Track{ti}") } else { src.name.clone() };
        for &c in &chans {
            let mut t = derived(src, format!("{base} [ch {}]", c + 1), c);
            t.notes = notes.iter().filter(|(nc, _)| *nc == c).map(|(_, n)| n.clone()).collect();
            t.others = src.others.iter().filter(|e| !matches!(e, Event::Note { .. }) && ev_channel(e) == Some(c)).cloned().collect();
            out.push(t);
            origin.push(ti);
        }
    }
    finish(out, origin, split)
}

/// Split the notes of `channels` (bit k = MIDI channel k+1) on each track in
/// `which` into one track per (channel, note), right after the track, which
/// keeps everything else (it goes away if nothing is left). Polyphonic
/// aftertouch moves with its note. `label(track, ch, note)` names a drum
/// instrument; tracks are named "CH 10 - Acoustic Snare (38)" or
/// "CH 1 - Note C3 (60)"; `count` = note tracks created.
pub fn by_note(
    tracks: &[EditTrack],
    which: &[usize],
    channels: u16,
    label: impl Fn(usize, u8, u8) -> Option<String>,
) -> Result<Demixed, String> {
    let on = |c: u8| channels & (1 << c) != 0;
    let mut out = Vec::with_capacity(tracks.len());
    let mut origin = Vec::with_capacity(tracks.len());
    let mut created = 0;
    for (ti, src) in tracks.iter().enumerate() {
        let notes = all_notes(src);
        let mut keys: Vec<(u8, u8)> = notes.iter().filter(|(c, _)| on(*c)).map(|(c, n)| (*c, n.pitch)).collect();
        keys.sort_unstable();
        keys.dedup();
        if src.conductor || !which.contains(&ti) || keys.is_empty() {
            out.push(src.clone());
            origin.push(ti);
            continue;
        }
        let poly_at = |e: &Event| match e {
            Event::Chan { data, .. } if data.len() >= 2 && data[0] & 0xF0 == 0xA0 => Some((data[0] & 0x0F, data[1])),
            _ => None,
        };
        let moved = |e: &Event| match e {
            Event::Note { ch, .. } => on(ch & 0x0F),
            _ => poly_at(e).is_some_and(|(c, _)| on(c)),
        };
        let mut rest = src.clone();
        if on(src.channel & 0x0F) {
            rest.notes.clear();
        }
        rest.others.retain(|e| !moved(e));
        if !rest.notes.is_empty() || !rest.others.is_empty() {
            out.push(rest);
            origin.push(ti);
        }
        let drum = src.channel & 0x0F == 9 || src.inst_type() >> 24 == 2;
        for &(c, p) in &keys {
            let name = match label(ti, c, p).or_else(|| (drum || c == 9).then(|| gm_drum(p).map(String::from)).flatten()) {
                Some(n) => format!("CH {} - {} ({p})", c + 1, n),
                None => format!("CH {} - Note {} ({p})", c + 1, note_name(p)),
            };
            let mut t = derived(src, name, c);
            t.notes = notes.iter().filter(|(nc, n)| *nc == c && n.pitch == p).map(|(_, n)| n.clone()).collect();
            t.others = src.others.iter().filter(|e| poly_at(e) == Some((c, p))).cloned().collect();
            out.push(t);
            origin.push(ti);
            created += 1;
        }
    }
    finish(out, origin, created)
}

/// "C3" for note 60, as in XGworks.
pub fn note_name(p: u8) -> String {
    const N: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    format!("{}{}", N[p as usize % 12], p as i32 / 12 - 2)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(t: u32, ch: u8, p: u8) -> Event {
        Event::Note { t, ch, note: p, vel: 100, gate: 120 }
    }

    /// A format-0 style track: notes on ch 1 and 10, CC on both, a tempo.
    fn mixed() -> EditTrack {
        EditTrack {
            name: "Song".into(),
            channel: 0,
            notes: vec![EditNote { tick: 0, dur: 120, pitch: 60, vel: 100 }],
            others: vec![
                Event::Meta { t: 0, mtype: 0x51, data: vec![7, 0xA1, 0x20] },
                Event::Chan { t: 0, data: vec![0xB0, 7, 100] },
                Event::Chan { t: 0, data: vec![0xB9, 7, 90] },
                note(0, 9, 36),
                note(240, 9, 38),
                note(480, 9, 36),
                Event::Chan { t: 10, data: vec![0xA9, 38, 50] },
            ],
            ..Default::default()
        }
    }

    #[test]
    fn split_by_channel() {
        let d = by_channel(&[mixed()], &[0]).unwrap();
        let (out, n) = (d.tracks, d.count);
        assert_eq!(d.origin, [0, 0, 0]);
        assert_eq!(n, 1);
        let names: Vec<&str> = out.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Song", "Song [ch 1]", "Song [ch 10]"]);
        assert_eq!(out[0].others.len(), 1); // the tempo only
        assert!(out[0].notes.is_empty());
        assert_eq!((out[1].channel, out[1].notes.len(), out[1].others.len()), (0, 1, 1));
        assert_eq!((out[2].channel, out[2].notes.len(), out[2].others.len()), (9, 3, 2));
        assert_eq!(out[2].transpose(), None);
        // a single-channel track is left alone
        let again = by_channel(&out, &[0, 1, 2]).unwrap();
        let (again, n2) = (again.tracks, again.count);
        assert_eq!((again.len(), n2), (3, 0));
    }

    #[test]
    fn split_by_note() {
        let d = by_note(&[mixed()], &[0], 1 << 9, |_, _, _| None).unwrap();
        let (out, n) = (d.tracks, d.count);
        assert_eq!(n, 2);
        let names: Vec<&str> = out.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Song", "CH 10 - Bass Drum 1 (36)", "CH 10 - Acoustic Snare (38)"]);
        // ch 1 note and both CCs stay; the snare's aftertouch moves with it
        assert_eq!(out[0].notes.len(), 1);
        assert_eq!(out[0].others.len(), 3);
        assert_eq!(out[1].notes.len(), 2);
        assert_eq!(out[2].others.len(), 1);
        // a kit name from the caller wins; a melodic note gets its note name
        let o2 = by_note(&[mixed()], &[0], 0xFFFF, |_, c, p| (c == 9 && p == 36).then(|| "BD Kick".into())).unwrap().tracks;
        let names: Vec<&str> = o2.iter().map(|t| t.name.as_str()).collect();
        assert_eq!(names, ["Song", "CH 1 - Note C3 (60)", "CH 10 - BD Kick (36)", "CH 10 - Acoustic Snare (38)"]);
    }

    #[test]
    fn pool_limit() {
        let t = EditTrack {
            notes: (0..128).map(|p| EditNote { tick: 0, dur: 1, pitch: p, vel: 1 }).collect(),
            ..Default::default()
        };
        assert!(by_note(&[t], &[0], 1, |_, _, _| None).is_err());
    }
}
