//! The original's tool cursors (RT_GROUP_CURSOR resources of XGworks.exe,
//! loaded at run time). egui has no custom cursors, so the system cursor is
//! hidden and the image is painted at the pointer on a top layer; without the
//! EXE a similar system cursor is used.

use eframe::egui::{self, Color32, Rect, Vec2};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Cursor group ids in XGworks.exe.
#[derive(Clone, Copy)]
pub enum Cur {
    Pencil = 2200,
    Eraser = 2201,
    Scissors = 2202,
    Glue = 2203,
}

type Cache = HashMap<u32, (egui::TextureHandle, (f32, f32))>;

fn texture(ctx: &egui::Context, c: Cur) -> Option<(egui::TextureHandle, (f32, f32))> {
    static CACHE: OnceLock<Mutex<Option<Cache>>> = OnceLock::new();
    let mut g = CACHE.get_or_init(|| Mutex::new(None)).lock().unwrap();
    let map = g.get_or_insert_with(|| {
        let mut m = HashMap::new();
        if let Some(exe) = crate::platform::read(&crate::platform::xg4_exe()) {
            for (id, cur) in crate::peres::load_cursors(&exe, |i| (2200..=2208).contains(&i)) {
                let img = egui::ColorImage::from_rgba_unmultiplied([cur.img.w, cur.img.h], &cur.img.rgba);
                let tex = ctx.load_texture(format!("cursor{id}"), img, egui::TextureOptions::NEAREST);
                m.insert(id, (tex, (cur.hot.0 as f32, cur.hot.1 as f32)));
            }
        }
        m
    });
    map.get(&(c as u32)).cloned()
}

/// Show cursor `c` at the pointer this frame (call while hovering the area).
pub fn show(ctx: &egui::Context, c: Cur) {
    let Some(pos) = ctx.input(|i| i.pointer.hover_pos()) else { return };
    let Some((tex, hot)) = texture(ctx, c) else {
        ctx.set_cursor_icon(match c {
            Cur::Pencil => egui::CursorIcon::Crosshair,
            Cur::Eraser => egui::CursorIcon::NotAllowed,
            Cur::Scissors => egui::CursorIcon::VerticalText,
            Cur::Glue => egui::CursorIcon::Cell,
        });
        return;
    };
    ctx.set_cursor_icon(egui::CursorIcon::None);
    let [w, h] = tex.size();
    let r = Rect::from_min_size(pos - Vec2::new(hot.0, hot.1), Vec2::new(w as f32, h as f32));
    let layer = egui::LayerId::new(egui::Order::Debug, egui::Id::new("xgw_cursor"));
    ctx.layer_painter(layer).image(tex.id(), r, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
}
