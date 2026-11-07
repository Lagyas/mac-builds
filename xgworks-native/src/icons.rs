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
/// scissors, bond, then the window buttons, then H zoom in / out, V zoom in / out (18..21).
pub const MAIN_BAR: u32 = 2003;
/// Indices of the zoom buttons in `MAIN_BAR`.
pub const ZOOM_H_IN: usize = 18;
pub const ZOOM_H_OUT: usize = 19;
pub const ZOOM_V_IN: usize = 20;
pub const ZOOM_V_OUT: usize = 21;
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
    tool_button_en(ui, id, idx, down, true, fallback, tip)
}

/// `tool_button` that can be disabled: the icon is drawn greyed and clicks
/// are ignored (the original greys its zoom buttons at the largest / smallest size).
pub fn tool_button_en(ui: &mut egui::Ui, id: u32, idx: usize, down: bool, enabled: bool, fallback: &str, tip: &str) -> egui::Response {
    let has_tex = strip(ui.ctx(), id).is_some();
    if !has_tex && vector_kind(id, idx).is_none() {
        return ui.add_enabled(enabled, egui::SelectableLabel::new(down, fallback)).on_hover_text(tip);
    }
    let (r, resp) = ui.allocate_exact_size(vec2(23.0, 22.0), if enabled { Sense::click() } else { Sense::hover() });
    let resp = resp.on_hover_text(tip);
    let mut p = ui.painter().clone();
    let pressed = enabled && (down || resp.is_pointer_button_down_on());
    bevel(&p, r, down, pressed);
    if !enabled {
        p.set_opacity(0.35);
    }
    let off = if pressed { 1.0 } else { 0.0 };
    let dst = Rect::from_min_size(r.min + vec2(3.0 + off, 3.0 + off), vec2(16.0, 16.0));
    draw_icon(&p, ui.ctx(), dst, id, idx);
    resp
}

/// A button at `r` showing icon `idx` of strip `id` (the scroll bars' zoom
/// buttons); greyed and ignoring clicks when not `enabled`.
pub fn icon_button_at(ui: &mut egui::Ui, r: Rect, id: u32, idx: usize, enabled: bool, tip: &str, salt: impl std::hash::Hash) -> egui::Response {
    let resp = ui.interact(r, ui.id().with(salt), if enabled { Sense::click() } else { Sense::hover() }).on_hover_text(tip);
    let mut p = ui.painter_at(r);
    let pressed = enabled && resp.is_pointer_button_down_on();
    bevel(&p, r, false, pressed);
    if !enabled {
        p.set_opacity(0.35);
    }
    let off = if pressed { 1.0 } else { 0.0 };
    let dst = Rect::from_min_size((r.center() - vec2(8.0 - off, 8.0 - off)).round(), vec2(16.0, 16.0));
    draw_icon(&p, ui.ctx(), dst, id, idx);
    resp
}

/// A Win95 scroll bar in `r`: arrow buttons at both ends (one `step`), a
/// thumb sized `page / total` to drag, a click beside the thumb moves one
/// page. `pos` is the first visible unit, kept in 0 ..= total - page; with
/// nothing to scroll the bar is drawn disabled. Returns true when `pos` moved.
#[allow(clippy::too_many_arguments)]
pub fn scroll_bar(ui: &mut egui::Ui, r: Rect, vertical: bool, pos: &mut f32, page: f32, total: f32, step: f32, salt: &str) -> bool {
    let id = ui.id().with(salt);
    let p = ui.painter_at(r);
    let max = (total - page).max(0.0);
    let on = max > 0.0;
    let old = *pos;
    // along / across the bar
    let (len, w) = if vertical { (r.height(), r.width()) } else { (r.width(), r.height()) };
    let w = w.min(len / 2.0);
    let at = |a0: f32, a1: f32| {
        if vertical {
            Rect::from_min_max(pos2(r.left(), r.top() + a0), pos2(r.right(), r.top() + a1))
        } else {
            Rect::from_min_max(pos2(r.left() + a0, r.top()), pos2(r.left() + a1, r.bottom()))
        }
    };
    let track = at(w, len - w);
    p.rect_filled(r, 0.0, Color32::from_gray(0xEC));
    let track_len = len - 2.0 * w;
    let thumb_len = (track_len * page / total.max(page)).max(8.0).min(track_len);
    let free = (track_len - thumb_len).max(0.0);
    let thumb_at = |v: f32| w + if max > 0.0 { free * v / max } else { 0.0 };
    // a click beside the thumb pages
    let tr = ui.interact(track, id.with("track"), if on { Sense::click() } else { Sense::hover() });
    if let (true, Some(m)) = (tr.clicked(), tr.interact_pointer_pos()) {
        let a = if vertical { m.y - r.top() } else { m.x - r.left() };
        *pos += if a < thumb_at(*pos) { -page } else { page };
    }
    // arrows
    for (k, dir) in [(0, -1.0_f32), (1, 1.0)] {
        let ar = if k == 0 { at(0.0, w) } else { at(len - w, len) };
        let resp = ui.interact(ar, id.with(("arrow", k)), if on { Sense::click() } else { Sense::hover() });
        let pressed = on && resp.is_pointer_button_down_on();
        bevel(&p, ar, false, pressed);
        if resp.clicked() {
            *pos += dir * step;
        }
        let c = ar.center() + if pressed { vec2(1.0, 1.0) } else { vec2(0.0, 0.0) };
        let (s, h) = (3.5, 2.0);
        let pts = match (vertical, k) {
            (true, 0) => vec![pos2(c.x, c.y - h), pos2(c.x - s, c.y + h), pos2(c.x + s, c.y + h)],
            (true, _) => vec![pos2(c.x, c.y + h), pos2(c.x - s, c.y - h), pos2(c.x + s, c.y - h)],
            (false, 0) => vec![pos2(c.x - h, c.y), pos2(c.x + h, c.y - s), pos2(c.x + h, c.y + s)],
            (false, _) => vec![pos2(c.x + h, c.y), pos2(c.x - h, c.y - s), pos2(c.x - h, c.y + s)],
        };
        let col = if on { Color32::BLACK } else { Color32::from_gray(0xA0) };
        p.add(egui::Shape::convex_polygon(pts, col, Stroke::NONE));
    }
    // thumb
    if on {
        let a0 = thumb_at(pos.clamp(0.0, max));
        let th = at(a0, a0 + thumb_len);
        let resp = ui.interact(th, id.with("thumb"), Sense::drag());
        if resp.dragged() && free > 0.0 {
            let d = resp.drag_delta();
            *pos += (if vertical { d.y } else { d.x }) * max / free;
        }
        bevel(&p, th, false, false);
    }
    *pos = pos.clamp(0.0, max);
    *pos != old
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
    ZoomHIn,
    ZoomHOut,
    ZoomVIn,
    ZoomVOut,
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
        (MAIN_BAR, ZOOM_H_IN) => Some(Vk::ZoomHIn),
        (MAIN_BAR, ZOOM_H_OUT) => Some(Vk::ZoomHOut),
        (MAIN_BAR, ZOOM_V_IN) => Some(Vk::ZoomVIn),
        (MAIN_BAR, ZOOM_V_OUT) => Some(Vk::ZoomVOut),
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
        Vk::ZoomHIn | Vk::ZoomHOut | Vk::ZoomVIn | Vk::ZoomVOut => {
            // magnifier with + / -, arrows apart (in) or together (out), as in bitmap 2003
            let horiz = matches!(k, Vk::ZoomHIn | Vk::ZoomHOut);
            let zin = matches!(k, Vk::ZoomHIn | Vk::ZoomVIn);
            let col = if zin { Color32::from_rgb(0, 0, 230) } else { Color32::from_rgb(230, 0, 0) };
            let s2 = Stroke::new(1.5_f32, col);
            let c = if horiz { at(8.0, 5.0) } else { at(5.5, 8.0) };
            p.circle_filled(c, 4.5, Color32::WHITE);
            p.circle_stroke(c, 4.5, Stroke::new(1.5_f32, blk));
            p.line_segment([c - vec2(2.5, 0.0), c + vec2(2.5, 0.0)], Stroke::new(1.5_f32, blk));
            if zin {
                p.line_segment([c - vec2(0.0, 2.5), c + vec2(0.0, 2.5)], Stroke::new(1.5_f32, blk));
            }
            // handle + arrows on the zoom axis
            let arrow = |from: egui::Pos2, to: egui::Pos2| {
                p.line_segment([from, to], s2);
                let d = (to - from).normalized();
                let n = vec2(-d.y, d.x);
                p.add(egui::Shape::convex_polygon(vec![to + d * 1.5, to - d * 2.0 + n * 2.5, to - d * 2.0 - n * 2.5], col, Stroke::NONE));
            };
            if horiz {
                p.line_segment([at(8.0, 9.5), at(8.0, 15.0)], Stroke::new(1.5_f32, blk));
                if zin {
                    arrow(at(6.0, 13.0), at(1.5, 13.0));
                    arrow(at(10.0, 13.0), at(14.5, 13.0));
                } else {
                    arrow(at(1.0, 13.0), at(5.5, 13.0));
                    arrow(at(15.0, 13.0), at(10.5, 13.0));
                }
            } else {
                p.line_segment([at(10.0, 8.0), at(15.5, 8.0)], Stroke::new(1.5_f32, blk));
                if zin {
                    arrow(at(13.0, 6.0), at(13.0, 1.5));
                    arrow(at(13.0, 10.0), at(13.0, 14.5));
                } else {
                    arrow(at(13.0, 1.0), at(13.0, 5.5));
                    arrow(at(13.0, 15.0), at(13.0, 10.5));
                }
            }
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
