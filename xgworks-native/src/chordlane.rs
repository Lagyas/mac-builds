//! Chord track (Track.hlp "スタイル/コードトラックでの操作"): XF chord events
//! (FF 7F 43 7B 01 cr ct bn bt) on the master track, shown in the C lane.
//! Pencil click = Chord Insert dialog (root + accidental, one of the XF
//! chord types, bass note, position); arrow click = select (Ctrl adds),
//! drag = move (beat grid), double-click = edit, eraser / Del = delete;
//! right-click the "C" header = Select All / Clear. Edit ▸ Insert Chord
//! opens the dialog at the song position.

use super::{xf_chord, App, CHORD_TYPES};
use eframe::egui::{self, Color32};
use xgworks_core::Event;

const LETTERS: [&str; 7] = ["C", "D", "E", "F", "G", "A", "B"];

#[derive(Clone)]
pub struct ChordDlg {
    /// Some = editing this chord (track, index in `others`).
    pub edit: Option<(usize, usize)>,
    pub root: usize,
    pub acc: i8,
    pub typ: usize,
    /// None = no on-bass note.
    pub bass: Option<(usize, i8)>,
    pub pos: [u32; 3],
}

fn root_byte(letter: usize, acc: i8) -> u8 {
    (((3 + acc as i32).clamp(0, 6) as u8) << 4) | (letter as u8 + 1)
}

fn from_byte(b: u8) -> Option<(usize, i8)> {
    let n = (b & 0x0F) as usize;
    (b != 0x7F && (1..=7).contains(&n)).then(|| (n - 1, ((b >> 4) as i8 - 3).clamp(-1, 1)))
}

impl ChordDlg {
    fn data(&self) -> Vec<u8> {
        let (bn, bt) = match self.bass {
            Some((l, a)) => (root_byte(l, a), 0),
            None => (0x7F, 0x7F),
        };
        vec![0x43, 0x7B, 0x01, root_byte(self.root, self.acc), self.typ as u8, bn, bt]
    }

    fn name(&self) -> String {
        xf_chord(&self.data()).unwrap_or_default()
    }
}

impl App {
    /// Chords of the song: (tick, track, index, name), by time.
    pub(super) fn chord_list(&self) -> Vec<(u32, usize, usize, String)> {
        let mut v: Vec<(u32, usize, usize, String)> = self
            .edit
            .iter()
            .enumerate()
            .flat_map(|(ti, t)| {
                t.others.iter().enumerate().filter_map(move |(i, e)| match e {
                    Event::Meta { t, mtype: 0x7F, data } => xf_chord(data).map(|c| (*t, ti, i, c)),
                    _ => None,
                })
            })
            .collect();
        v.sort_by_key(|c| (c.0, c.1, c.2));
        v
    }

    pub(super) fn open_chord_dialog(&mut self, tick: u32) {
        let (m, b, c) = self.time_map().to_mbc(tick);
        self.chord_dlg = Some(ChordDlg { edit: None, root: 0, acc: 0, typ: 0, bass: None, pos: [m, b, c] });
    }

    fn edit_chord_dialog(&mut self, ti: usize, i: usize) {
        let Some(Event::Meta { t, data, .. }) = self.edit.get(ti).and_then(|tr| tr.others.get(i)) else { return };
        if data.len() < 7 {
            return;
        }
        let (m, b, c) = self.time_map().to_mbc(*t);
        let (root, acc) = from_byte(data[3]).unwrap_or((0, 0));
        self.chord_dlg = Some(ChordDlg { edit: Some((ti, i)), root, acc, typ: (data[4] as usize).min(CHORD_TYPES.len() - 1), bass: from_byte(data[5]), pos: [m, b, c] });
    }

    /// Input on the chord lane (`lane` = the lane right of the header).
    pub(super) fn chord_lane_input(&mut self, ui: &egui::Ui, lane: egui::Rect, hdr: egui::Rect, x_of: &dyn Fn(u32) -> f32, tick_of: &dyn Fn(f32) -> u32) {
        let tm = self.time_map();
        let beat_snap = |t: u32| {
            let (m, b, c) = tm.to_mbc(t);
            let bl = tm.beat_len(t).max(1);
            tm.from_mbc(m, b, 0) + if c * 2 >= bl { bl } else { 0 }
        };
        let list = self.chord_list();
        let hit = |x: f32| list.iter().rev().find(|c| x >= x_of(c.0) - 3.0 && x <= x_of(c.0) + 40.0).map(|c| (c.1, c.2, c.0));
        let resp = ui.interact(lane, ui.id().with("chord_lane"), egui::Sense::click_and_drag());
        let ctrl = ui.input(|i| i.modifiers.ctrl || i.modifiers.command);
        // selection highlight
        let p = ui.painter_at(lane);
        for c in &list {
            if self.chord_sel.contains(&(c.1, c.2)) {
                let x = x_of(c.0);
                let r = egui::Rect::from_min_max(egui::pos2(x, lane.top() + 2.0), egui::pos2(x + 44.0, lane.bottom() - 2.0));
                p.rect_stroke(r, 2.0, egui::Stroke::new(2.0_f32, Color32::from_rgb(220, 0, 0)));
            }
        }
        if resp.hovered() {
            match self.tv_tool {
                super::TvTool::Pencil => crate::cursors::show(ui.ctx(), crate::cursors::Cur::Pencil),
                super::TvTool::Eraser => crate::cursors::show(ui.ctx(), crate::cursors::Cur::Eraser),
                _ => {}
            }
        }
        if resp.double_clicked() {
            if let Some((ti, i, _)) = resp.interact_pointer_pos().and_then(|p| hit(p.x)) {
                self.edit_chord_dialog(ti, i);
                return;
            }
        }
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                match (self.tv_tool, hit(pos.x)) {
                    (super::TvTool::Eraser, Some((ti, i, _))) => {
                        self.tv_snapshot();
                        self.edit[ti].others.remove(i);
                        self.chord_sel.clear();
                        self.song_edited();
                    }
                    (super::TvTool::Pencil, None) => self.open_chord_dialog(beat_snap(tick_of(pos.x))),
                    (_, Some((ti, i, _))) => {
                        if ctrl {
                            if let Some(k) = self.chord_sel.iter().position(|s| *s == (ti, i)) {
                                self.chord_sel.remove(k);
                            } else {
                                self.chord_sel.push((ti, i));
                            }
                        } else {
                            self.chord_sel = vec![(ti, i)];
                        }
                    }
                    (_, None) => self.chord_sel.clear(),
                }
            }
        }
        // drag = move the selected chords on the beat grid
        if resp.drag_started() {
            if let Some((ti, i, t)) = ui.input(|i| i.pointer.press_origin()).and_then(|p| hit(p.x)) {
                if !self.chord_sel.contains(&(ti, i)) {
                    self.chord_sel = vec![(ti, i)];
                }
                self.tv_snapshot();
                self.chord_drag = Some(t);
            }
        }
        if let (Some(t0), Some(pos)) = (self.chord_drag, resp.interact_pointer_pos()) {
            if resp.dragged() {
                let start = ui.input(|i| i.pointer.press_origin()).map_or(pos.x, |p| p.x);
                let target = beat_snap((t0 as i64 + tick_of(pos.x) as i64 - tick_of(start) as i64).max(0) as u32);
                let d = target as i64 - self.chord_tick_of_first_sel() as i64;
                if d != 0 {
                    for &(ti, i) in &self.chord_sel {
                        if let Some(Event::Meta { t, .. }) = self.edit[ti].others.get_mut(i) {
                            *t = (*t as i64 + d).max(0) as u32;
                        }
                    }
                }
            }
        }
        if resp.drag_stopped() && self.chord_drag.take().is_some() {
            self.song_edited();
        }
        // Del removes the selected chords
        if !self.chord_sel.is_empty() && !ui.ctx().wants_keyboard_input() && ui.input(|i| i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace)) {
            self.tv_snapshot();
            let mut sel = std::mem::take(&mut self.chord_sel);
            sel.sort_by(|a, b| b.cmp(a));
            for (ti, i) in sel {
                if i < self.edit[ti].others.len() {
                    self.edit[ti].others.remove(i);
                }
            }
            self.song_edited();
        }
        // the "C" header: Select All / Clear (Track.hlp popup)
        let hr = ui.interact(hdr, ui.id().with("chord_hdr"), egui::Sense::click());
        hr.context_menu(|ui| {
            if ui.button("Insert Chord…").clicked() {
                self.open_chord_dialog(self.cur_tick());
                ui.close_menu();
            }
            if ui.button("Select All").clicked() {
                self.chord_sel = self.chord_list().iter().map(|c| (c.1, c.2)).collect();
                ui.close_menu();
            }
            if ui.button("Clear (delete all chords)").clicked() {
                self.tv_snapshot();
                for t in self.edit.iter_mut() {
                    t.others.retain(|e| !matches!(e, Event::Meta { mtype: 0x7F, data, .. } if xf_chord(data).is_some()));
                }
                self.chord_sel.clear();
                self.song_edited();
                ui.close_menu();
            }
        });
    }

    fn chord_tick_of_first_sel(&self) -> u32 {
        self.chord_sel
            .iter()
            .filter_map(|&(ti, i)| match self.edit.get(ti).and_then(|t| t.others.get(i)) {
                Some(Event::Meta { t, .. }) => Some(*t),
                _ => None,
            })
            .min()
            .unwrap_or(0)
    }

    /// The Chord Insert dialog (own window).
    pub(super) fn chord_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.chord_dlg.take() else { return };
        let mut action = 0; // 1 insert / apply, 2 close
        let title = if d.edit.is_some() { "Edit Chord" } else { "Chord Insert" };
        let open = super::sub_window(ctx, "chord_dlg", title, [560.0, 400.0], |ui| {
            egui::Frame::none().inner_margin(10.0).show(ui, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("Root").strong());
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                for (k, l) in LETTERS.iter().enumerate() {
                                    ui.radio_value(&mut d.root, k, *l);
                                }
                            });
                            ui.vertical(|ui| {
                                ui.radio_value(&mut d.acc, 1, "#");
                                ui.radio_value(&mut d.acc, 0, "natural");
                                ui.radio_value(&mut d.acc, -1, "b");
                            });
                        });
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("Type").strong());
                        egui::ScrollArea::vertical().id_source("chord_types").max_height(220.0).show(ui, |ui| {
                            for (k, t) in CHORD_TYPES.iter().enumerate().take(34) {
                                let label = if t.is_empty() { "(major)" } else { t };
                                ui.selectable_value(&mut d.typ, k, label);
                            }
                        });
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        ui.label(egui::RichText::new("Bass note").strong());
                        let mut on = d.bass.is_some();
                        ui.checkbox(&mut on, "On-bass");
                        if on && d.bass.is_none() {
                            d.bass = Some((d.root, d.acc));
                        }
                        if !on {
                            d.bass = None;
                        }
                        if let Some((l, a)) = d.bass.as_mut() {
                            egui::ComboBox::from_id_source("bass_l").width(50.0).selected_text(LETTERS[*l]).show_ui(ui, |ui| {
                                for (k, x) in LETTERS.iter().enumerate() {
                                    ui.selectable_value(l, k, *x);
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.radio_value(a, 1, "#");
                                ui.radio_value(a, 0, "nat");
                                ui.radio_value(a, -1, "b");
                            });
                        }
                        ui.add_space(10.0);
                        ui.label(egui::RichText::new("Position").strong());
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut d.pos[0]).range(1..=9999));
                            ui.add(egui::DragValue::new(&mut d.pos[1]).range(1..=32));
                            ui.add(egui::DragValue::new(&mut d.pos[2]).range(0..=1919));
                        });
                        ui.add_space(10.0);
                        ui.label(egui::RichText::new(d.name()).size(24.0).strong());
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            let label = if d.edit.is_some() { "Apply" } else { "Insert" };
                            if ui.button(egui::RichText::new(label).strong().size(15.0)).clicked() {
                                action = 1;
                            }
                            if ui.button(egui::RichText::new("Close").size(15.0)).clicked() {
                                action = 2;
                            }
                        });
                        if d.edit.is_none() {
                            ui.label(egui::RichText::new("Insert keeps the dialog open; the position moves one bar on.").small());
                        }
                    });
                });
            });
        });
        if action == 1 {
            let tick = self.time_map().from_mbc(d.pos[0], d.pos[1], d.pos[2]);
            let data = d.data();
            let msg = format!("Chord {} at {:04}:{}:{:03}.", d.name(), d.pos[0], d.pos[1], d.pos[2]);
            self.tv_snapshot();
            match d.edit {
                Some((ti, i)) => {
                    if let Some(Event::Meta { t, data: dd, .. }) = self.edit.get_mut(ti).and_then(|tr| tr.others.get_mut(i)) {
                        *t = tick;
                        *dd = data;
                    }
                    action = 2;
                }
                None => {
                    if let Some(m) = self.master_track() {
                        self.edit[m].others.push(Event::Meta { t: tick, mtype: 0x7F, data });
                    }
                    // next chord one bar later
                    let (m, _, _) = self.time_map().to_mbc(tick);
                    d.pos = [m + 1, 1, 0];
                }
            }
            self.song_edited();
            self.status = msg;
        }
        if open && action != 2 {
            self.chord_dlg = Some(d);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chord_bytes() {
        let d = ChordDlg { edit: None, root: 5, acc: 0, typ: 10, bass: Some((1, 0)), pos: [1, 1, 0] };
        assert_eq!(d.data(), vec![0x43, 0x7B, 0x01, 0x36, 10, 0x32, 0]);
        assert_eq!(d.name(), "Am7/D");
        assert_eq!(from_byte(0x44), Some((3, 1))); // F#
    }
}
