//! Guitar Arranger (GtrAr.dll, GtrAr.hlp): guitar / bass groups of string
//! tracks, tunings, frets, the 6-string split and the chord window's chord
//! files.
//!
//! A group is one track per string. Each string track's InstType (S4WT header
//! +7..+10, see `EditTrack::inst_type`) is, as written by the Setup dialog
//! (GtrAr.dll 0x1004c7e9): kind 5 = Gtr Tab (Multi Track) / 6 = Bass Tab,
//! group letter index (A = 0), `string << 4 | strings - 1`, open-string note.

use crate::{EditBlock, EditNote, EditTrack};

/// The "Tuning Variation" list (dialog init data of the Setup dialog 24001),
/// strings 1.. (high to low).
pub const TUNINGS: [(&str, &[u8]); 13] = [
    ("Standard", &[64, 59, 55, 50, 45, 40]),
    ("Open D", &[62, 57, 54, 50, 45, 38]),
    ("Open E", &[64, 59, 56, 52, 47, 40]),
    ("Open G", &[62, 59, 55, 50, 43, 38]),
    ("Open A", &[64, 61, 57, 52, 45, 40]),
    ("Open B", &[63, 59, 54, 51, 47, 42]),
    ("Drop D", &[64, 59, 55, 50, 45, 38]),
    ("DADGAD", &[62, 57, 55, 50, 45, 38]),
    ("Bass 4", &[43, 38, 33, 28]),
    ("Bass high C", &[48, 43, 38, 33, 28]),
    ("Bass high B", &[47, 43, 38, 33, 28]),
    ("Bass low B", &[43, 38, 33, 28, 23]),
    ("Bass 6", &[48, 43, 38, 33, 28, 23]),
];

/// First bass tuning in `TUNINGS`.
pub const FIRST_BASS: usize = 8;

/// Frets 0..=24.
pub const MAX_FRET: u8 = 24;

/// XGworks note name: 60 = C3 ("E3(64)").
pub fn note_name(n: u8) -> String {
    const N: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    format!("{}{}", N[n as usize % 12], n as i32 / 12 - 2)
}

/// Fret of `pitch` on a string tuned to `open` (None = not playable, shown "?").
pub fn fret(pitch: u8, open: u8) -> Option<u8> {
    pitch.checked_sub(open).filter(|&f| f <= MAX_FRET)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub bass: bool,
    /// Group letter index (A = 0 .. H = 7).
    pub letter: u8,
    /// Per string (1..): (track index, open-string note).
    pub strings: Vec<(usize, u8)>,
}

impl Group {
    pub fn name(&self) -> String {
        format!("{}{}", if self.bass { "Bass" } else { "Guitar" }, (b'A' + self.letter) as char)
    }
    pub fn tuning(&self) -> Vec<u8> {
        self.strings.iter().map(|s| s.1).collect()
    }
}

/// Decode a track's group membership: (bass, letter, string index, strings, open note).
pub fn membership(t: &EditTrack) -> Option<(bool, u8, usize, usize, u8)> {
    let [kind, letter, sub, open] = t.inst_type().to_be_bytes();
    matches!(kind, 5 | 6).then(|| (kind == 6, letter & 7, (sub >> 4) as usize, (sub & 0x0F) as usize + 1, open))
}

/// All guitar / bass groups of the song (Guitar first, then by letter).
pub fn groups(tracks: &[EditTrack]) -> Vec<Group> {
    let mut out: Vec<Group> = Vec::new();
    for (ti, t) in tracks.iter().enumerate() {
        let Some((bass, letter, s, n, open)) = membership(t) else { continue };
        let g = match out.iter_mut().position(|g| g.bass == bass && g.letter == letter) {
            Some(i) => &mut out[i],
            None => {
                out.push(Group { bass, letter, strings: vec![(usize::MAX, 0); n.clamp(1, 6)] });
                out.last_mut().unwrap()
            }
        };
        if s < g.strings.len() && g.strings[s].0 == usize::MAX {
            g.strings[s] = (ti, open);
        }
    }
    for g in &mut out {
        g.strings.retain(|s| s.0 != usize::MAX);
    }
    out.sort_by_key(|g| (g.bass, g.letter));
    out
}

/// The group a track belongs to.
pub fn group_of(tracks: &[EditTrack], ti: usize) -> Option<Group> {
    groups(tracks).into_iter().find(|g| g.strings.iter().any(|s| s.0 == ti))
}

/// The first free group letter (GuitarA..H / BassA..H).
pub fn free_letter(tracks: &[EditTrack], bass: bool) -> Option<u8> {
    let used: Vec<u8> = groups(tracks).iter().filter(|g| g.bass == bass).map(|g| g.letter).collect();
    (0..8).find(|l| !used.contains(l))
}

/// Make `strings` (track, channel 0..15, open note) a group: InstType and
/// channel set; several blocks of a string track are merged into one
/// (GtrAr.hlp "Grouping dialog").
pub fn set_group(tracks: &mut [EditTrack], strings: &[(usize, u8, u8)], bass: bool, letter: u8) {
    let n = strings.len().clamp(1, 6) as u32;
    for (s, &(ti, ch, open)) in strings.iter().enumerate() {
        let Some(t) = tracks.get_mut(ti) else { continue };
        let v = (if bass { 6u32 } else { 5 }) << 24 | (letter as u32 & 7) << 16 | ((s as u32) << 4 | (n - 1)) << 8 | open.min(127) as u32;
        t.set_inst_type(v);
        t.channel = ch & 0x0F;
        merge_blocks(t);
    }
}

/// Merge a track's blocks into one (named after the first).
pub fn merge_blocks(t: &mut EditTrack) {
    if t.blocks.len() > 1 {
        let start = t.blocks.iter().map(|b| b.start).min().unwrap_or(0);
        let end = t.blocks.iter().map(|b| b.end).max().unwrap_or(start);
        let first = t.blocks[0].clone();
        t.blocks = vec![EditBlock { start, end, ..first }];
    }
}

/// Indices of tracks that are empty (no events, no name) — the Split dialog
/// offers them as string tracks.
pub fn empty_tracks(tracks: &[EditTrack]) -> Vec<usize> {
    tracks
        .iter()
        .enumerate()
        .filter(|(_, t)| !t.conductor && t.notes.is_empty() && t.others.is_empty() && t.blocks.is_empty() && membership(t).is_none())
        .map(|(i, _)| i)
        .collect()
}

// ---------------------------------------------------------------------------
// chord files (.chd: lo.chd, hi1.chd, hi2.chd, custom)
// ---------------------------------------------------------------------------

/// Muted string in a chord file.
pub const MUTE: u8 = 128;

/// Chord positions: [root 0..11 (C..B)][type 0..34 (XF order)] → notes of
/// strings 1..6 (`MUTE` = not played).
pub type ChordFile = Vec<Vec<[u8; 6]>>;

/// Parse a chord file: INI `[RootN]` `TypeM=n1,n2,n3,n4,n5,n6,`.
pub fn parse_chd(text: &str) -> ChordFile {
    let mut out = vec![vec![[MUTE; 6]; 35]; 12];
    let mut root = None;
    for l in text.lines() {
        let l = l.trim();
        if let Some(r) = l.strip_prefix("[Root").and_then(|r| r.strip_suffix(']')) {
            root = r.parse::<usize>().ok().filter(|&r| r < 12);
        } else if let (Some(r), Some((k, v))) = (root, l.split_once('=')) {
            if let Some(ty) = k.trim().strip_prefix("Type").and_then(|x| x.parse::<usize>().ok()).filter(|&t| t < 35) {
                let mut s = [MUTE; 6];
                for (i, n) in v.split(',').filter(|x| !x.trim().is_empty()).take(6).enumerate() {
                    s[i] = n.trim().parse().unwrap_or(MUTE);
                }
                out[r][ty] = s;
            }
        }
    }
    out
}

pub fn write_chd(c: &ChordFile) -> String {
    let mut s = String::new();
    for (r, types) in c.iter().enumerate() {
        s += &format!("[Root{r}]\r\n");
        for (t, n) in types.iter().enumerate() {
            s += &format!("Type{t}=");
            for v in n {
                s += &format!("{v},");
            }
            s += "\r\n";
        }
    }
    s
}

/// A chord's notes for the strings of `tuning`: the file's notes are for
/// standard tuning; another tuning keeps each string's fret position moved to
/// sound the same pitch where possible (GtrAr.hlp: "shown at the fret
/// positions of that tuning").
pub fn chord_notes(file_notes: &[u8; 6], tuning: &[u8]) -> Vec<Option<u8>> {
    tuning
        .iter()
        .enumerate()
        .map(|(s, &open)| {
            let n = *file_notes.get(s)?;
            (n != MUTE && n >= open && n - open <= MAX_FRET).then_some(n)
        })
        .collect()
}

/// Notes of one string between `from` and `to` (for tests / the tab view).
pub fn notes_in(t: &EditTrack, from: u32, to: u32) -> impl Iterator<Item = &EditNote> {
    t.notes.iter().filter(move |n| n.tick >= from && n.tick < to)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_and_chords() {
        let mut tr: Vec<EditTrack> = (0..8).map(|_| EditTrack::default()).collect();
        let strings: Vec<(usize, u8, u8)> = (0..6).map(|s| (s + 1, s as u8, TUNINGS[0].1[s])).collect();
        set_group(&mut tr, &strings, false, 0);
        assert_eq!(tr[1].inst_type(), 0x0500_0540);
        assert_eq!(tr[6].inst_type(), 0x0500_5528);
        let g = groups(&tr);
        assert_eq!(g.len(), 1);
        assert_eq!(g[0].name(), "GuitarA");
        assert_eq!(g[0].tuning(), [64, 59, 55, 50, 45, 40]);
        assert_eq!(free_letter(&tr, false), Some(1));
        assert_eq!(note_name(64), "E3");
        assert_eq!(fret(42, 40), Some(2));
        let c = parse_chd("[Root0]\r\nType0=64,60,55,52,48,128,\r\n");
        assert_eq!(c[0][0], [64, 60, 55, 52, 48, 128]);
        assert_eq!(parse_chd(&write_chd(&c)), c);
    }
}
