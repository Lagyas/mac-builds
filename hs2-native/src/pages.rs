//! The seven Display pages (Load, Edit, MIDI, Mix, FX, Combi, Setup), laid
//! out after the original display bitmaps (272 × 348 at panel 476, 26).

use crate::app::{act_int, note_name, App, ModView, Page, Sub};
use crate::panel::{self, col, Act, Panel};
use eframe::egui::{self, Align2, Color32, Id, Pos2, Rect, Rounding, Sense, Stroke, Ui, Vec2};
use hs2_core::arp;
use hs2_core::dsp::{FilterMode, LfoSync, LfoWave};
use hs2_core::fx;
use hs2_core::params;
use hs2_core::patch::{bipolar, enum_idx, enum_val, ep, pp, time_s, ElemType, Patch};
use hs2_core::synth::{FxSlot, PARTS};
use hs2_core::wavetable;

const X0: f32 = 476.0;
const Y0: f32 = 26.0;

/// Display-relative geometry.
struct D<'a> {
    pn: &'a Panel,
}

impl D<'_> {
    fn p(&self, x: f32, y: f32) -> Pos2 {
        self.pn.p(X0 + x, Y0 + y)
    }
    fn r(&self, x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        self.pn.r(X0 + x0, Y0 + y0, X0 + x1, Y0 + y1)
    }
    fn label(&self, ui: &Ui, x: f32, y: f32, t: &str) {
        ui.painter().text(self.p(x, y), Align2::LEFT_CENTER, t, self.pn.f(8.5), col::LCD_TEXT);
    }
    fn label_c(&self, ui: &Ui, x: f32, y: f32, t: &str) {
        ui.painter().text(self.p(x, y), Align2::CENTER_CENTER, t, self.pn.f(8.5), col::LCD_TEXT);
    }
    fn label_b(&self, ui: &Ui, x: f32, y: f32, t: &str) {
        ui.painter().text(self.p(x, y), Align2::LEFT_CENTER, t, egui::FontId::proportional(9.0 * self.pn.s), col::LCD_TEXT);
    }
    fn frame(&self, ui: &Ui, x0: f32, y0: f32, x1: f32, y1: f32) {
        ui.painter().rect_stroke(self.r(x0, y0, x1, y1), Rounding::same(2.0 * self.pn.s), Stroke::new(self.pn.s, col::LCD_LINE));
    }
}

pub fn show(app: &mut App, ui: &mut Ui, pn: &Panel, _dr: Rect) {
    let d = D { pn };
    match app.page {
        Page::Load => load(app, ui, &d),
        Page::Edit => edit(app, ui, &d),
        Page::Midi => midi(app, ui, &d),
        Page::Mix => mix(app, ui, &d),
        Page::Fx => fxpage(app, ui, &d),
        Page::Combi => combi(app, ui, &d),
        Page::Setup => setup(app, ui, &d),
    }
}

// ---------------------------------------------------------------------------
// shared parameter controls
// ---------------------------------------------------------------------------

fn cur_patch(app: &App) -> Option<std::sync::Arc<Patch>> {
    app.parts[app.sel].patch.clone()
}

/// Knob on element parameter `i` of the edited element.
fn ek(app: &mut App, ui: &mut Ui, d: &D, e: usize, i: usize, x: f32, y: f32, label: &str, bip: bool, fmt: fn(f32) -> String) {
    let Some(p) = cur_patch(app) else { return };
    let Some(el) = p.elements.get(e) else { return };
    let v = el.p[i];
    let def = if bip { 0.5 } else { v };
    let (nv, resp) = panel::knob(ui, d.pn, Id::new(("ek", e, i)), d.p(x, y), 9.0, v, bip, label, def);
    if let Some(nv) = nv {
        app.edit(|q| q.elements[e].p[i] = nv);
        app.show_hyper(format!("{}: {}", params::element(i, el.kind()), fmt(nv)), 1.5);
    }
    if resp.hovered() && nv.is_none() {
        app.show_hyper(format!("{}: {}", params::element(i, el.kind()), fmt(v)), 0.3);
    }
}

/// Small horizontal slider on element parameter `i`.
fn es(app: &mut App, ui: &mut Ui, d: &D, e: usize, i: usize, x0: f32, y: f32, w: f32, bip: bool) {
    let Some(p) = cur_patch(app) else { return };
    let Some(el) = p.elements.get(e) else { return };
    let v = el.p[i];
    let (nv, _) = panel::hslider(ui, d.pn, Id::new(("es", e, i)), d.r(x0, y - 3.5, x0 + w, y + 3.5), v, bip, if bip { 0.5 } else { 0.0 });
    if let Some(nv) = nv {
        app.edit(|q| q.elements[e].p[i] = nv);
        app.show_hyper(format!("{}: {:+.0}%", params::element(i, el.kind()), if bip { bipolar(nv) * 100.0 } else { nv * 100.0 }), 1.5);
    }
}

/// Vertical envelope slider (A D S F R) on element parameter `i`.
fn vslider(app: &mut App, ui: &mut Ui, d: &D, e: usize, i: usize, x: f32, y0: f32, y1: f32, label: &str) {
    let Some(p) = cur_patch(app) else { return };
    let Some(el) = p.elements.get(e) else { return };
    let v = el.p[i];
    let r = d.r(x - 3.0, y0, x + 3.0, y1);
    let resp = ui.interact(r.expand(2.0), Id::new(("vs", e, i)), Sense::click_and_drag());
    let painter = ui.painter();
    painter.rect_filled(r, Rounding::same(1.0), col::BLUE_LT);
    let yv = r.bottom() - r.height() * v;
    painter.rect_filled(Rect::from_min_max(Pos2::new(r.left(), yv), r.right_bottom()), Rounding::ZERO, col::BLUE);
    painter.rect_filled(Rect::from_center_size(Pos2::new(r.center().x, yv), Vec2::new(r.width() + 4.0 * d.pn.s, 2.5 * d.pn.s)), Rounding::ZERO, col::DARK);
    d.label_c(ui, x, y1 + 7.0, label);
    if resp.dragged() {
        let f = if ui.input(|x| x.modifiers.shift) { 0.25 } else { 1.0 };
        let nv = (v - resp.drag_delta().y / r.height() * f).clamp(0.0, 1.0);
        app.edit(|q| q.elements[e].p[i] = nv);
        let unit = if label == "S" { format!("{:.0}%", nv * 100.0) } else if label == "F" { fade_text(nv) } else { secs(time_s(nv)) };
        app.show_hyper(format!("{}: {unit}", params::element(i, el.kind())), 1.5);
    }
}

fn fade_text(v: f32) -> String {
    if (v - 0.5).abs() < 0.02 {
        "Hold".into()
    } else if v < 0.5 {
        format!("Fade out {}", secs(time_s((0.5 - v) * 2.0)))
    } else {
        format!("Fade up {}", secs(time_s((v - 0.5) * 2.0)))
    }
}

fn secs(s: f32) -> String {
    if s < 1.0 {
        format!("{:.0} ms", s * 1000.0)
    } else {
        format!("{:.2} s", s)
    }
}

fn pct(v: f32) -> String {
    format!("{:.0}%", v * 100.0)
}
fn bpct(v: f32) -> String {
    format!("{:+.0}%", bipolar(v) * 100.0)
}
fn semis24(v: f32) -> String {
    format!("{:+.0}", (bipolar(v) * 24.0).round())
}
fn cents(v: f32) -> String {
    format!("{:+.0} ct", bipolar(v) * 100.0)
}
fn cutoff_text(v: f32) -> String {
    format!("{:.0} Hz (at C3)", hs2_core::patch::pitch_hz(hs2_core::patch::cutoff_pitch(v)))
}
fn level_text(v: f32) -> String {
    let g = hs2_core::patch::level_gain(v);
    if g <= 0.0 {
        "-inf dB".into()
    } else {
        format!("{:+.1} dB", 20.0 * g.log10())
    }
}

/// An enumeration value field on element parameter `i` with a pop-up menu.
fn eenum(app: &mut App, ui: &mut Ui, d: &D, e: usize, i: usize, r: Rect, names: &[String]) {
    let Some(p) = cur_patch(app) else { return };
    let Some(el) = p.elements.get(e) else { return };
    let n = names.len();
    let cur = enum_idx(el.p[i], n);
    let id = Id::new(("eenum", e, i));
    let (_, resp) = panel::value(ui, d.pn, id, r, &names[cur], false);
    if let Some(k) = panel::menu(ui, &resp, id.with("m"), names, Some(cur)) {
        app.edit(|q| q.elements[e].p[i] = enum_val(k, n));
    }
}

/// Enumeration value field on patch parameter `i`.
fn penum(app: &mut App, ui: &mut Ui, d: &D, i: usize, r: Rect, names: &[String]) {
    let Some(p) = cur_patch(app) else { return };
    let n = names.len();
    let cur = enum_idx(p.p[i], n);
    let id = Id::new(("penum", i));
    let (_, resp) = panel::value(ui, d.pn, id, r, &names[cur], false);
    if let Some(k) = panel::menu(ui, &resp, id.with("m"), names, Some(cur)) {
        app.edit(|q| q.p[i] = enum_val(k, n));
    }
}

fn strs(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn pk(app: &mut App, ui: &mut Ui, d: &D, i: usize, x: f32, y: f32, label: &str, bip: bool) {
    let Some(p) = cur_patch(app) else { return };
    let v = p.p[i];
    let (nv, _) = panel::knob(ui, d.pn, Id::new(("pk", i)), d.p(x, y), 9.0, v, bip, label, if bip { 0.5 } else { v });
    if let Some(nv) = nv {
        app.edit(|q| q.p[i] = nv);
        app.show_hyper(format!("{label}: {}", if bip { bpct(nv) } else { pct(nv) }), 1.5);
    }
}

// ---------------------------------------------------------------------------
// LOAD
// ---------------------------------------------------------------------------

/// Bank disclosure triangle.
fn tri(ui: &Ui, d: &D, x: f32, y: f32, open: bool) {
    let pts = if open { vec![d.p(x - 3.0, y - 2.0), d.p(x + 3.0, y - 2.0), d.p(x, y + 3.0)] } else { vec![d.p(x - 2.0, y - 3.5), d.p(x + 3.0, y), d.p(x - 2.0, y + 3.5)] };
    ui.painter().add(egui::Shape::convex_polygon(pts, col::LCD_TEXT, Stroke::NONE));
}

enum Line {
    Bank(u32, String, bool, bool),
    Patch(u32, usize, String, bool),
    Results(usize, bool),
    CombiBank(usize, bool),
    Combi(usize, String),
}

fn load(app: &mut App, ui: &mut Ui, d: &D) {
    // search field
    let sr = d.r(4.0, 4.0, 268.0, 18.0);
    ui.painter().rect_filled(sr, Rounding::same(2.0), Color32::WHITE);
    let te = ui.put(sr, egui::TextEdit::singleline(&mut app.search).hint_text("search…").font(d.pn.f(9.0)).frame(false));
    if te.lost_focus() && ui.input(|x| x.key_pressed(egui::Key::Enter)) {
        app.results = app.lib.search(&app.search);
        app.open_banks.insert(u32::MAX);
        app.load_scroll = f32::MAX;
    }
    te.context_menu(|ui| {
        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
            for k in hs2_core::library::KEYWORDS {
                if ui.button(k).clicked() {
                    if !app.search.is_empty() {
                        app.search.push(' ');
                    }
                    app.search.push_str(k);
                    app.results = app.lib.search(&app.search);
                    app.open_banks.insert(u32::MAX);
                    ui.close_menu();
                }
            }
        });
    });
    // list
    let mut lines = Vec::new();
    for b in &app.lib.banks {
        let open = app.open_banks.contains(&b.number);
        lines.push(Line::Bank(b.number, format!("{:03} {}", b.number, b.name), open, b.user));
        if open {
            for (i, p) in b.patches.iter().enumerate() {
                lines.push(Line::Patch(b.number, i, if p.is_layer() { format!("≈ {}", p.display_name()) } else { p.display_name().to_string() }, app.load_sel == Some((b.number, i))));
            }
        }
    }
    if !app.lib.combis.is_empty() {
        let open = app.open_banks.contains(&(u32::MAX - 1));
        lines.push(Line::CombiBank(app.lib.combis.len(), open));
        if open {
            for (i, c) in app.lib.combis.iter().enumerate() {
                lines.push(Line::Combi(i, c.name.clone()));
            }
        }
    }
    if !app.results.is_empty() {
        let open = app.open_banks.contains(&u32::MAX);
        lines.push(Line::Results(app.results.len(), open));
        if open {
            for &(b, i) in &app.results {
                if let Some(p) = app.lib.bank(b).and_then(|x| x.patches.get(i)) {
                    lines.push(Line::Patch(b, i, format!("{} ({:03})", p.display_name(), b), app.load_sel == Some((b, i))));
                }
            }
        }
    }
    if app.lib.banks.is_empty() {
        d.label(ui, 10.0, 40.0, "No patches. Choose the content folder on the SETUP page.");
    }
    let lh = 12.5;
    let list = d.r(2.0, 22.0, 258.0, 344.0);
    let visible = ((344.0 - 22.0) / lh) as usize;
    let max_scroll = lines.len().saturating_sub(visible) as f32;
    let lresp = ui.interact(list, Id::new("loadlist"), Sense::hover());
    if lresp.hovered() {
        let dy = ui.input(|x| x.raw_scroll_delta.y);
        app.load_scroll -= dy / (lh * d.pn.s);
    }
    app.load_scroll = app.load_scroll.clamp(0.0, max_scroll);
    let first = app.load_scroll as usize;
    for (k, line) in lines.iter().enumerate().skip(first).take(visible) {
        let y = 22.0 + (k - first) as f32 * lh;
        let r = d.r(4.0, y, 256.0, y + lh);
        let resp = ui.interact(r, Id::new(("ll", k)), Sense::click_and_drag());
        match line {
            Line::Bank(n, name, open, user) => {
                tri(ui, d, 9.0, y + lh / 2.0, *open);
                ui.painter().text(d.p(16.0, y + lh / 2.0), Align2::LEFT_CENTER, name, d.pn.f(9.0), if *user { col::BLUE } else { col::LCD_TEXT });
                if resp.clicked() || resp.double_clicked() {
                    if *open {
                        app.open_banks.remove(n);
                    } else {
                        app.open_banks.insert(*n);
                    }
                }
                let (n, user) = (*n, *user);
                resp.context_menu(|ui| {
                    if ui.button("New Patch Bank").clicked() {
                        let b = app.lib.add_user_bank("New Bank", Vec::new());
                        app.open_banks.insert(b);
                        ui.close_menu();
                    }
                    if user && ui.button("Rename (to selected part's name)").clicked() {
                        let nm = app.parts[app.sel].name().to_string();
                        if let Some(b) = app.lib.bank_mut(n) {
                            b.name = nm;
                        }
                        ui.close_menu();
                    }
                });
            }
            Line::Results(cnt, open) => {
                tri(ui, d, 9.0, y + lh / 2.0, *open);
                ui.painter().text(d.p(16.0, y + lh / 2.0), Align2::LEFT_CENTER, format!("Search Results ({cnt})"), d.pn.f(9.0), col::BLUE);
                if resp.clicked() {
                    if *open {
                        app.open_banks.remove(&u32::MAX);
                    } else {
                        app.open_banks.insert(u32::MAX);
                    }
                }
            }
            Line::CombiBank(cnt, open) => {
                tri(ui, d, 9.0, y + lh / 2.0, *open);
                ui.painter().text(d.p(16.0, y + lh / 2.0), Align2::LEFT_CENTER, format!("120 Combis ({cnt})"), d.pn.f(9.0), col::LCD_TEXT);
                if resp.clicked() || resp.double_clicked() {
                    if *open {
                        app.open_banks.remove(&(u32::MAX - 1));
                    } else {
                        app.open_banks.insert(u32::MAX - 1);
                    }
                }
            }
            Line::Combi(i, name) => {
                if resp.hovered() {
                    ui.painter().rect_filled(r, Rounding::ZERO, Color32::from_rgb(214, 226, 242));
                }
                ui.painter().with_clip_rect(r).text(d.p(20.0, y + lh / 2.0), Align2::LEFT_CENTER, name, d.pn.f(9.0), col::BLUE);
                let i = *i;
                if resp.clicked() {
                    if let Some(c) = app.lib.combis.get(i).cloned() {
                        let parts: Vec<String> = c.parts.iter().map(|p| format!("{}{}: {}", p.part + 1, if p.link { " (link)" } else { "" }, p.patch.display_name())).collect();
                        app.show_hyper(format!("Combi {} (double-click loads all parts)
{}", c.name, parts.join(", ")), 4.0);
                    }
                }
                if resp.double_clicked() {
                    app.load_combi(i);
                }
            }
            Line::Patch(b, i, name, sel) => {
                if *sel {
                    ui.painter().rect_filled(r, Rounding::ZERO, col::SEL);
                } else if resp.hovered() {
                    ui.painter().rect_filled(r, Rounding::ZERO, Color32::from_rgb(214, 226, 242));
                }
                ui.painter().with_clip_rect(r).text(d.p(20.0, y + lh / 2.0), Align2::LEFT_CENTER, name, d.pn.f(9.0), if *sel { Color32::WHITE } else { col::LCD_TEXT });
                let (b, i) = (*b, *i);
                if resp.clicked() {
                    app.load_sel = Some((b, i));
                    if let Some(p) = app.lib.bank(b).and_then(|x| x.patches.get(i)).cloned() {
                        app.show_hyper(format!("{}\n{}\nBank {} · Prog {}", p.display_name(), p.attributes, b, i), 3.0);
                    }
                }
                if resp.double_clicked() {
                    let part = app.sel;
                    app.load_patch(part, b, i);
                }
                if resp.drag_stopped() {
                    // drag and drop into a part slot
                    if let Some(pos) = ui.input(|x| x.pointer.interact_pos()) {
                        let q = d.pn.inv(pos);
                        if q.x > 80.0 && q.x < 330.0 && q.y > 26.0 {
                            let t = ((q.y - 26.0) / 21.75) as usize;
                            if t < PARTS {
                                app.sel = t;
                                app.load_patch(t, b, i);
                            }
                        }
                    }
                }
                let user = app.lib.bank(b).map(|x| x.user).unwrap_or(false);
                resp.context_menu(|ui| {
                    if ui.button("Load into part").clicked() {
                        let part = app.sel;
                        app.load_patch(part, b, i);
                        ui.close_menu();
                    }
                    if user && ui.button("Delete").clicked() {
                        if let Some(bk) = app.lib.bank_mut(b) {
                            if i < bk.patches.len() {
                                bk.patches.remove(i);
                            }
                        }
                        ui.close_menu();
                    }
                    if user && ui.button("Rename (to selected part's name)").clicked() {
                        let nm = app.parts[app.sel].name().to_string();
                        if let Some(bk) = app.lib.bank_mut(b) {
                            if let Some(p) = bk.patches.get_mut(i) {
                                let mut q = (**p).clone();
                                q.name = nm;
                                *p = std::sync::Arc::new(q);
                            }
                        }
                        ui.close_menu();
                    }
                });
            }
        }
    }
    // scrollbar
    let sb = d.r(260.0, 22.0, 268.0, 344.0);
    ui.painter().rect_stroke(sb, Rounding::same(2.0), Stroke::new(d.pn.s, col::LCD_LINE));
    if max_scroll > 0.0 {
        let h = sb.height() * (visible as f32 / lines.len() as f32).max(0.05);
        let t = app.load_scroll / max_scroll;
        let thumb = Rect::from_min_size(Pos2::new(sb.left() + 1.0, sb.top() + (sb.height() - h) * t), Vec2::new(sb.width() - 2.0, h));
        ui.painter().rect_filled(thumb, Rounding::same(2.0), col::BLUE_LT);
        let resp = ui.interact(sb, Id::new("loadsb"), Sense::click_and_drag());
        if resp.dragged() {
            app.load_scroll += resp.drag_delta().y / (sb.height() - h).max(1.0) * max_scroll;
        } else if resp.clicked() {
            // jump: centre the thumb on the click
            if let Some(pos) = resp.interact_pointer_pos() {
                let t = ((pos.y - sb.top() - h / 2.0) / (sb.height() - h).max(1.0)).clamp(0.0, 1.0);
                app.load_scroll = t * max_scroll;
            }
        }
    }
}

// ---------------------------------------------------------------------------
// EDIT
// ---------------------------------------------------------------------------

fn edit(app: &mut App, ui: &mut Ui, d: &D) {
    let Some(p) = cur_patch(app) else {
        d.label(ui, 30.0, 40.0, "Load a patch into the part first.");
        return;
    };
    // "Edit:" selector
    d.label_b(ui, 76.0, 11.0, "Edit:");
    let names: Vec<String> = std::iter::once("Patch Settings".to_string()).chain(p.elements.iter().map(|e| format!("[{}] {}", e.kind().letter(), e.name))).collect();
    let cur = app.edit_elem.map(|e| e + 1).unwrap_or(0);
    let id = Id::new("editsel");
    let (_, resp) = panel::value(ui, d.pn, id, d.r(106.0, 4.0, 266.0, 18.0), &names[cur.min(names.len() - 1)], false);
    if let Some(k) = panel::menu(ui, &resp, id.with("m"), &names, Some(cur)) {
        app.edit_elem = if k == 0 { None } else { Some(k - 1) };
    }
    // structure column
    structure(app, ui, d, &p);
    match app.edit_elem {
        None => patch_settings(app, ui, d, &p),
        Some(e) if e < p.elements.len() => {
            let el = &p.elements[e];
            if el.kind() == ElemType::Fx {
                fx_element(app, ui, d, e, el.fx_type());
            } else {
                match app.sub {
                    Sub::Easy => easy(app, ui, d, e),
                    Sub::Osc => osc(app, ui, d, e, el.kind()),
                    Sub::Mod => modpage(app, ui, d, e),
                }
                // tabs
                for (k, (s, name)) in [(Sub::Easy, "easy"), (Sub::Osc, "osc"), (Sub::Mod, "mod")].iter().enumerate() {
                    let r = d.r(60.0 + k as f32 * 60.0, 332.0, 116.0 + k as f32 * 60.0, 346.0);
                    let resp = ui.interact(r, Id::new(("subtab", k)), Sense::click());
                    let on = app.sub == *s;
                    ui.painter().rect_filled(r, Rounding::same(3.0), if on { col::BLUE } else { col::BLUE_LT });
                    ui.painter().text(r.center(), Align2::CENTER_CENTER, *name, d.pn.f(8.5), if on { Color32::WHITE } else { col::LCD_TEXT });
                    if resp.clicked() {
                        app.sub = *s;
                    }
                }
            }
        }
        _ => app.edit_elem = None,
    }
}

/// Patch structure: "P" plus one icon per element; FX icons break the chain
/// when they route straight to the output.
fn structure(app: &mut App, ui: &mut Ui, d: &D, p: &Patch) {
    let n = p.elements.len() + 1;
    let h = ((340.0 - 24.0) / n as f32).min(18.0);
    for k in 0..n {
        let y = 24.0 + k as f32 * h;
        let r = d.r(3.0, y, 25.0, y + h - 3.0);
        let resp = ui.interact(r, Id::new(("struct", k)), Sense::click());
        let (letter, sel, muted) = if k == 0 {
            ("P".to_string(), app.edit_elem.is_none(), false)
        } else {
            let e = &p.elements[k - 1];
            (e.kind().letter().to_string(), app.edit_elem == Some(k - 1), e.muted())
        };
        let painter = ui.painter();
        let bg = if sel { col::BLUE } else if muted { Color32::from_rgb(200, 200, 200) } else { Color32::WHITE };
        painter.rect_filled(r, Rounding::same(2.0), bg);
        painter.rect_stroke(r, Rounding::same(2.0), Stroke::new(d.pn.s, col::LCD_TEXT));
        painter.text(r.center(), Align2::CENTER_CENTER, letter, d.pn.f(if h < 12.0 { 7.0 } else { 8.5 }), if sel { Color32::WHITE } else if muted { Color32::GRAY } else { col::LCD_TEXT });
        if k > 0 && k < n - 1 {
            painter.line_segment([Pos2::new(r.center().x, r.bottom()), Pos2::new(r.center().x, r.bottom() + 3.0 * d.pn.s)], Stroke::new(d.pn.s, col::LCD_TEXT));
        }
        if resp.clicked() {
            app.edit_elem = if k == 0 { None } else { Some(k - 1) };
        }
        if k > 0 && resp.secondary_clicked() {
            let e = k - 1;
            app.edit(|q| q.elements[e].p[ep::MUTE] = if q.elements[e].muted() { 0.0 } else { 1.0 });
        }
        if k > 0 && resp.hovered() {
            app.show_hyper(format!("{} ({})\nRight-click: mute", p.elements[k - 1].name, p.elements[k - 1].kind().letter()), 0.3);
        }
    }
}

fn patch_settings(app: &mut App, ui: &mut Ui, d: &D, p: &Patch) {
    if app.step_edit {
        step_seq(app, ui, d, p);
        return;
    }
    let mono = strs(&["Off", "Default", "On"]);
    d.label(ui, 34.0, 32.0, "Mono Mode");
    penum(app, ui, d, pp::MONO, d.r(96.0, 25.0, 156.0, 39.0), &mono);
    d.label(ui, 34.0, 52.0, "Glide Mode");
    penum(app, ui, d, pp::GLIDE, d.r(96.0, 45.0, 156.0, 59.0), &mono);
    pk(app, ui, d, pp::GLIDE_TIME, 220.0, 46.0, "Glide Time", false);
    let bends: Vec<String> = (0..16).map(|k| format!("{k}")).collect();
    d.label(ui, 34.0, 76.0, "Bend Up");
    penum(app, ui, d, pp::PB_UP, d.r(96.0, 69.0, 136.0, 83.0), &bends);
    d.label(ui, 146.0, 76.0, "Down");
    penum(app, ui, d, pp::PB_DOWN, d.r(176.0, 69.0, 216.0, 83.0), &bends);
    // arpeggiator
    d.frame(ui, 32.0, 92.0, 268.0, 290.0);
    let on = p.p[pp::ARP_ON] >= 0.5;
    let r = d.r(40.0, 96.0, 150.0, 110.0);
    let (_, resp) = panel::value(ui, d.pn, Id::new("arpon"), r, if on { "Arpeggiator ON" } else { "Arpeggiator OFF" }, on);
    if resp.clicked() || resp.secondary_clicked() {
        app.edit(|q| q.p[pp::ARP_ON] = if on { 0.0 } else { 1.0 });
    }
    let latch = p.p[pp::ARP_LATCH] >= 0.5;
    let (_, lr) = panel::value(ui, d.pn, Id::new("arplatch"), d.r(160.0, 96.0, 200.0, 110.0), "LATCH", latch);
    if lr.clicked() || lr.secondary_clicked() {
        app.edit(|q| q.p[pp::ARP_LATCH] = if latch { 0.0 } else { 1.0 });
    }
    let (_, er) = panel::value(ui, d.pn, Id::new("arpedit"), d.r(210.0, 96.0, 262.0, 110.0), "Edit...", false);
    if er.clicked() {
        app.step_edit = true;
    }
    let rates: Vec<String> = arp::RATES.iter().map(|r| r.0.to_string()).collect();
    d.label(ui, 48.0, 128.0, "Rate");
    penum(app, ui, d, pp::ARP_RATE, d.r(96.0, 121.0, 156.0, 135.0), &rates);
    pk(app, ui, d, pp::ARP_GATE, 190.0, 128.0, "Gate", false);
    pk(app, ui, d, pp::ARP_SWING, 238.0, 128.0, "Swing", false);
    let modes: Vec<String> = arp::Mode::ALL.iter().map(|m| m.name().to_string()).collect();
    d.label(ui, 48.0, 158.0, "Mode");
    penum(app, ui, d, pp::ARP_MODE, d.r(96.0, 151.0, 156.0, 165.0), &modes);
    let oct: Vec<String> = (1..=4).map(|k| k.to_string()).collect();
    d.label(ui, 48.0, 182.0, "Octaves");
    penum(app, ui, d, pp::ARP_OCT, d.r(96.0, 175.0, 156.0, 189.0), &oct);
    let keys = strs(&["Chromatic", "Major", "Minor", "Dorian", "Pentatonic", "Blues"]);
    d.label(ui, 166.0, 182.0, "Key");
    penum(app, ui, d, pp::ARP_SCALE, d.r(190.0, 175.0, 262.0, 189.0), &keys);
    let steps: Vec<String> = (1..=16).map(|k| k.to_string()).collect();
    d.label(ui, 48.0, 206.0, "Steps");
    penum(app, ui, d, pp::ARP_STEPS, d.r(96.0, 199.0, 156.0, 213.0), &steps);
    d.label(ui, 48.0, 230.0, "Sync");
    penum(app, ui, d, pp::ARP_SYNC, d.r(96.0, 223.0, 156.0, 237.0), &strs(&arp::SYNC));
    d.label(ui, 48.0, 254.0, "Phrase");
    // phrase: bank + index (patch params 26/27, value·128)
    let mut items = Vec::new();
    let mut map = Vec::new();
    for (num, bname, names) in &app.lib.phrase_banks {
        items.push(format!("--{bname}"));
        map.push(None);
        for (k, n) in names.iter().enumerate() {
            items.push(format!("{bname}: {n}"));
            map.push(Some((*num, k as u32)));
        }
    }
    let (pb, pi) = ((p.p[pp::PHRASE_BANK] * 128.0) as u32, (p.p[pp::PHRASE_PROG] * 128.0) as u32);
    let cur_name = app.lib.phrase_banks.iter().find(|b| b.0 == pb).or(app.lib.phrase_banks.first()).and_then(|b| b.2.get(pi as usize).map(|n| format!("{}: {n}", b.1))).unwrap_or_else(|| "(no phrases: load content)".into());
    let pid = Id::new("phrasesel");
    let (_, presp) = panel::value(ui, d.pn, pid, d.r(96.0, 247.0, 262.0, 261.0), &cur_name, false);
    if let Some(k) = panel::menu(ui, &presp, pid.with("m"), &items, map.iter().position(|m| *m == Some((pb, pi)))) {
        if let Some((b, i)) = map[k] {
            app.edit(|q| {
                q.p[pp::PHRASE_BANK] = (b as f32 + 0.5) / 128.0;
                q.p[pp::PHRASE_PROG] = (i as f32 + 0.5) / 128.0;
                q.p[pp::ARP_MODE] = enum_val(6, 10);
                q.p[pp::ARP_ON] = 1.0;
            });
        }
    }
    d.label(ui, 34.0, 304.0, "Patch Programmed By:");
    let au = p.author.clone();
    ui.painter().rect_filled(d.r(34.0, 312.0, 266.0, 326.0), Rounding::same(2.0), Color32::from_rgb(240, 245, 252));
    d.label(ui, 38.0, 319.0, &au);
}

fn step_seq(app: &mut App, ui: &mut Ui, d: &D, p: &Patch) {
    d.label_b(ui, 34.0, 32.0, "Step Sequencer");
    let (_, back) = panel::value(ui, d.pn, Id::new("stepback"), d.r(206.0, 25.0, 262.0, 39.0), "Done", false);
    if back.clicked() {
        app.step_edit = false;
    }
    for (k, h) in ["Note", "Vel", "Gate", "Seq 1", "Seq 2"].iter().enumerate() {
        d.label_c(ui, 70.0 + k as f32 * 44.0, 50.0, h);
    }
    for s in 0..16 {
        let y = 58.0 + s as f32 * 17.0;
        d.label(ui, 32.0, y + 7.0, &format!("{:2}", s + 1));
        for k in 0..5 {
            let i = pp::STEP + 5 * s + k;
            let v = p.p[i];
            let x = 50.0 + k as f32 * 44.0;
            let txt = match k {
                0 => format!("{:+}", ((v - 0.5) * 48.0).round() as i32),
                2 => format!("{:.0}%", v * 200.0),
                _ => format!("{:.0}", if k >= 3 { bipolar(v) * 100.0 } else { v * 127.0 }),
            };
            let id = Id::new(("step", s, k));
            let (act, _) = panel::value(ui, d.pn, id, d.r(x, y, x + 40.0, y + 14.0), &txt, false);
            let step = if k == 0 { 1.0 / 48.0 } else { 1.0 / 127.0 };
            let nv = match act {
                Act::Step(dd) => Some((v + dd as f32 * step).clamp(0.0, 1.0)),
                Act::Drag(dd) => Some((v + dd * step * 0.5).clamp(0.0, 1.0)),
                Act::Default => Some(if k == 1 { 0.75 } else { 0.5 }),
                _ => None,
            };
            if let Some(nv) = nv {
                app.edit(|q| q.p[i] = nv);
            }
        }
    }
}

fn easy(app: &mut App, ui: &mut Ui, d: &D, e: usize) {
    let Some(p) = cur_patch(app) else { return };
    let el = &p.elements[e];
    ek(app, ui, d, e, ep::OCTAVE, 50.0, 40.0, "Octave", true, |v| format!("{:+}", enum_idx(v, 9) as i32 - 4));
    ek(app, ui, d, e, ep::SEMI, 98.0, 40.0, "Semi", true, semis24);
    ek(app, ui, d, e, ep::CENT, 146.0, 40.0, "Cent", true, cents);
    d.label(ui, 196.0, 30.0, "Max");
    d.label(ui, 190.0, 40.0, "Voices:");
    let mv = (el.p[ep::MAX_VOICES] * 64.0).round().max(1.0) as i32;
    let id = Id::new(("maxv", e));
    let (act, _) = panel::value(ui, d.pn, id, d.r(190.0, 48.0, 230.0, 62.0), &format!("{mv}"), false);
    if let Some(n) = act_int(app, id, act, mv, 1, 64, 64, 6.0) {
        app.edit(|q| q.elements[e].p[ep::MAX_VOICES] = n as f32 / 64.0);
    }
    // filter
    d.frame(ui, 32.0, 72.0, 268.0, 196.0);
    let fm: Vec<String> = FilterMode::ALL.iter().map(|m| m.name().to_string()).collect();
    eenum(app, ui, d, e, ep::FILTER, d.r(40.0, 66.0, 140.0, 80.0), &fm);
    ek(app, ui, d, e, ep::CUTOFF, 52.0, 112.0, "Cutoff", false, cutoff_text);
    ek(app, ui, d, e, ep::RESO, 100.0, 112.0, "Reso", false, pct);
    ek(app, ui, d, e, ep::FENV_DEPTH, 148.0, 112.0, "Env", true, bpct);
    es(app, ui, d, e, ep::VEL_CUTOFF, 38.0, 170.0, 28.0, true);
    d.label_c(ui, 52.0, 182.0, "Vel");
    es(app, ui, d, e, ep::KEY_CUTOFF, 86.0, 170.0, 28.0, false);
    d.label_c(ui, 100.0, 182.0, "Key");
    es(app, ui, d, e, ep::FENV_VEL, 134.0, 170.0, 28.0, true);
    d.label_c(ui, 148.0, 182.0, "Vel");
    for (k, l) in ["A", "D", "S", "F", "R"].iter().enumerate() {
        vslider(app, ui, d, e, ep::FENV + k, 184.0 + k as f32 * 16.0, 98.0, 176.0, l);
    }
    // amp
    d.frame(ui, 32.0, 204.0, 268.0, 326.0);
    ek(app, ui, d, e, ep::LEVEL, 52.0, 236.0, "Level", false, level_text);
    ek(app, ui, d, e, ep::PAN, 100.0, 236.0, "Pan", true, bpct);
    ek(app, ui, d, e, ep::AENV_VEL, 148.0, 236.0, "Vel Sens", false, pct);
    for (k, l) in ["A", "D", "S", "F", "R"].iter().enumerate() {
        vslider(app, ui, d, e, ep::AENV + k, 184.0 + k as f32 * 16.0, 226.0, 306.0, l);
    }
}

fn osc(app: &mut App, ui: &mut Ui, d: &D, e: usize, kind: ElemType) {
    let Some(p) = cur_patch(app) else { return };
    let el = p.elements[e].clone();
    match kind {
        ElemType::Sample => {
            d.label_b(ui, 34.0, 32.0, "Sample:");
            let bank = (el.p[ep::S_BANK] * 128.0) as u32;
            let wave = (el.p[ep::S_WAVE] * 128.0) as u32;
            let name = app.lib.sample_name(bank, wave);
            let id = Id::new(("samplesel", e));
            let (act, resp) = panel::value(ui, d.pn, id, d.r(80.0, 25.0, 266.0, 39.0), &name, false);
            // menu of all sample banks + samples (names from the content folder)
            let mut items = Vec::new();
            let mut map = Vec::new();
            for (n, bname, waves) in &app.lib.sample_banks {
                items.push(format!("--{bname}"));
                map.push((u32::MAX, 0));
                for (w, wn) in waves.iter().enumerate().take(128) {
                    items.push(format!("{n:03} {bname}: {wn}"));
                    map.push((*n, w as u32));
                }
            }
            if items.is_empty() {
                for b in 0..124u32 {
                    items.push(format!("Bank {b:03}"));
                    map.push((b, 0));
                }
            }
            if let Some(k) = panel::menu(ui, &resp, id.with("m"), &items, None) {
                let (b, w) = map[k];
                if b != u32::MAX {
                    app.edit(|q| {
                        q.elements[e].p[ep::S_BANK] = (b as f32 + 0.5) / 128.0;
                        q.elements[e].p[ep::S_WAVE] = (w as f32 + 0.5) / 128.0;
                    });
                }
            }
            if let Act::Step(dd) = act {
                let nw = (wave as i32 + dd).clamp(0, 127) as f32;
                app.edit(|q| q.elements[e].p[ep::S_WAVE] = (nw + 0.5) / 128.0);
            }
            let (gm, progs) = hs2_core::sf2::gm_for_bank(bank);
            let plays = app.map_describe(bank).unwrap_or_else(|| format!("{} {}", if gm == 128 { "GM drum" } else { "GM prog" }, progs[wave as usize % progs.len()] + if gm == 128 { 0 } else { 1 }));
            d.label(ui, 34.0, 48.0, &format!("plays: {}", short(&plays, 40)));
            let (_, mr) = panel::value(ui, d.pn, Id::new(("mapbtn", e)), d.r(206.0, 41.0, 266.0, 55.0), "Map…", false);
            if mr.clicked() {
                app.map_open = true;
            }
            d.frame(ui, 32.0, 56.0, 148.0, 150.0);
            d.label(ui, 36.0, 64.0, "Startpoint");
            ek(app, ui, d, e, ep::START, 58.0, 92.0, "Offset", false, pct);
            let mono = el.p[ep::S_MONO] >= 0.5;
            let (_, mr) = panel::value(ui, d.pn, Id::new(("smono", e)), d.r(96.0, 82.0, 136.0, 96.0), "Mono", mono);
            if mr.clicked() {
                app.edit(|q| q.elements[e].p[ep::S_MONO] = if mono { 0.0 } else { 1.0 });
            }
            es(app, ui, d, e, ep::VEL_START, 40.0, 130.0, 28.0, true);
            d.label_c(ui, 54.0, 142.0, "Vel");
            es(app, ui, d, e, ep::RND_START, 96.0, 130.0, 28.0, false);
            d.label_c(ui, 110.0, 142.0, "Rand");
            d.frame(ui, 152.0, 56.0, 268.0, 150.0);
            d.label(ui, 156.0, 64.0, "Formant / Zone Shift");
            ek(app, ui, d, e, ep::S_FORMANT, 180.0, 92.0, "Offset", true, bpct);
            es(app, ui, d, e, ep::S_FORMANT + 1, 160.0, 130.0, 28.0, true);
            d.label_c(ui, 174.0, 142.0, "Vel");
            es(app, ui, d, e, ep::S_FORMANT + 2, 216.0, 130.0, 28.0, false);
            d.label_c(ui, 230.0, 142.0, "Rand");
            d.frame(ui, 32.0, 156.0, 268.0, 230.0);
            d.label(ui, 36.0, 164.0, "Velocity to Layer");
            ek(app, ui, d, e, 33, 66.0, 192.0, "Offset", true, bpct);
            ek(app, ui, d, e, 34, 146.0, 192.0, "Scale", false, pct);
            ek(app, ui, d, e, 35, 226.0, 192.0, "Random", false, pct);
            d.frame(ui, 32.0, 236.0, 268.0, 326.0);
            d.label(ui, 36.0, 244.0, "Mono Legato Offsets");
            ek(app, ui, d, e, 28, 66.0, 274.0, "Sample Start", false, pct);
            ek(app, ui, d, e, 29, 146.0, 274.0, "Amp Env Att", false, pct);
            ek(app, ui, d, e, 30, 226.0, 274.0, "Filt Env Att", false, pct);
            if app.sf_name.is_none() {
                d.label(ui, 34.0, 318.0, "No SoundFont: built-in substitute sound.");
            }
        }
        ElemType::Analog => {
            d.label_b(ui, 34.0, 32.0, "Wavetable:");
            let wts: Vec<String> = wavetable::NAMES.iter().map(|s| s.to_string()).chain((wavetable::NAMES.len()..64).map(|k| format!("Table {k}"))).collect();
            eenum(app, ui, d, e, ep::A_WAVETABLE, d.r(94.0, 25.0, 266.0, 39.0), &wts);
            let modes = strs(&["Saw Sync", "Square Sync", "Pulse Width", "Saw Pitch", "Square Pitch", "Wavetable"]);
            for (k, (mode, mix, semi, cent, wave)) in [(ep::A_OSC1_WAVE, ep::A_OSC1_MIX, ep::A_SEMI, ep::A_CENT, ep::WAVE), (ep::A_OSC2_WAVE, ep::A_OSC2_MIX, ep::A_OSC2_SEMI, ep::A_OSC2_CENT, ep::OSC2_WAVE_MOD)].iter().enumerate() {
                let y = 48.0 + k as f32 * 88.0;
                d.frame(ui, 32.0, y, 268.0, y + 80.0);
                d.label_b(ui, 38.0, y + 10.0, &format!("Analog Osc {}:", k + 1));
                eenum(app, ui, d, e, *mode, d.r(110.0, y + 3.0, 200.0, y + 17.0), &modes);
                ek(app, ui, d, e, *mix, 58.0, y + 44.0, "Mix", false, pct);
                ek(app, ui, d, e, *semi, 112.0, y + 44.0, "Semi", true, semis24);
                ek(app, ui, d, e, *cent, 166.0, y + 44.0, "Cent", true, cents);
                ek(app, ui, d, e, *wave, 220.0, y + 44.0, "Wave", false, pct);
            }
            let y = 224.0;
            d.frame(ui, 32.0, y, 214.0, y + 100.0);
            d.label_b(ui, 38.0, y + 10.0, "Analog Osc 3:");
            eenum(app, ui, d, e, ep::A_OSC3_WAVE, d.r(110.0, y + 3.0, 190.0, y + 17.0), &strs(&["Saw", "Square", "Triangle"]));
            ek(app, ui, d, e, ep::A_OSC3_MIX, 58.0, y + 50.0, "Mix", false, pct);
            ek(app, ui, d, e, ep::A_SUB, 112.0, y + 50.0, "Sub", false, pct);
            ek(app, ui, d, e, ep::A_NOISE, 166.0, y + 50.0, "Noise", false, pct);
            for (k, (i, l)) in [(ep::A_HPF, "HPF"), (ep::A_PW_MIN, "PWM Limit"), (ep::A_ALIAS, "WT Alias")].iter().enumerate() {
                let yy = y + 12.0 + k as f32 * 30.0;
                es(app, ui, d, e, *i, 222.0, yy, 40.0, false);
                d.label_c(ui, 242.0, yy + 11.0, l);
            }
        }
        ElemType::Wavetable => {
            d.label_b(ui, 34.0, 32.0, "Wavetable:");
            let wts: Vec<String> = wavetable::NAMES.iter().map(|s| s.to_string()).chain((wavetable::NAMES.len()..64).map(|k| format!("Table {k}"))).collect();
            eenum(app, ui, d, e, ep::W_INDEX, d.r(94.0, 25.0, 266.0, 39.0), &wts);
            d.frame(ui, 32.0, 48.0, 268.0, 150.0);
            ek(app, ui, d, e, ep::WAVE, 62.0, 78.0, "Index", false, |v| format!("wave {:.0}", v * 63.0));
            ek(app, ui, d, e, ep::W_SEMI, 112.0, 78.0, "Semi", true, semis24);
            ek(app, ui, d, e, ep::W_CENT, 162.0, 78.0, "Cent", true, cents);
            ek(app, ui, d, e, ep::W_ALIAS, 212.0, 78.0, "Alias", false, pct);
            es(app, ui, d, e, ep::KEY_WAVE, 46.0, 118.0, 30.0, true);
            d.label_c(ui, 61.0, 130.0, "Key");
            es(app, ui, d, e, ep::VEL_WAVE, 94.0, 118.0, 30.0, true);
            d.label_c(ui, 109.0, 130.0, "Vel");
            es(app, ui, d, e, ep::RND_WAVE, 142.0, 118.0, 30.0, false);
            d.label_c(ui, 157.0, 130.0, "Rand");
            let sync = el.p[ep::W_SYNC] >= 0.5;
            let (_, sr) = panel::value(ui, d.pn, Id::new(("wsync", e)), d.r(196.0, 112.0, 240.0, 126.0), "Sync", sync);
            if sr.clicked() {
                app.edit(|q| q.elements[e].p[ep::W_SYNC] = if sync { 0.0 } else { 1.0 });
            }
            d.frame(ui, 32.0, 160.0, 160.0, 240.0);
            d.label_b(ui, 38.0, 170.0, "Osc 2:");
            eenum(app, ui, d, e, ep::W_OSC2, d.r(80.0, 163.0, 156.0, 177.0), &strs(&["Saw", "Square", "Triangle", "Sine"]));
            ek(app, ui, d, e, ep::W_OSC2_MIX, 66.0, 206.0, "Mix", false, pct);
        }
        ElemType::Fm => {
            for (k, (depth, coarse, fine, extra, extra_l, vel)) in [
                (ep::F_M1_DEPTH, ep::F_M1_COARSE, ep::F_M1_FINE, ep::F_THRU, "Thru Mix", ep::F_M1_VEL),
                (ep::F_M2_DEPTH, ep::F_M2_COARSE, ep::F_M2_FINE, ep::F_M2_KEY, "Keytrack", ep::F_M2_VEL),
            ]
            .iter()
            .enumerate()
            {
                let y = 26.0 + k as f32 * 98.0;
                d.frame(ui, 32.0, y, 268.0, y + 92.0);
                d.label_b(ui, 38.0, y + 9.0, &format!("Modulator {}", k + 1));
                ek(app, ui, d, e, *depth, 58.0, y + 38.0, "Depth", false, pct);
                ek(app, ui, d, e, *coarse, 112.0, y + 38.0, "Coarse", false, |v| {
                    const R: [f32; 18] = [0.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 1.414];
                    format!("ratio {}", R[enum_idx(v, 18)])
                });
                ek(app, ui, d, e, *fine, 166.0, y + 38.0, "Fine", true, bpct);
                ek(app, ui, d, e, *extra, 220.0, y + 38.0, extra_l, false, pct);
                es(app, ui, d, e, *vel, 40.0, y + 74.0, 30.0, true);
                d.label_c(ui, 55.0, y + 85.0, "Vel Sens");
                es(app, ui, d, e, *vel + 1, 100.0, y + 74.0, 26.0, true);
                d.label_c(ui, 113.0, y + 85.0, "Lo");
                es(app, ui, d, e, *vel + 2, 146.0, y + 74.0, 30.0, false);
                d.label_c(ui, 161.0, y + 85.0, "Breakpoint");
                es(app, ui, d, e, *vel + 3, 200.0, y + 74.0, 26.0, true);
                d.label_c(ui, 213.0, y + 85.0, "Hi");
            }
            let y = 224.0;
            d.frame(ui, 32.0, y, 268.0, y + 100.0);
            d.label_b(ui, 38.0, y + 9.0, "Carrier");
            ek(app, ui, d, e, ep::WAVE, 58.0, y + 40.0, "Master Depth", false, pct);
            es(app, ui, d, e, ep::F_ENV_LEVEL, 100.0, y + 30.0, 30.0, true);
            d.label_c(ui, 115.0, y + 41.0, "Level");
            es(app, ui, d, e, ep::F_ENV_RATE, 150.0, y + 30.0, 30.0, false);
            d.label_c(ui, 165.0, y + 41.0, "Rate");
            es(app, ui, d, e, 36, 200.0, y + 30.0, 30.0, true);
            d.label_c(ui, 215.0, y + 41.0, "Vel");
            let lf = el.p[ep::F_LF] >= 0.5;
            let (_, lr) = panel::value(ui, d.pn, Id::new(("flf", e)), d.r(206.0, y + 66.0, 240.0, y + 80.0), "LF", lf);
            if lr.clicked() {
                app.edit(|q| q.elements[e].p[ep::F_LF] = if lf { 0.0 } else { 1.0 });
            }
        }
        ElemType::Drawbar => {
            d.label_b(ui, 34.0, 32.0, "Drawbars");
            for k in 0..9 {
                let x = 52.0 + k as f32 * 24.0;
                vslider(app, ui, d, e, ep::D_BARS + k, x, 48.0, 180.0, ["16", "5⅓", "8", "4", "2⅔", "2", "1⅗", "1⅓", "1"][k]);
            }
            let perc = el.p[ep::D_PERC] >= 0.5;
            let (_, pr) = panel::value(ui, d.pn, Id::new(("dperc", e)), d.r(40.0, 206.0, 120.0, 220.0), if perc { "Percussion ON" } else { "Percussion OFF" }, perc);
            if pr.clicked() {
                app.edit(|q| q.elements[e].p[ep::D_PERC] = if perc { 0.0 } else { 1.0 });
            }
            let h3 = el.p[ep::D_PERC_HARM] >= 0.5;
            let (_, hr) = panel::value(ui, d.pn, Id::new(("dharm", e)), d.r(130.0, 206.0, 190.0, 220.0), if h3 { "3rd" } else { "2nd" }, false);
            if hr.clicked() {
                app.edit(|q| q.elements[e].p[ep::D_PERC_HARM] = if h3 { 0.0 } else { 1.0 });
            }
            ek(app, ui, d, e, ep::D_PERC_LEVEL, 60.0, 254.0, "Perc Level", false, pct);
            ek(app, ui, d, e, ep::D_PERC_DECAY, 120.0, 254.0, "Perc Decay", false, pct);
            ek(app, ui, d, e, ep::D_CLICK, 180.0, 254.0, "Key Click", false, pct);
        }
        ElemType::Fx => {}
    }
}

fn modpage(app: &mut App, ui: &mut Ui, d: &D, e: usize) {
    let tabs = [
        (ModView::Matrix, "MOD"),
        (ModView::Lfo, "LFO"),
        (ModView::Me, "ME"),
        (ModView::Fe, "FE"),
        (ModView::Ae, "AE"),
        (ModView::Osc, "OSC"),
        (ModView::Pit, "PIT"),
        (ModView::Trig, "TRIG"),
        (ModView::Misc, "MISC"),
        (ModView::Knob, "◔"),
    ];
    for (k, (v, name)) in tabs.iter().enumerate() {
        let r = d.r(30.0 + k as f32 * 23.8, 24.0, 52.0 + k as f32 * 23.8, 36.0);
        let resp = ui.interact(r, Id::new(("modtab", k)), Sense::click());
        let on = app.mod_view == *v;
        ui.painter().rect_filled(r, Rounding::same(1.5), if on { col::LCD_TEXT } else { Color32::WHITE });
        ui.painter().rect_stroke(r, Rounding::same(1.5), Stroke::new(d.pn.s * 0.7, col::LCD_TEXT));
        ui.painter().text(r.center(), Align2::CENTER_CENTER, *name, d.pn.f(7.5), if on { Color32::WHITE } else { col::LCD_TEXT });
        if resp.clicked() {
            app.mod_view = *v;
        }
    }
    let Some(p) = cur_patch(app) else { return };
    let el = p.elements[e].clone();
    match app.mod_view {
        ModView::Matrix => {
            d.label_c(ui, 70.0, 46.0, "Source");
            d.label_c(ui, 148.0, 46.0, "Modify");
            d.label_c(ui, 226.0, 46.0, "Dest.");
            let rows = 7;
            let srcs: Vec<String> = (0..44).map(params::source_name).collect();
            let mods: Vec<String> = (0..57).map(params::modify_name).collect();
            let dests: Vec<String> = params::DESTS.iter().map(|d| d.0.to_string()).collect();
            for k in 0..rows {
                let r = app.matrix_scroll + k;
                if r >= ep::ROWS {
                    break;
                }
                let o = ep::MATRIX + 5 * r;
                let y = 54.0 + k as f32 * 38.0;
                d.label(ui, 30.0, y + 7.0, &format!("{}", r + 1));
                // source
                let si = enum_idx(el.p[o], 72);
                let id = Id::new(("msrc", e, r));
                let (_, resp) = panel::value(ui, d.pn, id, d.r(38.0, y, 104.0, y + 14.0), &params::source_name(si), false);
                if let Some(c) = panel::menu(ui, &resp, id.with("m"), &srcs, Some(si)) {
                    app.edit(|q| q.elements[e].p[o] = if c == 0 { 0.0 } else { enum_val(c, 72) });
                }
                // modify
                let mi = enum_idx(el.p[o + 1], 72);
                let id = Id::new(("mmod", e, r));
                let (_, resp) = panel::value(ui, d.pn, id, d.r(114.0, y, 182.0, y + 14.0), &params::modify_name(mi), false);
                if let Some(c) = panel::menu(ui, &resp, id.with("m"), &mods, Some(mi)) {
                    app.edit(|q| q.elements[e].p[o + 1] = if c == 0 { 0.0 } else { enum_val(c, 72) });
                }
                es(app, ui, d, e, o + 2, 114.0, y + 24.0, 68.0, true);
                // dest
                let id = Id::new(("mdst", e, r));
                let dn = params::dest_name(el.p[o + 3]);
                let (_, resp) = panel::value(ui, d.pn, id, d.r(192.0, y, 266.0, y + 14.0), dn, false);
                let cur = params::DESTS.iter().position(|x| x.0 == dn);
                if let Some(c) = panel::menu(ui, &resp, id.with("m"), &dests, cur) {
                    app.edit(|q| q.elements[e].p[o + 3] = params::DESTS[c].1 / 128.0);
                }
                es(app, ui, d, e, o + 4, 192.0, y + 24.0, 74.0, true);
            }
            // scroll arrows
            for (k, dir) in [-1i32, 1].iter().enumerate() {
                let r = d.r(230.0 + k as f32 * 18.0, 320.0, 246.0 + k as f32 * 18.0, 330.0);
                let resp = ui.interact(r, Id::new(("mscroll", k)), Sense::click());
                let c = d.pn.inv(r.center());
                let (cx, cy) = (c.x - X0, c.y - Y0);
                let pts = if *dir < 0 { vec![d.p(cx - 4.0, cy + 3.0), d.p(cx + 4.0, cy + 3.0), d.p(cx, cy - 3.0)] } else { vec![d.p(cx - 4.0, cy - 3.0), d.p(cx + 4.0, cy - 3.0), d.p(cx, cy + 3.0)] };
                ui.painter().add(egui::Shape::convex_polygon(pts, col::LCD_TEXT, Stroke::NONE));
                d.label(ui, 160.0, 325.0, &format!("rows {}-{}", app.matrix_scroll + 1, app.matrix_scroll + 7));
                let dir = dir;
                if resp.clicked() {
                    app.matrix_scroll = (app.matrix_scroll as i32 + dir * 7).clamp(0, (ep::ROWS - 7) as i32) as usize;
                }
            }
        }
        ModView::Lfo => {
            d.label_b(ui, 34.0, 50.0, "LFO 3 (polyphonic)");
            let waves: Vec<String> = LfoWave::ALL.iter().map(|w| w.name().to_string()).collect();
            d.label(ui, 34.0, 72.0, "Wave");
            eenum(app, ui, d, e, ep::LFO3_WAVE, d.r(90.0, 65.0, 190.0, 79.0), &waves);
            let syncs: Vec<String> = LfoSync::ALL.iter().map(|w| w.name().to_string()).collect();
            d.label(ui, 34.0, 96.0, "Sync");
            eenum(app, ui, d, e, ep::LFO3_SYNC, d.r(90.0, 89.0, 190.0, 103.0), &syncs);
            ek(app, ui, d, e, ep::LFO3_RATE, 70.0, 136.0, "Rate", false, |v| format!("{:.2} Hz", hs2_core::patch::lfo_hz(v)));
            ek(app, ui, d, e, ep::LFO3_PHASE, 130.0, 136.0, "Phase", false, |v| if v > 0.98 { "random".into() } else { format!("{:.0}°", v * 360.0) });
            // patch LFOs 1/2
            d.label_b(ui, 34.0, 176.0, "LFO 1 / LFO 2 (patch)");
            let lw: Vec<String> = LfoWave::ALL.iter().map(|w| w.name().to_string()).collect();
            let ls: Vec<String> = LfoSync::ALL.iter().map(|w| w.name().to_string()).collect();
            for (k, base) in [pp::LFO1, pp::LFO2].iter().enumerate() {
                let y = 192.0 + k as f32 * 64.0;
                d.label(ui, 34.0, y + 7.0, &format!("LFO {}", k + 1));
                penum(app, ui, d, *base, d.r(70.0, y, 150.0, y + 14.0), &lw);
                penum(app, ui, d, base + 1, d.r(160.0, y, 250.0, y + 14.0), &ls);
                pk(app, ui, d, base + 2, 90.0, y + 38.0, "Rate", false);
                pk(app, ui, d, base + 3, 150.0, y + 38.0, "Phase", false);
            }
        }
        ModView::Me => {
            d.label_b(ui, 34.0, 50.0, "Mod Envelope");
            for (k, l) in ["L1", "T1", "L2", "T2", "L3", "T3", "S", "R", "L5"].iter().enumerate() {
                let x = 46.0 + k as f32 * 24.0;
                let bip = l.starts_with('L') || *l == "S";
                if bip {
                    ek(app, ui, d, e, ep::MENV + k, x, 90.0, l, true, bpct);
                } else {
                    ek(app, ui, d, e, ep::MENV + k, x, 140.0, l, false, |v| secs(time_s(v)));
                }
            }
            ek(app, ui, d, e, ep::TEMPO_ENV, 60.0, 210.0, "Tempo>Env", false, pct);
            d.label(ui, 34.0, 250.0, "Envelope Mode");
            eenum(app, ui, d, e, ep::ENV_MODE, d.r(120.0, 243.0, 230.0, 257.0), &strs(&["Digital (Restart)", "Analog (Resume)"]));
        }
        ModView::Fe | ModView::Ae => {
            let fe = app.mod_view == ModView::Fe;
            let base = if fe { ep::FENV } else { ep::AENV };
            d.label_b(ui, 34.0, 50.0, if fe { "Filter Envelope" } else { "Amp Envelope" });
            for (k, l) in ["A", "D", "S", "F", "R"].iter().enumerate() {
                vslider(app, ui, d, e, base + k, 60.0 + k as f32 * 22.0, 62.0, 150.0, l);
            }
            if fe {
                ek(app, ui, d, e, ep::FENV_VEL, 60.0, 196.0, "Vel>Level", true, bpct);
                ek(app, ui, d, e, ep::FENV_VEL_ATT, 110.0, 196.0, "Vel>Attack", true, bpct);
                ek(app, ui, d, e, ep::FENV_VEL_DEC, 160.0, 196.0, "Vel>Decay", true, bpct);
                ek(app, ui, d, e, ep::FENV_KEY, 210.0, 196.0, "Dec Keytrk", true, bpct);
                ek(app, ui, d, e, ep::RND_CUTOFF, 60.0, 256.0, "Rnd Cutoff", false, pct);
                ek(app, ui, d, e, ep::VEL_RESO, 110.0, 256.0, "Vel>Reso", true, bpct);
            } else {
                ek(app, ui, d, e, ep::AENV_VEL, 60.0, 196.0, "Vel>Level", false, pct);
                ek(app, ui, d, e, ep::AENV_VEL_ATT, 110.0, 196.0, "Vel>Attack", true, bpct);
                ek(app, ui, d, e, ep::AENV_VEL_DEC, 160.0, 196.0, "Vel>Decay", true, bpct);
                ek(app, ui, d, e, ep::DECAY_KEY, 210.0, 196.0, "Dec Keytrk", true, bpct);
                ek(app, ui, d, e, ep::RELEASE_KEY, 60.0, 256.0, "Rel Keytrk", true, bpct);
                ek(app, ui, d, e, ep::CLICK, 110.0, 256.0, "Click", false, pct);
                ek(app, ui, d, e, ep::MUTE_FADE, 160.0, 256.0, "Mute Fade", false, |v| secs(time_s(v)));
            }
        }
        ModView::Osc => {
            d.label_b(ui, 34.0, 50.0, "Oscillator");
            ek(app, ui, d, e, ep::KEY_WAVE, 60.0, 90.0, "Key>Wave", true, bpct);
            ek(app, ui, d, e, ep::VEL_WAVE, 120.0, 90.0, "Vel>Wave", true, bpct);
            ek(app, ui, d, e, ep::RND_WAVE, 180.0, 90.0, "Rnd Wave", false, pct);
            ek(app, ui, d, e, ep::START, 60.0, 160.0, "Startpoint", false, pct);
            ek(app, ui, d, e, ep::VEL_START, 120.0, 160.0, "Vel>Start", true, bpct);
            ek(app, ui, d, e, ep::RND_START, 180.0, 160.0, "Rnd Start", false, pct);
        }
        ModView::Pit => {
            d.label_b(ui, 34.0, 50.0, "Pitch / Glide");
            ek(app, ui, d, e, ep::OCTAVE, 50.0, 84.0, "Octave", true, |v| format!("{:+}", enum_idx(v, 9) as i32 - 4));
            ek(app, ui, d, e, ep::SEMI, 98.0, 84.0, "Semitones", true, semis24);
            ek(app, ui, d, e, ep::CENT, 146.0, 84.0, "Cents", true, cents);
            ek(app, ui, d, e, ep::PITCH_KEY, 194.0, 84.0, "Keytrack", false, |v| format!("{:.0}%", v / 0.75 * 100.0));
            ek(app, ui, d, e, ep::RND_PITCH, 242.0, 84.0, "Rnd Pitch", false, pct);
            d.label(ui, 34.0, 130.0, "Glide Mode");
            eenum(app, ui, d, e, ep::GLIDE_MODE, d.r(110.0, 123.0, 200.0, 137.0), &strs(&["Off", "Bend Only", "Fingered", "On"]));
            d.label(ui, 34.0, 150.0, "Glide Type");
            eenum(app, ui, d, e, ep::GLIDE_TYPE, d.r(110.0, 143.0, 200.0, 157.0), &strs(&["Analog", "Digital"]));
            ek(app, ui, d, e, ep::GLIDE_TIME, 60.0, 196.0, "Glide Time", false, |v| secs(time_s(v)));
            ek(app, ui, d, e, ep::ATTACK_BEND, 120.0, 196.0, "Attack Bend", true, semis24);
            ek(app, ui, d, e, ep::VEL_BEND, 180.0, 196.0, "Vel>Bend", true, bpct);
            ek(app, ui, d, e, ep::RELEASE_BEND, 240.0, 196.0, "Rel Bend", true, semis24);
            d.label(ui, 34.0, 250.0, "Patch Pitch Bend");
            eenum(app, ui, d, e, ep::PATCH_PB, d.r(130.0, 243.0, 200.0, 257.0), &strs(&["Ignore", "Receive"]));
        }
        ModView::Trig => {
            d.label_b(ui, 34.0, 50.0, "Trigger");
            let notes: Vec<String> = (0..128).map(note_name).collect();
            for (k, (i, l)) in [(ep::KEY_LO, "Keyrange Low"), (ep::KEY_HI, "Keyrange High")].iter().enumerate() {
                let y = 64.0 + k as f32 * 20.0;
                d.label(ui, 34.0, y + 7.0, l);
                let v = (el.p[*i] * 127.0).round() as i32;
                let id = Id::new(("trig", e, *i));
                let (act, _) = panel::value(ui, d.pn, id, d.r(130.0, y, 180.0, y + 14.0), &notes[v.clamp(0, 127) as usize], false);
                if let Some(n) = act_int(app, id, act, v, 0, 127, if k == 0 { 0 } else { 127 }, 4.0) {
                    let i = *i;
                    app.edit(|q| q.elements[e].p[i] = n as f32 / 127.0);
                }
            }
            for (k, (i, l)) in [(ep::VEL_LO, "Velocity Low"), (ep::VEL_HI, "Velocity High")].iter().enumerate() {
                let y = 108.0 + k as f32 * 20.0;
                d.label(ui, 34.0, y + 7.0, l);
                let v = (el.p[*i] * 127.0).round() as i32;
                let id = Id::new(("trig", e, *i));
                let (act, _) = panel::value(ui, d.pn, id, d.r(130.0, y, 180.0, y + 14.0), &format!("{v}"), false);
                if let Some(n) = act_int(app, id, act, v, 0, 127, if k == 0 { 0 } else { 127 }, 3.0) {
                    let i = *i;
                    app.edit(|q| q.elements[e].p[i] = n as f32 / 127.0);
                }
            }
            d.label(ui, 34.0, 162.0, "Trigger Condition");
            eenum(app, ui, d, e, ep::TRIGGER, d.r(130.0, 155.0, 266.0, 169.0), &strs(&["Always", "Mod Wheel Up", "Mod Wheel Down", "Sustain Up", "Sustain Down", "Odd Patch Note", "Even Patch Note", "Odd Elem Note", "Even Elem Note"]));
            d.label(ui, 34.0, 182.0, "Mute Condition");
            eenum(
                app,
                ui,
                d,
                e,
                ep::MUTE_COND,
                d.r(130.0, 175.0, 266.0, 189.0),
                &strs(&["None", "Mute Same Note", "Mute Other Elems", "Mute Whole Patch", "Group 1", "Group 2", "Group 3", "Group 4", "Group 5", "Group 6", "Group 7", "Group 8"]),
            );
            d.label(ui, 34.0, 202.0, "Mono Mode");
            eenum(app, ui, d, e, ep::MONO, d.r(130.0, 195.0, 266.0, 209.0), &strs(&["Poly", "Lowest Note", "Highest Note", "First Note", "Last Note"]));
            let mv = (el.p[ep::MAX_VOICES] * 64.0).round().max(1.0) as i32;
            d.label(ui, 34.0, 222.0, "Max Voices");
            let id = Id::new(("trigmv", e));
            let (act, _) = panel::value(ui, d.pn, id, d.r(130.0, 215.0, 180.0, 229.0), &format!("{mv}"), false);
            if let Some(n) = act_int(app, id, act, mv, 1, 64, 64, 6.0) {
                app.edit(|q| q.elements[e].p[ep::MAX_VOICES] = n as f32 / 64.0);
            }
        }
        ModView::Misc => {
            d.label_b(ui, 34.0, 50.0, "Miscellaneous");
            d.label(ui, 34.0, 72.0, "Note Delay");
            eenum(app, ui, d, e, ep::NOTE_DELAY, d.r(110.0, 65.0, 200.0, 79.0), &strs(&["Off", "Delay"]));
            ek(app, ui, d, e, ep::DELAY_TIME, 60.0, 112.0, "Delay Time", false, |v| secs(time_s(v) * 0.25));
            ek(app, ui, d, e, ep::UNISON, 60.0, 172.0, "Unison", false, |v| format!("{} voices", 1 + (v * 7.0) as i32));
            ek(app, ui, d, e, ep::UNI_DETUNE, 120.0, 172.0, "Detune", false, pct);
            ek(app, ui, d, e, ep::UNI_PAN, 180.0, 172.0, "Uni Pan", false, pct);
            ek(app, ui, d, e, ep::LEVEL, 60.0, 232.0, "Output Lvl", false, level_text);
            ek(app, ui, d, e, ep::PAN, 120.0, 232.0, "Output Pan", true, bpct);
            ek(app, ui, d, e, ep::PAN_KEY, 180.0, 232.0, "Pan Key", true, bpct);
            ek(app, ui, d, e, ep::RND_PAN, 60.0, 292.0, "Rnd Pan", false, pct);
            ek(app, ui, d, e, ep::ALT_PAN, 120.0, 292.0, "Alt Pan", false, pct);
        }
        ModView::Knob => knob_view(app, ui, d, &p),
    }
}

/// Hyperknob assignment (Mod Page: Knob View): names, offset (neutral position).
fn knob_view(app: &mut App, ui: &mut Ui, d: &D, p: &Patch) {
    d.label_b(ui, 34.0, 50.0, "Hyperknob Assignment");
    d.label(ui, 34.0, 64.0, "The knob name selects what it controls:");
    d.label(ui, 34.0, 76.0, "Tone, Reso, Attack, Release, Reverb, Delay, Drive, …");
    for k in 0..6 {
        let y = 92.0 + k as f32 * 36.0;
        d.label(ui, 34.0, y + 7.0, &format!("Knob {}", k + 1));
        let mut name = p.knob_names[k].clone();
        let r = d.r(84.0, y, 190.0, y + 14.0);
        ui.painter().rect_filled(r, Rounding::same(2.0), Color32::WHITE);
        let te = ui.put(r, egui::TextEdit::singleline(&mut name).font(d.pn.f(9.0)).frame(false));
        if te.changed() {
            app.edit(|q| q.knob_names[k] = name.chars().take(15).collect());
        }
        let ofs = p.p[pp::KNOB[k] + 1];
        let (nv, _) = panel::hslider(ui, d.pn, Id::new(("kofs", k)), d.r(196.0, y + 2.0, 266.0, y + 12.0), ofs, false, 0.5);
        if let Some(v) = nv {
            app.edit(|q| q.p[pp::KNOB[k] + 1] = v);
            app.show_hyper(format!("Knob {} offset {:.0}%", k + 1, v * 100.0), 1.5);
        }
        d.label(ui, 196.0, y + 22.0, "Offset");
    }
}

fn fx_element(app: &mut App, ui: &mut Ui, d: &D, e: usize, id: u32) {
    d.label_b(ui, 34.0, 32.0, "Effect:");
    let types: Vec<String> = fx::TYPES.iter().map(|t| t.1.to_string()).collect();
    let cur = fx::TYPES.iter().rposition(|t| t.0 <= id);
    let mid = Id::new(("fxtype", e));
    let (_, resp) = panel::value(ui, d.pn, mid, d.r(80.0, 25.0, 200.0, 39.0), fx::type_name(id), false);
    if let Some(k) = panel::menu(ui, &resp, mid.with("m"), &types, cur) {
        let nid = fx::TYPES[k].0;
        app.edit(|q| {
            q.elements[e].p[ep::FX_TYPE] = nid as f32 / 128.0;
            q.elements[e].p[ep::FX_P..ep::FX_P + fx::NPARAM].copy_from_slice(&fx::defaults(nid));
        });
    }
    let names = fx::param_names(id);
    for (k, n) in names.iter().enumerate() {
        if n.is_empty() {
            continue;
        }
        let x = 56.0 + (k % 4) as f32 * 58.0;
        let y = 80.0 + (k / 4) as f32 * 64.0;
        ek(app, ui, d, e, ep::FX_P + k, x, y, n, false, pct);
    }
}

// ---------------------------------------------------------------------------
// MIDI / MIX
// ---------------------------------------------------------------------------

fn header(ui: &Ui, d: &D, cols: &[(f32, &str)]) {
    for (x, t) in cols {
        ui.painter().text(d.pn.p(X0 + x, 16.0), Align2::CENTER_CENTER, *t, d.pn.f(7.5), col::DARK);
    }
}

fn midi(app: &mut App, ui: &mut Ui, d: &D) {
    header(ui, d, &[(40.0, "KEY RANGE"), (116.0, "VEL RANGE"), (166.0, "SEMI"), (200.0, "CENT"), (234.0, "VOICES"), (260.0, "🔒")]);
    for i in 0..PARTS {
        let y = i as f32 * 21.75 + 2.0;
        let y1 = y + 15.0;
        let pv = app.parts[i].clone();
        let notes = |n: i32| note_name(n);
        let (lo, hi, xf) = pv.key_range;
        let fields: [(f32, f32, String, i32, i32, i32, i32, u8); 7] = [
            (4.0, 34.0, notes(lo), lo, 0, 127, 0, 0),
            (42.0, 72.0, notes(hi), hi, 0, 127, 127, 1),
            (80.0, 106.0, format!("{}", pv.vel_range.0), pv.vel_range.0, 1, 127, 1, 2),
            (126.0, 152.0, format!("{}", pv.vel_range.1), pv.vel_range.1, 1, 127, 127, 3),
            (156.0, 180.0, format!("{:+}", pv.semi), pv.semi, -36, 36, 0, 4),
            (186.0, 214.0, format!("{:+.0}", pv.cent), pv.cent as i32, -100, 100, 0, 5),
            (220.0, 248.0, format!("{}", pv.max_voices), pv.max_voices as i32, 1, 128, 64, 6),
        ];
        for (x0, x1, t, v, lo_, hi_, def, which) in fields {
            let id = Id::new(("midi", i, which));
            let (act, _) = panel::value(ui, d.pn, id, d.r(x0, y, x1, y1), &t, false);
            if let Some(n) = act_int(app, id, act, v, lo_, hi_, def, 4.0) {
                app.with(|s| {
                    let p = &mut s.parts[i];
                    match which {
                        0 => p.key_range.0 = n,
                        1 => p.key_range.1 = n,
                        2 => p.vel_range.0 = n,
                        3 => p.vel_range.1 = n,
                        4 => p.semi = n,
                        5 => p.cent = n as f32,
                        _ => p.max_voices = n as usize,
                    }
                });
            }
        }
        // crossfade pop-ups on the hyphens
        for (k, (x0, x1, cur)) in [(34.0, 42.0, xf), (106.0, 126.0, pv.vel_range.2)].iter().enumerate() {
            let r = d.r(*x0, y, *x1, y1);
            let id = Id::new(("xf", i, k));
            let resp = ui.interact(r, id, Sense::click());
            ui.painter().text(r.center(), Align2::CENTER_CENTER, if *cur > 0 { format!("{cur}") } else { "-".into() }, d.pn.f(if *cur > 0 { 7.0 } else { 9.0 }), col::LCD_TEXT);
            let items: Vec<String> = if k == 0 { [0, 1, 2, 3, 6, 12, 24].iter().map(|v| format!("X-Fade +{v}")).collect() } else { [0, 8, 16, 24, 36, 48, 64].iter().map(|v| format!("X-Fade {v}")).collect() };
            if let Some(c) = panel::menu(ui, &resp, id.with("m"), &items, None) {
                let v = if k == 0 { [0, 1, 2, 3, 6, 12, 24][c] } else { [0, 8, 16, 24, 36, 48, 64][c] };
                app.with(|s| {
                    if k == 0 {
                        s.parts[i].key_range.2 = v
                    } else {
                        s.parts[i].vel_range.2 = v
                    }
                });
            }
        }
        let lr = d.r(252.0, y, 268.0, y1);
        let resp = ui.interact(lr, Id::new(("lock", i)), Sense::click());
        ui.painter().text(lr.center(), Align2::CENTER_CENTER, "🔒", d.pn.f(8.0), if pv.lock { Color32::BLACK } else { Color32::from_gray(185) });
        if resp.clicked() {
            app.with(|s| s.parts[i].lock = !s.parts[i].lock);
        }
    }
}

fn mix(app: &mut App, ui: &mut Ui, d: &D) {
    header(ui, d, &[(30.0, "PAN"), (78.0, "OUT"), (114.0, "FX1"), (148.0, "FX2"), (182.0, "FX3"), (216.0, "FX4"), (240.0, "P"), (260.0, "🔒")]);
    for i in 0..PARTS {
        let y = i as f32 * 21.75 + 2.0;
        let y1 = y + 15.0;
        let pv = app.parts[i].clone();
        let (nv, _) = panel::hslider(ui, d.pn, Id::new(("pan", i)), d.r(4.0, y + 3.0, 60.0, y1 - 3.0), (pv.pan + 1.0) / 2.0, true, 0.5);
        if let Some(v) = nv {
            app.with(|s| s.parts[i].pan = v * 2.0 - 1.0);
            app.show_hyper(format!("Part {} Pan {:+.0}", i + 1, (v * 2.0 - 1.0) * 64.0), 1.0);
        }
        let id = Id::new(("out", i));
        let (act, _) = panel::value(ui, d.pn, id, d.r(66.0, y, 92.0, y1), &format!("{}", pv.output + 1), false);
        if let Some(n) = act_int(app, id, act, pv.output as i32, 0, 0, 0, 6.0) {
            app.with(|s| s.parts[i].output = n as usize);
        }
        for k in 0..4 {
            let x = 98.0 + k as f32 * 34.0;
            let (nv, _) = panel::hslider(ui, d.pn, Id::new(("send", i, k)), d.r(x, y + 3.0, x + 30.0, y1 - 3.0), pv.sends[k], false, 0.0);
            if let Some(v) = nv {
                app.with(|s| s.parts[i].sends[k] = v);
                app.show_hyper(format!("Part {} FX Send {} {:.0}%", i + 1, k + 1, v * 100.0), 1.0);
            }
        }
        let pr = d.r(234.0, y, 248.0, y1);
        let resp = ui.interact(pr, Id::new(("pfx", i)), Sense::click());
        ui.painter().rect_filled(pr, Rounding::same(2.0), if pv.patch_fx_on { Color32::BLACK } else { Color32::from_gray(210) });
        ui.painter().text(pr.center(), Align2::CENTER_CENTER, "P", d.pn.f(8.0), Color32::WHITE);
        if resp.clicked() {
            app.with(|s| s.parts[i].patch_fx_on = !s.parts[i].patch_fx_on);
        }
        let lr = d.r(252.0, y, 268.0, y1);
        let resp = ui.interact(lr, Id::new(("lock2", i)), Sense::click());
        ui.painter().text(lr.center(), Align2::CENTER_CENTER, "🔒", d.pn.f(8.0), if pv.lock { Color32::BLACK } else { Color32::from_gray(185) });
        if resp.clicked() {
            app.with(|s| s.parts[i].lock = !s.parts[i].lock);
        }
    }
}

// ---------------------------------------------------------------------------
// FX
// ---------------------------------------------------------------------------

fn fxpage(app: &mut App, ui: &mut Ui, d: &D) {
    let sel = app.sel;
    let pv = app.parts[sel].clone();
    d.label_c(ui, 136.0, 8.0, "Global FX");
    for k in 0..4 {
        let x = 20.0 + k as f32 * 58.0;
        let g = app.global_fx[k];
        let used = !(pv.patch_fx_on && pv.patch_fx[k].map(|f| f.on).unwrap_or(false));
        let selc = app.fx_sel == (true, k);
        let (_, resp) = panel::value(ui, d.pn, Id::new(("gfx", k)), d.r(x, 14.0, x + 54.0, 28.0), &format!("{} {}", k + 1, fx::type_name(g.id)), selc);
        if used && !selc {
            ui.painter().rect_stroke(d.r(x, 14.0, x + 54.0, 28.0), Rounding::same(1.5), Stroke::new(2.0 * d.pn.s, col::BLUE));
        }
        if resp.clicked() {
            app.fx_sel = (true, k);
        }
        if resp.secondary_clicked() {
            let mut g2 = g;
            g2.on = !g2.on;
            app.with(|s| s.set_global_fx(k, g2));
        }
    }
    d.label(ui, 20.0, 40.0, &format!("Patch FX (part {})", sel + 1));
    let (_, ar) = panel::value(ui, d.pn, Id::new("pactive"), d.r(200.0, 33.0, 262.0, 47.0), if pv.patch_fx_on { "P Active" } else { "P Off" }, pv.patch_fx_on);
    if ar.clicked() {
        app.with(|s| s.parts[sel].patch_fx_on = !s.parts[sel].patch_fx_on);
    }
    for k in 0..4 {
        let x = 20.0 + k as f32 * 58.0;
        let t = match pv.patch_fx[k] {
            Some(f) => format!("{} {}", k + 1, fx::type_name(f.id)),
            None => format!("{} ---", k + 1),
        };
        let selc = app.fx_sel == (false, k);
        let (_, resp) = panel::value(ui, d.pn, Id::new(("pfxs", k)), d.r(x, 52.0, x + 54.0, 66.0), &t, selc);
        if resp.clicked() {
            app.fx_sel = (false, k);
        }
        if resp.secondary_clicked() {
            app.with(|s| {
                if let Some(f) = &mut s.parts[sel].patch_fx[k] {
                    f.on = !f.on;
                }
            });
        }
    }
    d.frame(ui, 14.0, 74.0, 258.0, 296.0);
    let (global, k) = app.fx_sel;
    let slot = if global { Some(app.global_fx[k]) } else { pv.patch_fx[k] };
    let types: Vec<String> = std::iter::once("(none)".to_string()).chain(fx::TYPES.iter().map(|t| t.1.to_string())).collect();
    let cur = slot.map(|s| fx::TYPES.iter().rposition(|t| t.0 <= s.id).map(|i| i + 1).unwrap_or(0)).unwrap_or(0);
    let mid = Id::new("fxpagetype");
    let (_, resp) = panel::value(ui, d.pn, mid, d.r(22.0, 80.0, 140.0, 94.0), &types[cur], false);
    if let Some(c) = panel::menu(ui, &resp, mid.with("m"), &types, Some(cur)) {
        let ns = if c == 0 { None } else { Some(FxSlot::new(fx::TYPES[c - 1].0)) };
        set_slot(app, global, k, ns);
    }
    if let Some(s) = slot {
        let (_, onr) = panel::value(ui, d.pn, Id::new("fxon"), d.r(150.0, 80.0, 196.0, 94.0), if s.on { "On" } else { "Off" }, s.on);
        if onr.clicked() {
            let mut s2 = s;
            s2.on = !s2.on;
            set_slot(app, global, k, Some(s2));
        }
        if fx::family(s.id) == fx::Family::Delay {
            let sync = s.p[5] > 0.5;
            let (_, syr) = panel::value(ui, d.pn, Id::new("fxsync"), d.r(204.0, 80.0, 250.0, 94.0), "Sync", sync);
            if syr.clicked() {
                let mut s2 = s;
                s2.p[5] = if sync { 0.0 } else { 1.0 };
                set_slot(app, global, k, Some(s2));
            }
        }
        for (j, n) in fx::param_names(s.id).iter().enumerate() {
            if n.is_empty() {
                continue;
            }
            let x = 44.0 + (j % 4) as f32 * 58.0;
            let y = 124.0 + (j / 4) as f32 * 58.0;
            let (nv, _) = panel::knob(ui, d.pn, Id::new(("fxk", global, k, j)), d.p(x, y), 9.0, s.p[j], false, n, fx::defaults(s.id)[j]);
            if let Some(v) = nv {
                let mut s2 = s;
                s2.p[j] = v;
                set_slot(app, global, k, Some(s2));
                app.show_hyper(format!("{} {}: {:.0}%", fx::type_name(s.id), n, v * 100.0), 1.0);
            }
        }
    }
    if global {
        d.label(ui, 20.0, 318.0, "Global FX return: 100%   Pan: centre");
    } else {
        d.label(ui, 20.0, 318.0, "Patch FX are sends of this part only.");
    }
}

fn set_slot(app: &mut App, global: bool, k: usize, s: Option<FxSlot>) {
    let sel = app.sel;
    app.with(|sy| {
        if global {
            if let Some(s) = s {
                sy.set_global_fx(k, s);
            }
        } else {
            sy.parts[sel].patch_fx[k] = s;
        }
    });
}

// ---------------------------------------------------------------------------
// COMBI / SETUP
// ---------------------------------------------------------------------------

fn combi(app: &mut App, ui: &mut Ui, d: &D) {
    let name = d.r(10.0, 6.0, 262.0, 22.0);
    ui.painter().rect_filled(name, Rounding::same(2.0), Color32::BLACK);
    ui.painter().text(name.center(), Align2::CENTER_CENTER, &app.combi_name, d.pn.f(10.0), Color32::from_rgb(120, 220, 255));
    for (k, (t, dir)) in [("        Previous", -1i32), ("        Next", 1)].iter().enumerate() {
        let x0 = 20.0 + k as f32 * 124.0;
        let (_, r) = panel::value(ui, d.pn, Id::new(("chainnav", k)), d.r(x0, 30.0, x0 + 112.0, 54.0), t, false);
        let (cx, cy) = (x0 + 14.0, 42.0);
        let pts = if *dir < 0 { vec![d.p(cx + 5.0, cy - 7.0), d.p(cx + 5.0, cy + 7.0), d.p(cx - 6.0, cy)] } else { vec![d.p(cx - 5.0, cy - 7.0), d.p(cx - 5.0, cy + 7.0), d.p(cx + 6.0, cy)] };
        ui.painter().add(egui::Shape::closed_line(pts, Stroke::new(1.2 * d.pn.s, col::LCD_TEXT)));
        if r.clicked() && !app.chain.is_empty() {
            app.chain_pos = (app.chain_pos as i32 + dir).rem_euclid(app.chain.len() as i32) as usize;
            let (n, data) = app.chain[app.chain_pos].clone();
            if let Some(nm) = app.with(|s| hs2_core::state::load(s, &data)) {
                app.combi_name = nm;
            }
            app.status = format!("Combi Chain {:03}: {n}", app.chain_pos);
        }
    }
    d.frame(ui, 10.0, 62.0, 100.0, 236.0);
    d.label(ui, 16.0, 72.0, "Combi Chain");
    for (k, (t, act)) in [("Save", 0), ("Load", 1), ("Clear", 2), ("Add…", 3)].iter().enumerate() {
        let (_, r) = panel::value(ui, d.pn, Id::new(("chainbtn", k)), d.r(16.0, 82.0 + k as f32 * 20.0, 94.0, 96.0 + k as f32 * 20.0), t, false);
        if r.clicked() {
            match act {
                0 => {
                    let mut out = b"HS2//COMBICHAIN\n".to_vec();
                    for (n, _) in &app.chain {
                        out.extend_from_slice(n.as_bytes());
                        out.push(b'\n');
                    }
                    if let Some(st) = crate::platform::save_file("Combi Chain.h2c", "h2c", "Combi Chain", &out) {
                        app.status = st;
                    }
                }
                1 | 3 => {
                    if *act == 1 {
                        app.chain.clear();
                    }
                    app.pick("chain", ui.ctx(), "Combis", &["hs2"]);
                }
                _ => app.chain.clear(),
            }
        }
    }
    d.label(ui, 16.0, 172.0, "Preload Mode");
    let pre = ["Off", "Next", "Full"];
    let id = Id::new("preload");
    let (act, _) = panel::value(ui, d.pn, id, d.r(16.0, 180.0, 94.0, 194.0), pre[app.preload], false);
    if let Some(n) = act_int(app, id, act, app.preload as i32, 0, 2, 0, 8.0) {
        app.preload = n as usize;
    }
    let (_, pr) = panel::value(ui, d.pn, Id::new("panic"), d.r(16.0, 206.0, 94.0, 222.0), "PANIC", false);
    if pr.clicked() {
        app.with(|s| s.panic());
    }
    d.frame(ui, 10.0, 244.0, 100.0, 340.0);
    let (nv, _) = panel::knob(ui, d.pn, Id::new("combimv"), d.p(55.0, 280.0), 11.0, app.master, false, "Master Vol", 0.7);
    if let Some(v) = nv {
        app.with(|s| s.master = v);
    }
    // chain list
    d.frame(ui, 110.0, 62.0, 262.0, 340.0);
    for k in 0..21 {
        let y = 66.0 + k as f32 * 13.0;
        let idx = k;
        let t = match app.chain.get(idx) {
            Some((n, _)) => format!("{idx:03} {n}"),
            None => format!("{idx:03}"),
        };
        let r = d.r(114.0, y, 258.0, y + 12.0);
        if idx == app.chain_pos && idx < app.chain.len() {
            ui.painter().rect_filled(r, Rounding::ZERO, col::BLUE_LT);
        }
        ui.painter().with_clip_rect(r).text(r.left_center(), Align2::LEFT_CENTER, t, d.pn.f(8.5), col::LCD_TEXT);
        let resp = ui.interact(r, Id::new(("chainrow", k)), Sense::click());
        if resp.double_clicked() && idx < app.chain.len() {
            app.chain_pos = idx;
            let data = app.chain[idx].1.clone();
            if let Some(nm) = app.with(|s| hs2_core::state::load(s, &data)) {
                app.combi_name = nm;
            }
        }
    }
}

fn setup(app: &mut App, ui: &mut Ui, d: &D) {
    d.frame(ui, 6.0, 4.0, 266.0, 76.0);
    d.label_b(ui, 12.0, 13.0, "Content & Samples");
    let content = app.content_dir.clone().unwrap_or_else(|| "(none: built-in patches only)".into());
    d.label(ui, 12.0, 28.0, &format!("Content: {}", short(&content, 44)));
    let (_, cr) = panel::value(ui, d.pn, Id::new("setcontent"), d.r(12.0, 36.0, 132.0, 50.0), "Content Folder…", false);
    if cr.clicked() {
        crate::platform::pick_folder(&app.picked_dir, ui.ctx());
    }
    let (_, sr) = panel::value(ui, d.pn, Id::new("setsf"), d.r(140.0, 36.0, 260.0, 50.0), "Sample Mapping…", false);
    if sr.clicked() {
        app.map_open = true;
    }
    d.label(ui, 12.0, 64.0, &format!("Samples: {}  ·  {} banks mapped", short(&app.sf_label(), 30), app.map.len()));
    // MIDI settings
    d.frame(ui, 6.0, 82.0, 150.0, 196.0);
    d.label_b(ui, 12.0, 91.0, "MIDI Settings");
    d.label(ui, 12.0, 108.0, "Pgm Change");
    let (_, pr) = panel::value(ui, d.pn, Id::new("pgmch"), d.r(86.0, 101.0, 146.0, 115.0), if app.pgm_change { "On" } else { "Off" }, false);
    if pr.clicked() || pr.secondary_clicked() {
        app.pgm_change = !app.pgm_change;
    }
    d.label(ui, 12.0, 126.0, "MIDI Combi Sel");
    let cs = ["Off", "On", "Chain"];
    let id = Id::new("combisel");
    let (act, _) = panel::value(ui, d.pn, id, d.r(86.0, 119.0, 146.0, 133.0), cs[app.combi_sel], false);
    if let Some(n) = act_int(app, id, act, app.combi_sel as i32, 0, 2, 0, 8.0) {
        app.combi_sel = n as usize;
    }
    d.label(ui, 12.0, 144.0, "Vel Sens");
    let vc = ["Light", "Linear", "Heavy"];
    let id = Id::new("velcurve");
    let (act, _) = panel::value(ui, d.pn, id, d.r(86.0, 137.0, 146.0, 151.0), vc[app.vel_curve], false);
    if let Some(n) = act_int(app, id, act, app.vel_curve as i32, 0, 2, 1, 8.0) {
        app.vel_curve = n as usize;
    }
    let ins = app.midi.as_ref().map(|m| m.names.join(", ")).unwrap_or_default();
    d.label(ui, 12.0, 164.0, "Inputs:");
    ui.painter().with_clip_rect(d.r(12.0, 168.0, 150.0, 194.0)).text(d.p(12.0, 176.0), Align2::LEFT_CENTER, if ins.is_empty() { "(none)".to_string() } else { ins }, d.pn.f(7.5), col::LCD_TEXT);
    // output section
    d.frame(ui, 160.0, 82.0, 266.0, 196.0);
    let (nv, _) = panel::knob(ui, d.pn, Id::new("hype"), d.p(186.0, 112.0), 10.0, app.hype, false, "Hype", 0.0);
    if let Some(v) = nv {
        app.with(|s| s.hype = v);
        app.show_hyper(format!("Output Hype {:.0}%", v * 100.0), 1.0);
    }
    let (nv, _) = panel::knob(ui, d.pn, Id::new("mvol"), d.p(240.0, 112.0), 10.0, app.master, false, "Master Vol", 0.7);
    if let Some(v) = nv {
        app.with(|s| s.master = v);
    }
    d.label_c(ui, 213.0, 142.0, "Master Tuning");
    let mt = app.master_tune;
    let (nv, _) = panel::knob(ui, d.pn, Id::new("mtsemi"), d.p(186.0, 168.0), 9.0, (mt.round() / 24.0 + 0.5).clamp(0.0, 1.0), true, "Semi", 0.5);
    if let Some(v) = nv {
        let semi = ((v - 0.5) * 24.0).round();
        app.with(|s| s.master_tune = semi + (mt - mt.round()));
    }
    let (nv, _) = panel::knob(ui, d.pn, Id::new("mtcent"), d.p(240.0, 168.0), 9.0, ((mt - mt.round()) + 0.5).clamp(0.0, 1.0), true, "Cent", 0.5);
    if let Some(v) = nv {
        app.with(|s| s.master_tune = mt.round() + (v - 0.5));
    }
    // patch / combi files
    d.frame(ui, 6.0, 202.0, 266.0, 262.0);
    d.label_c(ui, 70.0, 211.0, "Patch (.hsp)");
    d.label_c(ui, 200.0, 211.0, "Combi (.hs2)");
    for (k, (t, a)) in [("Load", 0), ("Save", 1), ("Load", 2), ("Save", 3)].iter().enumerate() {
        let x = if k < 2 { 14.0 + k as f32 * 60.0 } else { 144.0 + (k - 2) as f32 * 60.0 };
        let (_, r) = panel::value(ui, d.pn, Id::new(("fileb", k)), d.r(x, 220.0, x + 54.0, 236.0), t, false);
        if r.clicked() {
            match a {
                0 => app.pick("patch", ui.ctx(), "Patch", &["hsp", "fxp"]),
                1 => app.save_patch_file(),
                2 => app.pick("combi", ui.ctx(), "Combi", &["hs2"]),
                _ => app.save_combi_file(),
            }
        }
    }
    let mut name = app.combi_name.clone();
    let nr = d.r(144.0, 240.0, 258.0, 254.0);
    ui.painter().rect_filled(nr, Rounding::same(2.0), Color32::WHITE);
    if ui.put(nr, egui::TextEdit::singleline(&mut name).font(d.pn.f(8.5)).frame(false)).changed() {
        app.combi_name = name;
    }
    d.label(ui, 14.0, 247.0, "Combi name:");
    // memory / status
    d.frame(ui, 6.0, 268.0, 266.0, 340.0);
    d.label_b(ui, 12.0, 277.0, "Status");
    d.label(ui, 12.0, 292.0, &format!("Polyphony: {} voices", app.total_poly));
    d.label(ui, 12.0, 306.0, &format!("Sample rate: {} Hz   Tempo: {:.0} bpm", app.sr, app.tempo));
    let id = Id::new("tempo");
    let (act, _) = panel::value(ui, d.pn, id, d.r(200.0, 286.0, 258.0, 300.0), &format!("{:.0} bpm", app.tempo), false);
    if let Some(n) = act_int(app, id, act, app.tempo as i32, 30, 300, 120, 3.0) {
        app.with(|s| s.tempo = n as f32);
    }
    let lib = format!("Library: {} patches, {} banks", app.lib.total_patches(), app.lib.banks.len());
    d.label(ui, 12.0, 320.0, &lib);
    if let Some(e) = &app.audio_err {
        d.label(ui, 12.0, 334.0, &format!("Audio error: {}", short(e, 40)));
    }
}

fn short(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        let t: String = s.chars().rev().take(n - 1).collect::<Vec<_>>().into_iter().rev().collect();
        format!("…{t}")
    }
}
