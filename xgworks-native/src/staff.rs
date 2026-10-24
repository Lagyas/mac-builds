//! Staff window (Staff.hlp, ref. orig_shots/staff_1.png): the Piano Roll's
//! time axis and tools, but the track drawn as notation — clef, key and
//! time signature on the left, note heads on a 5-line staff (treble, bass
//! or grand staff), stems, flags / beams within a beat, dots, ties across
//! bar lines, accidentals by the key signature, rests in the gaps.
//! Glyphs (clefs, accidentals, rests, flags, digits) come from XGworks'
//! own XGmusic.ttf (XG4), symbol code points U+F0xx; heads, stems, beams
//! and ties are drawn.
//!
//! The roll works on a "virtual pitch" (row); here a row is a diatonic step
//! (C = 0, D = 1 … one octave = 7), so the roll's linear y mapping places
//! notes on lines and spaces (`key_h` = half a staff space).

use eframe::egui::{self, Color32, FontFamily, FontId, Painter, Pos2, Stroke, Vec2};
use xgworks_core::EditNote;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Clef {
    Treble,
    Bass,
    Grand,
}

#[derive(Clone, Copy, Debug)]
pub struct StaffInfo {
    /// Key signature: sharps (+) / flats (−).
    pub sf: i8,
    pub ts_num: u32,
    pub ts_den: u32,
    pub clef: Clef,
    /// Written = sounding + this (guitar / bass are written an octave up).
    pub written: i8,
}

impl Default for StaffInfo {
    fn default() -> Self {
        StaffInfo { sf: 0, ts_num: 4, ts_den: 4, clef: Clef::Treble, written: 0 }
    }
}

/// Lines of the treble (E4 G4 B4 D5 F5) and bass (G2 B2 D3 F3 A3) staff, as steps.
pub const TREBLE_LINES: [i32; 5] = [37, 39, 41, 43, 45];
pub const BASS_LINES: [i32; 5] = [25, 27, 29, 31, 33];
const SHARP_ORDER: [usize; 7] = [3, 0, 4, 1, 5, 2, 6]; // F C G D A E B (letters C=0)
const FLAT_ORDER: [usize; 7] = [6, 2, 5, 1, 4, 0, 3]; // B E A D G C F
const LETTER_PC: [i32; 7] = [0, 2, 4, 5, 7, 9, 11];

/// Alteration the key gives a letter (−1, 0, +1).
pub fn key_alt(sf: i8, letter: usize) -> i32 {
    if sf > 0 {
        if SHARP_ORDER[..sf as usize].contains(&letter) {
            return 1;
        }
    } else if sf < 0 && FLAT_ORDER[..(-sf) as usize].contains(&letter) {
        return -1;
    }
    0
}

/// (step, alteration) of a MIDI pitch, spelled with sharps in sharp keys
/// (and C), flats in flat keys.
pub fn spell(p: u8, sf: i8) -> (i32, i32) {
    let pc = p as i32 % 12;
    let oct = p as i32 / 12;
    let (letter, alt) = if sf < 0 {
        [(0, 0), (1, -1), (1, 0), (2, -1), (2, 0), (3, 0), (4, -1), (4, 0), (5, -1), (5, 0), (6, -1), (6, 0)][pc as usize]
    } else {
        [(0, 0), (0, 1), (1, 0), (1, 1), (2, 0), (3, 0), (3, 1), (4, 0), (4, 1), (5, 0), (5, 1), (6, 0)][pc as usize]
    };
    // a key's own spelling wins (E# / Cb are not used)
    (oct * 7 + letter, alt)
}

/// The pitch a step gets in the key (for entering / moving notes).
pub fn pitch_of_step(step: i32, sf: i8) -> u8 {
    let (oct, letter) = (step.div_euclid(7), step.rem_euclid(7) as usize);
    (oct * 12 + LETTER_PC[letter] + key_alt(sf, letter)).clamp(0, 127) as u8
}

/// Clef for a track: treble / bass by the notes' middle, grand for a wide range.
pub fn clef_for(notes: &[EditNote]) -> Clef {
    if notes.is_empty() {
        return Clef::Treble;
    }
    let mut p: Vec<u8> = notes.iter().map(|n| n.pitch).collect();
    p.sort();
    let (lo, hi, mid) = (p[p.len() / 10], p[p.len() * 9 / 10], p[p.len() / 2]);
    if lo < 53 && hi > 67 {
        Clef::Grand
    } else if mid < 60 {
        Clef::Bass
    } else {
        Clef::Treble
    }
}

/// Note values (ticks for ppqn) from the whole note down to the 32nd:
/// (ticks, flags, dotted, open head, stem).
fn values(ppqn: u32) -> Vec<(u32, u8, bool, bool, bool)> {
    let q = ppqn;
    vec![
        (q * 4, 0, false, true, false),
        (q * 3, 0, true, true, true),
        (q * 2, 0, false, true, true),
        (q * 3 / 2, 0, true, false, true),
        (q, 0, false, false, true),
        (q * 3 / 4, 1, true, false, true),
        (q / 2, 1, false, false, true),
        (q * 3 / 8, 2, true, false, true),
        (q / 4, 2, false, false, true),
        (q / 8, 3, false, false, true),
    ]
}

/// Split `len` ticks into note values (largest first); tiny rests of the
/// grid are dropped.
fn decompose(len: u32, ppqn: u32) -> Vec<(u32, u8, bool, bool, bool)> {
    let vals = values(ppqn);
    let min = ppqn / 8;
    let mut out = Vec::new();
    let mut left = len;
    while left + min / 2 >= min && out.len() < 8 {
        let Some(v) = vals.iter().find(|v| v.0 <= left + min / 2) else { break };
        out.push(*v);
        left = left.saturating_sub(v.0);
    }
    if out.is_empty() {
        out.push(*vals.last().unwrap());
    }
    out
}

/// One drawn head: a piece of a note between bar lines.
struct Piece {
    tick: u32,
    step: i32,
    alt: i32,
    flags: u8,
    dotted: bool,
    open: bool,
    stem: bool,
    sel: bool,
    /// Tied to the next piece of the same note.
    tie: bool,
}

fn glyph(code: u32) -> String {
    char::from_u32(0xF000 + code).map(|c| c.to_string()).unwrap_or_default()
}

fn music(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("xgmusic".into()))
}

pub fn has_font(ctx: &egui::Context) -> bool {
    ctx.fonts(|f| f.families().contains(&FontFamily::Name("xgmusic".into())))
}

/// Draw a music glyph with the top of its ink at `at` (every XGmusic glyph
/// starts at the font's ascender line, which is the top of egui's galley;
/// one em = 4 staff spaces at the sizes used here).
fn put(p: &Painter, at: Pos2, code: u32, size: f32, col: Color32) {
    let g = p.layout_no_wrap(glyph(code), music(size), col);
    p.galley(at, g, col);
}

/// Everything the staff drawing needs from the roll.
pub struct View<'a> {
    pub x_of: &'a dyn Fn(u32) -> f32,
    /// y of a step's line / space centre.
    pub y_step: &'a dyn Fn(i32) -> f32,
    pub half: f32,
    pub left: f32,
    pub right: f32,
    pub gx0: f32,
    pub ppqn: u32,
}

/// Extra rows between the treble and bass staff of the grand staff.
pub const GRAND_GAP: i32 = 4;

impl StaffInfo {
    /// Display row of a step (the grand staff leaves a gap below middle C).
    pub fn row(&self, step: i32) -> i32 {
        if self.clef == Clef::Grand && step < 35 {
            step - GRAND_GAP
        } else {
            step
        }
    }

    /// Step of a display row (rows in the gap count as middle C).
    pub fn step_of_row(&self, row: i32) -> i32 {
        if self.clef == Clef::Grand && row < 35 {
            (row + GRAND_GAP).min(35)
        } else {
            row
        }
    }

    pub fn bar(&self, ppqn: u32) -> u32 {
        (ppqn * 4 * self.ts_num / self.ts_den.max(1)).max(1)
    }

    fn staves(&self) -> Vec<[i32; 5]> {
        match self.clef {
            Clef::Treble => vec![TREBLE_LINES],
            Clef::Bass => vec![BASS_LINES],
            Clef::Grand => vec![TREBLE_LINES, BASS_LINES],
        }
    }

    /// Staff lines, clef, key and time signature (left of `gx0`).
    pub fn draw_staff(&self, p: &Painter, v: &View) {
        let font = has_font(p.ctx());
        let space = v.half * 2.0;
        let size = space * 4.0;
        for lines in self.staves() {
            for &l in &lines {
                let y = (v.y_step)(l);
                p.line_segment([Pos2::new(v.left + 4.0, y), Pos2::new(v.right, y)], Stroke::new(1.0_f32, Color32::BLACK));
            }
            let treble = lines == TREBLE_LINES;
            // clef: treble curls around G4 (line 2), bass dots around F3 (line 4)
            let mut x = v.left + 8.0;
            if font {
                let top = (v.y_step)(lines[4]);
                if treble {
                    put(p, Pos2::new(x, top - space * 1.5), 0x26, size, Color32::BLACK);
                } else {
                    put(p, Pos2::new(x, top), 0x27, size, Color32::BLACK);
                }
            } else {
                p.text(Pos2::new(x, (v.y_step)(lines[2])), egui::Align2::LEFT_CENTER, if treble { "G" } else { "F" }, FontId::proportional(space * 3.0), Color32::BLACK);
            }
            x += space * 3.2;
            // key signature
            let sharp_steps_t = [45, 42, 46, 43, 40, 44, 41];
            let flat_steps_t = [41, 44, 40, 43, 39, 42, 38];
            let shift = if treble { 0 } else { -14 };
            let n = self.sf.unsigned_abs() as usize;
            for k in 0..n.min(7) {
                let st = if self.sf > 0 { sharp_steps_t[k] } else { flat_steps_t[k] } + shift;
                accidental(p, Pos2::new(x, (v.y_step)(st)), if self.sf > 0 { 1 } else { -1 }, space, font);
                x += space * 1.0;
            }
            x += space * 0.5;
            // time signature
            let digits = |n: u32| n.to_string().chars().map(|c| 0x30 + c.to_digit(10).unwrap_or(0)).collect::<Vec<_>>();
            for (num, y_step) in [(self.ts_num, lines[3]), (self.ts_den, lines[1])] {
                let mut dx = x;
                for d in digits(num) {
                    if font {
                        // numerator fills the upper half of the staff, denominator the lower
                        let top = if y_step == lines[3] { (v.y_step)(lines[4]) } else { (v.y_step)(lines[2]) };
                        put(p, Pos2::new(dx, top), d, size, Color32::BLACK);
                    } else {
                        p.text(Pos2::new(dx, (v.y_step)(y_step)), egui::Align2::LEFT_CENTER, ((d - 0x30) as u8 + b'0') as char, FontId::proportional(space * 2.0), Color32::BLACK);
                    }
                    dx += space * 1.6;
                }
            }
        }
    }

    /// Bar lines through the staves.
    pub fn draw_bars(&self, p: &Painter, v: &View, t0: u32, t1: u32) {
        let bar = self.bar(v.ppqn);
        let staves = self.staves();
        let top = (v.y_step)(staves[0][4]);
        let bot = (v.y_step)(staves.last().unwrap()[0]);
        let mut t = t0 / bar * bar;
        while t <= t1 {
            let x = (v.x_of)(t);
            if x >= v.gx0 {
                p.line_segment([Pos2::new(x - 3.0, top), Pos2::new(x - 3.0, bot)], Stroke::new(1.0_f32, Color32::BLACK));
            }
            t += bar;
        }
    }

    /// Which staff a step is drawn on (grand staff: from middle C up = treble).
    fn lines_for(&self, step: i32) -> [i32; 5] {
        match self.clef {
            Clef::Treble => TREBLE_LINES,
            Clef::Bass => BASS_LINES,
            Clef::Grand => {
                if step >= 35 {
                    TREBLE_LINES
                } else {
                    BASS_LINES
                }
            }
        }
    }

    /// Notes, rests, ties. `sel(i)` = note i is selected.
    pub fn draw_notes(&self, p: &Painter, v: &View, notes: &[EditNote], sel: &dyn Fn(usize) -> bool, t0: u32, t1: u32) {
        let font = has_font(p.ctx());
        let bar = self.bar(v.ppqn);
        let space = v.half * 2.0;
        let gsize = space * 4.0;
        // pieces of every visible note, split at bar lines and into values
        let mut pieces: Vec<Piece> = Vec::new();
        for (i, n) in notes.iter().enumerate() {
            let end = n.tick + n.dur.max(1);
            if end < t0.saturating_sub(bar) || n.tick > t1 + bar {
                continue;
            }
            let (step, alt) = spell((n.pitch as i32 + self.written as i32).clamp(0, 127) as u8, self.sf);
            let mut t = n.tick;
            let mut first = true;
            while t < end {
                let bar_end = (t / bar + 1) * bar;
                let seg = end.min(bar_end) - t;
                let vals = decompose(seg, v.ppqn);
                for (k, val) in vals.iter().enumerate() {
                    let last_of_note = t + val.0 >= end || (k + 1 == vals.len() && bar_end >= end);
                    pieces.push(Piece {
                        tick: t,
                        step,
                        alt: if first { alt } else { i32::MIN },
                        flags: val.1,
                        dotted: val.2,
                        open: val.3,
                        stem: val.4,
                        sel: sel(i),
                        tie: !last_of_note,
                    });
                    first = false;
                    t += val.0;
                    if t >= end {
                        break;
                    }
                }
                t = t.max(bar_end.min(end));
                if t < end {
                    if let Some(l) = pieces.last_mut() {
                        l.tie = true;
                    }
                }
            }
        }
        pieces.sort_by_key(|q| (q.tick, q.step));

        // accidentals: shown when the alteration differs from the key or from
        // the same step earlier in the bar
        let mut shown: Vec<(u32, i32, i32)> = Vec::new(); // (bar, step, alt)
        let head_w = space * 1.25;
        for q in &pieces {
            if q.alt == i32::MIN {
                continue;
            }
            let b = q.tick / bar;
            let letter = q.step.rem_euclid(7) as usize;
            let prev = shown.iter().rev().find(|s| s.0 == b && s.1 == q.step).map(|s| s.2);
            let expect = prev.unwrap_or_else(|| key_alt(self.sf, letter));
            if q.alt != expect {
                let x = (v.x_of)(q.tick) - head_w * 0.5 - space * 0.9;
                if x > v.gx0 {
                    accidental(p, Pos2::new(x, (v.y_step)(q.step)), if q.alt == 0 { 0 } else { q.alt }, space, font);
                }
                shown.push((b, q.step, q.alt));
            }
        }

        // chords: pieces at the same tick on the same staff share a stem
        let mut i = 0;
        let mut chords: Vec<(usize, usize)> = Vec::new();
        while i < pieces.len() {
            let mut j = i + 1;
            while j < pieces.len() && pieces[j].tick == pieces[i].tick && self.lines_for(pieces[j].step) == self.lines_for(pieces[i].step) && pieces[j].flags == pieces[i].flags {
                j += 1;
            }
            chords.push((i, j));
            i = j;
        }
        // beams: consecutive flagged chords inside one beat
        let beat = (v.ppqn * 4 / self.ts_den.max(1)).max(1);
        let mut groups: Vec<Vec<usize>> = Vec::new();
        for (ci, &(a, _)) in chords.iter().enumerate() {
            let q = &pieces[a];
            let joinable = |g: &Vec<usize>| {
                let &last = g.last().unwrap();
                let lp = &pieces[chords[last].0];
                lp.flags > 0 && q.flags > 0 && lp.tick / beat == q.tick / beat && self.lines_for(lp.step) == self.lines_for(q.step)
            };
            if q.flags > 0 && groups.last().is_some_and(joinable) {
                groups.last_mut().unwrap().push(ci);
            } else {
                groups.push(vec![ci]);
            }
        }
        for g in &groups {
            let steps: Vec<i32> = g.iter().flat_map(|&c| (chords[c].0..chords[c].1).map(|k| pieces[k].step)).collect();
            let lines = self.lines_for(steps[0]);
            let avg = steps.iter().sum::<i32>() as f32 / steps.len() as f32;
            let up = avg < lines[2] as f32;
            let stem_len = space * 3.5;
            // stem end y per chord; a beam group shares one height
            let ends: Vec<f32> = g
                .iter()
                .map(|&c| {
                    let (a, b) = chords[c];
                    let ys: Vec<f32> = (a..b).map(|k| (v.y_step)(pieces[k].step)).collect();
                    if up {
                        ys.iter().cloned().fold(f32::MAX, f32::min) - stem_len
                    } else {
                        ys.iter().cloned().fold(f32::MIN, f32::max) + stem_len
                    }
                })
                .collect();
            let beam_y = if g.len() > 1 {
                Some(if up { ends.iter().cloned().fold(f32::MAX, f32::min) } else { ends.iter().cloned().fold(f32::MIN, f32::max) })
            } else {
                None
            };
            let mut stem_xs = Vec::new();
            for (gi, &c) in g.iter().enumerate() {
                let (a, b) = chords[c];
                let x = (v.x_of)(pieces[a].tick);
                if x < v.gx0 - space || x > v.right + space {
                    stem_xs.push(x);
                    continue;
                }
                for q in &pieces[a..b] {
                    let col = if q.sel { Color32::from_rgb(220, 0, 0) } else { Color32::BLACK };
                    let y = (v.y_step)(q.step);
                    head(p, Pos2::new(x, y), space, q.open, col);
                    if q.dotted {
                        let dy = if q.step % 2 == lines[0] % 2 { -v.half } else { 0.0 };
                        p.circle_filled(Pos2::new(x + head_w * 0.5 + space * 0.5, y + dy), space * 0.15, col);
                    }
                    ledger(p, x, q.step, &lines, v, head_w);
                    if q.tie {
                        let x2 = x + space * 2.4;
                        let dy = if up { space * 0.6 } else { -space * 0.6 };
                        tie(p, Pos2::new(x + head_w * 0.3, y + dy), Pos2::new(x2, y + dy), up, col);
                    }
                }
                let q = &pieces[a];
                if q.stem {
                    let col = if pieces[a..b].iter().any(|q| q.sel) { Color32::from_rgb(220, 0, 0) } else { Color32::BLACK };
                    let sx = if up { x + head_w * 0.5 - 0.5 } else { x - head_w * 0.5 + 0.5 };
                    let ys: Vec<f32> = (a..b).map(|k| (v.y_step)(pieces[k].step)).collect();
                    let far = if up { ys.iter().cloned().fold(f32::MIN, f32::max) } else { ys.iter().cloned().fold(f32::MAX, f32::min) };
                    let end = beam_y.unwrap_or(ends[gi]);
                    p.line_segment([Pos2::new(sx, far), Pos2::new(sx, end)], Stroke::new(1.2_f32, col));
                    if beam_y.is_none() && q.flags > 0 {
                        flag(p, Pos2::new(sx, end), q.flags, up, space, font, gsize, col);
                    }
                }
                stem_xs.push(if up { x + head_w * 0.5 - 0.5 } else { x - head_w * 0.5 + 0.5 });
            }
            if let Some(by) = beam_y {
                let n = g.iter().map(|&c| pieces[chords[c].0].flags).min().unwrap_or(1).max(1);
                let (x0, x1) = (stem_xs[0], *stem_xs.last().unwrap());
                for k in 0..n {
                    let y = if up { by + k as f32 * space * 0.75 } else { by - k as f32 * space * 0.75 };
                    let th = space * 0.45;
                    let r = egui::Rect::from_min_max(Pos2::new(x0, y - th / 2.0), Pos2::new(x1, y + th / 2.0));
                    p.rect_filled(r, 0.0, Color32::BLACK);
                }
            }
        }

        // rests: bar-wise gaps of this track
        let mut b = t0 / bar;
        while b * bar <= t1 {
            let (bs, be) = (b * bar, (b + 1) * bar);
            let mut covered: Vec<(u32, u32)> = notes.iter().filter(|n| n.tick < be && n.tick + n.dur > bs).map(|n| (n.tick.max(bs), (n.tick + n.dur).min(be))).collect();
            covered.sort();
            let mut t = bs;
            let mut gaps = Vec::new();
            for (s, e) in covered {
                if s > t {
                    gaps.push((t, s));
                }
                t = t.max(e);
            }
            if t < be {
                gaps.push((t, be));
            }
            for (s, e) in gaps {
                let mut at = s;
                for val in decompose(e - s, v.ppqn) {
                    if e - s < v.ppqn / 4 {
                        break;
                    }
                    let x = (v.x_of)(at);
                    if x > v.gx0 && x < v.right {
                        for lines in self.staves() {
                            rest(p, Pos2::new(x, (v.y_step)(lines[2])), &val, (e - s) >= bar, space, font, gsize, &lines, v);
                        }
                    }
                    at += val.0;
                }
            }
            b += 1;
        }
    }
}

fn head(p: &Painter, c: Pos2, space: f32, open: bool, col: Color32) {
    // an ellipse tilted like engraved note heads
    let (rx, ry, a) = (space * 0.62, space * 0.45, -0.35f32);
    let pts: Vec<Pos2> = (0..16)
        .map(|k| {
            let t = k as f32 / 16.0 * std::f32::consts::TAU;
            let (x, y) = (rx * t.cos(), ry * t.sin());
            Pos2::new(c.x + x * a.cos() - y * a.sin(), c.y + x * a.sin() + y * a.cos())
        })
        .collect();
    if open {
        p.add(egui::Shape::closed_line(pts, Stroke::new(1.6_f32, col)));
    } else {
        p.add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
    }
}

fn ledger(p: &Painter, x: f32, step: i32, lines: &[i32; 5], v: &View, head_w: f32) {
    let w = head_w * 0.9;
    let mut s = lines[4] + 2;
    while s <= step {
        let y = (v.y_step)(s);
        p.line_segment([Pos2::new(x - w, y), Pos2::new(x + w, y)], Stroke::new(1.0_f32, Color32::BLACK));
        s += 2;
    }
    let mut s = lines[0] - 2;
    while s >= step {
        let y = (v.y_step)(s);
        p.line_segment([Pos2::new(x - w, y), Pos2::new(x + w, y)], Stroke::new(1.0_f32, Color32::BLACK));
        s -= 2;
    }
}

fn tie(p: &Painter, a: Pos2, b: Pos2, below: bool, col: Color32) {
    let mid = Pos2::new((a.x + b.x) / 2.0, a.y + if below { 4.0 } else { -4.0 });
    let pts: Vec<Pos2> = (0..=12)
        .map(|k| {
            let t = k as f32 / 12.0;
            let u = 1.0 - t;
            Pos2::new(u * u * a.x + 2.0 * u * t * mid.x + t * t * b.x, u * u * a.y + 2.0 * u * t * mid.y + t * t * b.y)
        })
        .collect();
    p.add(egui::Shape::line(pts, Stroke::new(1.0_f32, col)));
}

/// # / b / natural at `c` (centre on the line / space).
fn accidental(p: &Painter, c: Pos2, alt: i32, space: f32, font: bool) {
    if font {
        let code = match alt {
            1 => 0x41,
            -1 => 0x42,
            2 => 0x3E,
            -2 => 0x3F,
            _ => 0x40,
        };
        let top = match alt {
            -1 => c.y - space * 1.75,
            _ => c.y - space * 1.42,
        };
        put(p, Pos2::new(c.x - space * 0.4, top), code, space * 4.0, Color32::BLACK);
    } else {
        let s = match alt {
            1 => "#",
            -1 => "b",
            _ => "n",
        };
        p.text(c, egui::Align2::CENTER_CENTER, s, FontId::proportional(space * 1.8), Color32::BLACK);
    }
}

#[allow(clippy::too_many_arguments)]
fn flag(p: &Painter, at: Pos2, n: u8, up: bool, space: f32, font: bool, size: f32, col: Color32) {
    if font {
        // up-stem flags 0x4B.. hang down from the stem end, down-stem 0x4E..
        let code = if up { 0x4A + n as u32 } else { 0x4D + n as u32 };
        let top = if up { at.y } else { at.y - space * 3.2 };
        put(p, Pos2::new(at.x - 0.5, top), code, size, col);
    } else {
        for k in 0..n {
            let y = at.y + if up { k as f32 * space * 0.7 } else { -(k as f32) * space * 0.7 };
            p.line_segment([Pos2::new(at.x, y), Pos2::new(at.x + space, y + if up { space } else { -space })], Stroke::new(1.5_f32, col));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn rest(p: &Painter, mid: Pos2, val: &(u32, u8, bool, bool, bool), whole_bar: bool, space: f32, font: bool, size: f32, lines: &[i32; 5], v: &View) {
    let col = Color32::from_gray(40);
    let (ticks, flags, dotted, open, _) = *val;
    let q = v.ppqn;
    if open || whole_bar {
        // whole rest hangs from line 4, half rest sits on line 3
        let whole = whole_bar || ticks >= q * 4;
        let y = if whole { (v.y_step)(lines[3]) } else { (v.y_step)(lines[2]) - space * 0.5 };
        let r = egui::Rect::from_min_size(Pos2::new(mid.x, y), Vec2::new(space * 1.1, space * 0.5));
        p.rect_filled(r, 0.0, col);
        return;
    }
    if font {
        let (code, up) = match flags {
            0 => (0x52, 1.65),
            1 => (0x53, 1.0),
            2 => (0x54, 1.0),
            _ => (0x55, 1.6),
        };
        put(p, Pos2::new(mid.x, mid.y - space * up), code, size, col);
    } else {
        p.text(mid, egui::Align2::LEFT_CENTER, "z", FontId::proportional(space * 2.0), col);
    }
    if dotted {
        p.circle_filled(Pos2::new(mid.x + space * 1.4, mid.y - space * 0.5), space * 0.15, col);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spelling_and_keys() {
        assert_eq!(spell(60, 0), (35, 0)); // C4
        assert_eq!(spell(61, 0), (35, 1)); // C#4
        assert_eq!(spell(61, -2), (36, -1)); // Db4
        assert_eq!(key_alt(1, 3), 1); // G major: F#
        assert_eq!(key_alt(-1, 6), -1); // F major: Bb
        assert_eq!(pitch_of_step(38, 1), 66); // F#4 in G major
        assert_eq!(pitch_of_step(35, 0), 60);
    }

    #[test]
    fn note_values() {
        let d = decompose(480 * 5 / 2, 480); // half + eighth
        assert_eq!(d.iter().map(|v| v.0).collect::<Vec<_>>(), vec![960, 240]);
        assert_eq!(decompose(720, 480)[0].2, true); // dotted quarter
        assert_eq!(clef_for(&[EditNote { tick: 0, dur: 1, pitch: 40, vel: 1 }]), Clef::Bass);
    }
}
