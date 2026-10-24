//! Edit menu block commands (Menu.hlp "編集"): Create Block, Block Repeat,
//! Block Move, Create Link Block, Rename Block — dialogs laid out like the
//! original's (positions as measure / beat / clock), all undo-able.
//! A link block is made as a copy of the block (this model has no shared
//! block data); everything else works as in XGworks.

use super::{App, ClipBlock};
use eframe::egui;

pub enum BlockDlg {
    Create { track: usize, start: [u32; 3], end: [u32; 3] },
    Repeat { from: [u32; 3], to: [u32; 3], at: [u32; 3], count: u32 },
    Move { absolute: bool, pos: [u32; 3], amount: i32 },
    Rename { ti: usize, bi: usize, name: String },
    /// Chord Repeat: the chords in from..to copied to `at`, `count` times.
    ChordRepeat { from: [u32; 3], to: [u32; 3], at: [u32; 3], count: u32 },
}

fn loc(ui: &mut egui::Ui, label: &str, v: &mut [u32; 3]) {
    ui.label(label);
    ui.horizontal(|ui| {
        ui.add(egui::DragValue::new(&mut v[0]).range(1..=9999));
        ui.label(":");
        ui.add(egui::DragValue::new(&mut v[1]).range(1..=32));
        ui.label(":");
        ui.add(egui::DragValue::new(&mut v[2]).range(0..=1919));
    });
}

impl App {
    fn mbc(&self, t: u32) -> [u32; 3] {
        let (m, b, c) = self.time_map().to_mbc(t);
        [m, b, c]
    }

    fn tick_of(&self, v: [u32; 3]) -> u32 {
        self.time_map().from_mbc(v[0], v[1], v[2])
    }

    /// Edit menu entries (inside the Edit menu).
    pub(super) fn block_menu(&mut self, ui: &mut egui::Ui) {
        let has = !self.edit.is_empty();
        let sel = !self.blk_sel.is_empty();
        let bar = self.time_map().bar_len(0);
        if ui.add_enabled(has, egui::Button::new("Create Block…")).clicked() {
            let num = self.visible_tracks().iter().position(|&v| v == self.sel_track).map_or(1, |k| k + 1);
            let s = self.cur_tick();
            self.block_dlg = Some(BlockDlg::Create { track: num, start: self.mbc(s / bar * bar), end: self.mbc(s / bar * bar + bar * 4) });
            ui.close_menu();
        }
        if ui.add_enabled(has, egui::Button::new("Block Repeat…")).clicked() {
            let (s, e) = if self.loc_r > self.loc_l { (self.loc_l, self.loc_r) } else { (0, bar * 4) };
            self.block_dlg = Some(BlockDlg::Repeat { from: self.mbc(s), to: self.mbc(e), at: self.mbc(e), count: 1 });
            ui.close_menu();
        }
        if ui.add_enabled(sel, egui::Button::new("Block Move…")).clicked() {
            let first = self.blk_sel.iter().map(|&(ti, bi)| self.edit[ti].blocks[bi].start).min().unwrap_or(0);
            self.block_dlg = Some(BlockDlg::Move { absolute: true, pos: self.mbc(first), amount: 0 });
            ui.close_menu();
        }
        if ui.add_enabled(sel, egui::Button::new("Create Link Block")).on_hover_text("A copy of each selected block right after it").clicked() {
            self.link_blocks();
            ui.close_menu();
        }
        if ui.add_enabled(has, egui::Button::new("Chord Repeat…")).clicked() {
            let (s, e) = if self.loc_r > self.loc_l { (self.loc_l, self.loc_r) } else { (0, bar * 4) };
            self.block_dlg = Some(BlockDlg::ChordRepeat { from: self.mbc(s), to: self.mbc(e), at: self.mbc(e), count: 1 });
            ui.close_menu();
        }
        if ui.add_enabled(self.blk_sel.len() == 1, egui::Button::new("Rename Block…")).clicked() {
            let (ti, bi) = self.blk_sel[0];
            self.block_dlg = Some(BlockDlg::Rename { ti, bi, name: self.edit[ti].blocks[bi].name.clone() });
            ui.close_menu();
        }
    }

    fn link_blocks(&mut self) {
        self.tv_snapshot();
        let mut sel = self.blk_sel.clone();
        sel.sort_by(|a, b| b.cmp(a));
        let mut n = 0;
        for (ti, bi) in sel {
            let b = &self.edit[ti].blocks[bi];
            let (start, end) = (b.start, b.end.max(b.start + 1));
            let clip = ClipBlock::take(&self.edit[ti], bi, start, ti);
            clip.paste(&mut self.edit[ti], end);
            n += 1;
        }
        self.blk_sel.clear();
        self.song_edited();
        self.status = format!("{n} block(s) repeated after themselves.");
    }

    /// The open block dialog (own window).
    pub(super) fn block_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.block_dlg.take() else { return };
        let title = match d {
            BlockDlg::Create { .. } => "Create Block",
            BlockDlg::Repeat { .. } => "Block Repeat",
            BlockDlg::Move { .. } => "Block Move",
            BlockDlg::Rename { .. } => "Rename Block",
            BlockDlg::ChordRepeat { .. } => "Chord Repeat",
        };
        let mut act = 0; // 1 ok, 2 apply (keep open), 3 cancel
        let ntracks = self.visible_tracks().len() as u32 + 30;
        let open = super::sub_window(ctx, ("block_dlg", title), title, [380.0, 260.0], |ui| {
            egui::Grid::new("blk").num_columns(2).spacing([10.0, 8.0]).show(ui, |ui| {
                match &mut d {
                    BlockDlg::Create { track, start, end } => {
                        ui.label("Track");
                        ui.add(egui::DragValue::new(track).range(1..=ntracks));
                        ui.end_row();
                        ui.label("Type");
                        ui.label("MIDI");
                        ui.end_row();
                        loc(ui, "Start", start);
                        ui.end_row();
                        loc(ui, "End", end);
                        ui.end_row();
                    }
                    BlockDlg::Repeat { from, to, at, count } | BlockDlg::ChordRepeat { from, to, at, count } => {
                        loc(ui, "Copy from", from);
                        ui.end_row();
                        loc(ui, "Copy to", to);
                        ui.end_row();
                        loc(ui, "Insert at", at);
                        ui.end_row();
                        ui.label("Times");
                        ui.add(egui::DragValue::new(count).range(1..=999));
                        ui.end_row();
                    }
                    BlockDlg::Move { absolute, pos, amount } => {
                        ui.radio_value(absolute, true, "To position");
                        ui.horizontal(|ui| {
                            ui.add_enabled(*absolute, egui::DragValue::new(&mut pos[0]).range(1..=9999));
                            ui.add_enabled(*absolute, egui::DragValue::new(&mut pos[1]).range(1..=32));
                            ui.add_enabled(*absolute, egui::DragValue::new(&mut pos[2]).range(0..=1919));
                        });
                        ui.end_row();
                        ui.radio_value(absolute, false, "By (clocks)");
                        ui.add_enabled(!*absolute, egui::DragValue::new(amount).range(-999_999..=999_999));
                        ui.end_row();
                    }
                    BlockDlg::Rename { name, .. } => {
                        ui.label("Block name");
                        let r = ui.add(egui::TextEdit::singleline(name).desired_width(200.0));
                        r.request_focus();
                        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                            act = 1;
                        }
                        ui.end_row();
                    }
                }
            });
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button(egui::RichText::new("  OK  ").strong()).clicked() {
                    act = 1;
                }
                if matches!(d, BlockDlg::Move { .. }) && ui.button("Update").on_hover_text("Move and keep the dialog open").clicked() {
                    act = 2;
                }
                if ui.button("Cancel").clicked() {
                    act = 3;
                }
            });
        });
        if act == 1 || act == 2 {
            self.run_block_dlg(&d);
        }
        if open && (act == 0 || act == 2) {
            self.block_dlg = Some(d);
        }
    }

    fn run_block_dlg(&mut self, d: &BlockDlg) {
        match d {
            BlockDlg::Create { track, start, end } => {
                let (s, e) = (self.tick_of(*start), self.tick_of(*end));
                if e <= s {
                    self.status = "The end must be after the start.".into();
                    return;
                }
                self.tv_snapshot();
                let vis = self.visible_tracks();
                let ti = match vis.get(*track as usize - 1) {
                    Some(&ti) => ti,
                    None => {
                        // a pool row: new track on the row's channel
                        let ch = ((*track - 1) % 16) as u8;
                        self.edit.push(xgworks_core::EditTrack { channel: ch, ..Default::default() });
                        self.edit.len() - 1
                    }
                };
                self.materialize_blocks(ti);
                let t = &mut self.edit[ti];
                t.blocks.push(xgworks_core::EditBlock { name: String::new(), start: s, end: e, raw: Vec::new() });
                t.blocks.sort_by_key(|b| b.start);
                self.song_edited();
                self.status = format!("Block created on track {track}.");
            }
            BlockDlg::Repeat { from, to, at, count } => {
                let (s, e, a) = (self.tick_of(*from), self.tick_of(*to), self.tick_of(*at));
                if e <= s {
                    self.status = "Copy to must be after Copy from.".into();
                    return;
                }
                self.tv_snapshot();
                let mut n = 0;
                for ti in self.visible_tracks() {
                    self.materialize_blocks(ti);
                    let clips: Vec<ClipBlock> = (0..self.edit[ti].blocks.len())
                        .filter(|&bi| {
                            let b = &self.edit[ti].blocks[bi];
                            b.start >= s && b.end.max(b.start) <= e
                        })
                        .map(|bi| ClipBlock::take(&self.edit[ti], bi, s, ti))
                        .collect();
                    for k in 0..*count {
                        for c in &clips {
                            c.paste(&mut self.edit[ti], a + k * (e - s));
                            n += 1;
                        }
                    }
                }
                self.blk_sel.clear();
                self.song_edited();
                self.status = format!("{n} block copies made.");
            }
            BlockDlg::Move { absolute, pos, amount } => {
                if self.blk_sel.is_empty() {
                    return;
                }
                let first = self.blk_sel.iter().map(|&(ti, bi)| self.edit[ti].blocks[bi].start).min().unwrap_or(0) as i64;
                let delta = if *absolute { self.tick_of(*pos) as i64 - first } else { *amount as i64 };
                if delta == 0 {
                    return;
                }
                self.tv_snapshot();
                let mut sel = std::mem::take(&mut self.blk_sel);
                sel.sort_by(|a, b| b.cmp(a));
                // each block relative to its own start, pasted at start + delta
                let clips: Vec<(u32, ClipBlock)> = sel
                    .iter()
                    .map(|&(ti, bi)| {
                        let st = self.edit[ti].blocks[bi].start;
                        (st, ClipBlock::take(&self.edit[ti], bi, st, ti))
                    })
                    .collect();
                for &(ti, bi) in &sel {
                    ClipBlock::remove(&mut self.edit[ti], bi);
                }
                for (st, c) in &clips {
                    let ti = c.track;
                    let to = (*st as i64 + delta).max(0) as u32;
                    let bi = c.paste(&mut self.edit[ti], to);
                    self.blk_sel.push((ti, bi));
                }
                self.song_edited();
                self.status = format!("{} block(s) moved.", clips.len());
            }
            BlockDlg::ChordRepeat { from, to, at, count } => {
                let (s, e, a) = (self.tick_of(*from), self.tick_of(*to), self.tick_of(*at));
                let Some(m) = self.master_track() else { return };
                if e <= s {
                    return;
                }
                let chords: Vec<(u32, Vec<u8>)> = self
                    .chord_list()
                    .iter()
                    .filter(|c| c.0 >= s && c.0 < e)
                    .filter_map(|c| match &self.edit[c.1].others[c.2] {
                        xgworks_core::Event::Meta { data, .. } => Some((c.0 - s, data.clone())),
                        _ => None,
                    })
                    .collect();
                if chords.is_empty() {
                    self.status = "No chords in that range.".into();
                    return;
                }
                self.tv_snapshot();
                for k in 0..*count {
                    for (rel, d) in &chords {
                        self.edit[m].others.push(xgworks_core::Event::Meta { t: a + k * (e - s) + rel, mtype: 0x7F, data: d.clone() });
                    }
                }
                self.song_edited();
                self.status = format!("{} chords copied {count}×.", chords.len());
            }
            BlockDlg::Rename { ti, bi, name } => {
                if self.edit.get(*ti).is_some_and(|t| *bi < t.blocks.len()) {
                    self.tv_snapshot();
                    self.edit[*ti].blocks[*bi].name = name.clone();
                }
                self.song_edited();
            }
        }
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    #[test]
    #[ignore] // needs the sample song: cargo test -- --ignored block_commands
    fn block_commands() {
        let Ok(bytes) = std::fs::read(r"F:\rea\A Dream of a Dream.XWS") else { return };
        let mut app = App::new();
        app.load(&bytes, "dream.xws");
        let notes = |a: &App| a.edit.iter().map(|t| t.notes.len()).sum::<usize>();
        let n0 = notes(&app);
        // repeat the whole song (blocks fully inside the range) to bar 80, twice
        let d = BlockDlg::Repeat { from: [1, 1, 0], to: [76, 1, 0], at: [80, 1, 0], count: 2 };
        app.run_block_dlg(&d);
        let n1 = notes(&app);
        assert!(n1 > n0, "{n0} -> {n1}");
        // undo restores
        app.tv_undo_redo(true);
        assert_eq!(notes(&app), n0);
        // move the bass block by one beat, then rename it
        let ti = app.edit.iter().position(|t| t.name == "Bass").unwrap();
        app.materialize_blocks(ti);
        let bi = app.edit[ti].blocks.iter().position(|b| b.name == "Bass").unwrap();
        let st = app.edit[ti].blocks[bi].start;
        let blocks = app.edit[ti].blocks.clone();
        let first_note = app.edit[ti].notes.iter().filter(|n| super::super::block_owner(&blocks, n.tick) == bi).map(|n| n.tick).min().unwrap_or(st);
        app.blk_sel = vec![(ti, bi)];
        app.run_block_dlg(&BlockDlg::Move { absolute: false, pos: [1, 1, 0], amount: 480 });
        let (ti2, bi2) = app.blk_sel[0];
        assert_eq!(app.edit[ti2].blocks[bi2].start, st + 480);
        let ticks: Vec<u32> = app.edit[ti2].notes.iter().map(|n| n.tick).take(8).collect();
        let bl: Vec<(String, u32, u32)> = app.edit[ti2].blocks.iter().map(|b| (b.name.clone(), b.start, b.end)).collect();
        assert!(app.edit[ti2].notes.iter().any(|n| n.tick == first_note + 480), "bi {bi} st {st} first {first_note} ticks {ticks:?} blocks {bl:?} before {blocks:?}");
        app.run_block_dlg(&BlockDlg::Rename { ti: ti2, bi: bi2, name: "Bass A".into() });
        assert_eq!(app.edit[ti2].blocks[bi2].name, "Bass A");
    }
}
