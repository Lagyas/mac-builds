//! Print / export of the Staff window: the whole track laid out on pages
//! (systems of `BARS` bars, clef / key on every system, time signature on
//! the first) as an SVG document — opened in the browser / Preview, it is
//! printed or saved as PDF from there. The notation font (XG4/XGmusic.ttf)
//! is embedded in the file.

use crate::staff::{Canvas, StaffInfo, View};
use eframe::egui::{self, Color32, FontId, Pos2, Rect, Stroke};
use std::cell::RefCell;
use std::fmt::Write;
use xgworks_core::EditNote;

/// Bars per system.
pub const BARS: u32 = 4;
const PAGE_W: f32 = 794.0; // A4 at 96 dpi
const PAGE_H: f32 = 1123.0;
const MARGIN: f32 = 48.0;
/// Half a staff space (px).
const HALF: f32 = 3.5;

pub struct Svg {
    body: RefCell<String>,
    font: bool,
}

fn col(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

impl Svg {
    fn new(font: bool) -> Self {
        Svg { body: RefCell::new(String::new()), font }
    }
    fn push(&self, s: String) {
        self.body.borrow_mut().push_str(&s);
        self.body.borrow_mut().push('\n');
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

impl Canvas for Svg {
    fn line_segment(&self, p: [Pos2; 2], st: Stroke) {
        self.push(format!(
            r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{}" stroke-width="{:.2}"/>"#,
            p[0].x, p[0].y, p[1].x, p[1].y, col(st.color), st.width
        ));
    }
    fn circle_filled(&self, c: Pos2, r: f32, c2: Color32) {
        self.push(format!(r#"<circle cx="{:.1}" cy="{:.1}" r="{:.2}" fill="{}"/>"#, c.x, c.y, r, col(c2)));
    }
    fn add(&self, shape: egui::Shape) {
        if let egui::Shape::Path(ps) = shape {
            let pts: String = ps.points.iter().map(|p| format!("{:.1},{:.1}", p.x, p.y)).collect::<Vec<_>>().join(" ");
            let stroke = match ps.stroke.color {
                egui::epaint::ColorMode::Solid(c) if ps.stroke.width > 0.0 => format!(r#" stroke="{}" stroke-width="{:.2}""#, col(c), ps.stroke.width),
                _ => String::new(),
            };
            let fill = if ps.fill.a() > 0 { col(ps.fill) } else { "none".into() };
            let tag = if ps.closed { "polygon" } else { "polyline" };
            self.push(format!(r#"<{tag} points="{pts}" fill="{fill}"{stroke}/>"#));
        }
    }
    fn rect_filled(&self, r: Rect, _rounding: f32, c: Color32) {
        self.push(format!(r#"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="{}"/>"#, r.left(), r.top(), r.width(), r.height(), col(c)));
    }
    fn text(&self, pos: Pos2, align: egui::Align2, text: &str, font: FontId, c: Color32) {
        let anchor = match align.x() {
            egui::Align::Min => "start",
            egui::Align::Center => "middle",
            egui::Align::Max => "end",
        };
        let dy = match align.y() {
            egui::Align::Min => font.size * 0.8,
            egui::Align::Center => font.size * 0.35,
            egui::Align::Max => 0.0,
        };
        self.push(format!(
            r#"<text x="{:.1}" y="{:.1}" font-family="Helvetica, Arial, sans-serif" font-size="{:.1}" text-anchor="{anchor}" fill="{}">{}</text>"#,
            pos.x,
            pos.y + dy,
            font.size,
            col(c),
            esc(text)
        ));
    }
    fn glyph(&self, at: Pos2, code: u32, size: f32, c: Color32) {
        // SVG text sits on its baseline: the font's ascent is 1.404 em
        self.push(format!(
            r#"<text x="{:.1}" y="{:.1}" font-family="XGmusic" font-size="{:.1}" fill="{}">&#x{:X};</text>"#,
            at.x,
            at.y + size * 1.404,
            size,
            col(c),
            0xF000 + code
        ));
    }
    fn music_font(&self) -> bool {
        self.font
    }
}

/// The SVG document for `notes` (all pages stacked, page breaks for printing).
pub fn export(title: &str, notes: &[EditNote], info: &StaffInfo, ppqn: u32, music_ttf: Option<&[u8]>) -> String {
    let bar = info.bar(ppqn);
    let end = notes.iter().map(|n| n.tick + n.dur).max().unwrap_or(bar);
    let bars = end.div_ceil(bar).max(1);
    let systems = bars.div_ceil(BARS);
    let grand = info.clef == crate::staff::Clef::Grand;
    // rows: treble E4..F5 = steps 37..45, bass 25..33 (grand: shifted by the gap)
    let top_step = 52.0; // room above the treble staff
    let (lo_step, hi_step) = if grand { (14.0, 52.0) } else if info.clef == crate::staff::Clef::Bass { (14.0, 40.0) } else { (26.0, 52.0) };
    let sys_h = (hi_step - lo_step) * HALF + 24.0;
    let per_page = (((PAGE_H - 2.0 * MARGIN - 40.0) / sys_h).floor() as u32).max(1);
    let pages = systems.div_ceil(per_page);
    let svg = Svg::new(music_ttf.is_some());
    let left = MARGIN;
    let head_w = 70.0 + info.sf.unsigned_abs() as f32 * 7.0; // clef + key
    for s in 0..systems {
        let page = s / per_page;
        let row = s % per_page;
        let y0 = page as f32 * PAGE_H + MARGIN + 40.0 + row as f32 * sys_h;
        if row == 0 {
            svg.push(format!(r#"<text x="{:.1}" y="{:.1}" font-family="Helvetica, Arial, sans-serif" font-size="16" font-weight="bold">{}</text>"#, left, page as f32 * PAGE_H + MARGIN + 12.0, esc(title)));
            svg.push(format!(r#"<text x="{:.1}" y="{:.1}" font-family="Helvetica, Arial, sans-serif" font-size="10" text-anchor="end">{}/{}</text>"#, PAGE_W - MARGIN, page as f32 * PAGE_H + MARGIN + 12.0, page + 1, pages));
        }
        let (t0, t1) = (s * BARS * bar, ((s + 1) * BARS * bar).min(bars * bar));
        let gx0 = left + head_w + if s == 0 { 28.0 } else { 0.0 };
        let right = PAGE_W - MARGIN;
        let ppt = (right - gx0 - 8.0) / (BARS * bar) as f32;
        let x_of = move |t: u32| gx0 + 8.0 + (t as f32 - t0 as f32) * ppt;
        let y_step = move |st: i32| y0 + (top_step - info.row(st) as f32) * HALF;
        let v = View { x_of: &x_of, y_step: &y_step, half: HALF, left, right, gx0, ppqn };
        let mut si = *info;
        if s > 0 {
            si.ts_num = 0; // no time signature after the first system
        }
        draw_head(&svg, &si, &v);
        info.draw_bars(&svg, &v, t0, t1);
        // closing bar line of the system
        let staves_top = y_step(45);
        let staves_bot = y_step(if grand || info.clef == crate::staff::Clef::Bass { 25 } else { 37 });
        let top = if info.clef == crate::staff::Clef::Bass { y_step(33) } else { staves_top };
        svg.line_segment([Pos2::new(right, top), Pos2::new(right, staves_bot)], Stroke::new(1.0_f32, Color32::BLACK));
        let (m0, _, _) = (s * BARS + 1, 0, 0);
        svg.text(Pos2::new(left, top - 14.0), egui::Align2::LEFT_TOP, &m0.to_string(), FontId::proportional(9.0), Color32::from_gray(80));
        let sel = |_: usize| false;
        info.draw_notes(&svg, &v, notes, &sel, t0, t1.saturating_sub(1));
    }
    let mut out = String::new();
    let total_h = pages as f32 * PAGE_H;
    let _ = write!(
        out,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="{PAGE_W}" height="{total_h}" viewBox="0 0 {PAGE_W} {total_h}">
<title>{}</title>
<style>
"#,
        esc(title)
    );
    if let Some(ttf) = music_ttf {
        let _ = writeln!(out, "@font-face {{ font-family: 'XGmusic'; src: url(data:font/ttf;base64,{}) format('truetype'); }}", b64(ttf));
    }
    out.push_str("@page { size: A4; margin: 0; }\n</style>\n<rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n");
    out.push_str(&svg.body.borrow());
    out.push_str("</svg>\n");
    out
}

/// Staff lines + clef + key (+ time signature when `ts_num` > 0).
fn draw_head(c: &Svg, info: &StaffInfo, v: &View) {
    info.draw_staff(c, v);
}

fn b64(data: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(data.len() * 4 / 3 + 4);
    for ch in data.chunks(3) {
        let b = [ch[0], *ch.get(1).unwrap_or(&0), *ch.get(2).unwrap_or(&0)];
        let n = (b[0] as u32) << 16 | (b[1] as u32) << 8 | b[2] as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if ch.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if ch.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exports_pages() {
        let notes: Vec<EditNote> = (0..40).map(|k| EditNote { tick: k * 480, dur: 480, pitch: 60 + (k % 12) as u8, vel: 100 }).collect();
        let svg = export("Test", &notes, &StaffInfo::default(), 480, None);
        assert!(svg.starts_with("<?xml"));
        assert!(svg.contains("<circle") || svg.contains("<polygon"));
        assert!(svg.ends_with("</svg>\n"));
        assert_eq!(b64(b"Man"), "TWFu");
        assert_eq!(b64(b"Ma"), "TWE=");
    }
}

/// `cargo test -- --ignored svg_of_song` writes target/staff_test.svg from the sample song.
#[cfg(all(test, not(target_arch = "wasm32")))]
mod sample {
    #[test]
    #[ignore]
    fn svg_of_song() {
        let Ok(b) = std::fs::read(r"F:\rea\A Dream of a Dream.XWS") else { return };
        let (tracks, ppqn) = xgworks_core::edit_model(&b);
        let t = tracks.iter().find(|t| t.name == "EP").unwrap();
        let mut info = crate::staff::StaffInfo::default();
        info.sf = 1;
        info.clef = crate::staff::clef_for(&t.notes);
        let ttf = std::fs::read(r"F:\rea\XG4\XGmusic.ttf").ok();
        let svg = super::export("A Dream of a Dream — EP", &t.notes, &info, ppqn as u32, ttf.as_deref());
        std::fs::write("target/staff_test.svg", svg).unwrap();
    }
}
