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
    let Some(tex) = strip(ui.ctx(), id) else {
        return ui.add(egui::SelectableLabel::new(down, fallback)).on_hover_text(tip);
    };
    let (r, resp) = ui.allocate_exact_size(vec2(23.0, 22.0), Sense::click());
    let resp = resp.on_hover_text(tip);
    let p = ui.painter();
    let pressed = down || resp.is_pointer_button_down_on();
    let face = if down { Color32::from_gray(225) } else { Color32::from_gray(240) };
    p.rect_filled(r, 0.0, face);
    let (tl, br) = if pressed { (Color32::from_gray(100), Color32::WHITE) } else { (Color32::WHITE, Color32::from_gray(100)) };
    p.hline(r.x_range(), r.top() + 0.5, Stroke::new(1.0_f32, tl));
    p.vline(r.left() + 0.5, r.y_range(), Stroke::new(1.0_f32, tl));
    p.hline(r.x_range(), r.bottom() - 0.5, Stroke::new(1.0_f32, br));
    p.vline(r.right() - 0.5, r.y_range(), Stroke::new(1.0_f32, br));
    let [w, h] = tex.size();
    let (w, h) = (w as f32, h as f32);
    let uv = Rect::from_min_max(pos2(16.0 * idx as f32 / w, 0.0), pos2(16.0 * (idx + 1) as f32 / w, 1.0));
    let off = if pressed { 1.0 } else { 0.0 };
    let dst = Rect::from_min_size(r.min + vec2(3.0 + off, 3.0 + off), vec2(16.0, h.min(16.0)));
    p.image(tex.id(), dst, uv, Color32::WHITE);
    resp
}
