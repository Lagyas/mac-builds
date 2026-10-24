//! Song position, locators and the play-control keys (ShortCut.hlp):
//! Space = play / stop, numpad - = loop on/off; and (user request) numpad
//! 0 = song start, 1 = left locator, 3 = right locator, 2 = back
//! `rewind_bars` bars (Preferences). Playing starts at the song position;
//! everything but notes before it is sent first (the original's
//! "chase" of program / control / pitch bend state).

use super::App;
use eframe::egui;
use xgworks_core::listev::TimeMap;

/// Space pressed in any window (sub-windows report it here; the main
/// window handles it next frame).
fn space_id() -> egui::Id {
    egui::Id::new("transport_space")
}

/// Call inside every window: Space toggles play / stop.
pub fn note_space(ctx: &egui::Context) {
    if !ctx.wants_keyboard_input() && ctx.input(|i| i.key_pressed(egui::Key::Space) && i.modifiers.is_none()) {
        ctx.data_mut(|d| d.insert_temp(space_id(), true));
    }
}

/// Song messages `(ms, track, bytes)` re-timed to start at `start_ms`:
/// notes before it are dropped; the last program, controller, pitch bend
/// and aftertouch per track / channel and all SysEx before it are sent at 0.
pub fn from_position(msgs: Vec<(f64, usize, Vec<u8>)>, start_ms: f64) -> Vec<(f64, usize, Vec<u8>)> {
    if start_ms <= 0.0 {
        return msgs;
    }
    let mut chase: Vec<(f64, usize, Vec<u8>)> = Vec::new();
    let mut rest = Vec::new();
    for (t, k, m) in msgs {
        if t >= start_ms {
            rest.push((t - start_ms, k, m));
            continue;
        }
        let Some(&st) = m.first() else { continue };
        match st & 0xF0 {
            0x80 | 0x90 => {}
            0xF0 => chase.push((0.0, k, m)),
            0xB0 | 0xC0 | 0xD0 | 0xE0 => {
                // keep only the latest value of the same control
                let same = |x: &Vec<u8>| x[0] == st && (st & 0xF0 != 0xB0 || x.get(1) == m.get(1));
                chase.retain(|(_, kk, x)| !(*kk == k && same(x)));
                chase.push((0.0, k, m));
            }
            _ => {}
        }
    }
    chase.extend(rest);
    chase
}

impl App {
    /// Current position: the moving playhead while playing, else the song position.
    pub(super) fn cur_tick(&self) -> u32 {
        match (&self.play_start, &self.play_tm) {
            (Some(st), Some(tm)) => {
                let ms = tm.to_ms(self.play_from) + st.elapsed().as_secs_f64() * 1000.0;
                tm.from_ms(ms)
            }
            _ => self.song_pos,
        }
    }

    /// Move the song position; playback continues from there.
    pub(super) fn locate(&mut self, tick: u32) {
        self.song_pos = tick.min(self.total_ticks);
        if self.playing && !self.monitoring {
            self.play();
        }
    }

    pub(super) fn time_map(&self) -> TimeMap {
        TimeMap::new(&self.edit, self.ppqn)
    }

    /// "0001:1:000"
    pub(super) fn loc_label(&self, tick: u32) -> String {
        let (m, b, c) = self.time_map().to_mbc(tick);
        format!("{m:04}:{b}:{c:03}")
    }

    /// Play-control keys of the main window, and Space from any window.
    pub(super) fn transport_keys(&mut self, ctx: &egui::Context) {
        note_space(ctx);
        let space = ctx.data_mut(|d| d.remove_temp::<bool>(space_id())).unwrap_or(false);
        if space && !self.edit.is_empty() {
            if self.playing {
                self.stop();
                self.status = "Stopped.".into();
            } else {
                self.play();
            }
        }
        if self.edit.is_empty() || ctx.wants_keyboard_input() || self.trns_edit.is_some() || !super::keys_active(ctx, egui::LayerId::background()) {
            return;
        }
        let key = |k: egui::Key| ctx.input(|i| i.key_pressed(k) && i.modifiers.is_none());
        if key(egui::Key::Num0) {
            self.locate(0);
        } else if key(egui::Key::Num1) {
            self.locate(self.loc_l);
        } else if key(egui::Key::Num3) {
            self.locate(self.loc_r);
        } else if key(egui::Key::Num2) {
            let tm = self.time_map();
            let (m, _, _) = tm.to_mbc(self.cur_tick());
            self.locate(tm.from_mbc(m.saturating_sub(self.rewind_bars).max(1), 1, 0));
        } else if key(egui::Key::Minus) {
            self.loop_on = !self.loop_on;
        }
    }

    /// Loop playback between the locators.
    pub(super) fn loop_check(&mut self) {
        if self.playing && !self.monitoring && self.loop_on && self.loc_r > self.loc_l && self.cur_tick() >= self.loc_r {
            self.song_pos = self.loc_l;
            self.play();
        }
    }

    /// Toolbar: Top / Play / Stop / Loop, location, L / R locators.
    pub(super) fn transport_bar(&mut self, ui: &mut egui::Ui) {
        let has = !self.edit.is_empty();
        if ui.add_enabled(has, egui::Button::new("⏮")).on_hover_text("To the song start (numpad 0)").clicked() {
            self.locate(0);
        }
        if ui.add_enabled(has && !self.playing, egui::Button::new("▶ Play")).on_hover_text("Play (Space)").clicked() {
            self.play();
        }
        if ui.add_enabled(self.playing, egui::Button::new("■ Stop")).on_hover_text("Stop (Space)").clicked() {
            self.stop();
            self.status = "Stopped.".into();
        }
        ui.toggle_value(&mut self.loop_on, "🔁").on_hover_text("Loop between L and R (numpad -)");
        ui.label(egui::RichText::new(self.loc_label(self.cur_tick())).monospace().strong());
        let tm = self.time_map();
        for (name, which) in [("L", 0), ("R", 1)] {
            let tick = if which == 0 { self.loc_l } else { self.loc_r };
            let mut bar = tm.to_mbc(tick).0;
            let tip = if which == 0 { "Left locator / loop start (numpad 1); right-click the ruler to set" } else { "Right locator / loop end (numpad 3); right-click the ruler to set" };
            ui.label(name).on_hover_text(tip);
            if ui.add(egui::DragValue::new(&mut bar).range(1..=9999)).on_hover_text(tip).changed() {
                let t = tm.from_mbc(bar, 1, 0);
                if which == 0 {
                    self.loc_l = t;
                } else {
                    self.loc_r = t;
                }
            }
        }
    }

    /// Ruler: click = song position, right-click menu = locators. Draws L / R.
    pub(super) fn ruler_input(&mut self, ui: &egui::Ui, resp: &egui::Response, x_to_tick: impl Fn(f32) -> u32, tick_to_x: impl Fn(u32) -> f32) {
        let tm = self.time_map();
        let snap = |t: u32| {
            let (m, b, _) = tm.to_mbc(t);
            tm.from_mbc(m, b, 0)
        };
        if resp.clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                self.locate(snap(x_to_tick(p.x)));
            }
        }
        if resp.secondary_clicked() {
            if let Some(p) = resp.interact_pointer_pos() {
                ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new("ruler_menu_tick"), snap(x_to_tick(p.x))));
            }
        }
        resp.clone().context_menu(|ui| {
            let t = ui.ctx().data(|d| d.get_temp::<u32>(egui::Id::new("ruler_menu_tick"))).unwrap_or(0);
            ui.label(self.loc_label(t));
            if ui.button("Set L locator here").clicked() {
                self.loc_l = t;
                ui.close_menu();
            }
            if ui.button("Set R locator here").clicked() {
                self.loc_r = t;
                ui.close_menu();
            }
            if ui.button("Move song position here").clicked() {
                self.locate(t);
                ui.close_menu();
            }
        });
        let r = resp.rect;
        let p = ui.painter_at(r);
        let col = egui::Color32::from_rgb(255, 210, 60);
        if self.loop_on && self.loc_r > self.loc_l {
            let (a, b) = (tick_to_x(self.loc_l).max(r.left()), tick_to_x(self.loc_r).min(r.right()));
            if b > a {
                p.rect_filled(egui::Rect::from_x_y_ranges(a..=b, r.bottom() - 3.0..=r.bottom()), 0.0, col);
            }
        }
        for (t, s) in [(self.loc_l, "L"), (self.loc_r, "R")] {
            let x = tick_to_x(t);
            if x >= r.left() - 1.0 && x <= r.right() {
                p.line_segment([egui::pos2(x, r.top()), egui::pos2(x, r.bottom())], egui::Stroke::new(1.0_f32, col));
                p.text(egui::pos2(x + 2.0, r.top()), egui::Align2::LEFT_TOP, s, egui::FontId::monospace(9.0), col);
            }
        }
    }

    /// Preferences window.
    pub(super) fn prefs_window(&mut self, ctx: &egui::Context) {
        if !self.show_prefs {
            return;
        }
        let mut open = true;
        egui::Window::new("Preferences").open(&mut open).resizable(false).collapsible(false).show(ctx, |ui| {
            egui::Grid::new("prefs").num_columns(2).spacing([12.0, 8.0]).show(ui, |ui| {
                ui.label("Numpad 2 jumps back");
                ui.add(egui::Slider::new(&mut self.rewind_bars, 2..=4).suffix(" bars"));
                ui.end_row();
                ui.label("XGworks data folder");
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(crate::platform::xg4_dir().display().to_string()).monospace());
                    #[cfg(not(target_arch = "wasm32"))]
                    {
                        if ui.button("Choose…").clicked() {
                            if let Some(d) = crate::platform::pick_xg4_dir() {
                                self.status = format!("XGworks data: {} — restart the app to reload all tables.", d.display());
                            }
                        }
                        if ui.button("Default").on_hover_text("Use the XG4 folder next to the app").clicked() {
                            crate::platform::reset_xg4_dir();
                            self.status = "XGworks data: the XG4 folder next to the app — restart the app.".into();
                        }
                    }
                });
                ui.end_row();
                #[cfg(not(target_arch = "wasm32"))]
                {
                    ui.label("Data found");
                    ui.label(if crate::platform::xg4_found() { "yes" } else { "no — copy your XGworks files into XG4" });
                    ui.end_row();
                    ui.label("App folder");
                    ui.label(egui::RichText::new(crate::platform::app_dir().map_or("?".into(), |d| d.display().to_string())).monospace());
                    ui.end_row();
                    ui.label("Settings");
                    ui.label(egui::RichText::new(crate::platform::settings_dir().map_or("?".into(), |d| d.display().to_string())).monospace());
                    ui.end_row();
                }
            });
            ui.label(egui::RichText::new("Keys: Space play/stop · numpad 0 start · 1 L · 3 R · 2 back · − loop").small());
        });
        self.show_prefs = open;
    }
}

#[cfg(test)]
mod tests {
    use super::from_position;

    #[test]
    fn starts_mid_song_with_chased_state() {
        let m = |t: f64, b: &[u8]| (t, 0usize, b.to_vec());
        let msgs = vec![
            m(0.0, &[0xC0, 5]),
            m(0.0, &[0xB0, 7, 50]),
            m(100.0, &[0x90, 60, 100]),
            m(200.0, &[0xB0, 7, 90]),
            m(300.0, &[0x90, 60, 0]),
            m(500.0, &[0x90, 62, 100]),
        ];
        let out = from_position(msgs, 400.0);
        let bytes: Vec<Vec<u8>> = out.iter().map(|x| x.2.clone()).collect();
        assert_eq!(bytes, vec![vec![0xC0, 5], vec![0xB0, 7, 90], vec![0x90, 62, 100]]);
        assert_eq!(out[2].0, 100.0);
        assert_eq!(out[0].0, 0.0);
    }
}
