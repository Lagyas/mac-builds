//! Front-panel geometry and custom controls. Everything is laid out in the
//! 808 × 576 panel coordinates of the original instrument and scaled to the
//! window. Value fields follow the manual (p. 26): click = −1, right-click = +1,
//! Shift for bigger steps, drag up/down, Ctrl-click = default.

use eframe::egui::{self, Align2, Color32, FontId, Id, Pos2, Rect, Response, Rounding, Sense, Shape, Stroke, Ui, Vec2};

pub const W: f32 = 808.0;
pub const H: f32 = 576.0;

pub mod col {
    use eframe::egui::Color32;
    pub const CHASSIS: Color32 = Color32::from_rgb(196, 200, 205);
    pub const CHASSIS_LO: Color32 = Color32::from_rgb(150, 154, 160);
    pub const DARK: Color32 = Color32::from_rgb(28, 30, 34);
    pub const LCD: Color32 = Color32::from_rgb(232, 240, 248);
    pub const LCD_LINE: Color32 = Color32::from_rgb(120, 150, 190);
    pub const LCD_TEXT: Color32 = Color32::from_rgb(18, 40, 86);
    pub const LCD_DIM: Color32 = Color32::from_rgb(110, 130, 160);
    pub const BLUE: Color32 = Color32::from_rgb(30, 76, 160);
    pub const BLUE_LT: Color32 = Color32::from_rgb(150, 182, 225);
    pub const SEL: Color32 = Color32::from_rgb(40, 90, 190);
    pub const CYAN: Color32 = Color32::from_rgb(70, 214, 230);
    pub const ORANGE: Color32 = Color32::from_rgb(255, 150, 40);
    pub const LED_OFF: Color32 = Color32::from_rgb(70, 80, 92);
    pub const RED: Color32 = Color32::from_rgb(210, 40, 40);
}

#[derive(Clone, Copy)]
pub struct Panel {
    pub origin: Pos2,
    pub s: f32,
}

impl Panel {
    pub fn fit(avail: Rect) -> Panel {
        let s = (avail.width() / W).min(avail.height() / H);
        let origin = avail.center() - Vec2::new(W * s, H * s) / 2.0;
        Panel { origin, s }
    }
    pub fn p(&self, x: f32, y: f32) -> Pos2 {
        self.origin + Vec2::new(x, y) * self.s
    }
    pub fn r(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect::from_min_max(self.p(x0, y0), self.p(x1, y1))
    }
    pub fn f(&self, size: f32) -> FontId {
        FontId::proportional(size * self.s)
    }
    /// Panel coordinates of a screen position.
    pub fn inv(&self, p: Pos2) -> Pos2 {
        Pos2::new((p.x - self.origin.x) / self.s, (p.y - self.origin.y) / self.s)
    }
}

/// LCD frame (light blue display surface).
pub fn lcd(ui: &Ui, r: Rect, s: f32) {
    let p = ui.painter();
    p.rect_filled(r, Rounding::same(2.0 * s), col::LCD);
    p.rect_stroke(r, Rounding::same(2.0 * s), Stroke::new(s, col::CHASSIS_LO));
}

pub fn text(ui: &Ui, pos: Pos2, align: Align2, t: &str, font: FontId, c: Color32) {
    ui.painter().text(pos, align, t, font, c);
}

/// Silver push button with an LED (page buttons, mute).
pub fn led_button(ui: &mut Ui, pn: &Panel, id: Id, r: Rect, on: bool, led: Color32) -> Response {
    let resp = ui.interact(r, id, Sense::click());
    let s = pn.s;
    let p = ui.painter();
    p.rect_filled(r, Rounding::same(3.0 * s), col::DARK);
    let inner = r.shrink(2.0 * s);
    p.rect_filled(inner, Rounding::same(2.0 * s), if resp.hovered() { Color32::from_rgb(150, 156, 164) } else { Color32::from_rgb(120, 126, 134) });
    let ledr = Rect::from_center_size(inner.center(), Vec2::new(inner.width() * 0.62, inner.height() * 0.42));
    p.rect_filled(ledr, Rounding::same(2.0 * s), if on { led } else { col::LED_OFF });
    resp
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Act {
    None,
    /// discrete step (±1, ±big with Shift)
    Step(i32),
    /// continuous drag, in value units per pixel (positive = up)
    Drag(f32),
    Default,
}

/// An LCD value field: shows `t`, returns the user action.
pub fn value(ui: &mut Ui, pn: &Panel, id: Id, r: Rect, t: &str, sel: bool) -> (Act, Response) {
    let resp = ui.interact(r, id, Sense::click_and_drag());
    let p = ui.painter();
    let bg = if sel { col::SEL } else if resp.hovered() { Color32::from_rgb(214, 226, 242) } else { Color32::from_rgb(240, 245, 252) };
    p.rect_filled(r, Rounding::same(1.5 * pn.s), bg);
    p.rect_stroke(r, Rounding::same(1.5 * pn.s), Stroke::new(pn.s * 0.8, col::LCD_LINE));
    let fc = if sel { Color32::WHITE } else { col::LCD_TEXT };
    p.with_clip_rect(r).text(r.left_center() + Vec2::new(3.0 * pn.s, 0.0), Align2::LEFT_CENTER, t, pn.f(9.0), fc);
    let m = ui.input(|i| i.modifiers);
    let big = if m.shift { 10 } else { 1 };
    let act = if resp.drag_delta().y != 0.0 && resp.dragged_by(egui::PointerButton::Primary) {
        let d = -resp.drag_delta().y / pn.s;
        Act::Drag(if m.shift { d * 0.25 } else { d })
    } else if resp.clicked() && m.command {
        Act::Default
    } else if resp.clicked() {
        Act::Step(-big)
    } else if resp.secondary_clicked() {
        Act::Step(big)
    } else {
        Act::None
    };
    (act, resp)
}

/// Apply an `Act` to an integer value in `lo..=hi` (drag: `px_per_step` pixels).
pub fn apply_int(act: Act, v: i32, lo: i32, hi: i32, def: i32, acc: &mut f32, px_per_step: f32) -> Option<i32> {
    let n = match act {
        Act::Step(d) => v + d,
        Act::Default => def,
        Act::Drag(d) => {
            *acc += d / px_per_step;
            let k = acc.trunc() as i32;
            *acc -= k as f32;
            if k == 0 {
                return None;
            }
            v + k
        }
        _ => return None,
    };
    let n = n.clamp(lo, hi);
    (n != v).then_some(n)
}

/// Rotary LCD knob (0..1). Returns the new value while dragged.
pub fn knob(ui: &mut Ui, pn: &Panel, id: Id, c: Pos2, radius: f32, v: f32, bipolar: bool, label: &str, default: f32) -> (Option<f32>, Response) {
    let s = pn.s;
    let rr = radius * s;
    let r = Rect::from_center_size(c, Vec2::splat(rr * 2.2));
    let resp = ui.interact(r, id, Sense::click_and_drag());
    let p = ui.painter();
    let a0 = std::f32::consts::PI * 0.75;
    let span = std::f32::consts::PI * 1.5;
    // arc
    let pts: Vec<Pos2> = (0..=24).map(|k| {
        let a = a0 + span * k as f32 / 24.0;
        c + Vec2::new(a.cos(), a.sin()) * rr
    }).collect();
    p.add(Shape::line(pts, Stroke::new(1.6 * s, col::LCD_LINE)));
    let from = if bipolar { 0.5 } else { 0.0 };
    let (lo, hi) = if v < from { (v, from) } else { (from, v) };
    let n = ((hi - lo) * 24.0).ceil().max(1.0) as usize;
    let vpts: Vec<Pos2> = (0..=n).map(|k| {
        let a = a0 + span * (lo + (hi - lo) * k as f32 / n as f32);
        c + Vec2::new(a.cos(), a.sin()) * rr
    }).collect();
    p.add(Shape::line(vpts, Stroke::new(2.4 * s, col::BLUE)));
    let a = a0 + span * v;
    p.line_segment([c + Vec2::new(a.cos(), a.sin()) * rr * 0.25, c + Vec2::new(a.cos(), a.sin()) * rr], Stroke::new(1.8 * s, col::LCD_TEXT));
    if !label.is_empty() {
        p.text(c + Vec2::new(0.0, rr + 7.0 * s), Align2::CENTER_CENTER, label, pn.f(8.5), col::LCD_TEXT);
    }
    let m = ui.input(|i| i.modifiers);
    let mut out = None;
    if resp.clicked() && m.command {
        out = Some(default);
    } else if resp.dragged() {
        let d = (resp.drag_delta().x - resp.drag_delta().y) / s / if m.shift { 600.0 } else { 150.0 };
        out = Some((v + d).clamp(0.0, 1.0));
    }
    (out, resp)
}

/// Horizontal blue slider (0..1), bipolar from the centre.
pub fn hslider(ui: &mut Ui, pn: &Panel, id: Id, r: Rect, v: f32, bipolar: bool, default: f32) -> (Option<f32>, Response) {
    let resp = ui.interact(r, id, Sense::click_and_drag());
    let p = ui.painter();
    let s = pn.s;
    p.rect_filled(r, Rounding::same(1.5 * s), col::BLUE_LT);
    let x = |t: f32| r.left() + 2.0 * s + (r.width() - 4.0 * s) * t;
    let (a, b) = if bipolar { (x(0.5f32.min(v)), x(0.5f32.max(v))) } else { (x(0.0), x(v)) };
    p.rect_filled(Rect::from_x_y_ranges(a..=b.max(a + s), r.top() + 2.0 * s..=r.bottom() - 2.0 * s), Rounding::ZERO, col::BLUE);
    p.rect_filled(Rect::from_center_size(Pos2::new(x(v), r.center().y), Vec2::new(2.5 * s, r.height())), Rounding::ZERO, col::DARK);
    let m = ui.input(|i| i.modifiers);
    let mut out = None;
    if resp.clicked() && m.command {
        out = Some(default);
    } else if resp.dragged() {
        let d = resp.drag_delta().x / (r.width()) * if m.shift { 0.25 } else { 1.0 };
        out = Some((v + d).clamp(0.0, 1.0));
    } else if resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            out = Some(((pos.x - r.left()) / r.width()).clamp(0.0, 1.0));
        }
    }
    (out, resp)
}

/// Part level fader of the Level strip: cyan bar (or meter) with ticks.
pub fn level_strip(ui: &mut Ui, pn: &Panel, id: Id, r: Rect, v: f32, meter: Option<f32>, default: f32) -> (Option<f32>, Response) {
    let resp = ui.interact(r, id, Sense::click_and_drag());
    let s = pn.s;
    let p = ui.painter();
    p.rect_filled(r, Rounding::same(2.0 * s), Color32::from_rgb(60, 64, 70));
    let inner = r.shrink2(Vec2::new(2.0 * s, 3.0 * s));
    let ticks = 40;
    let shown = meter.unwrap_or(v);
    for k in 0..ticks {
        let t = k as f32 / ticks as f32;
        let x = inner.left() + inner.width() * t;
        let lit = t < shown;
        let c = if lit { col::CYAN } else { Color32::from_rgb(150, 156, 164) };
        p.line_segment([Pos2::new(x, inner.top()), Pos2::new(x, inner.bottom())], Stroke::new(1.3 * s, c));
    }
    if meter.is_some() {
        let x = inner.left() + inner.width() * v;
        p.line_segment([Pos2::new(x, r.top()), Pos2::new(x, r.bottom())], Stroke::new(2.0 * s, Color32::WHITE));
    }
    let m = ui.input(|i| i.modifiers);
    let mut out = None;
    if resp.clicked() && m.command {
        out = Some(default);
    } else if resp.dragged() || resp.clicked() {
        if let Some(pos) = resp.interact_pointer_pos() {
            if m.shift && resp.dragged() {
                out = Some((v + resp.drag_delta().x / r.width() * 0.25).clamp(0.0, 1.0));
            } else {
                out = Some(((pos.x - inner.left()) / inner.width()).clamp(0.0, 1.0));
            }
        }
    }
    (out, resp)
}

/// A Hyperknob: a ribbed roller turned by dragging (left/right or up/down).
pub fn roller(ui: &mut Ui, pn: &Panel, id: Id, r: Rect, v: f32, default: f32) -> (Option<f32>, Response) {
    let resp = ui.interact(r, id, Sense::click_and_drag());
    let s = pn.s;
    let p = ui.painter();
    p.rect_filled(r, Rounding::same(3.0 * s), Color32::from_rgb(40, 42, 46));
    let inner = r.shrink(3.0 * s);
    // ribs move with the value
    let n = 14;
    for k in 0..n {
        let t = ((k as f32 + v * 9.0) / n as f32).fract();
        let x = inner.left() + inner.width() * t;
        let shade = (1.0 - (t - 0.5).abs() * 2.0).powf(0.6);
        let c = Color32::from_gray((60.0 + 150.0 * shade) as u8);
        p.line_segment([Pos2::new(x, inner.top()), Pos2::new(x, inner.bottom())], Stroke::new(2.0 * s, c));
    }
    // value bar below
    let bar = Rect::from_min_max(Pos2::new(r.left() + 3.0 * s, r.bottom() - 4.0 * s), Pos2::new(r.left() + 3.0 * s + (r.width() - 6.0 * s) * v, r.bottom() - 2.0 * s));
    p.rect_filled(bar, Rounding::ZERO, col::CYAN);
    let m = ui.input(|i| i.modifiers);
    let mut out = None;
    if resp.clicked() && m.command {
        out = Some(default);
    } else if resp.dragged() {
        let d = (resp.drag_delta().x - resp.drag_delta().y) / s / if m.shift { 800.0 } else { 200.0 };
        out = Some((v + d).clamp(0.0, 1.0));
    }
    (out, resp)
}

/// Pop-up selection list under `resp`; returns the chosen index.
pub fn menu(ui: &mut Ui, resp: &Response, id: Id, items: &[String], current: Option<usize>) -> Option<usize> {
    if resp.clicked() || resp.secondary_clicked() {
        ui.memory_mut(|m| m.toggle_popup(id));
    }
    let mut chosen = None;
    egui::popup::popup_below_widget(ui, id, resp, egui::PopupCloseBehavior::CloseOnClick, |ui| {
        ui.set_min_width(160.0);
        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
            for (i, it) in items.iter().enumerate() {
                if it.starts_with("--") {
                    ui.separator();
                    continue;
                }
                if ui.selectable_label(current == Some(i), it).clicked() {
                    chosen = Some(i);
                }
            }
        });
    });
    chosen
}
