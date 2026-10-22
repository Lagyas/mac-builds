//! The Job menu of XGworks (JobDll.dll / JobDll2.dll dialogs; the work is done
//! by XGworks.exe's exported `CJob`): Transpose, Velocity Modify, Gate Time
//! Modify, Shift Clock, Sort Chord, Separate Chord, Thin Out, Quantize,
//! Create / Delete Measure, Transpose Chord, Modify Control Data, Stretch Time.
//!
//! Every job works on a `Scope`: tick ranges per track (a track and a
//! Meas From / Thru range, or the blocks selected in the Track View) and,
//! where the dialog has one, a note range.

use crate::listev::with_tick;
use crate::{EditNote, EditTrack, Event};

/// What a job works on: (track, from tick, to tick (exclusive)) spans and the
/// note range (Note From / Thru).
#[derive(Clone, Debug)]
pub struct Scope {
    pub spans: Vec<(usize, u32, u32)>,
    pub note_lo: u8,
    pub note_hi: u8,
}

impl Scope {
    pub fn track(ti: usize, from: u32, to: u32) -> Scope {
        Scope { spans: vec![(ti, from, to)], note_lo: 0, note_hi: 127 }
    }

    fn tracks(&self) -> Vec<usize> {
        let mut v: Vec<usize> = self.spans.iter().map(|s| s.0).collect();
        v.sort();
        v.dedup();
        v
    }

    fn has(&self, ti: usize, t: u32) -> bool {
        self.spans.iter().any(|&(k, a, b)| k == ti && t >= a && t < b)
    }

    fn note_in(&self, ti: usize, n: &EditNote) -> bool {
        self.has(ti, n.tick) && n.pitch >= self.note_lo && n.pitch <= self.note_hi
    }
}

/// Absolute value, or relative: value × rate / 100 + offset.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Modify {
    Absolute(i32),
    Relative { rate: i32, offset: i32 },
}

impl Modify {
    fn apply(self, v: i32) -> i32 {
        match self {
            Modify::Absolute(x) => x,
            Modify::Relative { rate, offset } => (v * rate + 50).div_euclid(100) + offset,
        }
    }
}

/// Transpose (−127..127 semitones). Notes leaving 0..127 move to the same
/// note name in the highest / lowest octave (Transpose.hlp).
pub fn transpose(tracks: &mut [EditTrack], sc: &Scope, semis: i32) -> usize {
    let mut n = 0;
    for ti in sc.tracks() {
        for note in tracks[ti].notes.iter_mut().filter(|x| sc.note_in(ti, x)) {
            let mut p = note.pitch as i32 + semis;
            while p > 127 {
                p -= 12;
            }
            while p < 0 {
                p += 12;
            }
            note.pitch = p as u8;
            n += 1;
        }
    }
    n
}

/// Velocity Modify: result kept in 1..127.
pub fn velocity(tracks: &mut [EditTrack], sc: &Scope, m: Modify) -> usize {
    let mut n = 0;
    for ti in sc.tracks() {
        for note in tracks[ti].notes.iter_mut().filter(|x| sc.note_in(ti, x)) {
            note.vel = m.apply(note.vel as i32).clamp(1, 127) as u8;
            n += 1;
        }
    }
    n
}

/// Gate Time Modify (clocks): at least 1.
pub fn gate_time(tracks: &mut [EditTrack], sc: &Scope, m: Modify) -> usize {
    let mut n = 0;
    for ti in sc.tracks() {
        for note in tracks[ti].notes.iter_mut().filter(|x| sc.note_in(ti, x)) {
            note.dur = m.apply(note.dur as i32).max(1) as u32;
            n += 1;
        }
    }
    n
}

/// Shift Clock: every event in the range moves by `clocks` (not before 0).
pub fn shift_clock(tracks: &mut [EditTrack], sc: &Scope, clocks: i32) -> usize {
    let mv = |t: u32| (t as i64 + clocks as i64).max(0) as u32;
    let mut n = 0;
    for ti in sc.tracks() {
        let t = &mut tracks[ti];
        for note in t.notes.iter_mut().filter(|x| sc.has(ti, x.tick)) {
            note.tick = mv(note.tick);
            n += 1;
        }
        for e in t.others.iter_mut() {
            if sc.has(ti, e.t()) {
                *e = with_tick(e, mv(e.t()));
                n += 1;
            }
        }
        t.notes.sort_by_key(|x| x.tick);
        t.others.sort_by_key(|e| e.t());
    }
    n
}

/// Sort Chord: notes on the same tick ordered low → high (or high → low).
pub fn sort_chord(tracks: &mut [EditTrack], sc: &Scope, reverse: bool) {
    for ti in sc.tracks() {
        let t = &mut tracks[ti];
        let mut i = 0;
        while i < t.notes.len() {
            let mut j = i;
            while j < t.notes.len() && t.notes[j].tick == t.notes[i].tick {
                j += 1;
            }
            if sc.has(ti, t.notes[i].tick) {
                let g = &mut t.notes[i..j];
                if reverse {
                    g.sort_by(|a, b| b.pitch.cmp(&a.pitch));
                } else {
                    g.sort_by_key(|x| x.pitch);
                }
            }
            i = j;
        }
    }
}

/// Separate Chord: the k-th note of a chord (in its list order) moves by k ×
/// `clocks`, like a strummed guitar after Sort Chord.
pub fn separate_chord(tracks: &mut [EditTrack], sc: &Scope, clocks: i32) {
    for ti in sc.tracks() {
        let t = &mut tracks[ti];
        let mut i = 0;
        while i < t.notes.len() {
            let mut j = i;
            while j < t.notes.len() && t.notes[j].tick == t.notes[i].tick {
                j += 1;
            }
            if sc.has(ti, t.notes[i].tick) {
                for (k, n) in t.notes[i..j].iter_mut().enumerate() {
                    n.tick = (n.tick as i64 + k as i64 * clocks as i64).max(0) as u32;
                }
            }
            i = j;
        }
        t.notes.sort_by_key(|x| x.tick); // stable: the chord order stays
    }
}

/// Modify Control Data: which events. `Cc(from, to)` also renumbers the
/// controller (to = from keeps it).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CtrlKind {
    PolyAfter,
    Cc(u8, u8),
    ChAfter,
    PitchBend,
}

/// Modify Control Data: values of the chosen kind in range set or scaled
/// (pitch bend −8192..8191, others 0..127).
pub fn modify_control(tracks: &mut [EditTrack], sc: &Scope, kind: CtrlKind, m: Modify) -> usize {
    let mut n = 0;
    for ti in sc.tracks() {
        for e in tracks[ti].others.iter_mut() {
            let tk = e.t();
            let Event::Chan { data, .. } = e else { continue };
            if !sc.has(ti, tk) {
                continue;
            }
            let st = data[0] & 0xF0;
            match (kind, st) {
                (CtrlKind::PolyAfter, 0xA0) if data.len() >= 3 => data[2] = m.apply(data[2] as i32).clamp(0, 127) as u8,
                (CtrlKind::Cc(from, to), 0xB0) if data.len() >= 3 && data[1] == from => {
                    data[1] = to;
                    data[2] = m.apply(data[2] as i32).clamp(0, 127) as u8;
                }
                (CtrlKind::ChAfter, 0xD0) if data.len() >= 2 => data[1] = m.apply(data[1] as i32).clamp(0, 127) as u8,
                (CtrlKind::PitchBend, 0xE0) if data.len() >= 3 => {
                    let v = ((data[2] as i32) << 7 | data[1] as i32) - 8192;
                    let v = m.apply(v).clamp(-8192, 8191) + 8192;
                    data[1] = (v & 0x7F) as u8;
                    data[2] = (v >> 7) as u8;
                }
                _ => continue,
            }
            n += 1;
        }
    }
    n
}

/// Create Measure: `count` bars of `num`/`den` inserted before bar `at_tick`
/// (a bar start): everything from there moves back; the new bars get their
/// time signature, and the old one comes back after them when it differs.
pub fn create_measure(tracks: &mut [EditTrack], conductor: usize, ppqn: u32, at_tick: u32, num: u8, den: u8, count: u32, old_sig: (u8, u8)) {
    let bar = ppqn * 4 * num as u32 / den.max(1) as u32;
    let len = bar * count;
    shift_from(tracks, at_tick, len as i64);
    let ts = |t: u32, n: u8, d: u8| Event::Meta { t, mtype: 0x58, data: vec![n, d.trailing_zeros() as u8, 24, 8] };
    let c = &mut tracks[conductor];
    c.others.retain(|e| !(matches!(e, Event::Meta { mtype: 0x58, .. }) && e.t() == at_tick));
    c.others.push(ts(at_tick, num, den));
    if (num, den) != old_sig && !c.others.iter().any(|e| matches!(e, Event::Meta { mtype: 0x58, .. }) && e.t() == at_tick + len) {
        c.others.push(ts(at_tick + len, old_sig.0, old_sig.1));
    }
    c.others.sort_by_key(|e| e.t());
}

/// Delete Measure: the bars [from, to) (ticks of bar starts) and everything in
/// them go; what follows moves forward.
pub fn delete_measure(tracks: &mut [EditTrack], from: u32, to: u32) {
    if to <= from {
        return;
    }
    // a time signature inside the cut that still rules afterwards moves to the cut
    let mut sig_after: Option<Event> = None;
    for t in tracks.iter() {
        for e in &t.others {
            if matches!(e, Event::Meta { mtype: 0x58, .. }) && e.t() >= from && e.t() < to {
                sig_after = Some(with_tick(e, from));
            }
        }
    }
    for t in tracks.iter_mut() {
        t.notes.retain(|n| n.tick < from || n.tick >= to);
        t.others.retain(|e| e.t() < from || e.t() >= to);
        for b in t.blocks.iter_mut() {
            let cut = |x: u32| if x <= from { x } else if x >= to { x - (to - from) } else { from };
            b.start = cut(b.start);
            b.end = cut(b.end);
        }
        t.blocks.retain(|b| b.end > b.start);
    }
    shift_from(tracks, to, -((to - from) as i64));
    if let Some(e) = sig_after {
        if let Some(t) = tracks.iter_mut().find(|t| t.others.iter().any(|x| matches!(x, Event::Meta { mtype: 0x58, .. }))) {
            if !t.others.iter().any(|x| matches!(x, Event::Meta { mtype: 0x58, .. }) && x.t() == from) {
                t.others.push(e);
                t.others.sort_by_key(|x| x.t());
            }
        }
    }
}

/// Everything at or after `at` moves by `by` ticks (blocks too).
fn shift_from(tracks: &mut [EditTrack], at: u32, by: i64) {
    let mv = |t: u32| (t as i64 + by).max(0) as u32;
    for t in tracks.iter_mut() {
        for n in t.notes.iter_mut().filter(|n| n.tick >= at) {
            n.tick = mv(n.tick);
        }
        for e in t.others.iter_mut() {
            if e.t() >= at {
                *e = with_tick(e, mv(e.t()));
            }
        }
        if by > 0 {
            for b in t.blocks.iter_mut() {
                if b.start >= at {
                    b.start = mv(b.start);
                    b.end = mv(b.end);
                } else if b.end > at {
                    b.end = mv(b.end); // a block across the insertion point grows
                }
            }
        } else {
            for b in t.blocks.iter_mut().filter(|b| b.start >= at) {
                b.start = mv(b.start);
                b.end = mv(b.end);
            }
        }
        t.notes.sort_by_key(|n| n.tick);
        t.others.sort_by_key(|e| e.t());
    }
}

/// XF chord root byte (low nibble 1..7 = C..B, high nibble 3 natural, 2 / 1 /
/// 0 one / two / three flats, 4 / 5 / 6 sharps) → pitch class.
fn xf_pc(b: u8) -> Option<i32> {
    const NAT: [i32; 8] = [0, 0, 2, 4, 5, 7, 9, 11];
    let n = (b & 0x0F) as usize;
    (1..=7).contains(&n).then(|| (NAT[n] + (b >> 4) as i32 - 3).rem_euclid(12))
}

/// Pitch class → XF root byte, naturals and sharps for up, flats for down.
fn xf_root(pc: i32, flats: bool) -> u8 {
    const SHARP: [(u8, u8); 12] = [(1, 3), (1, 4), (2, 3), (2, 4), (3, 3), (4, 3), (4, 4), (5, 3), (5, 4), (6, 3), (6, 4), (7, 3)];
    const FLAT: [(u8, u8); 12] = [(1, 3), (2, 2), (2, 3), (3, 2), (3, 3), (4, 3), (5, 2), (5, 3), (6, 2), (6, 3), (7, 2), (7, 3)];
    let (n, a) = if flats { FLAT[pc as usize] } else { SHARP[pc as usize] };
    a << 4 | n
}

/// Transpose Chord: XF chord names (FF 7F 43 7B 01 cr ct bn bt) in the range
/// move by `semis` (root and bass note).
pub fn transpose_chord(tracks: &mut [EditTrack], from: u32, to: u32, semis: i32) -> usize {
    let mut n = 0;
    for t in tracks.iter_mut() {
        for e in t.others.iter_mut() {
            let tk = e.t();
            let Event::Meta { mtype: 0x7F, data, .. } = e else { continue };
            if tk < from || tk >= to || data.len() < 7 || data[0..3] != [0x43, 0x7B, 0x01] {
                continue;
            }
            for k in [3usize, 5] {
                if let Some(pc) = xf_pc(data[k]) {
                    data[k] = xf_root((pc + semis).rem_euclid(12), semis < 0);
                }
            }
            n += 1;
        }
    }
    n
}

// ---------------------------------------------------------------------------
// Quantize (JobDll.dll CJob::Qntz / GetQntzTime / GetQntzWidth / IsSwing)
// ---------------------------------------------------------------------------

/// Quantize grids at 480 PPQN: 32nd, 16th triplet, 16th, 8th triplet, 8th,
/// quarter triplet, quarter, 8th + 8th triplet, 16th + 16th triplet.
pub const QNTZ_NAMES: [&str; 9] = ["1/32", "1/16 triplet", "1/16", "1/8 triplet", "1/8", "1/4 triplet", "1/4", "1/8 + 1/8 triplet", "1/16 + 1/16 triplet"];
const QNTZ_GRID: [i64; 9] = [60, 80, 120, 160, 240, 320, 480, 160, 120];
/// Mixed grids: half widths before / after the k-th point (k = index & 3).
const QNTZ_MIX_L: [[i64; 4]; 2] = [[80, 80, 40, 40], [40, 40, 20, 20]];
const QNTZ_MIX_R: [[i64; 4]; 2] = [[80, 40, 40, 80], [40, 20, 20, 40]];

#[derive(Clone, Copy, Debug)]
pub struct QntzOpts {
    /// Grid index (QNTZ_NAMES).
    pub grid: u8,
    /// Strength 0..100 %.
    pub strength: u8,
    /// Sensitivity -100..100 %.
    pub sens: i16,
    /// Swing rate 50..100 %.
    pub swing: u8,
    /// Gate time / velocity of the swung notes, 0..200 %.
    pub gate: u8,
    pub vel: u8,
}

/// (half width before, half width after) the i-th grid point.
fn qntz_width(q: u8, i: i64, scale: i64) -> (i64, i64) {
    let (l, r) = match q {
        7 | 8 => (QNTZ_MIX_L[(q - 7) as usize][(i & 3) as usize], QNTZ_MIX_R[(q - 7) as usize][(i & 3) as usize]),
        _ => (QNTZ_GRID[q as usize] / 2, QNTZ_GRID[q as usize] / 2),
    };
    (l * scale / 480, r * scale / 480)
}

/// Whether the i-th grid point of the bar is an "even" one (swing, gate, velocity).
fn qntz_swing(q: u8, i: i64) -> bool {
    match q {
        1 | 3 | 5 => i % 3 == 2,
        7 | 8 => i & 3 == 2,
        _ => i & 1 == 1,
    }
}

/// The new time of `t` for grid point `g`, or None when `t` is not caught by
/// it (outside its window, or outside the sensitivity range).
fn qntz_time(t: i64, g: i64, wl: i64, wr: i64, strength: i64, sens: i64) -> Option<i64> {
    if !(g - wl <= t && t < g + wr) {
        return None;
    }
    if t < g {
        if sens < 0 {
            if (sens * wl) / -100 - wl + g < t {
                return None;
            }
        } else if t < (sens * wl) / -100 + g {
            return None;
        }
        Some(t + strength * (g - t) / 100)
    } else {
        if sens < 0 {
            if t < (sens * wr) / 100 + wr + g {
                return None;
            }
        } else if (sens * wr) / 100 + g < t {
            return None;
        }
        Some(t - strength * (t - g) / 100)
    }
}

/// Quantize the notes in scope. `bar_at(tick)` gives (bar start, bar length);
/// grid points count from each bar start.
pub fn quantize(tracks: &mut [EditTrack], sc: &Scope, o: QntzOpts, ppqn: u32, bar_at: impl Fn(u32) -> (u32, u32)) -> usize {
    let q = o.grid.min(8);
    let scale = ppqn as i64;
    let step = |i: i64| -> i64 {
        match q {
            7 | 8 => 2 * qntz_width(q, i, scale).1,
            _ => QNTZ_GRID[q as usize] * scale / 480,
        }
    };
    let mut n = 0;
    for ti in sc.tracks() {
        let t = &mut tracks[ti];
        for note in t.notes.iter_mut() {
            if !sc.note_in(ti, note) {
                continue;
            }
            let (bs, bl) = bar_at(note.tick);
            let (bs, end) = (bs as i64, bs as i64 + bl as i64);
            let tk = note.tick as i64;
            let (mut g, mut i) = (bs, 0i64);
            while g <= end {
                let (wl, wr) = qntz_width(q, i, scale);
                if let Some(mut r) = qntz_time(tk, g, wl, wr, o.strength as i64, o.sens as i64) {
                    if qntz_swing(q, i) {
                        r += QNTZ_GRID[q as usize] * scale / 480 * (o.swing as i64 - 50) / 50;
                        note.dur = ((o.gate as u32 * note.dur) / 100).max(1);
                        note.vel = ((o.vel as u32 * note.vel as u32) / 100).clamp(1, 127) as u8;
                    }
                    note.tick = r.max(0) as u32;
                    n += 1;
                    break;
                }
                g += step(i);
                i += 1;
            }
        }
        t.notes.sort_by_key(|x| x.tick);
    }
    n
}

/// Thin Out: of the events of `kind` in range (per track, in time order) the
/// first two and the last stay, and of the rest every second one goes. This
/// is JobDll ThinCore: the run starts at the second event, keeps its first,
/// drops every other one and always keeps its last.
pub fn thin_out(tracks: &mut [EditTrack], sc: &Scope, kind: CtrlKind) -> usize {
    let mut removed = 0;
    for ti in sc.tracks() {
        let t = &mut tracks[ti];
        let matches = |e: &Event| -> bool {
            let Event::Chan { data, .. } = e else { return false };
            match (kind, data[0] & 0xF0) {
                (CtrlKind::PolyAfter, 0xA0) | (CtrlKind::ChAfter, 0xD0) | (CtrlKind::PitchBend, 0xE0) => true,
                (CtrlKind::Cc(c, _), 0xB0) => data.get(1) == Some(&c),
                _ => false,
            }
        };
        let idx: Vec<usize> = (0..t.others.len()).filter(|&i| sc.has(ti, t.others[i].t()) && matches(&t.others[i])).collect();
        let run = idx.get(1..).unwrap_or(&[]);
        if run.len() < 3 {
            continue;
        }
        let drop: Vec<usize> = run[..run.len() - 1].iter().skip(1).step_by(2).copied().collect();
        removed += drop.len();
        for &i in drop.iter().rev() {
            t.others.remove(i);
        }
    }
    removed
}

/// Stretch Time: events in [from, to) move to from + offset + (t - from) x
/// stretch / 100; notes' gate times scale too (JobDll2 CJob2::TimeStretch).
pub fn stretch_time(tracks: &mut [EditTrack], sc: &Scope, stretch: u32, offset: i32) -> usize {
    let mut n = 0;
    for &(ti, from, to) in &sc.spans {
        let f = from as i64;
        let mv = |t: u32| (f + offset as i64 + (t as i64 - f) * stretch as i64 / 100).max(0) as u32;
        let t = &mut tracks[ti];
        for note in t.notes.iter_mut().filter(|x| x.tick >= from && x.tick < to) {
            note.tick = mv(note.tick);
            note.dur = (note.dur * stretch / 100).max(1);
            n += 1;
        }
        for e in t.others.iter_mut() {
            if e.t() >= from && e.t() < to {
                *e = with_tick(e, mv(e.t()));
                n += 1;
            }
        }
        t.notes.sort_by_key(|x| x.tick);
        t.others.sort_by_key(|e| e.t());
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trk(notes: &[(u32, u32, u8, u8)]) -> EditTrack {
        EditTrack {
            notes: notes.iter().map(|&(tick, dur, pitch, vel)| EditNote { tick, dur, pitch, vel }).collect(),
            ..Default::default()
        }
    }

    #[test]
    fn transpose_folds_into_range() {
        let mut t = vec![trk(&[(0, 10, 120, 64), (10, 10, 60, 64), (5000, 10, 60, 64)])];
        transpose(&mut t, &Scope::track(0, 0, 1000), 12);
        assert_eq!(t[0].notes.iter().map(|n| n.pitch).collect::<Vec<_>>(), [120, 72, 60]); // 132 → 120 (same name)
    }

    #[test]
    fn velocity_and_gate() {
        let mut t = vec![trk(&[(0, 100, 60, 100), (0, 50, 64, 20)])];
        velocity(&mut t, &Scope::track(0, 0, 10), Modify::Relative { rate: 150, offset: 0 });
        assert_eq!(t[0].notes.iter().map(|n| n.vel).collect::<Vec<_>>(), [127, 30]);
        gate_time(&mut t, &Scope::track(0, 0, 10), Modify::Relative { rate: 50, offset: -40 });
        assert_eq!(t[0].notes.iter().map(|n| n.dur).collect::<Vec<_>>(), [10, 1]);
    }

    #[test]
    fn chords_sort_and_separate() {
        let mut t = vec![trk(&[(0, 10, 64, 1), (0, 10, 60, 1), (0, 10, 67, 1)])];
        let sc = Scope::track(0, 0, 10);
        sort_chord(&mut t, &sc, false);
        separate_chord(&mut t, &sc, 5);
        assert_eq!(t[0].notes.iter().map(|n| (n.tick, n.pitch)).collect::<Vec<_>>(), [(0, 60), (5, 64), (10, 67)]);
    }

    #[test]
    fn quantize_strength_and_swing() {
        // 16th grid, full strength: 130 -> 120, 250 -> 240, 5 -> 0
        let mut t = vec![trk(&[(130, 50, 60, 100), (250, 50, 60, 100), (5, 50, 60, 100)])];
        let o = QntzOpts { grid: 2, strength: 100, sens: 100, swing: 50, gate: 100, vel: 100 };
        quantize(&mut t, &Scope::track(0, 0, 1920), o, 480, |_| (0, 1920));
        assert_eq!(t[0].notes.iter().map(|n| n.tick).collect::<Vec<_>>(), [0, 120, 240]);
        // 8th grid, strength 50: 100 is caught by 0 -> 50
        let o = QntzOpts { grid: 4, strength: 50, sens: 100, swing: 75, gate: 50, vel: 120 };
        let mut t = vec![trk(&[(100, 50, 60, 100)])];
        quantize(&mut t, &Scope::track(0, 0, 1920), o, 480, |_| (0, 1920));
        assert_eq!(t[0].notes[0].tick, 50);
        // the 2nd 8th (240, an odd index): 250 -> 245, swung by 240 x 25 / 50 = 120 -> 365; gate 25, vel 120
        let mut t = vec![trk(&[(250, 50, 60, 100)])];
        quantize(&mut t, &Scope::track(0, 0, 1920), o, 480, |_| (0, 1920));
        assert_eq!((t[0].notes[0].tick, t[0].notes[0].dur, t[0].notes[0].vel), (365, 25, 120));
    }

    #[test]
    fn thin_and_stretch() {
        let cc = |t: u32, v: u8| Event::Chan { t, data: vec![0xB0, 11, v] };
        let mut t = vec![EditTrack { others: (0..7).map(|i| cc(i * 10, i as u8)).collect(), ..Default::default() }];
        thin_out(&mut t, &Scope::track(0, 0, 1000), CtrlKind::Cc(11, 11));
        assert_eq!(t[0].others.iter().map(|e| e.t()).collect::<Vec<_>>(), [0, 10, 30, 50, 60]);
        let mut t = vec![trk(&[(100, 40, 60, 1), (300, 40, 60, 1), (900, 40, 60, 1)])];
        stretch_time(&mut t, &Scope::track(0, 100, 500), 200, 10);
        assert_eq!(t[0].notes.iter().map(|n| (n.tick, n.dur)).collect::<Vec<_>>(), [(110, 80), (510, 80), (900, 40)]);
    }

    #[test]
    fn measures_and_chords() {
        let mut t = vec![trk(&[(0, 10, 60, 1), (1920, 10, 62, 1)])];
        create_measure(&mut t, 0, 480, 1920, 3, 4, 2, (4, 4));
        assert_eq!(t[0].notes[1].tick, 1920 + 2 * 1440);
        delete_measure(&mut t, 1920, 1920 + 2 * 1440);
        assert_eq!(t[0].notes[1].tick, 1920);
        let mut c = vec![EditTrack { others: vec![Event::Meta { t: 0, mtype: 0x7F, data: vec![0x43, 0x7B, 0x01, 0x31, 0x00, 0x7F, 0x7F] }], ..Default::default() }];
        transpose_chord(&mut c, 0, 10, 2); // C → D
        let Event::Meta { data, .. } = &c[0].others[0] else { panic!() };
        assert_eq!(data[3], 0x32);
    }
}
