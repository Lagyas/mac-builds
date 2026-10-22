//! xgworks-core: portable codec for the Yamaha XGworks `.XWS` project format.
//!
//! A direct port of the reverse-engineered Python reference (`xws_dump.py`),
//! validated to round-trip against real files. Pure `std`, no dependencies, so
//! it compiles unchanged for native targets (ARM / macOS / Windows / Linux) and
//! for `wasm32-unknown-unknown` (the web). This is milestone M0 of the port:
//! the shared core on which the platform MIDI + UI layers sit.
//!
//! Format summary (see the porting spec):
//!   Container : [4-byte ASCII tag][4-byte length BIG-endian][payload], to EOF.
//!   S4WH      : file header (version, block count, PPQN).
//!   S4WB      : event stream, SMF-like but notes are NoteOn + 4-byte BE gate
//!               (duration); there are no NoteOff events.
//!   S4WT      : track record = 0x20 header, name, a primary 32-byte placement,
//!               then [part-name][32-byte placement]*. A placement carries a
//!               start tick (BE32) and the S4WB block index (BE32).
//!   S4ST      : trailer.

use std::collections::{HashMap, VecDeque};

pub mod mixer;
pub mod xgtbl;
pub mod importer;
pub mod jobs;
pub mod xfinfo;
pub mod gtrtab;
pub mod listev;
mod listnames_data;
pub mod xgunits;
mod xgunits_data;

pub const TRACK_POOL: usize = 101; // fixed S4WT pool the XGworks loader expects

// ---------------------------------------------------------------------------
// events
// ---------------------------------------------------------------------------
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// NoteOn with an explicit gate (duration in ticks); no separate NoteOff.
    Note { t: u32, ch: u8, note: u8, vel: u8, gate: u32 },
    /// Any channel message other than NoteOn/NoteOff (full status + data bytes).
    Chan { t: u32, data: Vec<u8> },
    Meta { t: u32, mtype: u8, data: Vec<u8> },
    Sysex { t: u32, status: u8, data: Vec<u8> },
}

impl Event {
    pub fn t(&self) -> u32 {
        match self {
            Event::Note { t, .. }
            | Event::Chan { t, .. }
            | Event::Meta { t, .. }
            | Event::Sysex { t, .. } => *t,
        }
    }
    pub fn shifted(&self, off: u32) -> Event {
        let mut e = self.clone();
        match &mut e {
            Event::Note { t, .. }
            | Event::Chan { t, .. }
            | Event::Meta { t, .. }
            | Event::Sysex { t, .. } => *t = t.wrapping_add(off),
        }
        e
    }
}

// ---------------------------------------------------------------------------
// low-level helpers
// ---------------------------------------------------------------------------
fn read_varlen(buf: &[u8], i: &mut usize) -> u32 {
    let mut v = 0u32;
    while *i < buf.len() {
        let b = buf[*i];
        *i += 1;
        v = (v << 7) | (b & 0x7F) as u32;
        if b & 0x80 == 0 {
            break;
        }
    }
    v
}

fn write_varlen(mut v: u32) -> Vec<u8> {
    let mut out = vec![(v & 0x7F) as u8];
    v >>= 7;
    while v != 0 {
        out.insert(0, ((v & 0x7F) as u8) | 0x80);
        v >>= 7;
    }
    out
}

fn be_u32(buf: &[u8], i: usize) -> u32 {
    u32::from_be_bytes([buf[i], buf[i + 1], buf[i + 2], buf[i + 3]])
}

fn be_u16(buf: &[u8], i: usize) -> u16 {
    u16::from_be_bytes([buf[i], buf[i + 1]])
}

fn read_cstring(buf: &[u8], i: &mut usize) -> String {
    if *i >= buf.len() {
        return String::new();
    }
    let n0 = buf[*i];
    let len = if n0 == 0xFF {
        let l = u16::from_le_bytes([buf[*i + 1], buf[*i + 2]]) as usize;
        *i += 3;
        l
    } else {
        *i += 1;
        n0 as usize
    };
    let end = (*i + len).min(buf.len());
    let s = String::from_utf8_lossy(&buf[*i..end]).into_owned();
    *i += len;
    s
}

fn mfc_cstring(s: &str) -> Vec<u8> {
    let b = s.as_bytes();
    if b.len() < 0xFF {
        let mut v = vec![b.len() as u8];
        v.extend_from_slice(b);
        v
    } else {
        let mut v = vec![0xFF];
        v.extend_from_slice(&(b.len() as u16).to_le_bytes());
        v.extend_from_slice(b);
        v
    }
}

// ---------------------------------------------------------------------------
// chunk container
// ---------------------------------------------------------------------------
pub struct Chunk<'a> {
    pub offset: usize,
    pub tag: [u8; 4],
    pub payload: &'a [u8],
}

pub fn iter_chunks(data: &[u8]) -> Vec<Chunk<'_>> {
    let mut out = Vec::new();
    let mut p = 0;
    while p + 8 <= data.len() {
        let tag = [data[p], data[p + 1], data[p + 2], data[p + 3]];
        let len = be_u32(data, p + 4) as usize;
        let end = (p + 8 + len).min(data.len());
        out.push(Chunk { offset: p, tag, payload: &data[p + 8..end] });
        p += 8 + len;
    }
    out
}

// ---------------------------------------------------------------------------
// S4WH header
// ---------------------------------------------------------------------------
#[derive(Clone, Debug, Default)]
pub struct Header {
    pub version_major: u8,
    pub version_minor: u8,
    pub track_count: u16,
    pub block_count: u32,
    pub ppqn: u16,
}

pub fn parse_s4wh(pay: &[u8]) -> Header {
    let mut h = Header::default();
    if pay.len() >= 10 {
        h.version_major = pay[0];
        h.version_minor = pay[1];
        h.track_count = be_u16(pay, 2);
        h.block_count = be_u32(pay, 4);
        h.ppqn = be_u16(pay, 8);
    }
    h
}

fn build_s4wh(nblocks: u32, ppqn: u16) -> Vec<u8> {
    let mut b = vec![0u8; 64];
    b[0] = 4; // version major (V4)
    b[2..4].copy_from_slice(&(TRACK_POOL as u16).to_be_bytes());
    b[4..8].copy_from_slice(&nblocks.to_be_bytes());
    b[8..10].copy_from_slice(&ppqn.to_be_bytes());
    b
}

// ---------------------------------------------------------------------------
// S4WB block: parse / encode
// ---------------------------------------------------------------------------
/// Parse one S4WB block into absolute-timed events. NoteOn carries a trailing
/// 4-byte BE gate; running status is honored.
pub fn parse_s4wb(buf: &[u8]) -> Vec<Event> {
    let mut ev = Vec::new();
    let mut i = 0usize;
    let mut t = 0u32;
    let mut running: Option<u8> = None;
    let n = buf.len();
    while i < n {
        let d = read_varlen(buf, &mut i);
        t = t.wrapping_add(d);
        if i >= n {
            break;
        }
        let status = buf[i];
        if status == 0xFF {
            i += 1;
            if i >= n {
                break;
            }
            let meta = buf[i];
            i += 1;
            let len = read_varlen(buf, &mut i) as usize;
            let end = (i + len).min(n);
            ev.push(Event::Meta { t, mtype: meta, data: buf[i..end].to_vec() });
            i += len;
            if meta == 0x2F {
                break;
            }
        } else if status == 0xF0 || status == 0xF7 {
            i += 1;
            let len = read_varlen(buf, &mut i) as usize;
            let end = (i + len).min(n);
            ev.push(Event::Sysex { t, status, data: buf[i..end].to_vec() });
            i += len;
        } else {
            let st = if status & 0x80 != 0 {
                running = Some(status);
                i += 1;
                status
            } else {
                match running {
                    Some(s) => s,
                    None => break,
                }
            };
            let evt = st & 0xF0;
            let ch = st & 0x0F;
            if evt == 0x90 {
                if i + 6 > n {
                    break;
                }
                let note = buf[i];
                let vel = buf[i + 1];
                i += 2;
                let gate = be_u32(buf, i);
                i += 4;
                ev.push(Event::Note { t, ch, note, vel, gate });
            } else if evt == 0xC0 || evt == 0xD0 {
                if i + 1 > n {
                    break;
                }
                ev.push(Event::Chan { t, data: vec![st, buf[i]] });
                i += 1;
            } else {
                if i + 2 > n {
                    break;
                }
                ev.push(Event::Chan { t, data: vec![st, buf[i], buf[i + 1]] });
                i += 2;
            }
        }
    }
    ev
}

/// Encode events into an S4WB block stream (notes as NoteOn + 4-byte BE gate;
/// no NoteOff, no EndOfTrack).
pub fn encode_s4wb(events: &[Event]) -> Vec<u8> {
    let mut items: Vec<(u32, u8, &Event)> = events
        .iter()
        .map(|e| (e.t(), if matches!(e, Event::Note { .. }) { 1 } else { 0 }, e))
        .collect();
    items.sort_by_key(|x| (x.0, x.1));
    let mut out = Vec::new();
    let mut last = 0u32;
    for (t, _, e) in &items {
        out.extend(write_varlen(t.wrapping_sub(last)));
        last = *t;
        match e {
            Event::Note { ch, note, vel, gate, .. } => {
                out.extend_from_slice(&[0x90 | ch, *note, *vel]);
                out.extend_from_slice(&gate.to_be_bytes());
            }
            Event::Chan { data, .. } => out.extend_from_slice(data),
            Event::Meta { mtype, data, .. } => {
                out.extend_from_slice(&[0xFF, *mtype]);
                out.extend(write_varlen(data.len() as u32));
                out.extend_from_slice(data);
            }
            Event::Sysex { status, data, .. } => {
                out.push(*status);
                out.extend(write_varlen(data.len() as u32));
                out.extend_from_slice(data);
            }
        }
    }
    out
}

/// Turn parsed events into a standard SMF MTrk payload (delta-encoded, notes
/// split into NoteOn + NoteOff(vel 0), terminated by EndOfTrack).
pub fn block_to_mtrk(events: &[Event]) -> Vec<u8> {
    let mut items: Vec<(u32, u8, Vec<u8>)> = Vec::new();
    for e in events {
        match e {
            Event::Note { t, ch, note, vel, gate } => {
                let st = 0x90 | ch;
                items.push((*t, 2, vec![st, *note, *vel]));
                items.push((t.wrapping_add(*gate), 4, vec![st, *note, 0]));
            }
            Event::Chan { t, data } => items.push((*t, 1, data.clone())),
            Event::Meta { t, mtype, data } => {
                let mut v = vec![0xFF, *mtype];
                v.extend(write_varlen(data.len() as u32));
                v.extend_from_slice(data);
                items.push((*t, 0, v));
            }
            Event::Sysex { t, status, data } => {
                let mut v = vec![*status];
                v.extend(write_varlen(data.len() as u32));
                v.extend_from_slice(data);
                items.push((*t, 1, v));
            }
        }
    }
    items.sort_by_key(|x| (x.0, x.1));
    let mut out = Vec::new();
    let mut last = 0u32;
    for (t, _, raw) in &items {
        out.extend(write_varlen(t.wrapping_sub(last)));
        out.extend_from_slice(raw);
        last = *t;
    }
    out.extend(write_varlen(0));
    out.extend_from_slice(&[0xFF, 0x2F, 0x00]); // EndOfTrack
    out
}

// ---------------------------------------------------------------------------
// S4WT track record: parse / build
// ---------------------------------------------------------------------------
/// A placement: a block index arranged at a start tick on a track.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placement {
    pub block: u32,
    pub start: u32,
}

/// Decode a used S4WT record into (track name, placements). Returns `None` for
/// an empty (unused) slot.
pub fn parse_s4wt(pay: &[u8]) -> Option<(String, Vec<Placement>)> {
    if pay.is_empty() || pay[0] == 0 {
        return None;
    }
    let rec = |j: usize| -> Placement {
        Placement { block: be_u32(pay, j + 10), start: be_u32(pay, j + 2) }
    };
    let mut i = 0x20usize;
    let name = read_cstring(pay, &mut i);
    let mut pls = Vec::new();
    if i + 14 <= pay.len() {
        pls.push(rec(i)); // primary (track-level) placement
    }
    i += 32;
    while i + 14 <= pay.len() {
        let mut j = i;
        let part = read_cstring(pay, &mut j);
        if part.is_empty() || j + 14 > pay.len() {
            break;
        }
        pls.push(rec(j));
        i = j + 32;
    }
    Some((name, pls))
}

fn build_s4wt(name: &str, channel: u8, block_index: u32, end_tick: u32, used: bool) -> Vec<u8> {
    if !used {
        return vec![0u8; 33]; // empty slot
    }
    let mut hdr = vec![0u8; 0x20];
    hdr[0] = 1; // used
    hdr[2] = channel & 0x0F;
    hdr[6] = 1;
    let mut pl = vec![0u8; 32];
    pl[0] = 1; // "01 00" prefix
    pl[2..6].copy_from_slice(&0u32.to_be_bytes()); // start
    pl[6..10].copy_from_slice(&end_tick.to_be_bytes()); // end
    pl[10..14].copy_from_slice(&block_index.to_be_bytes());
    let mut out = hdr;
    out.extend(mfc_cstring(name));
    out.extend(pl);
    out
}

// ---------------------------------------------------------------------------
// SMF export (.XWS -> .MID), faithful named 18-track layout
// ---------------------------------------------------------------------------
fn build_smf(tracks: &[Vec<u8>], division: u16) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(b"MThd");
    out.extend_from_slice(&6u32.to_be_bytes());
    out.extend_from_slice(&1u16.to_be_bytes()); // format 1
    out.extend_from_slice(&(tracks.len() as u16).to_be_bytes());
    out.extend_from_slice(&division.to_be_bytes());
    for tr in tracks {
        out.extend_from_slice(b"MTrk");
        out.extend_from_slice(&(tr.len() as u32).to_be_bytes());
        out.extend_from_slice(tr);
    }
    out
}

/// Reconstruct the original named tracks from an .XWS: one MTrk per used S4WT,
/// arranging each track's blocks at their placement ticks.
pub fn export_named(data: &[u8]) -> Vec<u8> {
    let mut division = 480u16;
    let mut blocks: Vec<Vec<u8>> = Vec::new();
    let mut s4wt: Vec<Vec<u8>> = Vec::new();
    for c in iter_chunks(data) {
        if c.tag == *b"S4WH" {
            division = parse_s4wh(c.payload).ppqn.max(1);
        } else if c.tag == *b"S4WB" {
            blocks.push(c.payload.to_vec());
        } else if c.tag == *b"S4WT" {
            s4wt.push(c.payload.to_vec());
        }
    }
    let mut tracks: Vec<Vec<u8>> = Vec::new();
    for pay in &s4wt {
        if let Some((name, pls)) = parse_s4wt(pay) {
            let mut merged: Vec<Event> = Vec::new();
            for p in &pls {
                if (p.block as usize) >= blocks.len() {
                    continue;
                }
                for e in parse_s4wb(&blocks[p.block as usize]) {
                    merged.push(e.shifted(p.start));
                }
            }
            if merged.is_empty() {
                continue;
            }
            let has_name = merged.iter().any(|e| matches!(e, Event::Meta { mtype: 0x03, .. }));
            if !name.is_empty() && !has_name {
                merged.insert(0, Event::Meta { t: 0, mtype: 0x03, data: name.into_bytes() });
            }
            tracks.push(block_to_mtrk(&merged));
        }
    }
    build_smf(&tracks, division)
}

/// Raw export: one MTrk per S4WB block (lossless but not the named layout).
pub fn export_blocks(data: &[u8]) -> Vec<u8> {
    let mut division = 480u16;
    let mut tracks: Vec<Vec<u8>> = Vec::new();
    for c in iter_chunks(data) {
        if c.tag == *b"S4WH" {
            division = parse_s4wh(c.payload).ppqn.max(1);
        } else if c.tag == *b"S4WB" {
            tracks.push(block_to_mtrk(&parse_s4wb(c.payload)));
        }
    }
    build_smf(&tracks, division)
}

// ---------------------------------------------------------------------------
// SMF import (.MID -> .XWS)
// ---------------------------------------------------------------------------
/// Parse a standard SMF MTrk into absolute events, pairing NoteOn/NoteOff into
/// (ch, note, vel, gate). Returns (events, track_name, channel).
pub fn parse_smf_track(buf: &[u8]) -> (Vec<Event>, String, u8) {
    let mut events = Vec::new();
    let mut name = String::new();
    let mut channel: Option<u8> = None;
    let mut i = 0usize;
    let mut t = 0u32;
    let mut running: Option<u8> = None;
    let mut open: HashMap<(u8, u8), VecDeque<(u32, u8)>> = HashMap::new();
    let n = buf.len();
    while i < n {
        let d = read_varlen(buf, &mut i);
        t = t.wrapping_add(d);
        if i >= n {
            break;
        }
        let status = buf[i];
        if status == 0xFF {
            i += 1;
            if i >= n {
                break;
            }
            let meta = buf[i];
            i += 1;
            let len = read_varlen(buf, &mut i) as usize;
            let end = (i + len).min(n);
            let data = buf[i..end].to_vec();
            i += len;
            if meta == 0x2F {
                break;
            }
            if meta == 0x03 {
                name = String::from_utf8_lossy(&data).into_owned();
            }
            events.push(Event::Meta { t, mtype: meta, data });
        } else if status == 0xF0 || status == 0xF7 {
            i += 1;
            let len = read_varlen(buf, &mut i) as usize;
            let end = (i + len).min(n);
            events.push(Event::Sysex { t, status, data: buf[i..end].to_vec() });
            i += len;
        } else {
            let st = if status & 0x80 != 0 {
                running = Some(status);
                i += 1;
                status
            } else {
                match running {
                    Some(s) => s,
                    None => break,
                }
            };
            let evt = st & 0xF0;
            let ch = st & 0x0F;
            if evt == 0xC0 || evt == 0xD0 {
                if i + 1 > n {
                    break;
                }
                events.push(Event::Chan { t, data: vec![st, buf[i]] });
                i += 1;
            } else {
                if i + 2 > n {
                    break;
                }
                let d1 = buf[i];
                let d2 = buf[i + 1];
                i += 2;
                if evt == 0x90 && d2 > 0 {
                    open.entry((ch, d1)).or_default().push_back((t, d2));
                    if channel.is_none() {
                        channel = Some(ch);
                    }
                } else if evt == 0x80 || (evt == 0x90 && d2 == 0) {
                    if let Some(q) = open.get_mut(&(ch, d1)) {
                        if let Some((ot, ov)) = q.pop_front() {
                            events.push(Event::Note {
                                t: ot,
                                ch,
                                note: d1,
                                vel: ov,
                                gate: t.wrapping_sub(ot),
                            });
                        }
                    }
                } else {
                    events.push(Event::Chan { t, data: vec![st, d1, d2] });
                }
            }
        }
    }
    (events, name, channel.unwrap_or(0))
}

fn push_chunk(out: &mut Vec<u8>, tag: &[u8; 4], payload: &[u8]) {
    out.extend_from_slice(tag);
    out.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    out.extend_from_slice(payload);
}

/// Build an .XWS from a standard MIDI file: one S4WB block + one named S4WT
/// track per MTrk (placed at tick 0); the S4WT pool is padded to 101.
pub fn import_mid(data: &[u8]) -> Vec<u8> {
    let hlen = be_u32(data, 4) as usize;
    let division = be_u16(data, 8 + 4); // MThd: format(2) ntrk(2) division(2)
    let mut mtrks: Vec<&[u8]> = Vec::new();
    let mut p = 8 + hlen;
    while p + 8 <= data.len() {
        let tag = &data[p..p + 4];
        let len = be_u32(data, p + 4) as usize;
        if tag == b"MTrk" {
            let end = (p + 8 + len).min(data.len());
            mtrks.push(&data[p + 8..end]);
        }
        p += 8 + len;
    }

    let mut blocks: Vec<Vec<u8>> = Vec::new();
    let mut s4wt: Vec<Vec<u8>> = Vec::new();
    for (k, m) in mtrks.iter().enumerate() {
        let (events, name, channel) = parse_smf_track(m);
        blocks.push(encode_s4wb(&events));
        let end = events.iter().map(|e| e.t()).max().unwrap_or(0);
        let nm = if name.is_empty() { format!("Track{}", k) } else { name };
        s4wt.push(build_s4wt(&nm, channel, k as u32, end, true));
    }
    while s4wt.len() < TRACK_POOL {
        s4wt.push(build_s4wt("", 0, 0, 0, false));
    }

    let mut out = Vec::new();
    push_chunk(&mut out, b"S4WH", &build_s4wh(blocks.len() as u32, division));
    for b in &blocks {
        push_chunk(&mut out, b"S4WB", b);
    }
    for s in &s4wt {
        push_chunk(&mut out, b"S4WT", s);
    }
    push_chunk(&mut out, b"S4ST", &vec![0u8; 138]); // trailer (ignored on re-import)
    out
}

// ---------------------------------------------------------------------------
// playback: merge all tracks and resolve ticks -> wall-clock time
// ---------------------------------------------------------------------------
/// All events of a song merged across tracks (absolute ticks) plus the PPQN.
/// Accepts either an `.XWS` or a Standard MIDI File.
pub fn song_events(data: &[u8]) -> (Vec<Event>, u16) {
    if data.len() >= 4 && &data[0..4] == b"MThd" {
        let hlen = be_u32(data, 4) as usize;
        let ppqn = be_u16(data, 8 + 4);
        let mut all = Vec::new();
        let mut p = 8 + hlen;
        while p + 8 <= data.len() {
            let tag = &data[p..p + 4];
            let len = be_u32(data, p + 4) as usize;
            if tag == b"MTrk" {
                let (ev, _, _) = parse_smf_track(&data[p + 8..(p + 8 + len).min(data.len())]);
                all.extend(ev);
            }
            p += 8 + len;
        }
        (all, ppqn.max(1))
    } else {
        let mut ppqn = 480u16;
        let mut blocks: Vec<Vec<u8>> = Vec::new();
        let mut s4wt: Vec<Vec<u8>> = Vec::new();
        for c in iter_chunks(data) {
            if c.tag == *b"S4WH" {
                ppqn = parse_s4wh(c.payload).ppqn.max(1);
            } else if c.tag == *b"S4WB" {
                blocks.push(c.payload.to_vec());
            } else if c.tag == *b"S4WT" {
                s4wt.push(c.payload.to_vec());
            }
        }
        let mut all = Vec::new();
        for pay in &s4wt {
            if let Some((_, pls)) = parse_s4wt(pay) {
                for p in &pls {
                    if (p.block as usize) >= blocks.len() {
                        continue;
                    }
                    for e in parse_s4wb(&blocks[p.block as usize]) {
                        all.push(e.shifted(p.start));
                    }
                }
            }
        }
        (all, ppqn)
    }
}

/// A flat, time-sorted playback plan: (time_ms, raw MIDI message bytes). Notes
/// are expanded to NoteOn + NoteOff(vel 0) using the gate; tempo meta events
/// (`FF 51`) drive the tick->time conversion. Meta events are not emitted.
pub fn playback_plan(data: &[u8]) -> Vec<(f64, Vec<u8>)> {
    let (events, ppqn) = song_events(data);
    let mut msgs: Vec<(u32, Vec<u8>)> = Vec::new();
    let mut tempos: Vec<(u32, u32)> = Vec::new();
    for e in &events {
        match e {
            Event::Note { t, ch, note, vel, gate } => {
                let st = 0x90 | ch;
                msgs.push((*t, vec![st, *note, *vel]));
                msgs.push((t.wrapping_add(*gate), vec![st, *note, 0]));
            }
            Event::Chan { t, data } => msgs.push((*t, data.clone())),
            Event::Sysex { t, status, data } => {
                let mut m = vec![*status];
                m.extend_from_slice(data);
                msgs.push((*t, m));
            }
            Event::Meta { t, mtype, data } => {
                if *mtype == 0x51 && data.len() == 3 {
                    let us = ((data[0] as u32) << 16) | ((data[1] as u32) << 8) | data[2] as u32;
                    tempos.push((*t, us));
                }
            }
        }
    }
    if tempos.iter().all(|x| x.0 != 0) {
        tempos.push((0, 500_000)); // default 120 BPM at the start
    }
    tempos.sort_by_key(|x| x.0);
    msgs.sort_by_key(|x| x.0);

    let mut out = Vec::with_capacity(msgs.len());
    let mut ti = 0usize;
    let mut cur_us = tempos[0].1 as f64;
    let mut last_tick = 0u32;
    let mut ms = 0f64;
    let pp = ppqn as f64;
    for (tick, bytes) in &msgs {
        while ti + 1 < tempos.len() && tempos[ti + 1].0 <= *tick {
            let seg = (tempos[ti + 1].0 - last_tick) as f64;
            ms += seg * cur_us / pp / 1000.0;
            last_tick = tempos[ti + 1].0;
            ti += 1;
            cur_us = tempos[ti].1 as f64;
        }
        let seg = (tick - last_tick) as f64;
        out.push((ms + seg * cur_us / pp / 1000.0, bytes.clone()));
    }
    out
}

// ---------------------------------------------------------------------------
// per-track plan (for a UI: track list + piano roll)
// ---------------------------------------------------------------------------
/// A note resolved to wall-clock time, for drawing a piano roll.
#[derive(Clone, Debug)]
pub struct PlanNote {
    pub t_ms: f64,
    pub dur_ms: f64,
    pub pitch: u8,
    pub vel: u8,
}

/// One track: its name/channel, drawable notes, and the raw MIDI messages to
/// play it (notes expanded to on/off, plus CC/program/pitch/SysEx), time-sorted.
#[derive(Clone, Debug)]
pub struct TrackPlan {
    pub name: String,
    pub channel: u8,
    pub notes: Vec<PlanNote>,
    pub messages: Vec<(f64, Vec<u8>)>,
}

struct TempoMap {
    pts: Vec<(u32, u32, f64)>, // (tick, us_per_qn, ms_at_tick)
    pp: f64,
}

impl TempoMap {
    fn new(mut tempos: Vec<(u32, u32)>, ppqn: u16) -> Self {
        if tempos.iter().all(|x| x.0 != 0) {
            tempos.push((0, 500_000));
        }
        tempos.sort_by_key(|x| x.0);
        tempos.dedup_by_key(|x| x.0);
        let pp = ppqn.max(1) as f64;
        let mut pts: Vec<(u32, u32, f64)> = Vec::with_capacity(tempos.len());
        let mut ms = 0f64;
        for (i, (tick, us)) in tempos.iter().enumerate() {
            if i > 0 {
                let (ptick, pus, _) = pts[i - 1];
                ms += (*tick - ptick) as f64 * pus as f64 / pp / 1000.0;
            }
            pts.push((*tick, *us, ms));
        }
        TempoMap { pts, pp }
    }
    fn ms(&self, tick: u32) -> f64 {
        let mut idx = 0usize;
        for (i, p) in self.pts.iter().enumerate() {
            if p.0 <= tick {
                idx = i;
            } else {
                break;
            }
        }
        let (ptick, pus, pms) = self.pts[idx];
        pms + (tick - ptick) as f64 * pus as f64 / self.pp / 1000.0
    }
}

/// Per-track playback + drawing plan. Timing (tempo map) is global; names come
/// from the track record or a TrackName meta. Accepts either `.XWS` or SMF.
pub fn track_plan(data: &[u8]) -> Vec<TrackPlan> {
    let (tracks, ppqn) = song_tracks(data);
    let mut tempos = Vec::new();
    for (_, _, evs) in &tracks {
        for e in evs {
            if let Event::Meta { t, mtype: 0x51, data } = e {
                if data.len() == 3 {
                    let us = ((data[0] as u32) << 16) | ((data[1] as u32) << 8) | data[2] as u32;
                    tempos.push((*t, us));
                }
            }
        }
    }
    let tm = TempoMap::new(tempos, ppqn);

    let mut out = Vec::new();
    for (idx, (name, ch, evs)) in tracks.iter().enumerate() {
        let mut notes = Vec::new();
        let mut msgs: Vec<(f64, Vec<u8>)> = Vec::new();
        let mut nm = name.clone();
        for e in evs {
            match e {
                Event::Note { t, ch: nch, note, vel, gate } => {
                    let on = tm.ms(*t);
                    let off = tm.ms(t.wrapping_add(*gate));
                    notes.push(PlanNote { t_ms: on, dur_ms: (off - on).max(0.0), pitch: *note, vel: *vel });
                    let st = 0x90 | nch;
                    msgs.push((on, vec![st, *note, *vel]));
                    msgs.push((off, vec![st, *note, 0]));
                }
                Event::Chan { t, data } => msgs.push((tm.ms(*t), data.clone())),
                Event::Sysex { t, status, data } => {
                    let mut m = vec![*status];
                    m.extend_from_slice(data);
                    msgs.push((tm.ms(*t), m));
                }
                Event::Meta { mtype: 0x03, data, .. } => {
                    if nm.is_empty() {
                        nm = String::from_utf8_lossy(data).into_owned();
                    }
                }
                _ => {}
            }
        }
        msgs.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        if nm.is_empty() {
            nm = format!("Track {}", idx + 1);
        }
        out.push(TrackPlan { name: nm, channel: *ch, notes, messages: msgs });
    }
    out
}

fn song_tracks(data: &[u8]) -> (Vec<(String, u8, Vec<Event>)>, u16) {
    if data.len() >= 4 && &data[0..4] == b"MThd" {
        let hlen = be_u32(data, 4) as usize;
        let ppqn = be_u16(data, 8 + 4).max(1);
        let mut out = Vec::new();
        let mut p = 8 + hlen;
        while p + 8 <= data.len() {
            let tag = &data[p..p + 4];
            let len = be_u32(data, p + 4) as usize;
            if tag == b"MTrk" {
                let (ev, name, ch) = parse_smf_track(&data[p + 8..(p + 8 + len).min(data.len())]);
                out.push((name, ch, ev));
            }
            p += 8 + len;
        }
        (out, ppqn)
    } else {
        let mut ppqn = 480u16;
        let mut blocks: Vec<Vec<u8>> = Vec::new();
        let mut s4wt: Vec<Vec<u8>> = Vec::new();
        for c in iter_chunks(data) {
            if c.tag == *b"S4WH" {
                ppqn = parse_s4wh(c.payload).ppqn.max(1);
            } else if c.tag == *b"S4WB" {
                blocks.push(c.payload.to_vec());
            } else if c.tag == *b"S4WT" {
                s4wt.push(c.payload.to_vec());
            }
        }
        let mut out = Vec::new();
        for pay in &s4wt {
            if let Some((name, pls)) = parse_s4wt(pay) {
                let ch = if pay.len() > 2 { pay[2] } else { 0 };
                let mut ev = Vec::new();
                for pl in &pls {
                    if (pl.block as usize) >= blocks.len() {
                        continue;
                    }
                    for e in parse_s4wb(&blocks[pl.block as usize]) {
                        ev.push(e.shifted(pl.start));
                    }
                }
                out.push((name, ch, ev));
            }
        }
        (out, ppqn)
    }
}

// ---------------------------------------------------------------------------
// editable model (for an editor: notes in ticks, write back to .XWS / .MID)
// ---------------------------------------------------------------------------
#[derive(Clone, Debug)]
pub struct EditNote {
    pub tick: u32,
    pub dur: u32,
    pub pitch: u8,
    pub vel: u8,
}

/// A block (XGworks "part") placed on a track: name and tick span. Events stay
/// merged on the track (absolute ticks); on save each event goes back to the
/// last block starting at or before it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditBlock {
    pub name: String,
    pub start: u32,
    pub end: u32,
    /// The 32-byte placement as loaded (unknown bytes are kept on save).
    pub raw: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct EditTrack {
    pub name: String,
    pub channel: u8,
    pub notes: Vec<EditNote>,
    /// Non-note events (CC / program / pitch / SysEx / meta) preserved in ticks.
    pub others: Vec<Event>,
    /// Blocks in time order (empty = one implicit block spanning the track).
    pub blocks: Vec<EditBlock>,
    /// Raw 0x20-byte S4WT header as loaded (port, transpose, ...), kept on save.
    pub header: Vec<u8>,
    /// XGworks' unnamed first track (tempo / markers / setup); not listed in
    /// the Track View.
    pub conductor: bool,
}

/// Instrument types of the Track View "Ins" column / Instrument Type dialog,
/// indexed by the top byte of `CTrk::InstType`.
pub const INST_TYPES: [&str; 8] = [
    "Normal",
    "Transposing Instrument",
    "Drum",
    "Gtr Tab (Single Track)",
    "Bass Tab (Single Track)",
    "Gtr Tab (Multi Track)",
    "Bass Tab (Multi Track)",
    "Electone",
];

/// S4WT track-info header, as read by `CSmf::LoadS4wTrkInfo` (XGworks.exe
/// 0x62caee): `+0` Type (0 unused, 1 MIDI, 2 WAVE), `+1` Port, `+2` Ch,
/// `+3` Transpose (i8), `+4` Solo, `+5` Mute, `+6` TransEnable (0 = "Off"),
/// `+7..+10` InstType (BE32: kind, group, string<<4 | n, tuning note),
/// `+11` StaffType, `+12` WavVol, `+13..+31` reserved.
impl EditTrack {
    fn hdr(&self, i: usize) -> u8 {
        self.header.get(i).copied().unwrap_or(match i {
            0 => 1,
            6 => (self.channel & 0x0F != 9) as u8,
            12 => 100,
            _ => 0,
        })
    }
    fn hdr_set(&mut self, i: usize, v: u8) {
        if self.header.len() != 0x20 {
            self.header = (0..0x20).map(|k| self.hdr(k)).collect();
        }
        self.header[i] = v;
    }
    pub fn port(&self) -> u8 {
        self.hdr(1)
    }
    pub fn set_port(&mut self, p: u8) {
        self.hdr_set(1, p)
    }
    /// Track transpose in semitones; `None` = "Off" (drum tracks by default).
    pub fn transpose(&self) -> Option<i8> {
        (self.hdr(6) != 0).then(|| self.hdr(3) as i8)
    }
    pub fn set_transpose(&mut self, t: Option<i8>) {
        self.hdr_set(6, t.is_some() as u8);
        self.hdr_set(3, t.unwrap_or(0).clamp(-36, 36) as u8);
    }
    pub fn solo(&self) -> bool {
        self.hdr(4) != 0
    }
    pub fn mute(&self) -> bool {
        self.hdr(5) != 0
    }
    pub fn set_solo_mute(&mut self, solo: bool, mute: bool) {
        self.hdr_set(4, solo as u8);
        self.hdr_set(5, mute as u8);
    }
    pub fn track_type(&self) -> u8 {
        self.hdr(0)
    }
    pub fn inst_type(&self) -> u32 {
        u32::from_be_bytes([self.hdr(7), self.hdr(8), self.hdr(9), self.hdr(10)])
    }
    pub fn set_inst_type(&mut self, v: u32) {
        for (k, b) in v.to_be_bytes().into_iter().enumerate() {
            self.hdr_set(7 + k, b);
        }
    }
    pub fn staff_type(&self) -> u8 {
        self.hdr(11)
    }
    pub fn wav_vol(&self) -> u8 {
        self.hdr(12)
    }
    /// The "Ins" column text, formatted like XGworks ("Normal", "Drum",
    /// "Gtr Tab A-1", "Electone A-Upper"...). Transposing instruments are
    /// named from `Trk.ini [TransInst]` by the caller; this returns its index.
    pub fn inst_label(&self) -> String {
        let [kind, group, sub, _] = self.inst_type().to_be_bytes();
        let g = (b'A' + (group & 7)) as char;
        let n = (sub >> 4) + 1;
        match kind {
            0 => "Normal".into(),
            1 => format!("TransInst {}", group),
            2 => "Drum".into(),
            3 => "Gtr Tab".into(),
            4 => "Bass Tab".into(),
            5 => format!("Gtr Tab {g}-{n}"),
            6 => format!("Bass Tab {g}-{n}"),
            7 => format!("Electone {g}-{}", ["Upper", "Lower", "Bass"][((sub >> 4) as usize).min(2)]),
            _ => "-".into(),
        }
    }
}

/// Decode a used S4WT record with block names: `[0x20 header][track name]`
/// then `[32-byte placement][block name]*`. Placement = `01 00`, start BE32,
/// end BE32, S4WB index BE32, padding. Returns (name, [(block index, block)]).
pub fn parse_s4wt_blocks(pay: &[u8]) -> Option<(String, Vec<(u32, EditBlock)>)> {
    if pay.len() <= 0x20 || pay[0] == 0 {
        return None;
    }
    let mut i = 0x20usize;
    let name = read_cstring(pay, &mut i);
    let mut out = Vec::new();
    while i + 32 <= pay.len() && pay[i] == 1 {
        let raw = pay[i..i + 32].to_vec();
        let (start, end, idx) = (be_u32(pay, i + 2), be_u32(pay, i + 6), be_u32(pay, i + 10));
        i += 32;
        let bname = if i < pay.len() { read_cstring(pay, &mut i) } else { String::new() };
        out.push((idx, EditBlock { name: bname, start, end, raw }));
    }
    Some((name, out))
}

/// Build an editable, tick-based model from an `.XWS` or SMF. Notes are split
/// out as `EditNote`; everything else is kept verbatim in `others`.
pub fn edit_model(data: &[u8]) -> (Vec<EditTrack>, u16) {
    if !(data.len() >= 4 && &data[0..4] == b"MThd") {
        return edit_model_xws(data);
    }
    let (tracks, ppqn) = song_tracks(data);
    let mut out = Vec::new();
    for (name, ch, evs) in tracks {
        let mut notes = Vec::new();
        let mut others = Vec::new();
        for e in evs {
            match e {
                Event::Note { t, note, vel, gate, .. } => {
                    notes.push(EditNote { tick: t, dur: gate, pitch: note, vel });
                }
                other => others.push(other),
            }
        }
        out.push(EditTrack { name, channel: ch, notes, others, ..Default::default() });
    }
    (out, ppqn)
}

fn edit_model_xws(data: &[u8]) -> (Vec<EditTrack>, u16) {
    let mut ppqn = 480u16;
    let mut blocks: Vec<&[u8]> = Vec::new();
    let mut s4wt: Vec<&[u8]> = Vec::new();
    for c in iter_chunks(data) {
        match &c.tag {
            b"S4WH" => ppqn = parse_s4wh(c.payload).ppqn.max(1),
            b"S4WB" => blocks.push(c.payload),
            b"S4WT" => s4wt.push(c.payload),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for (k, pay) in s4wt.iter().enumerate() {
        let Some((name, pls)) = parse_s4wt_blocks(pay) else { continue };
        let mut t = EditTrack {
            name: name.clone(),
            channel: pay[2] & 0x0F,
            header: pay[..0x20].to_vec(),
            conductor: k == 0 && name.is_empty(),
            ..Default::default()
        };
        for (idx, b) in pls {
            if let Some(buf) = blocks.get(idx as usize) {
                for e in parse_s4wb(buf) {
                    match e.shifted(b.start) {
                        Event::Note { t: tk, note, vel, gate, .. } => {
                            t.notes.push(EditNote { tick: tk, dur: gate, pitch: note, vel })
                        }
                        other => t.others.push(other),
                    }
                }
            }
            t.blocks.push(b);
        }
        t.blocks.sort_by_key(|b| b.start);
        out.push(t);
    }
    (out, ppqn)
}

fn edit_track_events(t: &EditTrack) -> Vec<Event> {
    let mut ev = t.others.clone();
    for n in &t.notes {
        ev.push(Event::Note { t: n.tick, ch: t.channel, note: n.pitch, vel: n.vel, gate: n.dur });
    }
    ev
}

fn edit_track_end(t: &EditTrack) -> u32 {
    t.notes
        .iter()
        .map(|n| n.tick.wrapping_add(n.dur))
        .chain(t.others.iter().map(|e| e.t()))
        .max()
        .unwrap_or(0)
}

/// Serialize an edited model back to an `.XWS`: one S4WB per block, one S4WT
/// per track (original header and placement bytes kept), pool padded to 101.
pub fn write_xws(tracks: &[EditTrack], ppqn: u16) -> Vec<u8> {
    let mut blocks: Vec<Vec<u8>> = Vec::new();
    let mut s4wt: Vec<Vec<u8>> = Vec::new();
    for (k, t) in tracks.iter().enumerate() {
        let name = if t.name.is_empty() && !t.conductor { format!("Track{}", k) } else { t.name.clone() };
        let spans: Vec<EditBlock> = if t.blocks.is_empty() {
            vec![EditBlock { name: name.clone(), start: 0, end: edit_track_end(t), raw: Vec::new() }]
        } else {
            t.blocks.clone()
        };
        // each event -> the last block starting at or before it (else the first)
        let mut per: Vec<Vec<Event>> = vec![Vec::new(); spans.len()];
        for e in edit_track_events(t) {
            let et = e.t();
            let bi = spans.iter().rposition(|b| b.start <= et).unwrap_or(0);
            let rel = 0u32.wrapping_sub(spans[bi].start.min(et));
            per[bi].push(e.shifted(rel));
        }
        let mut rec: Vec<u8> = (0..0x20).map(|i| t.hdr(i)).collect();
        if rec[0] == 0 {
            rec[0] = 1;
        }
        rec[2] = t.channel & 0x0F;
        rec.extend(mfc_cstring(&name));
        for (b, evs) in spans.iter().zip(per.iter()) {
            let idx = blocks.len() as u32;
            let last = evs
                .iter()
                .map(|e| match e {
                    Event::Note { t, gate, .. } => t.wrapping_add(*gate),
                    other => other.t(),
                })
                .max()
                .unwrap_or(0);
            let end = b.end.max(b.start.wrapping_add(last));
            blocks.push(encode_s4wb(evs));
            let mut pl = if b.raw.len() == 32 { b.raw.clone() } else { vec![0u8; 32] };
            pl[0] = 1;
            pl[1] = 0;
            pl[2..6].copy_from_slice(&b.start.to_be_bytes());
            pl[6..10].copy_from_slice(&end.to_be_bytes());
            pl[10..14].copy_from_slice(&idx.to_be_bytes());
            rec.extend(pl);
            rec.extend(mfc_cstring(&b.name));
        }
        s4wt.push(rec);
    }
    while s4wt.len() < TRACK_POOL {
        s4wt.push(build_s4wt("", 0, 0, 0, false));
    }
    let mut out = Vec::new();
    push_chunk(&mut out, b"S4WH", &build_s4wh(blocks.len() as u32, ppqn));
    for b in &blocks {
        push_chunk(&mut out, b"S4WB", b);
    }
    for s in &s4wt {
        push_chunk(&mut out, b"S4WT", s);
    }
    push_chunk(&mut out, b"S4ST", &vec![0u8; 138]);
    out
}

/// Serialize an edited model to a Standard MIDI File (one MTrk per `EditTrack`).
pub fn write_mid(tracks: &[EditTrack], ppqn: u16) -> Vec<u8> {
    let mut mtrks = Vec::new();
    for (k, t) in tracks.iter().enumerate() {
        let mut evs = edit_track_events(t);
        let has_name = evs.iter().any(|e| matches!(e, Event::Meta { mtype: 0x03, .. }));
        if !has_name {
            let nm = if t.name.is_empty() { format!("Track{}", k) } else { t.name.clone() };
            evs.push(Event::Meta { t: 0, mtype: 0x03, data: nm.into_bytes() });
        }
        mtrks.push(block_to_mtrk(&evs));
    }
    build_smf(&mtrks, ppqn)
}

/// Playback plan straight from the edit model, each message tagged with the
/// index of its track: `(time_ms, track, bytes)`, time-sorted. Lets a player
/// mute/solo tracks live. Tempo (`FF 51`) from any track drives the timing.
pub fn edit_playback_plan(tracks: &[EditTrack], ppqn: u16) -> Vec<(f64, usize, Vec<u8>)> {
    let mut msgs: Vec<(u32, usize, Vec<u8>)> = Vec::new();
    let mut tempos: Vec<(u32, u32)> = Vec::new();
    for (k, t) in tracks.iter().enumerate() {
        let ch = t.channel & 0x0F;
        // Track View transpose: applied at playback only, the data is kept
        let tr = t.transpose().unwrap_or(0) as i32;
        for n in &t.notes {
            let p = n.pitch as i32 + tr;
            if !(0..=127).contains(&p) {
                continue;
            }
            msgs.push((n.tick, k, vec![0x90 | ch, p as u8, n.vel]));
            msgs.push((n.tick.wrapping_add(n.dur), k, vec![0x90 | ch, p as u8, 0]));
        }
        for e in &t.others {
            match e {
                Event::Note { t, ch, note, vel, gate } => {
                    msgs.push((*t, k, vec![0x90 | ch, *note, *vel]));
                    msgs.push((t.wrapping_add(*gate), k, vec![0x90 | ch, *note, 0]));
                }
                Event::Chan { t, data } => msgs.push((*t, k, data.clone())),
                Event::Sysex { t, status, data } => {
                    let mut m = vec![*status];
                    m.extend_from_slice(data);
                    msgs.push((*t, k, m));
                }
                Event::Meta { t, mtype: 0x51, data } if data.len() == 3 => {
                    tempos.push((*t, ((data[0] as u32) << 16) | ((data[1] as u32) << 8) | data[2] as u32));
                }
                Event::Meta { .. } => {}
            }
        }
    }
    if tempos.iter().all(|x| x.0 != 0) {
        tempos.push((0, 500_000));
    }
    tempos.sort_by_key(|x| x.0);
    // within a tick: everything else (note-offs, CC, program) before note-ons,
    // so a note re-struck where the previous one ends is not cut off
    msgs.sort_by_key(|x| (x.0, (x.2.len() == 3 && x.2[0] & 0xF0 == 0x90 && x.2[2] > 0) as u8));

    let mut out = Vec::with_capacity(msgs.len());
    let (mut ti, mut last_tick, mut ms) = (0usize, 0u32, 0f64);
    let mut cur_us = tempos[0].1 as f64;
    let pp = ppqn.max(1) as f64;
    for (tick, k, bytes) in msgs {
        while ti + 1 < tempos.len() && tempos[ti + 1].0 <= tick {
            ms += (tempos[ti + 1].0 - last_tick) as f64 * cur_us / pp / 1000.0;
            last_tick = tempos[ti + 1].0;
            ti += 1;
            cur_us = tempos[ti].1 as f64;
        }
        out.push((ms + (tick - last_tick) as f64 * cur_us / pp / 1000.0, k, bytes));
    }
    out
}

// ---------------------------------------------------------------------------
// unit tests (format internals; integration round-trip is in tests/)
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn varlen_roundtrip() {
        for v in [0u32, 1, 127, 128, 480, 34560, 132480, 0x0FFF_FFFF] {
            let mut i = 0;
            let enc = write_varlen(v);
            assert_eq!(read_varlen(&enc, &mut i), v);
            assert_eq!(i, enc.len());
        }
    }
}
