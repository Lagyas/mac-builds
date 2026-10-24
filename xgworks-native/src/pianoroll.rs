//! Piano Roll window, laid out like XGworks' "Piano Roll" (checked against the
//! original, see F:\rea\orig_shots\02_main.png):
//!
//! - top toolbar: track selector, vertical zoom, quantize FREE / 1/4 / 1/8 /
//!   1/16, dotted, triplet
//! - ruler with bar numbers and markers (FF 06)
//! - left: vertical keyboard, octave C labels in red (Yamaha naming, C3 = 60)
//! - note grid: grey rows for black keys, blue bar lines, notes as blue bars
//! - bottom controller pane: controller selector (the original's 44 entries,
//!   read from the running XGworks), value box, 127/64 scale, the events
//! - bottom tool bar: select / pencil / eraser, line shapes, Interval
//! - status: pitch under the mouse "A0(33)" and position "MMMM: B: TTT"

use eframe::egui::{self, Color32, Pos2, Rect, Sense, Stroke, Vec2};
use xgworks_core::{EditNote, EditTrack, Event};

use crate::icons;

/// Drum window (Drum.hlp, ref. orig_shots/drum_2.png): the same window as the
/// Piano Roll, but rows are drum instruments in the order of XGworks'
/// inst.ini [User] (Brush…, cymbals, hi-hats, snares, kicks, toms,
/// percussion), with the kit's instrument names on the left buttons, and
/// notes drawn as "drum marks" (balls sized by velocity, coloured by kind).
#[derive(Clone)]
pub struct DrumMap {
    /// Row (top = 0) → note number.
    pub order: [u8; 128],
    /// Instrument name per note ("************" = none).
    pub names: Vec<String>,
    /// Kind per note (inst.ini InstColor): 0 snare/brush, 1 percussion,
    /// 2 kick, 3 hi-hat, 4 cymbal, 5 tom.
    pub kinds: [u8; 128],
}

impl DrumMap {
    /// From inst.ini (order + kinds) and a name lookup for the kit.
    pub fn new(inst_ini: &str, name: impl Fn(u8) -> Option<String>) -> DrumMap {
        let mut order: Vec<u8> = Vec::new();
        let mut kinds = [1u8; 128];
        let mut in_user = false;
        for line in inst_ini.lines() {
            let l = line.trim();
            if l.starts_with('[') {
                in_user = l.eq_ignore_ascii_case("[User]");
                continue;
            }
            if !in_user {
                continue;
            }
            let Some((k, v)) = l.split_once('=') else { continue };
            if let Some(row) = k.strip_prefix("InstColor").and_then(|r| r.parse::<usize>().ok()) {
                if let Some(&n) = order.get(row) {
                    kinds[n as usize] = v.trim().parse().unwrap_or(1);
                }
            } else if k.parse::<usize>().is_ok() {
                let digits: String = v.chars().take_while(|c| c.is_ascii_digit()).collect();
                if let Ok(n) = digits.parse::<u8>() {
                    if n < 128 && !order.contains(&n) {
                        order.push(n);
                    }
                }
            }
        }
        if order.is_empty() {
            // no inst.ini: plain order, highest note on top like the Piano Roll
            order = (0..128u8).rev().collect();
            for (n, k) in kinds.iter_mut().enumerate() {
                *k = match n {
                    35 | 36 => 2,
                    42 | 44 | 46 => 3,
                    49 | 51 | 52 | 53 | 55 | 57 | 59 => 4,
                    41 | 43 | 45 | 47 | 48 | 50 => 5,
                    37 | 38 | 40 => 0,
                    _ => 1,
                };
            }
        }
        for n in 0..128u8 {
            if !order.contains(&n) {
                order.push(n);
            }
        }
        let mut o = [0u8; 128];
        o.copy_from_slice(&order[..128]);
        let names = (0..128u8).map(|n| name(n).filter(|s| !s.trim().is_empty()).unwrap_or_else(|| "************".into())).collect();
        DrumMap { order: o, names, kinds }
    }

    fn colour(&self, note: u8) -> Color32 {
        match self.kinds[note as usize & 127] {
            0 => Color32::from_rgb(20, 40, 230),
            2 => Color32::from_rgb(70, 70, 70),
            3 => Color32::from_rgb(255, 110, 20),
            4 => Color32::from_rgb(220, 40, 220),
            5 => Color32::from_rgb(70, 160, 255),
            _ => Color32::from_rgb(40, 170, 60),
        }
    }
}

const KEY_W: f32 = 120.0;
const RULER_H: f32 = 18.0;
const CTRL_H: f32 = 72.0;

const BLUE_NOTE: Color32 = Color32::from_rgb(0, 0, 220);
const SEL_NOTE: Color32 = Color32::from_rgb(220, 0, 0);
const BAR_LINE: Color32 = Color32::from_rgb(40, 40, 255);
const CTRL_LINE: Color32 = Color32::from_rgb(230, 0, 230);
const LABEL_RED: Color32 = Color32::from_rgb(220, 0, 0);
const MARKER: Color32 = Color32::from_rgb(0, 170, 190);

/// Controller list exactly as the original's combo box (CB_GETLBTEXT).
pub const CONTROLLERS: [&str; 44] = [
    "Velocity", "PolyAftr", "Program", "AftrTouch", "PitchBend", "Lyric", "Chord", "Tempo", "MstrVol",
    "BankMSB", "Mod", "BrthCtrl", "PrtTm", "EntryMSB", "Volume", "Pan", "Exp", "BankLSB", "EntryLSB",
    "Hold1", "Portament", "Sostenuto", "SoftPedal", "HrmncCntnt", "ReleaseTm", "AttackTm", "Brightness",
    "PrtCtrl", "RevSend", "ChoSend", "VarSend", "RPN Inc", "RPN Dec", "NRPN LSB", "NRPN MSB", "RPN LSB",
    "RPN MSB", "AllSndOff", "RstAllCtrl", "AllNoteOff", "OmniOff", "OmnOn", "Mono", "Poly",
];
/// CC numbers for entries 9.. of `CONTROLLERS`.
const CC_NUMS: [u8; 35] = [
    0, 1, 2, 5, 6, 7, 10, 11, 32, 38, 64, 65, 66, 67, 71, 72, 73, 74, 84, 91, 93, 94, 96, 97, 98, 99, 100, 101,
    120, 121, 123, 124, 125, 126, 127,
];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Ctl {
    Velocity,
    PolyAftr,
    Program,
    AftrTouch,
    PitchBend,
    Lyric,
    Chord,
    Tempo,
    MstrVol,
    Cc(u8),
}

fn ctl_of(i: usize) -> Ctl {
    match i {
        0 => Ctl::Velocity,
        1 => Ctl::PolyAftr,
        2 => Ctl::Program,
        3 => Ctl::AftrTouch,
        4 => Ctl::PitchBend,
        5 => Ctl::Lyric,
        6 => Ctl::Chord,
        7 => Ctl::Tempo,
        8 => Ctl::MstrVol,
        n => Ctl::Cc(CC_NUMS[n - 9]),
    }
}

impl Ctl {
    /// (min, max, centre line) of the value axis.
    fn range(self) -> (i32, i32, Option<i32>) {
        match self {
            Ctl::PitchBend => (-8192, 8191, Some(0)),
            Ctl::Tempo => (30, 300, None),
            Ctl::Cc(10) => (0, 127, Some(64)),
            _ => (0, 127, Some(64)),
        }
    }
    fn editable(self) -> bool {
        !matches!(self, Ctl::Lyric | Ctl::Chord | Ctl::PolyAftr | Ctl::MstrVol)
    }
    /// Value of an event of this controller on the track, if it is one.
    fn value(self, e: &Event, ch: u8) -> Option<i32> {
        match (self, e) {
            (Ctl::Cc(n), Event::Chan { data, .. }) if data.len() >= 3 && data[0] == 0xB0 | ch && data[1] == n => {
                Some(data[2] as i32)
            }
            (Ctl::Program, Event::Chan { data, .. }) if data.len() >= 2 && data[0] == 0xC0 | ch => Some(data[1] as i32),
            (Ctl::AftrTouch, Event::Chan { data, .. }) if data.len() >= 2 && data[0] == 0xD0 | ch => Some(data[1] as i32),
            (Ctl::PolyAftr, Event::Chan { data, .. }) if data.len() >= 3 && data[0] == 0xA0 | ch => Some(data[2] as i32),
            (Ctl::PitchBend, Event::Chan { data, .. }) if data.len() >= 3 && data[0] == 0xE0 | ch => {
                Some(((data[2] as i32) << 7 | data[1] as i32) - 8192)
            }
            (Ctl::Tempo, Event::Meta { mtype: 0x51, data, .. }) if data.len() == 3 => {
                let us = (data[0] as u32) << 16 | (data[1] as u32) << 8 | data[2] as u32;
                Some((60_000_000.0 / us.max(1) as f64).round() as i32)
            }
            (Ctl::Lyric, Event::Meta { mtype: 0x05, .. }) => Some(64),
            (Ctl::Chord, Event::Meta { mtype: 0x7F, .. }) => Some(64),
            (Ctl::MstrVol, Event::Sysex { data, .. })
                if data.len() >= 7 && data[0] == 0x43 && data[2] == 0x4C && data[3..6] == [0, 0, 4] =>
            {
                Some(data[6] as i32)
            }
            _ => None,
        }
    }
    fn make(self, t: u32, ch: u8, v: i32) -> Option<Event> {
        let b = |x: i32| x.clamp(0, 127) as u8;
        Some(match self {
            Ctl::Cc(n) => Event::Chan { t, data: vec![0xB0 | ch, n, b(v)] },
            Ctl::Program => Event::Chan { t, data: vec![0xC0 | ch, b(v)] },
            Ctl::AftrTouch => Event::Chan { t, data: vec![0xD0 | ch, b(v)] },
            Ctl::PitchBend => {
                let u = (v.clamp(-8192, 8191) + 8192) as u16;
                Event::Chan { t, data: vec![0xE0 | ch, (u & 0x7F) as u8, (u >> 7) as u8] }
            }
            Ctl::Tempo => {
                let us = (60_000_000.0 / v.clamp(20, 400) as f64).round() as u32;
                Event::Meta { t, mtype: 0x51, data: vec![(us >> 16) as u8, (us >> 8) as u8, us as u8] }
            }
            _ => return None,
        })
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum Tool {
    Select,
    Pencil,
    Eraser,
}

/// Pencil edit shapes of the controller pane (CTRL.HLP: straight line,
/// free hand, parabola A / B, sine curve; command IDs 47006..47010).
#[derive(Clone, Copy, PartialEq)]
enum Shape {
    Line,
    Free,
    ParabolaA,
    ParabolaB,
    Sine,
}

impl Shape {
    /// The curve from the press point (0, 0) to the drag point (1, 1), as
    /// XGworks.exe computes it (0x63e16f / 0x63e553 / 0x63de11): parabola A
    /// y = √x, parabola B y = x², sine y = sin(2πx) — one full period over the
    /// drag, the vertical drag being the amplitude; with Ctrl held when the
    /// drag started the sine's amplitude grows from 0 (y = x·sin(2πx)).
    fn curve(self, x: f64, ctrl: bool) -> f64 {
        match self {
            Shape::ParabolaA => x.sqrt(),
            Shape::ParabolaB => x * x,
            Shape::Sine if ctrl => x * (std::f64::consts::TAU * x).sin(),
            Shape::Sine => (std::f64::consts::TAU * x).sin(),
            Shape::Line | Shape::Free => x,
        }
    }
}

#[derive(Clone, Copy)]
enum Drag {
    None,
    Create { pitch: u8, start: u32, cur: u32 },
    Move { dtick: i64, dpitch: i32, anchor: usize },
    Resize { anchor: usize, dlen: i64 },
    Rubber { from: Pos2 },
    Ctrl { from: Pos2 },
}

/// Per-window state.
pub struct RollState {
    pub track: usize,
    px_per_tick: f32,
    t0: f32,
    key_h: f32,
    top_pitch: f32,
    quant: u32, // 0 = FREE, else note value denominator 4/8/16
    dotted: bool,
    triplet: bool,
    tool: Tool,
    shape: Shape,
    /// Ctrl was held when the controller drag started (growing sine).
    curve_ctrl: bool,
    interval: u32,
    ctl: usize,
    ctl_value: i32,
    sel: Vec<usize>,
    drag: Drag,
    free_path: Vec<Pos2>,
    hover: String,
    pub new_vel: u8,
    fitted: bool,
    ppqn: u32,
    /// Edit cursor (paste position), set by clicking the ruler or empty grid.
    cursor: u32,
    undo: Vec<(Vec<EditNote>, Vec<Event>)>,
    redo: Vec<(Vec<EditNote>, Vec<Event>)>,
    /// Some = Drum window (rows = drum instruments), refreshed by the app.
    pub drum: Option<DrumMap>,
    /// Some = Staff window (notation), refreshed by the app.
    pub staff: Option<crate::staff::StaffInfo>,
    /// Staff window: "Print / Export…" was clicked (the app writes the SVG).
    pub export_req: bool,
    /// Note palette (ShortCut.hlp): length of notes the pencil enters, as
    /// (quarter-note multiple ×12, so triplets are whole numbers); 0 = the grid.
    pub note_len: u32,
    /// Edit ▸ Repeat (Alt+R): [from bar, to bar, insert bar, times].
    repeat: Option<[u32; 4]>,
}

impl RollState {
    pub fn new_staff(track: usize, info: crate::staff::StaffInfo) -> Self {
        let mut st = RollState::new(track);
        st.key_h = 5.0;
        st.quant = 8;
        st.staff = Some(info);
        st
    }

    pub fn new_drum(track: usize, map: DrumMap) -> Self {
        let mut st = RollState::new(track);
        st.key_h = 24.0;
        st.tool = Tool::Pencil;
        st.drum = Some(map);
        st
    }

    pub fn new(track: usize) -> Self {
        RollState {
            track,
            px_per_tick: 0.25,
            t0: 0.0,
            key_h: 10.0,
            top_pitch: 84.0,
            quant: 16,
            dotted: false,
            triplet: false,
            tool: Tool::Pencil,
            shape: Shape::Free,
            curve_ctrl: false,
            interval: 200,
            ctl: 0,
            ctl_value: 64,
            sel: Vec::new(),
            drag: Drag::None,
            free_path: Vec::new(),
            hover: String::new(),
            new_vel: 100,
            fitted: false,
            ppqn: 480,
            cursor: 0,
            undo: Vec::new(),
            redo: Vec::new(),
            drum: None,
            staff: None,
            export_req: false,
            note_len: 0,
            repeat: None,
        }
    }
}

/// Song context a roll needs besides its track.
pub struct RollCtx<'a> {
    /// Track indices listed in the selector (conductor hidden), display number = position + 1.
    pub visible: &'a [usize],
    pub ppqn: u32,
    pub total_ticks: u32,
    pub markers: &'a [(u32, String)],
    pub track_names: &'a [String],
    pub play_tick: Option<f32>,
    /// Shared note clipboard (all piano rolls): notes relative to tick 0.
    pub clipboard: &'a std::cell::RefCell<Vec<EditNote>>,
    /// Play a note of the track for monitoring (note, velocity).
    pub monitor: &'a dyn Fn(u8, u8),
}

/// The note palette: (label, length in twelfths of a quarter note, key, with Ctrl).
pub const PALETTE: [(&str, u32, egui::Key, bool); 15] = [
    ("1", 48, egui::Key::F2, false),
    ("1/2.", 36, egui::Key::F3, false),
    ("1/2", 24, egui::Key::F4, false),
    ("1/4.", 18, egui::Key::F5, false),
    ("1/4", 12, egui::Key::F6, false),
    ("1/8.", 9, egui::Key::F7, false),
    ("1/8", 6, egui::Key::F8, false),
    ("1/16.", 9 / 2, egui::Key::F9, false),
    ("1/16", 3, egui::Key::F10, false),
    ("1/32.", 2, egui::Key::F11, false),
    ("1/32", 1, egui::Key::F12, false),
    ("1/2 3", 16, egui::Key::F1, true),
    ("1/4 3", 8, egui::Key::F2, true),
    ("1/8 3", 4, egui::Key::F3, true),
    ("1/16 3", 2, egui::Key::F5, true),
];
/// Shift+F2..F12: velocity ppp..fff (List.hlp values).
const VEL_KEYS: [(egui::Key, u8, &str); 11] = [
    (egui::Key::F2, 8, "ppp"),
    (egui::Key::F3, 24, "pp"),
    (egui::Key::F4, 40, "p"),
    (egui::Key::F5, 56, "mp"),
    (egui::Key::F6, 72, "mf"),
    (egui::Key::F7, 88, "f"),
    (egui::Key::F8, 104, "ff"),
    (egui::Key::F9, 120, "fff"),
    (egui::Key::F10, 127, "fff"),
    (egui::Key::F11, 127, "fff"),
    (egui::Key::F12, 127, "fff"),
];

/// Sort notes by (tick, pitch) and remap the selection to the new order.
fn sort_notes(notes: &mut Vec<EditNote>, sel: &mut Vec<usize>) {
    let mut idx: Vec<usize> = (0..notes.len()).collect();
    idx.sort_by_key(|&i| (notes[i].tick, notes[i].pitch));
    let mut pos = vec![0usize; notes.len()];
    for (new, &old) in idx.iter().enumerate() {
        pos[old] = new;
    }
    let sorted: Vec<EditNote> = idx.iter().map(|&i| notes[i].clone()).collect();
    *notes = sorted;
    for s in sel.iter_mut() {
        *s = pos[*s];
    }
    sel.sort();
    sel.dedup();
}

impl RollState {
    fn snapshot(&mut self, tr: &EditTrack) {
        self.undo.push((tr.notes.clone(), tr.others.clone()));
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
}

pub fn note_name(p: u8) -> String {
    const N: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    format!("{}{}", N[p as usize % 12], p as i32 / 12 - 2)
}

fn is_black(p: u8) -> bool {
    matches!(p % 12, 1 | 3 | 6 | 8 | 10)
}

/// "MMMM: B: TTT" (1-based measure and beat, 4/4).
pub fn pos_text(tick: u32, ppqn: u32) -> String {
    let bar = ppqn * 4;
    format!("{:04}: {}: {:03}", tick / bar + 1, (tick % bar) / ppqn + 1, tick % ppqn)
}

impl RollState {
    fn step(&self, ppqn: u32) -> u32 {
        if self.quant == 0 {
            return 1;
        }
        let mut s = ppqn as f32 * 4.0 / self.quant as f32;
        if self.dotted {
            s *= 1.5;
        }
        if self.triplet {
            s *= 2.0 / 3.0;
        }
        (s.round() as u32).max(1)
    }

    /// Draw the whole roll; returns true when the track was edited.
    pub fn ui(&mut self, ui: &mut egui::Ui, tracks: &mut [EditTrack], cx: &RollCtx) -> bool {
        let mut changed = false;
        if self.track >= tracks.len() {
            return false;
        }
        self.toolbar(ui, cx);
        let ppqn = cx.ppqn.max(1);
        self.ppqn = ppqn;
        let step = self.step(ppqn);
        // "virtual pitch": the Drum window's row order mapped onto 127..0
        let mut vmap = [0u8; 128];
        let mut rmap = [0u8; 128];
        for i in 0..128u8 {
            vmap[i as usize] = i;
            rmap[i as usize] = i;
        }
        if let Some(d) = &self.drum {
            for (row, &note) in d.order.iter().enumerate() {
                let v = 127 - row as u8;
                vmap[note as usize] = v;
                rmap[v as usize] = note;
            }
        }
        if let Some(info) = &self.staff {
            // rows = diatonic steps (lines and spaces)
            let w = info.written as i32;
            for i in 0..128u8 {
                let step = crate::staff::spell((i as i32 + w).clamp(0, 127) as u8, info.sf).0;
                vmap[i as usize] = info.row(step).clamp(0, 127) as u8;
                rmap[i as usize] = (crate::staff::pitch_of_step(info.step_of_row(i as i32), info.sf) as i32 - w).clamp(0, 127) as u8;
            }
        }
        let is_drum = self.drum.is_some();
        let is_staff = self.staff.is_some();
        let vp = move |n: u8| vmap[n as usize & 127];
        let rp = move |v: u8| rmap[v as usize & 127];

        // layout: ruler, notes, controller pane, tool bar, status
        let avail = ui.available_rect_before_wrap();
        let bottom_h = CTRL_H + 30.0 + 18.0;
        let ruler = Rect::from_min_size(avail.min, Vec2::new(avail.width(), RULER_H));
        let notes = Rect::from_min_max(Pos2::new(avail.left(), ruler.bottom()), Pos2::new(avail.right(), avail.bottom() - bottom_h));
        let ctrl = Rect::from_min_size(Pos2::new(avail.left(), notes.bottom() + 2.0), Vec2::new(avail.width(), CTRL_H));
        let grid_w = (avail.width() - KEY_W).max(50.0);
        if !self.fitted {
            self.px_per_tick = (grid_w / cx.total_ticks.max(ppqn * 16) as f32).max(0.05);
            let t = &tracks[self.track];
            if let Some(info) = &self.staff {
                let rows = notes.height() / self.key_h;
                let centre = match info.clef {
                    crate::staff::Clef::Treble => 41.0,
                    crate::staff::Clef::Bass => 29.0,
                    crate::staff::Clef::Grand => 33.0,
                };
                self.top_pitch = (centre + rows / 2.0).clamp(rows.min(127.0), 127.0);
            } else if let (Some(lo), Some(hi)) = (t.notes.iter().map(|n| vp(n.pitch)).min(), t.notes.iter().map(|n| vp(n.pitch)).max()) {
                let rows = notes.height() / self.key_h;
                self.top_pitch = ((lo as f32 + hi as f32) / 2.0 + rows / 2.0).clamp(rows, 127.0);
            }
            self.fitted = true;
        }

        // ---- input: wheel = vertical scroll, Shift = horizontal, Ctrl = zoom
        if ui.rect_contains_pointer(Rect::from_min_max(ruler.min, ctrl.max)) {
            let (d, zoom, shift, ctrl_m, mouse) = ui.input(|i| {
                (i.raw_scroll_delta, i.zoom_delta(), i.modifiers.shift, i.modifiers.ctrl, i.pointer.hover_pos())
            });
            let gx0 = avail.left() + KEY_W;
            if ctrl_m && zoom != 1.0 {
                let mx = mouse.map_or(0.0, |p| (p.x - gx0).max(0.0));
                let at = self.t0 + mx / self.px_per_tick;
                self.px_per_tick = (self.px_per_tick * zoom).clamp(0.01, 4.0);
                self.t0 = (at - mx / self.px_per_tick).max(0.0);
            } else if shift || d.x != 0.0 {
                let dx = if d.x != 0.0 { d.x } else { d.y };
                self.t0 = (self.t0 - dx / self.px_per_tick).clamp(0.0, cx.total_ticks as f32);
            } else if d.y != 0.0 {
                let rows = notes.height() / self.key_h;
                self.top_pitch = (self.top_pitch + d.y / self.key_h).clamp(rows.min(127.0), 127.0);
            }
        }

        let gx0 = avail.left() + KEY_W;
        let (t0, ppt) = (self.t0, self.px_per_tick);
        let x_of = move |t: u32| gx0 + (t as f32 - t0) * ppt;
        let tick_of = move |x: f32| ((x - gx0) / ppt + t0).max(0.0) as u32;
        let snap = |t: u32| if step <= 1 { t } else { (t + step / 2) / step * step };
        let key_h = self.key_h;
        let top = self.top_pitch;
        let y_of = |p: u8| notes.top() + (top - p as f32 - 1.0) * key_h;
        let pitch_of = |y: f32| (top - 1.0 - ((y - notes.top()) / key_h).floor()).clamp(0.0, 127.0) as u8;
        let bar = ppqn * 4;

        // ---------------- ruler
        let p = ui.painter_at(ruler);
        p.rect_filled(ruler, 0.0, Color32::WHITE);
        let mut b = (t0 as u32) / bar;
        let label_every = ((40.0 / (bar as f32 * ppt)).ceil() as u32).max(1);
        loop {
            let x = x_of(b * bar);
            if x > ruler.right() {
                break;
            }
            if x >= gx0 {
                p.line_segment([Pos2::new(x, ruler.bottom() - 6.0), Pos2::new(x, ruler.bottom())], Stroke::new(1.0_f32, Color32::GRAY));
                if b % label_every == 0 {
                    p.text(Pos2::new(x + 2.0, ruler.bottom() - 2.0), egui::Align2::LEFT_BOTTOM, format!("{}", b + 1),
                        egui::FontId::monospace(10.0), Color32::BLACK);
                }
            }
            b += 1;
        }
        for (t, name) in cx.markers {
            let x = x_of(*t);
            if x >= gx0 && x <= ruler.right() {
                p.add(egui::Shape::convex_polygon(
                    vec![Pos2::new(x - 4.0, ruler.bottom() - 6.0), Pos2::new(x + 4.0, ruler.bottom() - 6.0), Pos2::new(x, ruler.bottom())],
                    MARKER, Stroke::NONE));
                p.text(Pos2::new(x, ruler.top()), egui::Align2::CENTER_TOP, name, egui::FontId::proportional(10.0), Color32::BLACK);
            }
        }

        let rr = ui.interact(Rect::from_min_max(Pos2::new(gx0, ruler.top()), ruler.max), ui.id().with(("roll_ruler", self.track)), Sense::click());
        if let Some(pos) = rr.interact_pointer_pos().filter(|_| rr.clicked()) {
            self.cursor = snap(tick_of(pos.x));
        }

        // ---------------- keyboard + grid
        let p = ui.painter_at(notes);
        p.rect_filled(notes, 0.0, Color32::WHITE);
        let kb = Rect::from_min_max(notes.min, Pos2::new(gx0, notes.bottom()));
        let first = pitch_of(notes.bottom() - 1.0);
        let last = pitch_of(notes.top() + 1.0);
        for pi in first..=last {
            if is_staff {
                break;
            }
            let y = y_of(pi);
            let row = Rect::from_min_size(Pos2::new(gx0, y), Vec2::new(notes.right() - gx0, key_h));
            if let Some(d) = &self.drum {
                // striped rows, instrument buttons with the kit's names
                let note = rp(pi);
                if pi % 2 == 0 {
                    p.rect_filled(row, 0.0, Color32::from_gray(200));
                }
                let btn = Rect::from_min_max(Pos2::new(kb.left() + 1.0, y + 1.0), Pos2::new(gx0 - 2.0, y + key_h - 1.0));
                p.rect_filled(btn, 0.0, Color32::from_gray(236));
                p.rect_stroke(btn, 0.0, Stroke::new(1.0_f32, Color32::from_gray(120)));
                let sw = Rect::from_min_size(Pos2::new(btn.left() + 3.0, btn.center().y - 5.0), Vec2::splat(10.0));
                p.circle_filled(sw.center(), 4.5, d.colour(note));
                p.text(Pos2::new(sw.right() + 4.0, btn.center().y), egui::Align2::LEFT_CENTER, &d.names[note as usize],
                    egui::FontId::proportional((key_h * 0.55).clamp(8.0, 12.0)), Color32::BLACK);
                continue;
            }
            if is_black(pi) {
                p.rect_filled(row, 0.0, Color32::from_gray(192));
                p.rect_filled(Rect::from_min_size(Pos2::new(kb.left(), y + 1.0), Vec2::new(KEY_W * 0.5, key_h - 2.0)), 1.0, Color32::BLACK);
            }
            p.line_segment([Pos2::new(gx0, y + key_h), Pos2::new(notes.right(), y + key_h)], Stroke::new(1.0_f32, Color32::from_gray(225)));
            if pi % 12 == 0 || pi % 12 == 5 {
                // white-key boundary under C and F
                p.line_segment([Pos2::new(kb.left(), y + key_h), Pos2::new(gx0, y + key_h)], Stroke::new(1.0_f32, Color32::BLACK));
            }
            if pi % 12 == 0 {
                p.text(Pos2::new(gx0 - 22.0, y + key_h / 2.0), egui::Align2::CENTER_CENTER, note_name(pi),
                    egui::FontId::monospace(9.0), LABEL_RED);
            }
        }
        p.line_segment([Pos2::new(gx0, notes.top()), Pos2::new(gx0, notes.bottom())], Stroke::new(1.0_f32, Color32::BLACK));
        // beat / bar lines
        let mut t = (t0 as u32) / ppqn * ppqn;
        while x_of(t) <= notes.right() {
            let x = x_of(t);
            if x > gx0 {
                let col = if is_staff { Color32::from_gray(225) } else if t % bar == 0 { BAR_LINE } else { Color32::from_gray(205) };
                p.line_segment([Pos2::new(x, notes.top()), Pos2::new(x, notes.bottom())], Stroke::new(1.0_f32, col));
            }
            t += ppqn;
        }

        // staff: lines, clef, key / time signature, bar lines
        let staff_info = self.staff;
        let y_step = |st: i32| y_of(staff_info.map_or(st, |i| i.row(st)).clamp(0, 127) as u8) + key_h / 2.0;
        let t_end = tick_of(notes.right());
        if let Some(info) = &self.staff {
            let v = crate::staff::View { x_of: &x_of, y_step: &y_step, half: key_h, left: notes.left(), right: notes.right(), gx0, ppqn };
            info.draw_staff(&p, &v);
            info.draw_bars(&p, &v, t0 as u32, t_end);
        }

        // notes
        let trk_ch = tracks[self.track].channel & 0x0F;
        let mark_r = |vel: u8| (key_h * 0.5 - 2.0).max(3.0) * (0.45 + 0.55 * vel as f32 / 127.0);
        let note_rect = |n: &EditNote| {
            if is_staff {
                let c = Pos2::new(x_of(n.tick), y_of(vp(n.pitch)) + key_h / 2.0);
                return Rect::from_center_size(c, Vec2::new(key_h * 2.6, key_h * 2.0));
            }
            if is_drum {
                let c = Pos2::new(x_of(n.tick), y_of(vp(n.pitch)) + key_h / 2.0);
                return Rect::from_center_size(c, Vec2::splat(2.0 * mark_r(n.vel).max(4.0)));
            }
            let x = x_of(n.tick).max(gx0);
            Rect::from_min_max(Pos2::new(x, y_of(vp(n.pitch)) + 1.0), Pos2::new(x_of(n.tick + n.dur).max(x + 2.0), y_of(vp(n.pitch)) + key_h - 1.0))
        };
        {
            let trk = &tracks[self.track];
            if let Some(info) = &self.staff {
                let v = crate::staff::View { x_of: &x_of, y_step: &y_step, half: key_h, left: notes.left(), right: notes.right(), gx0, ppqn };
                let sel = &self.sel;
                info.draw_notes(&p, &v, &trk.notes, &|i| sel.contains(&i), t0 as u32, t_end);
            }
            for (i, n) in trk.notes.iter().enumerate() {
                if is_staff {
                    break;
                }
                let r = note_rect(n);
                if r.right() < gx0 || r.left() > notes.right() || r.bottom() < notes.top() || r.top() > notes.bottom() {
                    continue;
                }
                if let Some(d) = &self.drum {
                    let col = if self.sel.contains(&i) { SEL_NOTE } else { d.colour(n.pitch) };
                    let rad = mark_r(n.vel);
                    p.circle_filled(r.center(), rad, col);
                    p.circle_filled(r.center() - Vec2::splat(rad * 0.35), rad * 0.3, Color32::from_white_alpha(140));
                    continue;
                }
                p.rect_filled(r, 2.0, if self.sel.contains(&i) { SEL_NOTE } else { BLUE_NOTE });
            }
        }

        // note-area interaction
        let grid_rect = Rect::from_min_max(Pos2::new(gx0, notes.top()), notes.max);
        let resp = ui.interact(grid_rect, ui.id().with(("roll_grid", self.track)), Sense::click_and_drag());
        let kb_resp = ui.interact(kb, ui.id().with(("roll_kb", self.track)), Sense::click());
        if let Some(pos) = kb_resp.interact_pointer_pos().filter(|_| kb_resp.clicked()) {
            let n = rp(pitch_of(pos.y));
            (cx.monitor)(n, self.new_vel);
            if is_drum && kb_resp.double_clicked() {
                // double-click an instrument button = select its whole row
                self.sel = (0..tracks[self.track].notes.len()).filter(|&i| tracks[self.track].notes[i].pitch == n).collect();
            }
        }
        if let Some(pos) = resp.hover_pos().or(kb_resp.hover_pos()) {
            let pi = rp(pitch_of(pos.y));
            self.hover = match &self.drum {
                Some(d) => format!("{}  {}({})", d.names[pi as usize].trim(), note_name(pi), pi),
                None => format!("{}({})", note_name(pi), pi),
            };
            if resp.hovered() {
                self.hover = format!("{}   {}", self.hover, pos_text(tick_of(pos.x), ppqn));
            }
        }
        let hit = |pos: Pos2, notes: &[EditNote]| notes.iter().rposition(|n| note_rect(n).contains(pos));
        let near_end = |pos: Pos2, n: &EditNote| !is_drum && !is_staff && (note_rect(n).right() - pos.x).abs() <= 4.0;
        let tr = &mut tracks[self.track];
        // a click in the grid takes the keyboard back from the toolbar fields,
        // otherwise Del / Ctrl+C / Ctrl+V would keep going to a DragValue
        if resp.is_pointer_button_down_on() {
            ui.ctx().memory_mut(|m| {
                if let Some(id) = m.focused() {
                    m.surrender_focus(id);
                }
            });
        }
        let (shift, ctrl_m) = ui.input(|i| (i.modifiers.shift, i.modifiers.ctrl || i.modifiers.command));
        // where the button went down (drag starts are detected only after some
        // movement, by then the pointer may already have left the note)
        let press = ui.input(|i| i.pointer.press_origin());

        if let Some(pos) = resp.hover_pos() {
            // the tool's cursor, as the original (pencil / eraser from XGworks.exe)
            let on = hit(pos, &tr.notes);
            if on.is_some_and(|i| near_end(pos, &tr.notes[i])) && self.tool != Tool::Eraser {
                ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
            } else {
                match self.tool {
                    Tool::Pencil => crate::cursors::show(ui.ctx(), crate::cursors::Cur::Pencil),
                    Tool::Eraser => crate::cursors::show(ui.ctx(), crate::cursors::Cur::Eraser),
                    Tool::Select if on.is_some() => ui.ctx().set_cursor_icon(egui::CursorIcon::Grab),
                    Tool::Select => {}
                }
            }
        }

        // clicks
        if resp.secondary_clicked() || (resp.clicked() && self.tool == Tool::Eraser) {
            if let Some(pos) = resp.interact_pointer_pos() {
                if let Some(i) = hit(pos, &tr.notes) {
                    self.snapshot(tr);
                    tr.notes.remove(i);
                    self.sel.clear();
                    changed = true;
                }
            }
        } else if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                match hit(pos, &tr.notes) {
                    Some(i) if shift || ctrl_m => {
                        if let Some(k) = self.sel.iter().position(|&s| s == i) {
                            self.sel.remove(k);
                        } else {
                            self.sel.push(i);
                        }
                    }
                    Some(i) => {
                        self.sel = vec![i];
                        (cx.monitor)(tr.notes[i].pitch, tr.notes[i].vel);
                    }
                    None if self.tool == Tool::Pencil => {
                        // click on empty grid with the pencil: one note of the quantize length
                        self.snapshot(tr);
                        let raw = tick_of(pos.x);
                        let st = if step <= 1 { raw } else { raw / step * step };
                        let pitch = rp(pitch_of(pos.y));
                        let dur = if self.note_len > 0 {
                            (ppqn * self.note_len / 12).max(1)
                        } else if is_drum {
                            (step.max(ppqn / 4) / 2).max(1)
                        } else {
                            step.max(ppqn / 4)
                        };
                        tr.notes.push(EditNote { tick: st, dur, pitch, vel: self.new_vel });
                        (cx.monitor)(pitch, self.new_vel);
                        self.sel = vec![tr.notes.len() - 1];
                        self.cursor = st;
                        changed = true;
                    }
                    None => {
                        self.sel.clear();
                        self.cursor = snap(tick_of(pos.x));
                    }
                }
            }
        }

        // drags
        if resp.drag_started_by(egui::PointerButton::Primary) {
            if let Some(pos) = press.or(resp.interact_pointer_pos()) {
                let raw = tick_of(pos.x);
                let pi = pitch_of(pos.y); // virtual (row) pitch
                match (self.tool, hit(pos, &tr.notes)) {
                    (Tool::Eraser, _) => self.drag = Drag::Rubber { from: pos },
                    (_, Some(i)) => {
                        if !self.sel.contains(&i) {
                            self.sel = if shift || ctrl_m { let mut v = self.sel.clone(); v.push(i); v } else { vec![i] };
                        }
                        self.snapshot(tr);
                        let n = &tr.notes[i];
                        self.drag = if near_end(pos, n) {
                            Drag::Resize { anchor: i, dlen: (n.tick + n.dur) as i64 - raw as i64 }
                        } else {
                            Drag::Move { dtick: n.tick as i64 - raw as i64, dpitch: vp(n.pitch) as i32 - pi as i32, anchor: i }
                        };
                    }
                    (Tool::Pencil, None) if is_drum => self.drag = Drag::Rubber { from: pos },
                    (Tool::Pencil, None) => {
                        let st = if step <= 1 { raw } else { raw / step * step };
                        let len = if self.note_len > 0 { (ppqn * self.note_len / 12).max(1) } else { step.max(ppqn / 4) };
                        self.drag = Drag::Create { pitch: rp(pi), start: st, cur: st + len };
                    }
                    (Tool::Select, None) => self.drag = Drag::Rubber { from: pos },
                }
            }
        }
        if resp.dragged() {
            if let Some(pos) = resp.interact_pointer_pos() {
                match self.drag {
                    Drag::Move { dtick, dpitch, anchor } if anchor < tr.notes.len() => {
                        let nt = snap(((tick_of(pos.x) as i64 + dtick).max(0)) as u32) as i64;
                        let np = (pitch_of(pos.y) as i32 + dpitch).clamp(0, 127);
                        let (dt, dp) = (nt - tr.notes[anchor].tick as i64, np - vp(tr.notes[anchor].pitch) as i32);
                        if dt != 0 || dp != 0 {
                            for &i in &self.sel {
                                if let Some(n) = tr.notes.get_mut(i) {
                                    n.tick = (n.tick as i64 + dt).max(0) as u32;
                                    n.pitch = rp((vp(n.pitch) as i32 + dp).clamp(0, 127) as u8);
                                }
                            }
                            if dp != 0 {
                                if let Some(n) = tr.notes.get(anchor) {
                                    (cx.monitor)(n.pitch, n.vel);
                                }
                            }
                            changed = true;
                        }
                    }
                    Drag::Resize { anchor, dlen } if anchor < tr.notes.len() => {
                        let end = snap(((tick_of(pos.x) as i64 + dlen).max(0)) as u32) as i64;
                        let d = end - (tr.notes[anchor].tick + tr.notes[anchor].dur) as i64;
                        if d != 0 {
                            for &i in &self.sel {
                                if let Some(n) = tr.notes.get_mut(i) {
                                    n.dur = (n.dur as i64 + d).max(step.max(1) as i64) as u32;
                                }
                            }
                            changed = true;
                        }
                    }
                    Drag::Create { pitch, start, .. } => {
                        let cur = snap(tick_of(pos.x)).max(start + step.max(1));
                        self.drag = Drag::Create { pitch, start, cur };
                    }
                    Drag::Rubber { from } => {
                        let r = Rect::from_two_pos(from, pos);
                        p.rect_stroke(r, 0.0, Stroke::new(1.0_f32, Color32::BLACK));
                    }
                    _ => {}
                }
            }
        }
        if let Drag::Create { pitch, start, cur } = self.drag {
            let r = note_rect(&EditNote { tick: start, dur: cur - start, pitch, vel: self.new_vel });
            p.rect_stroke(r, 2.0, Stroke::new(1.5_f32, BLUE_NOTE));
        }
        if resp.drag_stopped() {
            match self.drag {
                Drag::Create { pitch, start, cur } => {
                    self.snapshot(tr);
                    tr.notes.push(EditNote { tick: start, dur: cur - start, pitch, vel: self.new_vel });
                    self.sel = vec![tr.notes.len() - 1];
                    changed = true;
                }
                Drag::Rubber { from } => {
                    if let Some(pos) = resp.interact_pointer_pos() {
                        let r = Rect::from_two_pos(from, pos);
                        let hits: Vec<usize> = (0..tr.notes.len()).filter(|&i| note_rect(&tr.notes[i]).intersects(r)).collect();
                        if self.tool == Tool::Eraser {
                            if !hits.is_empty() {
                                self.snapshot(tr);
                                let mut k = 0;
                                tr.notes.retain(|_| {
                                    k += 1;
                                    !hits.contains(&(k - 1))
                                });
                                changed = true;
                            }
                            self.sel.clear();
                        } else if shift || ctrl_m {
                            self.sel.extend(hits);
                        } else {
                            self.sel = hits;
                        }
                    }
                }
                Drag::Move { .. } | Drag::Resize { .. } => changed = true,
                _ => {}
            }
            self.drag = Drag::None;
            if changed {
                sort_notes(&mut tr.notes, &mut self.sel);
            }
        }

        // note palette keys (ShortCut.hlp): F2..F12 length, Ctrl = triplets, Shift = velocity
        if ui.ctx().input(|i| i.focused) && !ui.ctx().wants_keyboard_input() {
            for (_, len, key, ctrl) in PALETTE {
                if ui.input(|i| i.key_pressed(key) && i.modifiers.ctrl == ctrl && !i.modifiers.shift) {
                    self.note_len = len;
                }
            }
            for (key, vel, _) in VEL_KEYS {
                if ui.input(|i| i.key_pressed(key) && i.modifiers.shift) {
                    self.new_vel = vel;
                    // and the selected notes, like the original (Caps Lock off)
                    if !self.sel.is_empty() {
                        self.snapshot(tr);
                        for &i in &self.sel {
                            if let Some(n) = tr.notes.get_mut(i) {
                                n.vel = vel;
                            }
                        }
                        changed = true;
                    }
                }
            }
        }

        // keyboard: Del, Ctrl+A/C/X/V/Z/Y, arrows (only for the focused window)
        if ui.ctx().input(|i| i.focused) && !ui.ctx().wants_keyboard_input() && matches!(self.drag, Drag::None) {
            let ek = crate::clip::edit_keys(ui.ctx());
            let (up, down, left, right) = ui.input(|i| {
                let k = |key| i.key_pressed(key);
                (k(egui::Key::ArrowUp), k(egui::Key::ArrowDown), k(egui::Key::ArrowLeft), k(egui::Key::ArrowRight))
            });
            let del = ek.del || ui.input(|i| i.key_pressed(egui::Key::Backspace));
            let sel_notes = |tr: &EditTrack, sel: &[usize]| -> Vec<EditNote> {
                let v: Vec<EditNote> = sel.iter().filter_map(|&i| tr.notes.get(i).cloned()).collect();
                let t0 = v.iter().map(|n| n.tick).min().unwrap_or(0);
                v.into_iter().map(|mut n| { n.tick -= t0; n }).collect()
            };
            if ek.all {
                self.sel = (0..tr.notes.len()).collect();
            }
            if (ek.copy || ek.cut) && !self.sel.is_empty() {
                *cx.clipboard.borrow_mut() = sel_notes(tr, &self.sel);
                crate::clip::announce(ui.ctx(), "notes");
            }
            if (ek.cut || del) && !self.sel.is_empty() {
                self.snapshot(tr);
                let sel = std::mem::take(&mut self.sel);
                let mut k = 0;
                tr.notes.retain(|_| {
                    k += 1;
                    !sel.contains(&(k - 1))
                });
                changed = true;
            }
            if ek.paste && !cx.clipboard.borrow().is_empty() {
                self.snapshot(tr);
                let base = tr.notes.len();
                let clip = cx.clipboard.borrow().clone();
                let len = clip.iter().map(|n| n.tick + n.dur).max().unwrap_or(0);
                for mut n in clip {
                    n.tick += self.cursor;
                    tr.notes.push(n);
                }
                self.sel = (base..tr.notes.len()).collect();
                self.cursor += len; // repeated paste lays copies end to end
                changed = true;
            }
            if ek.undo {
                if let Some((n, o)) = self.undo.pop() {
                    self.redo.push((std::mem::replace(&mut tr.notes, n), std::mem::replace(&mut tr.others, o)));
                    self.sel.clear();
                    changed = true;
                }
            }
            if ek.redo {
                if let Some((n, o)) = self.redo.pop() {
                    self.undo.push((std::mem::replace(&mut tr.notes, n), std::mem::replace(&mut tr.others, o)));
                    self.sel.clear();
                    changed = true;
                }
            }
            if !self.sel.is_empty() && (up || down || left || right) {
                self.snapshot(tr);
                let dp: i32 = if up { 1 } else if down { -1 } else { 0 } * if shift { 12 } else { 1 };
                let dt: i64 = if right { step.max(1) as i64 } else if left { -(step.max(1) as i64) } else { 0 };
                for &i in &self.sel {
                    if let Some(n) = tr.notes.get_mut(i) {
                        n.pitch = rp((vp(n.pitch) as i32 + dp).clamp(0, 127) as u8);
                        n.tick = (n.tick as i64 + dt).max(0) as u32;
                    }
                }
                changed = true;
            }
            if changed {
                sort_notes(&mut tr.notes, &mut self.sel);
            }
        }
        // Repeat (Alt+R, Menu.hlp "リピート"): copies the events of a bar range
        // (only the selected notes when there is a selection) to a bar, n times
        if ui.input(|i| i.modifiers.alt && i.key_pressed(egui::Key::R)) && self.repeat.is_none() {
            let (a, b) = if self.sel.is_empty() {
                (self.cursor / bar + 1, self.cursor / bar + 2)
            } else {
                let lo = self.sel.iter().filter_map(|&i| tr.notes.get(i)).map(|n| n.tick).min().unwrap_or(0);
                let hi = self.sel.iter().filter_map(|&i| tr.notes.get(i)).map(|n| n.tick + n.dur).max().unwrap_or(lo + 1);
                (lo / bar + 1, (hi + bar - 1) / bar + 1)
            };
            self.repeat = Some([a, b, b, 1]);
        }
        if let Some(mut r) = self.repeat {
            let mut act = 0;
            egui::Window::new("Repeat").collapsible(false).resizable(false).show(ui.ctx(), |ui| {
                egui::Grid::new("rep").num_columns(2).show(ui, |ui| {
                    for (k, label) in ["Copy from bar", "Copy to bar", "Insert at bar", "Times"].iter().enumerate() {
                        ui.label(*label);
                        ui.add(egui::DragValue::new(&mut r[k]).range(1..=9999));
                        ui.end_row();
                    }
                });
                ui.label(if self.sel.is_empty() { "All events of the range" } else { "Only the selected notes" });
                ui.horizontal(|ui| {
                    if ui.button(egui::RichText::new("OK").strong()).clicked() {
                        act = 1;
                    }
                    if ui.button("Cancel").clicked() {
                        act = 2;
                    }
                });
            });
            self.repeat = Some(r);
            if act == 1 && r[1] > r[0] {
                self.snapshot(tr);
                let (s0, s1, at) = ((r[0] - 1) * bar, (r[1] - 1) * bar, (r[2] - 1) * bar);
                let len = s1 - s0;
                let src: Vec<EditNote> = if self.sel.is_empty() {
                    tr.notes.iter().filter(|n| n.tick >= s0 && n.tick < s1).cloned().collect()
                } else {
                    self.sel.iter().filter_map(|&i| tr.notes.get(i)).cloned().collect()
                };
                let others: Vec<Event> = if self.sel.is_empty() { tr.others.iter().filter(|e| e.t() >= s0 && e.t() < s1).cloned().collect() } else { Vec::new() };
                for k in 0..r[3] {
                    let off = at + k * len;
                    for n in &src {
                        tr.notes.push(EditNote { tick: n.tick - s0 + off, ..n.clone() });
                    }
                    for e in &others {
                        tr.others.push(e.shifted(off.wrapping_sub(s0)));
                    }
                }
                tr.others.sort_by_key(|e| e.t());
                self.sel.clear();
                sort_notes(&mut tr.notes, &mut self.sel);
                changed = true;
            }
            if act != 0 {
                self.repeat = None;
            }
        }

        // edit cursor
        let cx_x = x_of(self.cursor);
        if cx_x >= gx0 && cx_x <= notes.right() {
            p.line_segment([Pos2::new(cx_x, notes.top()), Pos2::new(cx_x, notes.bottom())], Stroke::new(1.0_f32, Color32::from_rgb(0, 150, 0)));
        }
        if let Some(t) = cx.play_tick {
            let x = x_of(t as u32);
            if x >= gx0 && x <= notes.right() {
                p.line_segment([Pos2::new(x, notes.top()), Pos2::new(x, notes.bottom())], Stroke::new(1.5_f32, Color32::from_rgb(224, 114, 74)));
            }
        }

        // ---------------- controller pane
        changed |= self.controller_pane(ui, ctrl, tr, trk_ch, gx0, step, &x_of, &tick_of);
        // ruler/notes/ctrl were painted directly; reserve their space
        ui.allocate_rect(Rect::from_min_max(avail.min, ctrl.max), Sense::hover());

        // ---------------- bottom tool bar + status
        ui.horizontal(|ui| {
            // tools + line shapes, icons of the original's bottom bar (bitmap 7200)
            ui.spacing_mut().item_spacing.x = 0.0;
            for (k, (t, txt, tip)) in [(Tool::Select, "Select", "Select"), (Tool::Pencil, "Pencil", "Pencil"), (Tool::Eraser, "Eraser", "Eraser")].into_iter().enumerate() {
                if icons::tool_button(ui, icons::ROLL_TOOLS, k, self.tool == t, txt, tip).clicked() {
                    self.tool = t;
                }
            }
            ui.add_space(8.0);
            for (k, (sh, txt, tip)) in [
                (Shape::Line, "Line", "Insert/Edit in straight line"),
                (Shape::Free, "Free", "Insert/Edit freely"),
                (Shape::ParabolaA, "Par A", "Insert/Edit in parabolic motion"),
                (Shape::ParabolaB, "Par B", "Insert/Edit in parabolic motion"),
                (Shape::Sine, "Sine", "Insert/Edit in sine curve motion"),
            ].into_iter().enumerate() {
                if icons::tool_button(ui, icons::ROLL_TOOLS, 3 + k, self.shape == sh, txt, tip).clicked() {
                    self.shape = sh;
                }
            }
            ui.spacing_mut().item_spacing.x = 8.0;
            ui.separator();
            ui.label("Interval");
            ui.add(egui::DragValue::new(&mut self.interval).range(1..=1920));
            ui.separator();
            ui.label("Vel");
            ui.add(egui::DragValue::new(&mut self.new_vel).range(1..=127));
        });
        ui.horizontal(|ui| {
            ui.monospace(&self.hover);
            if let Some(t) = cx.play_tick {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| ui.monospace(pos_text(t as u32, ppqn)));
            }
        });
        self.sel.retain(|&i| i < tr.notes.len());
        changed
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, cx: &RollCtx) {
        ui.horizontal_wrapped(|ui| {
            let name = cx.track_names.get(self.track).cloned().unwrap_or_default();
            let num = cx.visible.iter().position(|&v| v == self.track).map_or(0, |k| k + 1);
            egui::ComboBox::from_id_source(("roll_trk", self as *const _ as usize))
                .width(220.0)
                .selected_text(format!("{num} {name}"))
                .show_ui(ui, |ui| {
                    for (k, &i) in cx.visible.iter().enumerate() {
                        let n = &cx.track_names[i];
                        if ui.selectable_label(i == self.track, format!("{} {}", k + 1, n)).clicked() {
                            self.track = i;
                            self.sel.clear();
                            self.fitted = false;
                        }
                    }
                });
            ui.separator();
            // icons of the original's top bar (XGworks.exe bitmap 6008)
            ui.spacing_mut().item_spacing.x = 0.0;
            if icons::tool_button(ui, icons::ROLL_TOP, 0, false, "V+", "Zoom in (vertical)").clicked() {
                self.key_h = (self.key_h + 2.0).min(24.0);
            }
            if icons::tool_button(ui, icons::ROLL_TOP, 1, false, "V-", "Zoom out (vertical)").clicked() {
                self.key_h = (self.key_h - 2.0).max(4.0);
            }
            ui.add_space(8.0);
            for (k, (q, txt, tip)) in [(0, "FREE", "No quantize"), (4, "1/4", "Quarter note"), (8, "1/8", "8th note"), (16, "1/16", "16th note")].into_iter().enumerate() {
                if icons::tool_button(ui, icons::ROLL_TOP, 2 + k, self.quant == q, txt, tip).clicked() {
                    self.quant = q;
                }
            }
            if icons::tool_button(ui, icons::ROLL_TOP, 6, self.dotted, " . ", "Dotted").clicked() {
                self.dotted = !self.dotted;
            }
            if icons::tool_button(ui, icons::ROLL_TOP, 7, self.triplet, "3", "Triplet").clicked() {
                self.triplet = !self.triplet;
            }
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.end_row();
            ui.label(egui::RichText::new("Note").strong()).on_hover_text("Length of the notes the pencil enters (F2–F12, Ctrl = triplets); Grid = the quantize value");
            if ui.selectable_label(self.note_len == 0, "Grid").on_hover_text("Pencil notes as long as the quantize step").clicked() {
                self.note_len = 0;
            }
            for (label, len, key, ctrl) in PALETTE {
                let tip = format!("{}{:?}", if ctrl { "Ctrl+" } else { "" }, key);
                if ui.selectable_label(self.note_len == len, label).on_hover_text(tip).clicked() {
                    self.note_len = len;
                }
            }
            if self.staff.is_some() {
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.separator();
                if ui.button(egui::RichText::new("Print / Export…").strong()).on_hover_text("The whole track as printable pages (SVG, opens in the browser / Preview: print or save as PDF there)").clicked() {
                    self.export_req = true;
                }
            }
        });
    }

    #[allow(clippy::too_many_arguments)]
    fn controller_pane(
        &mut self,
        ui: &mut egui::Ui,
        area: Rect,
        tr: &mut EditTrack,
        ch: u8,
        gx0: f32,
        _step: u32,
        x_of: &dyn Fn(u32) -> f32,
        tick_of: &dyn Fn(f32) -> u32,
    ) -> bool {
        let mut changed = false;
        let ctl = ctl_of(self.ctl);
        let (lo, hi, centre) = ctl.range();
        let left = Rect::from_min_max(area.min, Pos2::new(gx0, area.bottom()));
        let graph = Rect::from_min_max(Pos2::new(gx0, area.top()), area.max);
        let p = ui.painter_at(area);
        p.rect_filled(left, 0.0, ui.visuals().panel_fill);
        p.rect_filled(graph, 0.0, Color32::WHITE);
        let y_of = |v: i32| graph.bottom() - 2.0 - (v - lo) as f32 / (hi - lo) as f32 * (graph.height() - 4.0);
        let v_of = |y: f32| (lo as f32 + (graph.bottom() - 2.0 - y) / (graph.height() - 4.0) * (hi - lo) as f32).round() as i32;
        for (v, lbl) in [(hi, hi.to_string()), (centre.unwrap_or((lo + hi) / 2), centre.unwrap_or((lo + hi) / 2).to_string())] {
            p.text(Pos2::new(gx0 - 3.0, y_of(v)), egui::Align2::RIGHT_CENTER, lbl, egui::FontId::monospace(9.0), LABEL_RED);
            p.line_segment([Pos2::new(gx0, y_of(v)), Pos2::new(graph.right(), y_of(v))], Stroke::new(1.0_f32, Color32::from_gray(215)));
        }
        p.line_segment([Pos2::new(gx0, area.top()), Pos2::new(gx0, area.bottom())], Stroke::new(2.0_f32, Color32::from_rgb(0, 200, 0)));
        // left panel widgets: selector + value box
        let mut child = ui.child_ui(left.shrink(4.0), egui::Layout::top_down(egui::Align::LEFT), None);
        egui::ComboBox::from_id_source(("roll_ctl", self as *const _ as usize))
            .width(KEY_W - 34.0)
            .selected_text(CONTROLLERS[self.ctl])
            .show_ui(&mut child, |ui| {
                for (i, c) in CONTROLLERS.iter().enumerate() {
                    ui.selectable_value(&mut self.ctl, i, *c);
                }
            });
        child.add(egui::DragValue::new(&mut self.ctl_value).range(lo..=hi));

        // events
        let events: Vec<(u32, i32)> = if ctl == Ctl::Velocity {
            tr.notes.iter().map(|n| (n.tick, n.vel as i32)).collect()
        } else {
            tr.others.iter().filter_map(|e| ctl.value(e, ch).map(|v| (e.t(), v))).collect()
        };
        let base = y_of(centre.filter(|_| ctl == Ctl::PitchBend).unwrap_or(lo));
        for (i, (t, v)) in events.iter().enumerate() {
            let x = x_of(*t);
            if x < gx0 || x > graph.right() {
                continue;
            }
            let y = y_of(*v);
            p.line_segment([Pos2::new(x, base), Pos2::new(x, y)], Stroke::new(1.0_f32, CTRL_LINE));
            if ctl == Ctl::Velocity {
                let end = x_of(tr.notes[i].tick + tr.notes[i].dur).min(graph.right());
                p.line_segment([Pos2::new(x, y), Pos2::new(end, y)], Stroke::new(1.0_f32, CTRL_LINE));
            } else if let Some((nt, _)) = events.get(i + 1) {
                p.line_segment([Pos2::new(x, y), Pos2::new(x_of(*nt).min(graph.right()), y)], Stroke::new(1.0_f32, CTRL_LINE));
            }
        }

        // editing
        let resp = ui.interact(graph, ui.id().with(("roll_ctl_g", self.track)), Sense::click_and_drag());
        if let Some(pos) = resp.hover_pos() {
            self.hover = format!("{} = {}   {}", CONTROLLERS[self.ctl], v_of(pos.y).clamp(lo, hi), pos_text(tick_of(pos.x), self.ppqn));
            match self.tool {
                Tool::Pencil => crate::cursors::show(ui.ctx(), crate::cursors::Cur::Pencil),
                Tool::Eraser => crate::cursors::show(ui.ctx(), crate::cursors::Cur::Eraser),
                Tool::Select => {}
            }
        }
        if !ctl.editable() {
            return false;
        }
        if resp.drag_started() {
            // from where the button went down, not where the drag was recognised
            if let Some(pos) = ui.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos()) {
                self.drag = Drag::Ctrl { from: pos };
                self.free_path = vec![pos];
                self.curve_ctrl = ui.input(|i| i.modifiers.command);
            }
        }
        if let Drag::Ctrl { from } = self.drag {
            if let Some(pos) = resp.interact_pointer_pos() {
                if resp.dragged() {
                    if self.shape == Shape::Free {
                        if self.free_path.last().map_or(true, |l| (l.x - pos.x).abs() >= 1.0) {
                            self.free_path.push(pos);
                        }
                        p.add(egui::Shape::line(self.free_path.clone(), Stroke::new(1.0_f32, Color32::BLACK)));
                    } else {
                        // 96 segments from the press point, like the original's preview
                        let (sh, c) = (self.shape, self.curve_ctrl);
                        let d = pos - from;
                        let pts: Vec<Pos2> = (0..=96).map(|i| {
                            let x = i as f64 / 96.0;
                            from + egui::vec2(d.x * x as f32, d.y * sh.curve(x, c) as f32)
                        }).collect();
                        p.add(egui::Shape::line(pts, Stroke::new(1.0_f32, Color32::BLACK)));
                    }
                }
            }
        }
        let click = resp.clicked().then(|| resp.interact_pointer_pos()).flatten();
        let stopped = resp.drag_stopped();
        if stopped || click.is_some() {
            let (path, end) = match (self.drag, click) {
                (_, Some(c)) => (vec![c], c),
                (Drag::Ctrl { from }, None) => {
                    let end = resp.interact_pointer_pos().unwrap_or(from);
                    if self.shape == Shape::Free { (self.free_path.clone(), end) } else { (vec![from, end], end) }
                }
                _ => (Vec::new(), Pos2::ZERO),
            };
            self.drag = Drag::None;
            if path.is_empty() {
                return false;
            }
            // value along the path at tick t (linear between samples)
            let mut pts: Vec<(u32, i32)> = path.iter().map(|q| (tick_of(q.x.max(gx0)), v_of(q.y).clamp(lo, hi))).collect();
            pts.sort_by_key(|x| x.0);
            let (t_from, t_to) = (pts[0].0, pts[pts.len() - 1].0);
            // curves run from the press point (which may be the right end)
            let anchor = path.first().map(|q| (tick_of(q.x.max(gx0)), v_of(q.y).clamp(lo, hi)));
            let target = (tick_of(end.x.max(gx0)), v_of(end.y));
            let (shape, cctl) = (if path.len() == 2 { self.shape } else { Shape::Free }, self.curve_ctrl);
            let value_at = |t: u32| -> i32 {
                if let (Shape::ParabolaA | Shape::ParabolaB | Shape::Sine, Some((ta, va))) = (shape, anchor) {
                    let span = (target.0 as f64 - ta as f64).abs();
                    let x = if span > 0.0 { ((t as f64 - ta as f64).abs() / span).min(1.0) } else { 1.0 };
                    return (va as f64 + (target.1 - va) as f64 * shape.curve(x, cctl)).round().clamp(lo as f64, hi as f64) as i32;
                }
                match pts.iter().position(|q| q.0 >= t) {
                    Some(0) | None => pts.iter().min_by_key(|q| (q.0 as i64 - t as i64).abs()).unwrap().1,
                    Some(k) => {
                        let (a, b) = (pts[k - 1], pts[k]);
                        if b.0 == a.0 { b.1 } else { a.1 + ((b.1 - a.1) as f64 * (t - a.0) as f64 / (b.0 - a.0) as f64).round() as i32 }
                    }
                }
            };
            self.snapshot(tr);
            if ctl == Ctl::Velocity {
                for n in tr.notes.iter_mut() {
                    let hit = if t_from == t_to { (n.tick as i64 - t_from as i64).abs() <= (4.0 / 0.25) as i64 } else { n.tick >= t_from && n.tick <= t_to };
                    if hit {
                        n.vel = if self.tool == Tool::Eraser { n.vel } else { value_at(n.tick).clamp(1, 127) as u8 };
                        changed = true;
                    }
                }
            } else {
                // replace this controller's events in [from, to]
                tr.others.retain(|e| !(ctl.value(e, ch).is_some() && e.t() >= t_from && e.t() <= t_to));
                if self.tool != Tool::Eraser {
                    let mut t = t_from;
                    loop {
                        if let Some(e) = ctl.make(t, ch, value_at(t)) {
                            tr.others.push(e);
                        }
                        if t >= t_to {
                            break;
                        }
                        t = (t + self.interval.max(1)).min(t_to);
                    }
                    self.ctl_value = value_at(t_to);
                }
                tr.others.sort_by_key(|e| e.t());
                changed = true;
            }
        }
        changed
    }
}
