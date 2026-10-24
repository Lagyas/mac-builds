//! The Transport window: XGworks' "Control Panel" (play control, Alt+F7
//! shows / hides it) laid out like a Cubase transport bar — its own window,
//! always on top: a big location display (bar : beat : clock, time, tempo,
//! signature), big buttons with bold labels (Top, −1 bar, +1 bar, Stop,
//! Play, Rec, Loop, Click) and the L / R locators.

use super::App;
use eframe::egui::{self, pos2, vec2, Color32, FontId, Rect, Sense, Stroke};

#[derive(Clone, Copy, PartialEq)]
enum Sym {
    Top,
    Back,
    Fwd,
    Stop,
    Play,
    Rec,
    Loop,
    Click,
}

/// A big transport button: symbol on top, bold label below.
fn big_button(ui: &mut egui::Ui, sym: Sym, label: &str, on: bool, enabled: bool, tip: &str) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(64.0, 54.0), if enabled { Sense::click() } else { Sense::hover() });
    let resp = resp.on_hover_text(tip);
    let p = ui.painter();
    let pressed = on || (enabled && resp.is_pointer_button_down_on());
    let face = match (on, sym) {
        (true, Sym::Rec) => Color32::from_rgb(200, 30, 30),
        (true, Sym::Play) => Color32::from_rgb(40, 150, 60),
        (true, _) => Color32::from_rgb(70, 110, 190),
        _ if resp.hovered() && enabled => Color32::from_gray(232),
        _ => Color32::from_gray(218),
    };
    p.rect_filled(r, 5.0, face);
    p.rect_stroke(r, 5.0, Stroke::new(1.0_f32, if pressed { Color32::from_gray(60) } else { Color32::from_gray(140) }));
    let fg = if !enabled {
        Color32::from_gray(160)
    } else if on {
        Color32::WHITE
    } else {
        match sym {
            Sym::Rec => Color32::from_rgb(200, 30, 30),
            Sym::Play => Color32::from_rgb(30, 130, 50),
            _ => Color32::from_gray(25),
        }
    };
    let c = pos2(r.center().x, r.top() + 20.0);
    let tri = |dir: f32, x: f32| vec![pos2(x - 7.0 * dir, c.y - 8.0), pos2(x + 7.0 * dir, c.y), pos2(x - 7.0 * dir, c.y + 8.0)];
    match sym {
        Sym::Play => {
            p.add(egui::Shape::convex_polygon(vec![pos2(c.x - 8.0, c.y - 10.0), pos2(c.x + 10.0, c.y), pos2(c.x - 8.0, c.y + 10.0)], fg, Stroke::NONE));
        }
        Sym::Stop => {
            p.rect_filled(Rect::from_center_size(c, vec2(17.0, 17.0)), 1.0, fg);
        }
        Sym::Rec => {
            p.circle_filled(c, 9.5, fg);
        }
        Sym::Top => {
            p.rect_filled(Rect::from_center_size(pos2(c.x - 9.0, c.y), vec2(3.0, 16.0)), 0.0, fg);
            p.add(egui::Shape::convex_polygon(tri(-1.0, c.x + 2.0), fg, Stroke::NONE));
        }
        Sym::Back => {
            p.add(egui::Shape::convex_polygon(tri(-1.0, c.x - 5.0), fg, Stroke::NONE));
            p.add(egui::Shape::convex_polygon(tri(-1.0, c.x + 7.0), fg, Stroke::NONE));
        }
        Sym::Fwd => {
            p.add(egui::Shape::convex_polygon(tri(1.0, c.x - 6.0), fg, Stroke::NONE));
            p.add(egui::Shape::convex_polygon(tri(1.0, c.x + 6.0), fg, Stroke::NONE));
        }
        Sym::Loop => {
            let s = Stroke::new(2.5_f32, fg);
            let pts: Vec<egui::Pos2> = (0..=24).map(|k| {
                let a = k as f32 / 24.0 * std::f32::consts::TAU * 0.85 + 0.6;
                pos2(c.x + 11.0 * a.cos(), c.y + 7.0 * a.sin())
            }).collect();
            let end = *pts.last().unwrap();
            p.add(egui::Shape::line(pts, s));
            p.add(egui::Shape::convex_polygon(vec![end + vec2(-1.0, -6.0), end + vec2(6.0, 1.0), end + vec2(-4.0, 3.0)], fg, Stroke::NONE));
        }
        Sym::Click => {
            p.add(egui::Shape::convex_polygon(vec![pos2(c.x - 9.0, c.y + 9.0), pos2(c.x - 3.0, c.y - 10.0), pos2(c.x + 3.0, c.y - 10.0), pos2(c.x + 9.0, c.y + 9.0)], fg, Stroke::NONE));
            p.line_segment([pos2(c.x, c.y + 4.0), pos2(c.x + 7.0, c.y - 9.0)], Stroke::new(2.0_f32, if on { Color32::from_rgb(70, 110, 190) } else { Color32::from_gray(218) }));
        }
    }
    let font = FontId::proportional(12.5);
    let tp = pos2(r.center().x, r.bottom() - 9.0);
    // bold: drawn twice, a fraction of a pixel apart
    p.text(tp, egui::Align2::CENTER_CENTER, label, font.clone(), fg);
    p.text(tp + vec2(0.6, 0.0), egui::Align2::CENTER_CENTER, label, font, fg);
    resp
}

impl App {
    /// Show / hide (Window ▸ Transport, Alt+F7 like the original).
    pub(super) fn transport_window(&mut self, ctx: &egui::Context) {
        if ctx.input(|i| i.modifiers.alt && i.key_pressed(egui::Key::F7)) {
            self.show_transport = !self.show_transport;
        }
        if !self.show_transport {
            return;
        }
        let mut open = true;
        if ctx.embed_viewports() {
            open = super::embedded_window(ctx, "transport", "Transport", [830.0, 140.0], false, |ui| self.transport_ui(ui));
        } else {
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("transport"),
                egui::ViewportBuilder::default()
                    .with_title("Transport")
                    .with_inner_size([830.0, 128.0])
                    .with_resizable(false)
                    .with_window_level(egui::WindowLevel::AlwaysOnTop),
                |ctx, _| {
                    egui::CentralPanel::default().frame(egui::Frame::none().fill(Color32::from_gray(200)).inner_margin(8.0)).show(ctx, |ui| self.transport_ui(ui));
                    super::transport::note_space(ctx);
                    if ctx.input(|i| i.viewport().close_requested()) {
                        open = false;
                    }
                },
            );
        }
        self.show_transport = open;
    }

    fn transport_ui(&mut self, ui: &mut egui::Ui) {
        let has = !self.edit.is_empty();
        let tm = self.time_map();
        let cur = self.cur_tick();
        ui.horizontal(|ui| {
            // ---- location display
            let (r, _) = ui.allocate_exact_size(vec2(250.0, 98.0), Sense::hover());
            let p = ui.painter();
            p.rect_filled(r, 6.0, Color32::from_rgb(12, 16, 14));
            let (m, b, c) = tm.to_mbc(cur);
            let green = Color32::from_rgb(90, 255, 120);
            let big = FontId::monospace(32.0);
            let loc = format!("{m:04}:{b:02}:{c:03}");
            p.text(pos2(r.left() + 12.0, r.top() + 8.0), egui::Align2::LEFT_TOP, &loc, big.clone(), green);
            p.text(pos2(r.left() + 12.6, r.top() + 8.0), egui::Align2::LEFT_TOP, &loc, big, green);
            let ms = tm.to_ms(cur).max(0.0) as u64;
            let time = format!("{:02}:{:02}.{:03}", ms / 60_000, ms / 1000 % 60, ms % 1000);
            p.text(pos2(r.left() + 12.0, r.top() + 50.0), egui::Align2::LEFT_TOP, time, FontId::monospace(18.0), Color32::from_rgb(170, 220, 180));
            let us = self.ms_per_tick * self.ppqn as f64 * 1000.0;
            let bpm = if us > 0.0 { 60_000_000.0 / us } else { 120.0 };
            let beat = tm.beat_len(cur).max(1);
            let num = tm.bar_len(cur) / beat;
            let den = self.ppqn as u32 * 4 / beat;
            p.text(pos2(r.left() + 12.0, r.bottom() - 8.0), egui::Align2::LEFT_BOTTOM, format!("{bpm:.1} BPM   {num}/{den}"), FontId::proportional(15.0), Color32::from_rgb(255, 210, 90));
            let state = if self.recording { "● REC" } else if self.playing { "PLAY" } else { "STOP" };
            p.text(pos2(r.right() - 10.0, r.bottom() - 8.0), egui::Align2::RIGHT_BOTTOM, state, FontId::proportional(14.0), if self.recording { Color32::from_rgb(255, 80, 80) } else { green });

            ui.add_space(8.0);
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    if big_button(ui, Sym::Top, "Top", false, has, "To the song start (numpad 0)").clicked() {
                        self.locate(0);
                    }
                    if big_button(ui, Sym::Back, "−1 bar", false, has, "One bar back (numpad 2 = several)").clicked() {
                        let (m, _, _) = tm.to_mbc(cur);
                        self.locate(tm.from_mbc(m.saturating_sub(1).max(1), 1, 0));
                    }
                    if big_button(ui, Sym::Fwd, "+1 bar", false, has, "One bar forward").clicked() {
                        let (m, _, _) = tm.to_mbc(cur);
                        self.locate(tm.from_mbc(m + 1, 1, 0));
                    }
                    if big_button(ui, Sym::Stop, "Stop", !self.playing, has, "Stop (Space)").clicked() {
                        self.stop();
                        self.status = "Stopped.".into();
                    }
                    if big_button(ui, Sym::Play, "Play", self.playing && !self.recording, has, "Play (Space)").clicked() && !self.playing {
                        self.play();
                    }
                    if big_button(ui, Sym::Rec, "Rec", self.rec_armed || self.recording, has && !self.recording, "Record the MIDI input into the selected track on Play (numpad +)").clicked() {
                        self.rec_armed = !self.rec_armed;
                        if self.rec_armed && self.in_dev.is_none() {
                            self.status = "Choose a MIDI input in Setup ▸ MIDI input.".into();
                        }
                    }
                    if big_button(ui, Sym::Loop, "Loop", self.loop_on, true, "Loop between L and R (numpad −)").clicked() {
                        self.loop_on = !self.loop_on;
                    }
                    if big_button(ui, Sym::Click, "Click", self.metro.on_play, true, "Metronome while playing (Setup ▸ Metronome)").clicked() {
                        self.metro.on_play = !self.metro.on_play;
                    }
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    for (name, which) in [("L", 0), ("R", 1)] {
                        let tick = if which == 0 { self.loc_l } else { self.loc_r };
                        let (m, b, c) = tm.to_mbc(tick);
                        ui.label(egui::RichText::new(name).strong().size(15.0));
                        let mut bar = m;
                        if ui.add(egui::DragValue::new(&mut bar).range(1..=9999)).on_hover_text("Bar").changed() {
                            let t = tm.from_mbc(bar, 1, 0);
                            if which == 0 { self.loc_l = t } else { self.loc_r = t }
                        }
                        ui.label(egui::RichText::new(format!(":{b:02}:{c:03}")).monospace());
                        if ui.button(egui::RichText::new("Set").strong()).on_hover_text("Set to the song position").clicked() {
                            if which == 0 { self.loc_l = cur } else { self.loc_r = cur }
                        }
                        if ui.button(egui::RichText::new("Go").strong()).on_hover_text(if which == 0 { "Numpad 1" } else { "Numpad 3" }).clicked() {
                            self.locate(tick);
                        }
                        ui.add_space(14.0);
                    }
                });
            });
        });
    }
}
