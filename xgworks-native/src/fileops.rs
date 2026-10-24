//! File menu like the original (Menu.hlp "File"): New (Ctrl+N), Open
//! (Ctrl+O), Close, Save (Ctrl+S, overwrites the file it came from), Save
//! As (.XWS / .MID) — and the question "save the changes?" before a song is
//! replaced or the app quits.

use super::App;
use eframe::egui;
use xgworks_core::{write_mid, write_xws, EditTrack, Event};

use crate::platform;

/// What to do once the user answered the "save changes?" question.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum After {
    New,
    Open,
    Close,
    Quit,
}

fn hash(b: &[u8]) -> u64 {
    // FNV-1a
    b.iter().fold(0xcbf2_9ce4_8422_2325u64, |h, &x| (h ^ x as u64).wrapping_mul(0x0100_0000_01b3))
}

/// An empty song: 16 tracks on channels 1–16, tempo 120 and 4/4 at the start.
pub fn new_song() -> Vec<EditTrack> {
    (0..16u8)
        .map(|ch| {
            let mut t = EditTrack { channel: ch, ..Default::default() };
            if ch == 0 {
                t.others.push(Event::Meta { t: 0, mtype: 0x51, data: vec![0x07, 0xA1, 0x20] });
                t.others.push(Event::Meta { t: 0, mtype: 0x58, data: vec![4, 2, 24, 8] });
            }
            t
        })
        .collect()
}

impl App {
    fn song_hash(&self) -> u64 {
        hash(&write_xws(&self.edit, self.ppqn))
    }

    /// Remember the current song as saved (after load / save / new).
    pub(super) fn mark_saved(&mut self) {
        self.saved_hash = self.song_hash();
    }

    pub(super) fn has_changes(&self) -> bool {
        !self.edit.is_empty() && self.song_hash() != self.saved_hash
    }

    /// Run `after`, asking first when the song has unsaved changes.
    pub(super) fn file_command(&mut self, after: After, ctx: &egui::Context) {
        if self.has_changes() {
            self.ask_save = Some(after);
        } else {
            self.do_after(after, ctx);
        }
    }

    fn do_after(&mut self, after: After, ctx: &egui::Context) {
        match after {
            After::New => {
                self.load_model(new_song(), 480, "Untitled.XWS");
                self.file_path = None;
                self.status = "New song.".into();
            }
            After::Open => platform::open_song(&self.picked, ctx),
            After::Close => {
                self.stop();
                self.load_model(Vec::new(), 480, "");
                self.file_path = None;
                self.status = "Song closed.".into();
            }
            After::Quit => {
                self.quitting = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
    }

    /// Save to the file the song came from (same format), else Save As.
    pub(super) fn save_current(&mut self) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(p) = self.file_path.clone() {
            let mid = p.extension().is_some_and(|e| e.eq_ignore_ascii_case("mid"));
            let bytes = if mid { write_mid(&self.edit, self.ppqn) } else { write_xws(&self.edit, self.ppqn) };
            return match std::fs::write(&p, &bytes) {
                Ok(_) => {
                    self.mark_saved();
                    self.status = format!("Saved {}.", p.display());
                    true
                }
                Err(e) => {
                    self.status = format!("Save failed: {e}");
                    false
                }
            };
        }
        self.save_as(false)
    }

    /// Save As .XWS / .MID; returns true when written.
    pub(super) fn save_as(&mut self, as_mid: bool) -> bool {
        let (ext, filter) = if as_mid { ("mid", "MIDI") } else { ("xws", "XGworks") };
        let stem = std::path::Path::new(&self.file_name).file_stem().and_then(|s| s.to_str()).unwrap_or("song").to_string();
        let bytes = if as_mid { write_mid(&self.edit, self.ppqn) } else { write_xws(&self.edit, self.ppqn) };
        match platform::save_song_to(&format!("{stem}.{ext}"), ext, filter, &bytes) {
            Some((msg, path)) => {
                self.status = msg;
                if path.is_some() || cfg!(target_arch = "wasm32") {
                    self.mark_saved();
                    if let Some(p) = path {
                        self.file_name = p.file_name().and_then(|s| s.to_str()).unwrap_or("song").to_string();
                        self.file_path = Some(p);
                    }
                }
                true
            }
            None => false,
        }
    }

    /// Ctrl/Cmd + N / O / S, and the window's close button.
    pub(super) fn file_keys(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting && self.has_changes() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.ask_save = Some(After::Quit);
        }
        if ctx.wants_keyboard_input() {
            return;
        }
        let cmd = |k: egui::Key| ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, k));
        if cmd(egui::Key::N) {
            self.file_command(After::New, ctx);
        }
        if cmd(egui::Key::O) {
            self.file_command(After::Open, ctx);
        }
        if cmd(egui::Key::S) && !self.edit.is_empty() {
            self.save_current();
        }
    }

    /// The File menu.
    pub(super) fn file_menu(&mut self, ui: &mut egui::Ui) {
        let has = !self.edit.is_empty();
        ui.menu_button("File", |ui| {
            let ctx = ui.ctx().clone();
            if ui.button("New                 Ctrl+N").clicked() {
                ui.close_menu();
                self.file_command(After::New, &ctx);
            }
            if ui.button("Open…               Ctrl+O").clicked() {
                ui.close_menu();
                self.file_command(After::Open, &ctx);
            }
            if ui.add_enabled(has, egui::Button::new("Close")).clicked() {
                ui.close_menu();
                self.file_command(After::Close, &ctx);
            }
            ui.separator();
            if ui.add_enabled(has, egui::Button::new("Save                Ctrl+S")).clicked() {
                ui.close_menu();
                self.save_current();
            }
            if ui.add_enabled(has, egui::Button::new("Save As .XWS…")).clicked() {
                ui.close_menu();
                self.save_as(false);
            }
            if ui.add_enabled(has, egui::Button::new("Save As .MID…")).clicked() {
                ui.close_menu();
                self.save_as(true);
            }
            ui.separator();
            if ui.button("Exit").clicked() {
                ui.close_menu();
                self.file_command(After::Quit, &ctx);
            }
        });
    }

    /// "Save changes to <song>?" — Yes / No / Cancel.
    pub(super) fn save_question(&mut self, ctx: &egui::Context) {
        let Some(after) = self.ask_save else { return };
        let mut answer = None;
        egui::Window::new("XGWorks NextGen").collapsible(false).resizable(false).anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0]).show(ctx, |ui| {
            ui.label(format!("Save the changes to {}?", if self.file_name.is_empty() { "the song" } else { &self.file_name }));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("  Yes  ").clicked() {
                    answer = Some(1);
                }
                if ui.button("  No  ").clicked() {
                    answer = Some(2);
                }
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    answer = Some(0);
                }
            });
        });
        match answer {
            Some(1) => {
                self.ask_save = None;
                if self.save_current() {
                    self.do_after(after, ctx);
                }
            }
            Some(2) => {
                self.ask_save = None;
                self.do_after(after, ctx);
            }
            Some(_) => self.ask_save = None,
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn new_song_round_trips() {
        let t = super::new_song();
        assert_eq!(t.len(), 16);
        let bytes = xgworks_core::write_xws(&t, 480);
        let (back, ppqn) = xgworks_core::edit_model(&bytes);
        assert_eq!(ppqn, 480);
        assert!(!back.is_empty());
    }
}
