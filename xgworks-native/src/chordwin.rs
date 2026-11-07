//! Chord editor window (not in XGworks; the Lyrics window's counterpart for
//! XF chords, FF 7F 43 7B 01 …): the song's chords as a list (click =
//! select, double-click = song position), insert / edit / move / delete,
//! clear all, a big preview of the chord now and the next ones, typing a
//! chord sequence onto bars or beats, transposing, and Export / Import as
//! Midimusic Chord and Lyrics Protocol text (chords only) with replace or add.

use eframe::egui::{self, Color32};
use xgworks_core::listev::TimeMap;
use xgworks_core::{EditTrack, Event};

use crate::lyricwin::{decode_file, export_text, parse_text, time_line, xf_chord_data, xf_chord_name, Enc};

pub enum ChordAction {
    /// The song was changed (after a snapshot for undo).
    Edit(Box<dyn FnOnce(&mut [EditTrack])>),
    Locate(u32),
    Status(String),
}

/// Every XF chord of the song: (tick, track, event index, name).
pub fn chords(tracks: &[EditTrack]) -> Vec<(u32, usize, usize, String)> {
    let mut v: Vec<(u32, usize, usize, String)> = tracks
        .iter()
        .enumerate()
        .flat_map(|(ti, t)| {
            t.others.iter().enumerate().filter_map(move |(ei, e)| match e {
                Event::Meta { t, mtype: 0x7F, data } => xf_chord_name(data).map(|n| (*t, ti, ei, n)),
                _ => None,
            })
        })
        .collect();
    v.sort_by_key(|c| c.0);
    v
}

const SHARP: [&str; 12] = ["C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];

/// "Am7/D" moved by `semis` half-tones (roots spelled C C# D Eb E F F# G Ab A Bb B).
pub fn transpose_name(name: &str, semis: i32) -> Option<String> {
    if name.eq_ignore_ascii_case("N.C.") {
        return Some(name.to_string());
    }
    let shift = |root: &str| -> Option<String> {
        let mut c = root.chars();
        let l = c.next()?.to_ascii_uppercase();
        let base = match l {
            'C' => 0,
            'D' => 2,
            'E' => 4,
            'F' => 5,
            'G' => 7,
            'A' => 9,
            'B' => 11,
            _ => return None,
        };
        let acc: i32 = c.map(|x| match x {
            '#' => 1,
            'b' => -1,
            _ => 0,
        }).sum();
        Some(SHARP[(base + acc + semis).rem_euclid(12) as usize].to_string())
    };
    let split_root = |s: &str| -> (String, String) {
        let n = 1 + s.chars().skip(1).take_while(|c| *c == '#' || *c == 'b').count();
        (s.chars().take(n).collect(), s.chars().skip(n).collect())
    };
    let (main, bass) = match name.split_once('/') {
        Some((m, b)) => (m, Some(b)),
        None => (name, None),
    };
    let (r, suffix) = split_root(main);
    let mut out = shift(&r)? + &suffix;
    if let Some(b) = bass {
        out += "/";
        out += &shift(b)?;
    }
    Some(out)
}

/// Where typed chords go: every bar, half bar or beat.
pub const STEPS: [&str; 3] = ["one per bar", "two per bar", "one per beat"];

/// Chords typed as "C G Am F" (or one per line; "-" or "." keeps a step
/// empty) onto the steps from bar `from`; returns (tick, XF data) and the
/// names not understood.
pub fn place_typed(typed: &str, tm: &TimeMap, from: u32, step: u8) -> (Vec<(u32, Vec<u8>)>, Vec<String>) {
    let mut out = Vec::new();
    let mut bad = Vec::new();
    let mut t = tm.from_mbc(from.max(1), 1, 0);
    for w in typed.split_whitespace() {
        let len = match step {
            0 => tm.bar_len(t),
            1 => tm.bar_len(t) / 2,
            _ => tm.beat_len(t),
        }
        .max(1);
        if w != "-" && w != "." {
            match xf_chord_data(w) {
                Some(d) => out.push((t, d)),
                None => bad.push(w.to_string()),
            }
        }
        t += len;
    }
    (out, bad)
}

pub struct ChordsState {
    sel: Option<usize>,
    edit_text: String,
    font: f32,
    typed: String,
    from_bar: u32,
    step: u8,
    replace: bool,
    pick: crate::platform::PickedFiles,
    pending: Option<(Vec<(u32, Vec<u8>)>, usize)>,
}

impl Default for ChordsState {
    fn default() -> Self {
        ChordsState { sel: None, edit_text: String::new(), font: 1.0, typed: String::new(), from_bar: 1, step: 0, replace: true, pick: Default::default(), pending: None }
    }
}

/// Remove every XF chord of the song.
fn clear(tr: &mut [EditTrack]) {
    for t in tr.iter_mut() {
        t.others.retain(|e| !matches!(e, Event::Meta { mtype: 0x7F, data, .. } if xf_chord_name(data).is_some()));
    }
}

impl ChordsState {
    #[allow(clippy::too_many_arguments)]
    pub fn ui(&mut self, ui: &mut egui::Ui, tracks: &[EditTrack], tm: &TimeMap, song_pos: u32, playing: bool, master: usize, file_name: &str) -> Vec<ChordAction> {
        let mut out = Vec::new();
        let list = chords(tracks);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("{} chords (XF)", list.len())).strong());
            ui.separator();
            ui.label("Text size");
            if ui.button(" A− ").clicked() {
                self.font = (self.font - 0.1).max(0.7);
            }
            if ui.button(" A+ ").clicked() {
                self.font = (self.font + 0.1).min(2.5);
            }
            ui.label(format!("{:.0} %", self.font * 100.0));
            ui.separator();
            ui.label("Transpose all");
            for (txt, d) in [(" −1 ", -1), (" +1 ", 1)] {
                if ui.add_enabled(!list.is_empty(), egui::Button::new(txt)).on_hover_text("Every chord of the song a half-tone down / up").clicked() {
                    out.push(ChordAction::Edit(Box::new(move |tr| {
                        for t in tr.iter_mut() {
                            for e in t.others.iter_mut() {
                                if let Event::Meta { mtype: 0x7F, data, .. } = e {
                                    if let Some(n) = xf_chord_name(data).and_then(|n| transpose_name(&n, d)).and_then(|n| xf_chord_data(&n)) {
                                        *data = n;
                                    }
                                }
                            }
                        }
                    })));
                    out.push(ChordAction::Status(format!("Chords transposed {d:+} (undo with Ctrl+Z).")));
                }
            }
        });
        ui.columns(2, |cols| {
            // ---- list and editing
            let ui = &mut cols[0];
            let sel = self.sel.and_then(|k| list.get(k)).map(|c| (c.1, c.2));
            ui.horizontal(|ui| {
                ui.label("Chord");
                let r = ui.add(egui::TextEdit::singleline(&mut self.edit_text).desired_width(110.0).hint_text("Am7, G/B, N.C."));
                let valid = xf_chord_data(&self.edit_text).is_some();
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.add_enabled(valid, egui::Button::new("Insert at song position")).clicked() {
                    let d = xf_chord_data(&self.edit_text).unwrap();
                    out.push(ChordAction::Edit(Box::new(move |tr| {
                        if let Some(t) = tr.get_mut(master) {
                            t.others.push(Event::Meta { t: song_pos, mtype: 0x7F, data: d });
                        }
                    })));
                }
                if ui.add_enabled(valid && sel.is_some(), egui::Button::new("Apply")).clicked() || (enter && valid && sel.is_some()) {
                    let ((ti, ei), d) = (sel.unwrap(), xf_chord_data(&self.edit_text).unwrap());
                    out.push(ChordAction::Edit(Box::new(move |tr| {
                        if let Some(Event::Meta { data, .. }) = tr.get_mut(ti).and_then(|t| t.others.get_mut(ei)) {
                            *data = d;
                        }
                    })));
                }
                if !self.edit_text.trim().is_empty() && !valid {
                    ui.colored_label(Color32::from_rgb(0xC0, 0x40, 0x20), "not an XF chord");
                }
            });
            ui.horizontal(|ui| {
                if ui.add_enabled(sel.is_some(), egui::Button::new("Move to song position")).clicked() {
                    let (ti, ei) = sel.unwrap();
                    out.push(ChordAction::Edit(Box::new(move |tr| {
                        if let Some(Event::Meta { t, .. }) = tr.get_mut(ti).and_then(|x| x.others.get_mut(ei)) {
                            *t = song_pos;
                        }
                    })));
                }
                if ui.add_enabled(sel.is_some(), egui::Button::new("Delete")).clicked() {
                    let (ti, ei) = sel.unwrap();
                    self.sel = None;
                    out.push(ChordAction::Edit(Box::new(move |tr| {
                        if ei < tr[ti].others.len() {
                            tr[ti].others.remove(ei);
                        }
                    })));
                }
                if ui.add_enabled(!list.is_empty(), egui::Button::new("Clear all chords")).on_hover_text("Delete every XF chord of the song (undo-able)").clicked() {
                    let n = list.len();
                    self.sel = None;
                    out.push(ChordAction::Edit(Box::new(clear)));
                    out.push(ChordAction::Status(format!("{n} chords deleted (undo with Ctrl+Z).")));
                }
            });
            // Midimusic Chord and Lyrics Protocol (.txt), chords only
            ui.horizontal(|ui| {
                if ui.button("Export…").on_hover_text("Midimusic Chord and Lyrics Protocol text, Chd lines with bar:beat:tick").clicked() {
                    let text = export_text(tracks, master, tm, Enc::Cp1250, file_name, 2);
                    let stem = std::path::Path::new(file_name).file_stem().and_then(|x| x.to_str()).unwrap_or("song").to_string();
                    if let Some(m) = crate::platform::save_song(&format!("{stem}_chords.txt"), "txt", "Midimusic lyrics (*.txt)", &Enc::Cp1250.encode(&text)) {
                        out.push(ChordAction::Status(m));
                    }
                }
                if ui.button("Import…").on_hover_text("Chords (Chd lines) from a Midimusic lyrics text; then replace the old ones or add").clicked() {
                    crate::platform::pick_files(&self.pick, ui.ctx(), "Midimusic lyrics (*.txt, *.lyrics)", &["txt", "lyrics"]);
                }
            });
            if let Some(f) = self.pick.borrow_mut().take().and_then(|v| v.into_iter().next()) {
                match f.read() {
                    Some(bytes) => {
                        let (_, chd, bad) = parse_text(&decode_file(&bytes), tm);
                        if chd.is_empty() {
                            out.push(ChordAction::Status("No Chd lines in that file.".into()));
                        } else {
                            self.pending = Some((chd, bad));
                        }
                    }
                    None => out.push(ChordAction::Status("Import: the file cannot be read.".into())),
                }
            }
            if let Some((chd, bad)) = self.pending.clone() {
                let mut go = if list.is_empty() { Some(true) } else { None };
                if go.is_none() {
                    ui.horizontal(|ui| {
                        ui.colored_label(Color32::from_rgb(0xC0, 0x60, 0x00), format!("Import {} chords: the song has {} already.", chd.len(), list.len()));
                        if ui.button("Replace").clicked() {
                            go = Some(true);
                        }
                        if ui.button("Add").clicked() {
                            go = Some(false);
                        }
                        if ui.button("Cancel").clicked() {
                            self.pending = None;
                        }
                    });
                }
                if let Some(replace) = go {
                    self.pending = None;
                    let n = chd.len();
                    out.push(ChordAction::Edit(Box::new(move |tr| {
                        if replace {
                            clear(tr);
                        }
                        if let Some(t) = tr.get_mut(master) {
                            for (tick, d) in chd {
                                t.others.push(Event::Meta { t: tick, mtype: 0x7F, data: d });
                            }
                        }
                    })));
                    out.push(ChordAction::Status(format!(
                        "Imported {n} chords{}{} (undo with Ctrl+Z).",
                        if replace { ", the old ones replaced" } else { ", added" },
                        if bad > 0 { format!("; {bad} lines not understood") } else { String::new() }
                    )));
                }
            }
            ui.label(egui::RichText::new("click = select + song position").small());
            // a fixed height, as in the Lyrics window (the window must not grow with it)
            egui::ScrollArea::vertical().id_source("chd_list").max_height(340.0).auto_shrink([false, false]).show(ui, |ui| {
                egui::Grid::new("chd_grid").striped(true).num_columns(2).show(ui, |ui| {
                    let fs = 13.0 * self.font;
                    for (k, (t, _, _, name)) in list.iter().enumerate() {
                        let (m, b, c) = tm.to_mbc(*t);
                        let r1 = ui.selectable_label(self.sel == Some(k), egui::RichText::new(format!("{m:04}:{b}:{c:03}")).monospace().size(fs));
                        let r2 = ui.selectable_label(self.sel == Some(k), egui::RichText::new(name).strong().size(fs * 1.15));
                        let r = r1.union(r2);
                        if r.clicked() {
                            // the song position (and the preview) follow the clicked chord
                            self.sel = Some(k);
                            self.edit_text = name.clone();
                            out.push(ChordAction::Locate(*t));
                        }
                        ui.end_row();
                    }
                });
            });

            // ---- preview + typing
            let ui = &mut cols[1];
            ui.label(egui::RichText::new("Chord preview").strong());
            let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 150.0), egui::Sense::hover());
            let p = ui.painter_at(rect);
            p.rect_filled(rect, 4.0, Color32::from_rgb(16, 20, 40));
            let _ = playing;
            let at = song_pos;
            let now = list.iter().rposition(|c| c.0 <= at);
            if list.is_empty() {
                p.text(rect.center(), egui::Align2::CENTER_CENTER, "No chords in the song.", egui::FontId::proportional(14.0), Color32::from_gray(150));
            } else {
                let cur = now.map(|k| list[k].3.as_str()).unwrap_or("");
                p.text(egui::pos2(rect.center().x, rect.top() + 55.0), egui::Align2::CENTER_CENTER, cur, egui::FontId::proportional(44.0 * self.font), Color32::from_rgb(255, 200, 40));
                let next: Vec<&str> = list.iter().skip(now.map_or(0, |k| k + 1)).take(4).map(|c| c.3.as_str()).collect();
                p.text(egui::pos2(rect.center().x, rect.bottom() - 28.0), egui::Align2::CENTER_CENTER, next.join("   "), egui::FontId::proportional(20.0 * self.font), Color32::WHITE);
            }
            if let Some(t) = time_line(ui, tracks, tm, song_pos, "chd_scrub") {
                out.push(ChordAction::Locate(t));
            }
            ui.add_space(6.0);
            ui.separator();
            ui.label(egui::RichText::new("Type chords onto bars / beats").strong());
            ui.label(egui::RichText::new("Chords separated by spaces or lines (C G Am7 F/A); \"-\" keeps a step empty.").small());
            ui.add(egui::TextEdit::multiline(&mut self.typed).desired_rows(5).desired_width(f32::INFINITY).font(egui::TextStyle::Monospace));
            ui.horizontal(|ui| {
                ui.label("from bar");
                ui.add(egui::DragValue::new(&mut self.from_bar).range(1..=9999));
                egui::ComboBox::from_id_source("chd_step").selected_text(STEPS[self.step as usize]).show_ui(ui, |ui| {
                    for (k, s) in STEPS.iter().enumerate() {
                        ui.selectable_value(&mut self.step, k as u8, *s);
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.replace, "Replace chords in that range");
                if ui.button("Place").clicked() {
                    let (placed, bad) = place_typed(&self.typed, tm, self.from_bar, self.step);
                    if placed.is_empty() {
                        out.push(ChordAction::Status(if bad.is_empty() { "Type the chords first.".into() } else { format!("Not XF chords: {}", bad.join(" ")) }));
                    } else {
                        let (from, to) = (placed[0].0, placed.last().map_or(0, |p| p.0) + 1);
                        let (n, replace) = (placed.len(), self.replace);
                        out.push(ChordAction::Edit(Box::new(move |tr| {
                            if replace {
                                for t in tr.iter_mut() {
                                    t.others.retain(|e| !matches!(e, Event::Meta { t, mtype: 0x7F, data } if *t >= from && *t < to && xf_chord_name(data).is_some()));
                                }
                            }
                            if let Some(t) = tr.get_mut(master) {
                                for (tick, d) in placed {
                                    t.others.push(Event::Meta { t: tick, mtype: 0x7F, data: d });
                                }
                            }
                        })));
                        out.push(ChordAction::Status(if bad.is_empty() { format!("{n} chords placed.") } else { format!("{n} chords placed; not XF chords: {}", bad.join(" ")) }));
                    }
                }
                if ui.button("Load current chords").clicked() {
                    self.typed = list.iter().map(|c| c.3.clone()).collect::<Vec<_>>().join(" ");
                }
            });
        });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transpose_and_type() {
        assert_eq!(transpose_name("Am7/C", 2).as_deref(), Some("Bm7/D"));
        assert_eq!(transpose_name("Bb", 1).as_deref(), Some("B"));
        assert_eq!(transpose_name("C#m", -1).as_deref(), Some("Cm"));
        assert_eq!(transpose_name("N.C.", 3).as_deref(), Some("N.C."));
        let t = EditTrack { notes: vec![xgworks_core::EditNote { tick: 7680, dur: 10, pitch: 60, vel: 100 }], ..Default::default() };
        let tm = TimeMap::new(std::slice::from_ref(&t), 480);
        let (v, bad) = place_typed("C G - Am7 Hxx", &tm, 2, 0);
        assert_eq!(v.iter().map(|x| x.0).collect::<Vec<_>>(), vec![1920, 3840, 7680]);
        assert_eq!(xf_chord_name(&v[2].1).as_deref(), Some("Am7"));
        assert_eq!(bad, vec!["Hxx".to_string()]);
        let (v, _) = place_typed("C F", &tm, 1, 2);
        assert_eq!(v[1].0, 480);
    }
}
