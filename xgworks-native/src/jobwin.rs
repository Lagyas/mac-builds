//! The Job menu dialogs (JobDll.dll / JobDll2.dll), laid out like the
//! original's: Track, Meas From / Thru, Note From / Thru and each job's own
//! fields. The jobs themselves are `xgworks_core::jobs`.
//!
//! Defaults come from the original's `XG4\JobDll.ini`; with blocks selected
//! in the Track View the job works on them and Track / Meas are disabled.

use eframe::egui;
use xgworks_core::jobs::{self, CtrlKind, Modify, QntzOpts, Scope, QNTZ_NAMES};
use xgworks_core::listev::{ListNames, TimeMap};
use xgworks_core::EditTrack;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Job {
    Transpose,
    Velocity,
    Gate,
    Shift,
    Sort,
    Separate,
    Thin,
    Quantize,
    CreateMeasure,
    DeleteMeasure,
    TransposeChord,
    Control,
    Stretch,
}

/// Menu entries in the original's order and wording.
pub const JOB_MENU: [(Job, &str); 13] = [
    (Job::Transpose, "Transpose ..."),
    (Job::Velocity, "Velocity Modify ..."),
    (Job::Gate, "Gate Time Modify ..."),
    (Job::Shift, "Shift Clock ..."),
    (Job::Sort, "Sort Chord ..."),
    (Job::Separate, "Separate Chord ..."),
    (Job::Thin, "Thin Out ..."),
    (Job::Quantize, "Quantize ..."),
    (Job::CreateMeasure, "Create Measure..."),
    (Job::DeleteMeasure, "Delete Measure..."),
    (Job::TransposeChord, "Transpose Chord..."),
    (Job::Control, "Modify Control Data..."),
    (Job::Stretch, "Stretch Time..."),
];

const CTRL_KINDS: [&str; 4] = ["Poly After Touch", "Control Change", "Ch After Touch", "Pitch Bend"];

/// All dialog values (kept between uses, like the original's JobDll.ini).
pub struct JobState {
    pub open: Option<Job>,
    track: usize,
    from: String,
    thru: String,
    note_lo: String,
    note_hi: String,
    transpose: i32,
    vel: (bool, i32, i32),  // (absolute, rate, value / offset)
    gate: (bool, i32, i32),
    shift: i32,
    sort_reverse: bool,
    separate: i32,
    thin_kind: usize,
    thin_cc: u8,
    q: QntzOpts,
    cm: (u32, u8, u8, u32), // from bar, numerator, denominator, times
    dm: (u32, u32),
    chord_semis: i32,
    ctl_kind: usize,
    ctl_from: u8,
    ctl_to: u8,
    ctl: (bool, i32, i32),
    stretch: (u32, i32),
    error: String,
}

fn ini_get<'a>(ini: &'a str, sec: &str, key: &str) -> Option<&'a str> {
    let mut on = false;
    for l in ini.lines() {
        let l = l.trim();
        if l.starts_with('[') {
            on = l.eq_ignore_ascii_case(&format!("[{sec}]"));
        } else if on {
            if let Some((k, v)) = l.split_once('=') {
                if k.trim().eq_ignore_ascii_case(key) {
                    return Some(v.trim());
                }
            }
        }
    }
    None
}

impl JobState {
    /// Defaults from the original's JobDll.ini (else its built-in ones).
    pub fn new(ini: &str) -> JobState {
        let num = |s: &str, k: &str, d: i32| ini_get(ini, s, k).and_then(|v| v.parse().ok()).unwrap_or(d);
        let on = |s: &str, k: &str| ini_get(ini, s, k).map_or(true, |v| v.eq_ignore_ascii_case("ON"));
        JobState {
            open: None,
            track: 1,
            from: "0001:01:000".into(),
            thru: "9999:01:000".into(),
            note_lo: "C-2(000)".into(),
            note_hi: "G8(127)".into(),
            transpose: num("Trans", "Trans", 12),
            vel: (on("Vel", "Fix"), num("Vel", "Rate", 120), num("Vel", "Ofst", 127)),
            gate: (on("Gate", "Fix"), num("Gate", "Rate", 200), num("Gate", "Ofst", 1)),
            shift: num("Shft", "Clk", -480),
            sort_reverse: !on("Sort", "Nrm"),
            separate: num("Separate", "Clk", 1),
            thin_kind: num("Thin", "Type", 1).clamp(0, 3) as usize,
            thin_cc: num("Thin", "Cntrl", 11).clamp(0, 127) as u8,
            q: QntzOpts {
                grid: num("Qntz", "Qntz", 2).clamp(0, 8) as u8,
                strength: num("Qntz", "Streng", 100).clamp(0, 100) as u8,
                sens: num("Qntz", "Sens", 100).clamp(-100, 100) as i16,
                swing: num("Qntz", "Swing", 50).clamp(50, 100) as u8,
                gate: num("Qntz", "Gate", 100).clamp(0, 200) as u8,
                vel: num("Qntz", "Vel", 100).clamp(0, 200) as u8,
            },
            cm: (1, 4, 4, 1),
            dm: (1, 2),
            chord_semis: 1,
            ctl_kind: 0,
            ctl_from: 0,
            ctl_to: 0,
            ctl: (true, 100, 0),
            stretch: (100, 0),
            error: String::new(),
        }
    }
}

fn parse_note_text(s: &str) -> Option<u8> {
    let s = s.trim();
    if let (Some(a), Some(b)) = (s.find('('), s.find(')')) {
        return s[a + 1..b].trim().parse().ok();
    }
    s.parse().ok().or_else(|| xgworks_core::listev::parse_note(s))
}

fn parse_loc(tm: &TimeMap, s: &str) -> Option<u32> {
    let p: Vec<u32> = s.split(':').map(|x| x.trim().parse().ok()).collect::<Option<Vec<_>>>()?;
    match p.as_slice() {
        [m] => Some(tm.from_mbc(*m, 1, 0)),
        [m, b] => Some(tm.from_mbc(*m, *b, 0)),
        [m, b, c] => Some(tm.from_mbc(*m, *b, *c)),
        _ => None,
    }
}

/// What the app hands the dialogs.
pub struct JobCtx<'a> {
    pub tm: &'a TimeMap,
    pub ppqn: u32,
    /// Visible tracks: Track View row → song track index.
    pub visible: &'a [usize],
    /// Track View block selection as (track, from tick, to tick).
    pub selected: Vec<(usize, u32, u32)>,
    pub names: &'a ListNames,
}

/// Show the open dialog; on OK run the job and return its status text.
/// `before` is called right before the song changes (undo snapshot).
pub fn ui(ctx: &egui::Context, st: &mut JobState, cx: &JobCtx, tracks: &mut Vec<EditTrack>, before: &mut dyn FnMut(&[EditTrack])) -> Option<String> {
    let job = st.open?;
    let title = match job {
        Job::Transpose => "Transpose",
        Job::Velocity => "Velocity Modify",
        Job::Gate => "Gate Time Modify",
        Job::Shift => "Shift Clock",
        Job::Sort => "Sort Chord",
        Job::Separate => "Separate Chord",
        Job::Thin => "Thin Out",
        Job::Quantize => "Quantize",
        Job::CreateMeasure => "Create Measure",
        Job::DeleteMeasure => "Delete Measure",
        Job::TransposeChord => "Transpose Chord",
        Job::Control => "Modify Control Data",
        Job::Stretch => "Stretch Time",
    };
    let sel = !cx.selected.is_empty();
    let has_track = !matches!(job, Job::CreateMeasure | Job::DeleteMeasure | Job::TransposeChord);
    let has_notes = matches!(job, Job::Transpose | Job::Velocity | Job::Gate | Job::Quantize);
    let has_range = !matches!(job, Job::CreateMeasure | Job::DeleteMeasure);
    let mut result = None;
    let mut open = true;
    let mut ok = false;
    let mut cancel = false;
    egui::Window::new(title).id(egui::Id::new("job_dialog")).collapsible(false).resizable(false)
        .pivot(egui::Align2::CENTER_CENTER).default_pos(ctx.screen_rect().center()).open(&mut open)
        .show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    egui::Grid::new("job_grid").num_columns(4).spacing([6.0, 6.0]).show(ui, |ui| {
                        if has_track {
                            ui.label("Track");
                            ui.add_enabled(!sel, egui::DragValue::new(&mut st.track).range(1..=cx.visible.len().max(1)));
                            ui.end_row();
                        }
                        if has_range {
                            ui.label("Meas From:");
                            ui.add_enabled(!sel || !has_track, egui::TextEdit::singleline(&mut st.from).desired_width(90.0).min_size(egui::vec2(96.0, 0.0)));
                            ui.label(if matches!(job, Job::TransposeChord | Job::Control | Job::Stretch) { "To:" } else { "Thru:" });
                            ui.add_enabled(!sel || !has_track, egui::TextEdit::singleline(&mut st.thru).desired_width(90.0).min_size(egui::vec2(96.0, 0.0)));
                            ui.end_row();
                        }
                        if has_notes {
                            ui.label("Note From:");
                            ui.add(egui::TextEdit::singleline(&mut st.note_lo).desired_width(90.0).min_size(egui::vec2(96.0, 0.0)));
                            ui.label("Thru:");
                            ui.add(egui::TextEdit::singleline(&mut st.note_hi).desired_width(90.0).min_size(egui::vec2(96.0, 0.0)));
                            ui.end_row();
                        }
                    });
                    ui.add_space(4.0);
                    job_fields(ui, job, st, cx);
                    if !st.error.is_empty() {
                        ui.colored_label(egui::Color32::RED, &st.error);
                    }
                });
                ui.add_space(12.0);
                ui.vertical(|ui| {
                    if ui.add_sized([70.0, 22.0], egui::Button::new("OK")).clicked() {
                        ok = true;
                    }
                    if ui.add_sized([70.0, 22.0], egui::Button::new("Cancel")).clicked() {
                        cancel = true;
                    }
                });
            });
        });
    if ok {
        match run(job, st, cx, tracks, before) {
            Ok(msg) => {
                st.open = None;
                st.error.clear();
                result = Some(msg);
            }
            Err(e) => st.error = e,
        }
    }
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        cancel = true;
    }
    if cancel || !open {
        st.open = None;
        st.error.clear();
    }
    result
}

fn modify_fields(ui: &mut egui::Ui, m: &mut (bool, i32, i32), vmin: i32, vmax: i32, omin: i32, omax: i32) {
    ui.horizontal(|ui| {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.vertical(|ui| {
                ui.radio_value(&mut m.0, true, "Absolute");
                ui.radio_value(&mut m.0, false, "Relative");
            });
        });
        egui::Grid::new("modify_grid").num_columns(2).show(ui, |ui| {
            ui.label("Rate");
            if m.0 {
                ui.add_enabled(false, egui::TextEdit::singleline(&mut "--".to_string()).desired_width(60.0));
            } else {
                ui.add(egui::DragValue::new(&mut m.1).range(0..=200).suffix(" %"));
            }
            ui.end_row();
            ui.label(if m.0 { "Value" } else { "Offset" });
            if m.0 {
                ui.add(egui::DragValue::new(&mut m.2).range(vmin..=vmax));
            } else {
                ui.add(egui::DragValue::new(&mut m.2).range(omin..=omax));
            }
            ui.end_row();
        });
    });
}

fn cc_combo(ui: &mut egui::Ui, id: &str, v: &mut u8, names: &ListNames, enabled: bool) {
    let name = |c: u8| format!("{c} {}", names.cc.get(&c).map_or("", |s| s.as_str()));
    ui.add_enabled_ui(enabled, |ui| {
        egui::ComboBox::from_id_source(id).width(170.0).selected_text(name(*v)).show_ui(ui, |ui| {
            for c in 0..=127u8 {
                ui.selectable_value(v, c, name(c));
            }
        });
    });
}

fn job_fields(ui: &mut egui::Ui, job: Job, st: &mut JobState, cx: &JobCtx) {
    match job {
        Job::Transpose => {
            ui.horizontal(|ui| {
                ui.label("Transpose");
                ui.add(egui::DragValue::new(&mut st.transpose).range(-127..=127));
            });
        }
        Job::Velocity => modify_fields(ui, &mut st.vel, 1, 127, -127, 127),
        Job::Gate => modify_fields(ui, &mut st.gate, 1, 99999, -99999, 99999),
        Job::Shift => {
            ui.horizontal(|ui| {
                ui.label("Clock");
                ui.add(egui::DragValue::new(&mut st.shift).range(-99999..=99999));
            });
        }
        Job::Sort => {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.label("Type");
                ui.horizontal(|ui| {
                    ui.radio_value(&mut st.sort_reverse, false, "Normal");
                    ui.radio_value(&mut st.sort_reverse, true, "Reverse");
                });
            });
        }
        Job::Separate => {
            ui.horizontal(|ui| {
                ui.label("Clock");
                ui.add(egui::DragValue::new(&mut st.separate).range(-480..=480));
            });
        }
        Job::Thin => {
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_source("thin_kind").width(150.0).selected_text(CTRL_KINDS[st.thin_kind]).show_ui(ui, |ui| {
                    for (k, n) in CTRL_KINDS.iter().enumerate() {
                        ui.selectable_value(&mut st.thin_kind, k, *n);
                    }
                });
                cc_combo(ui, "thin_cc", &mut st.thin_cc, cx.names, st.thin_kind == 1);
            });
        }
        Job::Quantize => {
            egui::Grid::new("qntz_grid").num_columns(4).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label("Quantize");
                egui::ComboBox::from_id_source("qntz_val").width(120.0).selected_text(QNTZ_NAMES[st.q.grid as usize]).show_ui(ui, |ui| {
                    for (k, n) in QNTZ_NAMES.iter().enumerate() {
                        ui.selectable_value(&mut st.q.grid, k as u8, *n);
                    }
                });
                ui.label("Swing Rate");
                ui.add(egui::DragValue::new(&mut st.q.swing).range(50..=100));
                ui.end_row();
                ui.label("Strength");
                ui.add(egui::DragValue::new(&mut st.q.strength).range(0..=100));
                ui.label("Gate Time");
                ui.add(egui::DragValue::new(&mut st.q.gate).range(0..=200));
                ui.end_row();
                ui.label("Sensitivity");
                ui.add(egui::DragValue::new(&mut st.q.sens).range(-100..=100));
                ui.label("Velocity");
                ui.add(egui::DragValue::new(&mut st.q.vel).range(0..=200));
                ui.end_row();
            });
        }
        Job::CreateMeasure => {
            ui.horizontal(|ui| {
                ui.label("Meas From:");
                ui.add(egui::DragValue::new(&mut st.cm.0).range(1..=9999));
            });
            ui.horizontal(|ui| {
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.label("Time Signature");
                        ui.add(egui::DragValue::new(&mut st.cm.1).range(1..=255));
                        ui.label("----------");
                        egui::ComboBox::from_id_source("cm_den").width(50.0).selected_text(st.cm.2.to_string()).show_ui(ui, |ui| {
                            for d in [1u8, 2, 4, 8, 16, 32] {
                                ui.selectable_value(&mut st.cm.2, d, d.to_string());
                            }
                        });
                    });
                });
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.vertical(|ui| {
                        ui.label("Create Times");
                        ui.add(egui::DragValue::new(&mut st.cm.3).range(1..=999));
                    });
                });
            });
        }
        Job::DeleteMeasure => {
            ui.horizontal(|ui| {
                ui.label("Meas From:");
                ui.add(egui::DragValue::new(&mut st.dm.0).range(1..=9999));
                ui.label("To:");
                ui.add(egui::DragValue::new(&mut st.dm.1).range(1..=9999));
            });
        }
        Job::TransposeChord => {
            ui.horizontal(|ui| {
                ui.label("Transpose");
                ui.add(egui::DragValue::new(&mut st.chord_semis).range(-11..=11));
            });
        }
        Job::Control => {
            ui.horizontal(|ui| {
                ui.label("Type");
                egui::ComboBox::from_id_source("ctl_kind").width(180.0).selected_text(CTRL_KINDS[st.ctl_kind]).show_ui(ui, |ui| {
                    for (k, n) in CTRL_KINDS.iter().enumerate() {
                        ui.selectable_value(&mut st.ctl_kind, k, *n);
                    }
                });
            });
            ui.horizontal(|ui| {
                cc_combo(ui, "ctl_from", &mut st.ctl_from, cx.names, st.ctl_kind == 1);
                cc_combo(ui, "ctl_to", &mut st.ctl_to, cx.names, st.ctl_kind == 1);
            });
            let (lo, hi) = if st.ctl_kind == 3 { (-8192, 8191) } else { (0, 127) };
            modify_fields(ui, &mut st.ctl, lo, hi, lo - hi, hi - lo);
        }
        Job::Stretch => {
            egui::Grid::new("stretch_grid").num_columns(2).show(ui, |ui| {
                ui.label("Stretch");
                ui.add(egui::DragValue::new(&mut st.stretch.0).range(1..=1000).suffix(" %"));
                ui.end_row();
                ui.label("Offset");
                ui.add(egui::DragValue::new(&mut st.stretch.1).range(-99999..=99999));
                ui.end_row();
            });
        }
    }
}

fn modify_of(m: (bool, i32, i32)) -> Modify {
    if m.0 { Modify::Absolute(m.2) } else { Modify::Relative { rate: m.1, offset: m.2 } }
}

fn run(job: Job, st: &mut JobState, cx: &JobCtx, tracks: &mut Vec<EditTrack>, before: &mut dyn FnMut(&[EditTrack])) -> Result<String, String> {
    let tm = cx.tm;
    let bar_tick = |m: u32| tm.from_mbc(m.max(1), 1, 0);
    // the range: selected blocks, else Track + Meas From / Thru
    let range = || -> Result<(u32, u32), String> {
        let a = parse_loc(tm, &st.from).ok_or("Meas From: mmmm:bb:ccc")?;
        let b = parse_loc(tm, &st.thru).ok_or("Thru: mmmm:bb:ccc")?;
        if b <= a {
            return Err("The range is empty.".into());
        }
        Ok((a, b))
    };
    let scope = || -> Result<Scope, String> {
        let lo = parse_note_text(&st.note_lo).ok_or("Note From: e.g. C-2(000)")?;
        let hi = parse_note_text(&st.note_hi).ok_or("Note Thru: e.g. G8(127)")?;
        let spans = if !cx.selected.is_empty() {
            cx.selected.clone()
        } else {
            let ti = *cx.visible.get(st.track - 1).ok_or("No such track.")?;
            let (a, b) = range()?;
            vec![(ti, a, b)]
        };
        Ok(Scope { spans, note_lo: lo.min(hi), note_hi: lo.max(hi) })
    };
    let kind = |k: usize, cc_from: u8, cc_to: u8| match k {
        0 => CtrlKind::PolyAfter,
        1 => CtrlKind::Cc(cc_from, cc_to),
        2 => CtrlKind::ChAfter,
        _ => CtrlKind::PitchBend,
    };
    let msg = match job {
        Job::Transpose => {
            let sc = scope()?;
            before(tracks);
            format!("Transpose: {} note(s).", jobs::transpose(tracks, &sc, st.transpose))
        }
        Job::Velocity => {
            let sc = scope()?;
            before(tracks);
            format!("Velocity Modify: {} note(s).", jobs::velocity(tracks, &sc, modify_of(st.vel)))
        }
        Job::Gate => {
            let sc = scope()?;
            before(tracks);
            format!("Gate Time Modify: {} note(s).", jobs::gate_time(tracks, &sc, modify_of(st.gate)))
        }
        Job::Shift => {
            let sc = scope()?;
            before(tracks);
            format!("Shift Clock: {} event(s).", jobs::shift_clock(tracks, &sc, st.shift))
        }
        Job::Sort => {
            let sc = scope()?;
            before(tracks);
            jobs::sort_chord(tracks, &sc, st.sort_reverse);
            "Sort Chord done.".into()
        }
        Job::Separate => {
            let sc = scope()?;
            before(tracks);
            jobs::separate_chord(tracks, &sc, st.separate);
            "Separate Chord done.".into()
        }
        Job::Thin => {
            let sc = scope()?;
            before(tracks);
            format!("Thin Out: {} event(s) removed.", jobs::thin_out(tracks, &sc, kind(st.thin_kind, st.thin_cc, st.thin_cc)))
        }
        Job::Quantize => {
            let sc = scope()?;
            before(tracks);
            let n = jobs::quantize(tracks, &sc, st.q, cx.ppqn, |t| {
                let (m, _, _) = tm.to_mbc(t);
                let s = tm.from_mbc(m, 1, 0);
                (s, tm.bar_len(s))
            });
            format!("Quantize: {n} note(s).")
        }
        Job::CreateMeasure => {
            let at = bar_tick(st.cm.0);
            let old = time_sig_at(tracks, at);
            let conductor = tracks.iter().position(|t| t.conductor).unwrap_or(0);
            before(tracks);
            jobs::create_measure(tracks, conductor, cx.ppqn, at, st.cm.1, st.cm.2, st.cm.3, old);
            format!("{} measure(s) created at {}.", st.cm.3, st.cm.0)
        }
        Job::DeleteMeasure => {
            let (a, b) = (st.dm.0.min(st.dm.1), st.dm.0.max(st.dm.1));
            before(tracks);
            jobs::delete_measure(tracks, bar_tick(a), bar_tick(b + 1));
            format!("Measures {a}–{b} deleted.")
        }
        Job::TransposeChord => {
            let (a, b) = range()?;
            before(tracks);
            format!("Transpose Chord: {} chord(s).", jobs::transpose_chord(tracks, a, b, st.chord_semis))
        }
        Job::Control => {
            let sc = scope()?;
            before(tracks);
            let n = jobs::modify_control(tracks, &sc, kind(st.ctl_kind, st.ctl_from, st.ctl_to), modify_of(st.ctl));
            format!("Modify Control Data: {n} event(s).")
        }
        Job::Stretch => {
            let sc = scope()?;
            before(tracks);
            format!("Stretch Time: {} event(s).", jobs::stretch_time(tracks, &sc, st.stretch.0, st.stretch.1))
        }
    };
    Ok(msg)
}

/// The time signature in force at `tick` (FF 58), 4/4 by default.
fn time_sig_at(tracks: &[EditTrack], tick: u32) -> (u8, u8) {
    let mut best: Option<(u32, (u8, u8))> = None;
    for t in tracks {
        for e in &t.others {
            if let xgworks_core::Event::Meta { t: at, mtype: 0x58, data } = e {
                if *at <= tick && data.len() >= 2 && best.map_or(true, |b| *at >= b.0) {
                    best = Some((*at, (data[0], 1u8 << data[1].min(5))));
                }
            }
        }
    }
    best.map_or((4, 4), |b| b.1)
}
