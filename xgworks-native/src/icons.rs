//! Toolbar icon strips of the original XGworks.exe (16×15 icons side by side),
//! loaded at run time and drawn as Win95-style toolbar buttons. Without the EXE
//! the buttons fall back to text.

use eframe::egui::{self, pos2, vec2, Color32, Rect, Sense, Stroke};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Piano Roll bottom bar: arrow, pencil, eraser, line, free, curve ↗, curve ↘, sine.
pub const ROLL_TOOLS: u32 = 7200;
/// Piano Roll top bar: V zoom in, V zoom out, FREE, 1/4, 1/8, 1/16, dot, triplet, colour.
pub const ROLL_TOP: u32 = 6008;
/// List window bar: font "A", mono monitor, poly monitor, wave monitor, MCI monitor.
pub const LIST_BAR: u32 = 1026;
/// Main toolbar (16×16): new, open, save, undo, redo, arrow, pencil, eraser,
/// scissors, bond, then the window buttons.
pub const MAIN_BAR: u32 = 2003;
const STRIPS: [u32; 4] = [ROLL_TOOLS, ROLL_TOP, LIST_BAR, MAIN_BAR];

fn strip(ctx: &egui::Context, id: u32) -> Option<egui::TextureHandle> {
    static CACHE: OnceLock<Mutex<Option<HashMap<u32, egui::TextureHandle>>>> = OnceLock::new();
    let mut g = CACHE.get_or_init(|| Mutex::new(None)).lock().unwrap();
    let map = g.get_or_insert_with(|| {
        let mut m = HashMap::new();
        if let Some(exe) = crate::platform::read(&crate::platform::xg4_exe()) {
            for (k, mut b) in crate::peres::load_bitmaps(&exe, |i| STRIPS.contains(&i)) {
                crate::peres::face_to_alpha(&mut b);
                let img = egui::ColorImage::from_rgba_unmultiplied([b.w, b.h], &b.rgba);
                m.insert(k, ctx.load_texture(format!("strip{k}"), img, egui::TextureOptions::NEAREST));
            }
        }
        m
    });
    map.get(&id).cloned()
}

/// A 23×22 toolbar button showing icon `idx` of strip `id`; drawn pressed when
/// `down`. Falls back to a text button when the strip is not available.
pub fn tool_button(ui: &mut egui::Ui, id: u32, idx: usize, down: bool, fallback: &str, tip: &str) -> egui::Response {
    let has_tex = strip(ui.ctx(), id).is_some();
    if !has_tex && vector_kind(id, idx).is_none() {
        return ui.add(egui::SelectableLabel::new(down, fallback)).on_hover_text(tip);
    }
    let (r, resp) = ui.allocate_exact_size(vec2(23.0, 22.0), Sense::click());
    let resp = resp.on_hover_text(tip);
    let p = ui.painter();
    let pressed = down || resp.is_pointer_button_down_on();
    bevel(p, r, down, pressed);
    let off = if pressed { 1.0 } else { 0.0 };
    let dst = Rect::from_min_size(r.min + vec2(3.0 + off, 3.0 + off), vec2(16.0, 16.0));
    draw_icon(p, ui.ctx(), dst, id, idx);
    resp
}

/// Win95 button face with raised / sunken edges.
fn bevel(p: &egui::Painter, r: Rect, down: bool, pressed: bool) {
    let face = if down { Color32::from_gray(225) } else { Color32::from_gray(240) };
    p.rect_filled(r, 0.0, face);
    let (tl, br) = if pressed { (Color32::from_gray(100), Color32::WHITE) } else { (Color32::WHITE, Color32::from_gray(100)) };
    p.hline(r.x_range(), r.top() + 0.5, Stroke::new(1.0_f32, tl));
    p.vline(r.left() + 0.5, r.y_range(), Stroke::new(1.0_f32, tl));
    p.hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0_f32, br));
    p.vline(r.right() - 0.5, r.y_range(), Stroke::new(1.0_f32, br));
}

/// Icon `idx` of strip `id` into the 16×16 `dst`: the original bitmap when
/// XGworks.exe is there, else a drawn one.
pub fn draw_icon(p: &egui::Painter, ctx: &egui::Context, dst: Rect, id: u32, idx: usize) {
    if let Some(tex) = strip(ctx, id) {
        let [w, h] = tex.size();
        let (w, h) = (w as f32, h as f32);
        let uv = Rect::from_min_max(pos2(16.0 * idx as f32 / w, 0.0), pos2(16.0 * (idx + 1) as f32 / w, 1.0));
        p.image(tex.id(), Rect::from_min_size(dst.min, vec2(16.0, h.min(16.0))), uv, Color32::WHITE);
    } else if let Some(k) = vector_kind(id, idx) {
        vector_icon(p, dst, k);
    }
}

/// Drawn icons (used when XGworks.exe is not available).
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Vk {
    Arrow,
    Pencil,
    Eraser,
    Scissors,
    Glue,
    List,
    PianoRoll,
    Staff,
    Drum,
    Mixer,
    Xg,
    Master,
    Lyrics,
    Markers,
    Line,
    Free,
    New,
    Open,
    Save,
    Undo,
    Redo,
    Transport,
}

fn vector_kind(id: u32, idx: usize) -> Option<Vk> {
    match (id, idx) {
        (MAIN_BAR, 0) => Some(Vk::New),
        (MAIN_BAR, 1) => Some(Vk::Open),
        (MAIN_BAR, 2) => Some(Vk::Save),
        (MAIN_BAR, 3) => Some(Vk::Undo),
        (MAIN_BAR, 4) => Some(Vk::Redo),
        (MAIN_BAR, 5) | (ROLL_TOOLS, 0) => Some(Vk::Arrow),
        (MAIN_BAR, 6) | (ROLL_TOOLS, 1) => Some(Vk::Pencil),
        (MAIN_BAR, 7) | (ROLL_TOOLS, 2) => Some(Vk::Eraser),
        (MAIN_BAR, 8) => Some(Vk::Scissors),
        (MAIN_BAR, 9) => Some(Vk::Glue),
        (MAIN_BAR, 11) => Some(Vk::List),
        (MAIN_BAR, 12) => Some(Vk::PianoRoll),
        (MAIN_BAR, 13) => Some(Vk::Staff),
        (MAIN_BAR, 14) => Some(Vk::Drum),
        (MAIN_BAR, 15) => Some(Vk::Mixer),
        (MAIN_BAR, 16) => Some(Vk::Xg),
        (MAIN_BAR, 17) => Some(Vk::Master),
        (ROLL_TOOLS, 3) => Some(Vk::Line),
        (ROLL_TOOLS, 4) => Some(Vk::Free),
        _ => None,
    }
}

pub fn vector_icon(p: &egui::Painter, r: Rect, k: Vk) {
    let o = r.min;
    let at = |x: f32, y: f32| pos2(o.x + x, o.y + y);
    let blk = Color32::BLACK;
    let s1 = Stroke::new(1.2_f32, blk);
    match k {
        Vk::Arrow => {
            p.add(egui::Shape::convex_polygon(vec![at(4.0, 1.0), at(12.5, 9.5), at(8.0, 9.5), at(4.0, 13.5)], blk, Stroke::NONE));
            p.line_segment([at(8.0, 9.0), at(11.0, 15.0)], Stroke::new(2.2_f32, blk));
        }
        Vk::Pencil => {
            p.add(egui::Shape::convex_polygon(vec![at(11.0, 1.0), at(15.0, 5.0), at(5.0, 15.0), at(1.0, 11.0)], Color32::from_rgb(40, 80, 220), s1));
            p.add(egui::Shape::convex_polygon(vec![at(1.0, 11.0), at(5.0, 15.0), at(0.5, 15.5)], Color32::from_rgb(230, 200, 140), s1));
        }
        Vk::Eraser => {
            p.add(egui::Shape::convex_polygon(vec![at(9.0, 2.0), at(15.0, 8.0), at(9.0, 14.0), at(3.0, 8.0)], Color32::from_rgb(250, 240, 120), s1));
            p.add(egui::Shape::convex_polygon(vec![at(3.0, 8.0), at(6.0, 11.0), at(3.0, 14.0), at(0.5, 11.5)], Color32::from_rgb(240, 150, 170), s1));
        }
        Vk::Scissors => {
            p.circle_stroke(at(4.0, 12.0), 2.6, Stroke::new(1.5_f32, Color32::from_rgb(200, 0, 0)));
            p.circle_stroke(at(11.0, 12.0), 2.6, Stroke::new(1.5_f32, Color32::from_rgb(200, 0, 0)));
            p.line_segment([at(5.5, 10.0), at(12.0, 1.0)], s1);
            p.line_segment([at(9.5, 10.0), at(3.0, 1.0)], s1);
        }
        Vk::Glue => {
            p.rect_filled(Rect::from_min_max(at(3.0, 5.0), at(11.0, 15.0)), 1.0, Color32::from_gray(200));
            p.rect_stroke(Rect::from_min_max(at(3.0, 5.0), at(11.0, 15.0)), 1.0, s1);
            p.rect_filled(Rect::from_min_max(at(5.0, 1.0), at(9.0, 5.0)), 0.0, blk);
            p.line_segment([at(7.0, 1.0), at(14.0, 1.0)], s1);
        }
        Vk::List => {
            for k in 0..4 {
                let y = 3.0 + k as f32 * 3.5;
                p.rect_filled(Rect::from_min_max(at(1.0, y - 1.0), at(4.0, y + 1.0)), 0.0, Color32::from_rgb(0, 0, 180));
                p.line_segment([at(6.0, y), at(15.0, y)], s1);
            }
        }
        Vk::PianoRoll => {
            p.rect_filled(Rect::from_min_max(at(0.0, 0.0), at(4.0, 16.0)), 0.0, Color32::WHITE);
            p.rect_stroke(Rect::from_min_max(at(0.0, 0.0), at(4.0, 16.0)), 0.0, s1);
            for (x, y, w) in [(5.0, 3.0, 6.0), (8.0, 7.0, 7.0), (5.0, 11.0, 4.0)] {
                p.rect_filled(Rect::from_min_size(at(x, y), vec2(w, 2.5)), 0.0, Color32::from_rgb(0, 0, 220));
            }
        }
        Vk::Staff => {
            for k in 0..5 {
                p.line_segment([at(0.0, 3.0 + k as f32 * 2.5), at(16.0, 3.0 + k as f32 * 2.5)], Stroke::new(0.8_f32, blk));
            }
            p.circle_filled(at(6.0, 11.5), 2.0, blk);
            p.line_segment([at(7.8, 11.0), at(7.8, 1.0)], s1);
            p.circle_filled(at(12.0, 9.0), 2.0, blk);
            p.line_segment([at(13.8, 8.5), at(13.8, 1.0)], s1);
            p.line_segment([at(7.8, 1.0), at(13.8, 1.0)], Stroke::new(2.0_f32, blk));
        }
        Vk::Drum => {
            p.rect_filled(Rect::from_min_max(at(2.0, 7.0), at(14.0, 14.0)), 0.0, Color32::from_rgb(200, 30, 30));
            p.add(egui::Shape::ellipse_filled(at(8.0, 7.0), vec2(6.0, 2.0), Color32::WHITE));
            p.add(egui::Shape::ellipse_stroke(at(8.0, 7.0), vec2(6.0, 2.0), s1));
            p.line_segment([at(3.0, 1.0), at(8.0, 6.0)], s1);
            p.line_segment([at(13.0, 1.0), at(9.0, 6.0)], s1);
        }
        Vk::Mixer => {
            for (k, y) in [(0, 10.0), (1, 5.0), (2, 8.0)] {
                let x = 3.0 + k as f32 * 5.0;
                p.line_segment([at(x, 1.0), at(x, 15.0)], Stroke::new(1.0_f32, Color32::from_gray(60)));
                p.rect_filled(Rect::from_center_size(at(x, y), vec2(4.0, 3.0)), 0.0, Color32::from_rgb(0, 120, 200));
            }
        }
        Vk::Xg => {
            p.rect_filled(r, 1.0, blk);
            p.text(r.center(), egui::Align2::CENTER_CENTER, "XG", egui::FontId::proportional(10.0), Color32::WHITE);
        }
        Vk::Master => {
            p.circle_filled(at(8.0, 3.0), 2.2, blk);
            p.line_segment([at(8.0, 5.0), at(8.0, 11.0)], Stroke::new(2.0_f32, blk));
            p.line_segment([at(8.0, 7.0), at(14.0, 2.0)], s1);
            p.line_segment([at(8.0, 11.0), at(5.0, 15.0)], s1);
            p.line_segment([at(8.0, 11.0), at(11.0, 15.0)], s1);
        }
        Vk::Lyrics => {
            p.text(r.center(), egui::Align2::CENTER_CENTER, "Aa", egui::FontId::proportional(11.0), Color32::from_rgb(180, 0, 120));
        }
        Vk::Markers => {
            p.line_segment([at(4.0, 1.0), at(4.0, 15.0)], Stroke::new(1.5_f32, blk));
            p.add(egui::Shape::convex_polygon(vec![at(4.0, 1.0), at(14.0, 4.5), at(4.0, 8.0)], Color32::from_rgb(0, 170, 190), Stroke::NONE));
        }
        Vk::Line => {
            p.line_segment([at(1.0, 14.0), at(15.0, 2.0)], Stroke::new(1.5_f32, blk));
        }
        Vk::New => {
            p.add(egui::Shape::convex_polygon(vec![at(3.0, 1.0), at(10.0, 1.0), at(13.0, 4.0), at(13.0, 15.0), at(3.0, 15.0)], Color32::WHITE, s1));
            p.line_segment([at(10.0, 1.0), at(10.0, 4.0)], s1);
            p.line_segment([at(10.0, 4.0), at(13.0, 4.0)], s1);
        }
        Vk::Open => {
            p.rect_filled(Rect::from_min_max(at(1.0, 4.0), at(12.0, 14.0)), 1.0, Color32::from_rgb(230, 190, 70));
            p.add(egui::Shape::convex_polygon(vec![at(3.0, 7.0), at(15.0, 7.0), at(12.0, 14.0), at(1.0, 14.0)], Color32::from_rgb(250, 220, 110), s1));
            p.line_segment([at(9.0, 1.0), at(13.0, 3.5)], Stroke::new(1.5_f32, Color32::from_rgb(0, 120, 0)));
        }
        Vk::Save => {
            p.rect_filled(Rect::from_min_max(at(1.0, 1.0), at(15.0, 15.0)), 1.5, Color32::from_rgb(40, 60, 140));
            p.rect_filled(Rect::from_min_max(at(4.0, 1.0), at(12.0, 6.0)), 0.0, Color32::from_gray(225));
            p.rect_filled(Rect::from_min_max(at(4.0, 9.0), at(12.0, 15.0)), 0.0, Color32::WHITE);
        }
        Vk::Undo | Vk::Redo => {
            let d = if k == Vk::Undo { 1.0 } else { -1.0 };
            let cx = 8.0;
            let pts: Vec<egui::Pos2> = (0..=12)
                .map(|i| {
                    let a = std::f32::consts::PI * (i as f32 / 12.0);
                    at(cx - d * 5.0 * a.cos(), 9.0 - 5.0 * a.sin())
                })
                .collect();
            p.add(egui::Shape::line(pts, Stroke::new(2.0_f32, Color32::from_rgb(20, 60, 160))));
            let tip = at(cx - d * 5.0, 9.0);
            p.add(egui::Shape::convex_polygon(vec![tip + vec2(-3.5, -1.0), tip + vec2(3.5, -1.0), tip + vec2(0.0, 4.0)], Color32::from_rgb(20, 60, 160), Stroke::NONE));
        }
        Vk::Transport => {
            p.add(egui::Shape::convex_polygon(vec![at(4.0, 2.0), at(14.0, 8.0), at(4.0, 14.0)], Color32::from_rgb(30, 140, 50), Stroke::NONE));
        }
        Vk::Free => {
            let pts: Vec<egui::Pos2> = (0..=16).map(|i| at(i as f32, 8.0 + 5.0 * (i as f32 * 0.6).sin())).collect();
            p.add(egui::Shape::line(pts, Stroke::new(1.5_f32, blk)));
        }
    }
}

/// A toolbar button with an icon and a bold label (the editors' bar).
pub fn editor_button(ui: &mut egui::Ui, icon: Option<(u32, usize)>, vk: Vk, label: &str, on: bool, tip: &str) -> egui::Response {
    let font = egui::FontId::proportional(13.0);
    let galley = ui.painter().layout_no_wrap(label.to_string(), font, Color32::BLACK);
    let size = vec2(galley.size().x + 34.0, 26.0);
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    let resp = resp.on_hover_text(tip);
    let p = ui.painter();
    let pressed = on || resp.is_pointer_button_down_on();
    bevel(p, r, on, pressed);
    if resp.hovered() && !on {
        p.rect_stroke(r.shrink(1.5), 0.0, Stroke::new(1.0_f32, Color32::from_rgb(90, 140, 230)));
    }
    let off = if pressed { 1.0 } else { 0.0 };
    let dst = Rect::from_min_size(r.min + vec2(6.0 + off, 5.0 + off), vec2(16.0, 16.0));
    match icon {
        Some((id, idx)) if strip(ui.ctx(), id).is_some() => draw_icon(p, ui.ctx(), dst, id, idx),
        _ => vector_icon(p, dst, vk),
    }
    // bold: the text drawn twice, a fraction of a pixel apart
    let tp = pos2(r.left() + 27.0 + off, r.center().y - galley.size().y / 2.0 + off);
    p.galley(tp, galley.clone(), Color32::BLACK);
    p.galley(tp + vec2(0.6, 0.0), galley, Color32::BLACK);
    resp
}
