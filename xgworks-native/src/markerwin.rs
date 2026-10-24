//! Markers (Menu.hlp Edit ▸ "Add Marker" / "Marker List"): marker meta
//! events (FF 06) on the master track. One window: add at the song position
//! (or a typed position), rename, move, delete, double-click = locate.

use eframe::egui;
use xgworks_core::listev::TimeMap;
use xgworks_core::{EditTrack, Event};

pub const MARKER: u8 = 0x06;

pub enum MarkerAction {
    Edit(Box<dyn FnOnce(&mut [EditTrack])>),
    Locate(u32),
}

pub struct MarkerState {
    sel: Option<(usize, usize)>, // (track, index in others)
    name: String,
    pos: [u32; 3],
    pos_from_song: bool,
}

impl MarkerState {
    pub fn new() -> Self {
        MarkerState { sel: None, name: String::new(), pos: [1, 1, 0], pos_from_song: true }
    }

    /// All markers of the song: (tick, track, index, name).
    pub fn markers(tracks: &[EditTrack]) -> Vec<(u32, usize, usize, String)> {
        let mut v: Vec<(u32, usize, usize, String)> = tracks
            .iter()
            .enumerate()
            .flat_map(|(ti, t)| {
                t.others.iter().enumerate().filter_map(move |(i, e)| match e {
                    Event::Meta { t, mtype: MARKER, data } => Some((*t, ti, i, String::from_utf8_lossy(data).into_owned())),
                    _ => None,
                })
            })
            .collect();
        v.sort_by_key(|m| (m.0, m.1, m.2));
        v
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, tracks: &[EditTrack], master: usize, tm: &TimeMap, song_pos: u32) -> Vec<MarkerAction> {
        let mut out = Vec::new();
        let list = Self::markers(tracks);
        if self.pos_from_song {
            let (m, b, c) = tm.to_mbc(song_pos);
            self.pos = [m, b, c];
        }
        ui.label(egui::RichText::new("Add Marker").strong());
        ui.horizontal(|ui| {
            ui.label("Position");
            let mut ch = false;
            ch |= ui.add(egui::DragValue::new(&mut self.pos[0]).range(1..=9999)).changed();
            ch |= ui.add(egui::DragValue::new(&mut self.pos[1]).range(1..=32)).changed();
            ch |= ui.add(egui::DragValue::new(&mut self.pos[2]).range(0..=1919)).changed();
            if ch {
                self.pos_from_song = false;
            }
            if ui.small_button("= song position").clicked() {
                self.pos_from_song = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Name");
            ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(180.0));
            if ui.add_enabled(!self.name.trim().is_empty(), egui::Button::new("Add")).clicked() {
                let t = tm.from_mbc(self.pos[0], self.pos[1], self.pos[2]);
                let name = self.name.trim().to_string();
                out.push(MarkerAction::Edit(Box::new(move |tr| {
                    tr[master].others.push(Event::Meta { t, mtype: MARKER, data: name.into_bytes() });
                })));
                self.name.clear();
            }
        });
        ui.separator();
        ui.label(egui::RichText::new(format!("Marker List ({})", list.len())).strong());
        let sel = self.sel.filter(|s| list.iter().any(|m| (m.1, m.2) == *s));
        ui.horizontal(|ui| {
            if ui.add_enabled(sel.is_some(), egui::Button::new("Rename")).on_hover_text("Give the selected marker the name above").clicked() && !self.name.trim().is_empty() {
                let (ti, i) = sel.unwrap();
                let name = self.name.trim().to_string();
                out.push(MarkerAction::Edit(Box::new(move |tr| {
                    if let Some(Event::Meta { data, .. }) = tr[ti].others.get_mut(i) {
                        *data = name.into_bytes();
                    }
                })));
            }
            if ui.add_enabled(sel.is_some(), egui::Button::new("Move to position")).on_hover_text("Move the selected marker to the position above").clicked() {
                let (ti, i) = sel.unwrap();
                let t = tm.from_mbc(self.pos[0], self.pos[1], self.pos[2]);
                out.push(MarkerAction::Edit(Box::new(move |tr| {
                    if let Some(Event::Meta { t: tk, .. }) = tr[ti].others.get_mut(i) {
                        *tk = t;
                    }
                })));
            }
            if ui.add_enabled(sel.is_some(), egui::Button::new("Delete")).clicked() {
                let (ti, i) = sel.unwrap();
                out.push(MarkerAction::Edit(Box::new(move |tr| {
                    tr[ti].others.remove(i);
                })));
                self.sel = None;
            }
            if ui.add_enabled(sel.is_some(), egui::Button::new("Go to")).clicked() {
                if let Some(m) = list.iter().find(|m| Some((m.1, m.2)) == sel) {
                    out.push(MarkerAction::Locate(m.0));
                }
            }
        });
        egui::ScrollArea::vertical().max_height(ui.available_height()).show(ui, |ui| {
            egui::Grid::new("markers").striped(true).num_columns(2).show(ui, |ui| {
                for (t, ti, i, name) in &list {
                    let (m, b, c) = tm.to_mbc(*t);
                    let on = sel == Some((*ti, *i));
                    let r = ui.selectable_label(on, egui::RichText::new(format!("{m:04}:{b}:{c:03}")).monospace()).union(ui.selectable_label(on, name));
                    if r.clicked() {
                        self.sel = Some((*ti, *i));
                        self.name = name.clone();
                        self.pos = [m, b, c];
                        self.pos_from_song = false;
                    }
                    if r.double_clicked() {
                        out.push(MarkerAction::Locate(*t));
                    }
                    ui.end_row();
                }
            });
        });
        out
    }
}
