//! XG Editor main window, rebuilt after XGEditor.dll (XGeditor.hlp; dialog
//! bitmap 9491 and its sprites; reference shots taken from XGworks V4.0).
//!
//! The 660×402 panel bitmap sits under a 30 px tool bar; the live parts are
//! drawn over it at the positions of the original child windows (client
//! coordinates): the Multi Part list (voice, Insertion 1–4 / Variation part,
//! drum mode), the 16 assignable sliders with Preset 1–4, the Insertion and
//! Variation types, the Variation / Chorus / Reverb units, System and EQ.
//! The artwork is read from the user's XGEditor.dll at run time; without it
//! the editor falls back to its plain parameter lists. DETAIL opens those
//! lists for the unit until the original detail dialogs are rebuilt.

use super::{block_keys, Change, XgEditor, XgTool};
use eframe::egui::{self, pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke};
use std::collections::HashMap;
use xgworks_core::xgtbl::{FxSlot, TblBlock, TblParam, XgTable};
use xgworks_core::xgunits::param_display;

use crate::peres;

pub const CLIENT: [f32; 2] = [660.0, 432.0];
const BG_Y: f32 = 30.0;

// ---- bitmap ids in XGEditor.dll ----
const BM_BG: u32 = 9491;
const BM_TOOLS: u32 = 8172; // 10 icons 16×15
const BM_VAR_TOP_GREY: u32 = 9241; // 127×62 at bitmap (528,5)
const BM_VAR_UNIT: u32 = 9242; // 130×121 at bitmap (526,73), coloured (system mode)
const BM_INS: u32 = 9200; // INS / SYS switch, 79×24: INS lit
const BM_SYS: u32 = 9201; //                         SYS lit
const BM_I: [u32; 4] = [9209, 9210, 8648, 8649];
const BM_V: u32 = 9211;
const BM_DR: u32 = 9244; // "Dr" (drum)
const BM_D: [u32; 4] = [9215, 9216, 9217, 9218]; // drum setup 1..4
const BM_A_TRACK: u32 = 8779;
const BM_A_TRACK_ON: u32 = 9251; // orange: the part's assignable switch is on
const BM_A_THUMB: u32 = 8780;
const BM_A_ON: u32 = 8806;
const BM_A_OFF: u32 = 8805;
const BM_PRESET: u32 = 8827;
const BM_PRESET_SEL: u32 = 8828;
const BM_CAP: u32 = 9205; // fader cap 13×13 (red line)
const BM_CAP_OFF: u32 = 8081; // greyed fader cap
/// Knob in 7 positions (min … max) and the EQ band knob in 5 (with dots).
const BM_KNOB: [u32; 7] = [9221, 9222, 9223, 9224, 9225, 9226, 9228];
const BM_KNOB5: [u32; 5] = [9245, 9246, 9247, 9248, 9249];
const BM_FREQ_THUMB: u32 = 9207; // 12×7
const BM_DETAIL: u32 = 8860; // active DETAIL button 44×15 (the background has the greyed one)

// ---- geometry (client coordinates) ----
const TAB: (f32, f32, f32, f32) = (7.0, 91.0, 42.0, 16.0);
const ROW_Y: f32 = 120.0;
const ROW_H: f32 = 13.0;
const NUM_X: (f32, f32) = (12.0, 20.0);
const VOICE_X: (f32, f32) = (33.0, 131.0);
const COL_X: f32 = 165.0; // i1 i2 i3 i4 V, 16 px apart
const DR_X: f32 = 245.0;
// ASSIGNABLE, Preset / ASSIGN, INS / SYS and the DETAIL buttons: positions
// found by locating the DLL's bitmaps in a screenshot of the original
const A_BTN_X: f32 = 319.0;
const A_SLD_X: f32 = 338.0;
const A_Y: f32 = 121.0;
const PRESET: (f32, f32) = (441.0, 136.0); // 50×15, 19 px apart
const ASSIGN: (f32, f32) = (441.0, 113.0);
/// Tool bar buttons (22×21, the icon 3 px in): icons at x 7, 30, 61 … y 6 in the original.
const TOOL_X: [f32; 10] = [4.0, 27.0, 58.0, 81.0, 112.0, 135.0, 158.0, 181.0, 212.0, 235.0];
const EQ_COLS: (f32, f32) = (341.0, 382.0); // 15 columns 10 px apart, cyan base line

/// What a DETAIL button opens (the parameter lists of the old editor).
#[derive(Clone, Debug, PartialEq)]
pub enum Detail {
    Fx(FxSlot),
    Part,
    Block(&'static str),
}

/// An open drop-down list of the main window.
#[derive(Clone, Debug, PartialEq)]
pub enum Menu {
    FxType(FxSlot),
    Voice(usize),
    Model,
}

/// The controls that can be dragged.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Knob {
    Param(&'static str, u8),
    EqBand,
    EqGain,
}

/// Per-editor state of the main window.
pub struct Panel {
    sprites: Option<HashMap<u32, egui::TextureHandle>>,
    tried: bool,
    pub page: usize,
    pub preset: usize,
    /// Preset 1–4: (name, button param lo, slider param lo) in the Multi Part block.
    pub presets: [(String, u8, u8); 4],
    eq_band: usize,
    drag: Option<(Knob, f32, u32)>,
    pub menu: Option<(Menu, Pos2)>,
    pub detail: Option<Detail>,
    pub assign_dlg: bool,
}

impl Default for Panel {
    fn default() -> Self {
        // the original's defaults (XG Editor Assignable Setup)
        let p = |n: usize, b, s| (format!("Preset{n}"), b, s);
        Panel {
            sprites: None,
            tried: false,
            page: 0,
            preset: 0,
            presets: [p(1, 0x39, 0x0B), p(2, 0x3A, 0x0E), p(3, 0x3B, 0x13), p(4, 0x3C, 0x12)],
            eq_band: 0,
            drag: None,
            menu: None,
            detail: None,
            assign_dlg: false,
        }
    }
}

fn r(x: f32, y: f32, w: f32, h: f32) -> Rect {
    Rect::from_min_size(pos2(x, y), vec2(w, h))
}

/// A Multi Part parameter (any of the Part_n / PartAdd blocks) by low address.
fn part_param(t: &XgTable, lo: u8) -> Option<(&TblBlock, &TblParam)> {
    t.blocks
        .iter()
        .filter(|b| b.key.starts_with("Part_"))
        .find_map(|b| b.params.iter().find(|p| p.lo == lo && !p.name.is_empty()).map(|p| (b, p)))
}

fn param<'a>(t: &'a XgTable, key: &str, lo: u8) -> Option<(&'a TblBlock, &'a TblParam)> {
    let b = t.block(key)?;
    Some((b, b.params.iter().find(|p| p.lo == lo)?))
}

impl XgEditor {
    fn load_panel(&mut self, ctx: &egui::Context) {
        self.panel.tried = true;
        let Some(dll) = crate::platform::read(&self.data_dir.parent().map_or_else(|| crate::platform::xg4("XGEditor.dll"), |d| d.join("XGEditor.dll")))
        else {
            return;
        };
        let bms = peres::load_bitmaps(&dll, |id| (8000..=9600).contains(&id));
        if !bms.contains_key(&BM_BG) {
            return;
        }
        let mut map = HashMap::new();
        for (id, mut b) in bms {
            if id == BM_TOOLS {
                peres::face_to_alpha(&mut b);
            }
            let img = egui::ColorImage::from_rgba_unmultiplied([b.w, b.h], &b.rgba);
            map.insert(id, ctx.load_texture(format!("xged{id}"), img, egui::TextureOptions::NEAREST));
        }
        self.panel.sprites = Some(map);
    }

    /// Whether the original artwork was found (else the plain lists are used).
    pub fn has_panel(&mut self, ctx: &egui::Context) -> bool {
        if !self.panel.tried {
            self.load_panel(ctx);
        }
        self.panel.sprites.is_some()
    }

    /// Window title like the original's: "XG Editor   MU128".
    pub fn title(&self) -> String {
        format!("XG Editor   {}", self.models.get(self.sel_model).map_or("", |m| m.0.as_str()))
    }

    fn spr(&self, p: &egui::Painter, o: Pos2, id: u32, at: Pos2) -> bool {
        let Some(t) = self.panel.sprites.as_ref().and_then(|s| s.get(&id)) else { return false };
        let [w, h] = t.size();
        let rect = Rect::from_min_size(o + at.to_vec2(), vec2(w as f32, h as f32));
        p.image(t.id(), rect, Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0)), Color32::WHITE);
        true
    }

    fn val(&self, b: &TblBlock, p: &TblParam, inst: usize) -> u32 {
        let (h, m) = b.hm(inst);
        p.combine(&self.mem.read([h, m, p.lo], &p.split(p.default(inst))))
    }

    fn set_val(&mut self, b: &TblBlock, p: &TblParam, inst: usize, v: u32, ch: &mut Vec<Change>) {
        let (h, m) = b.hm(inst);
        let (lo, hi) = (p.min().min(p.max()), p.max().max(p.min()));
        let data = p.split(v.clamp(lo, hi));
        let addr = [h, m, p.lo];
        if self.mem.read(addr, &data) != data || self.mem.get(addr).is_none() {
            self.mem.set(addr, &data);
            ch.push((addr, data));
        }
    }

    /// Current (MSB, LSB) of an effect type.
    fn fx_cur(&self, t: &XgTable, slot: FxSlot) -> (u8, u8) {
        let def = block_keys(slot)
            .first()
            .and_then(|k| t.block(k))
            .and_then(|b| b.params.first())
            .map(|p| p.bytes.iter().map(|b| *b.2.first().unwrap_or(&0)).collect::<Vec<u8>>())
            .unwrap_or_else(|| vec![0, 0]);
        let c = self.mem.read(slot.type_addr(), &def);
        (c[0], *c.get(1).unwrap_or(&0))
    }

    fn parts(&self, t: &XgTable) -> usize {
        t.block("Part_1").map_or(16, |b| b.instances())
    }

    /// The main window; returns the changes (sent and written into the song).
    pub fn panel_ui(&mut self, ui: &mut egui::Ui) -> Vec<Change> {
        let mut ch = Vec::new();
        if std::mem::take(&mut self.reload_model) {
            self.load_model();
        }
        let Some(t) = self.table.take() else {
            ui.colored_label(Color32::from_rgb(220, 90, 70), &self.error);
            return ch;
        };
        let (resp, p) = ui.allocate_painter(vec2(CLIENT[0], CLIENT[1]), Sense::hover());
        let o = resp.rect.min;
        p.rect_filled(r(o.x, o.y, CLIENT[0], BG_Y), 0.0, Color32::from_gray(0xF0));
        self.spr(&p, o, BM_BG, pos2(0.0, BG_Y));
        let font = FontId::proportional(10.0);
        let small = FontId::monospace(9.0);
        let text = |pp: &egui::Painter, at: Rect, s: &str, f: &FontId, c: Color32| {
            pp.with_clip_rect(at.translate(o.to_vec2())).text(
                o + vec2(at.left() + 2.0, at.center().y),
                Align2::LEFT_CENTER,
                s,
                f.clone(),
                c,
            );
        };

        self.toolbar(ui, &p, o, &t, &mut ch);

        // ---- Multi Part list ----
        let nparts = self.parts(&t);
        let pages = nparts.div_ceil(16).clamp(1, 4);
        self.panel.page = self.panel.page.min(pages - 1);
        for k in 0..5 {
            let tr = r(TAB.0 + TAB.2 * k as f32, TAB.1, TAB.2, TAB.3);
            let label = if k < pages {
                format!("{}-{}", k * 16 + 1, k * 16 + 16)
            } else if k == 4 && t.block("Ad_1").is_some() {
                "A/D".into()
            } else {
                "*****".into()
            };
            let sel = k == self.panel.page;
            // the background has the tabs with the first one in front: only
            // the faces change when another page is shown
            if self.panel.page > 0 && (k == 0 || sel) {
                // tab faces in the background: x 6..46, then 39 px every 42 px
                let face = if k == 0 { r(6.0, TAB.1, 41.0, 15.0) } else { r(TAB.0 + TAB.2 * k as f32 + 1.0, TAB.1, 39.0, 15.0) };
                p.rect_filled(face.translate(o.to_vec2()), 0.0, Color32::from_gray(if sel { 0xC0 } else { 0xA0 }));
            }
            p.text(o + tr.center().to_vec2(), Align2::CENTER_CENTER, label, FontId::proportional(if sel { 12.0 } else { 9.0 }), Color32::BLACK);
            if k < pages && ui.interact(tr.translate(o.to_vec2()), ui.id().with(("xgtab", k)), Sense::click()).clicked() {
                self.panel.page = k;
            }
        }
        let first = self.panel.page * 16;
        let ins_n = (0..4).filter(|n| t.block(&format!("Ins{}_1", n + 1)).is_some()).count();
        let ins_part: Vec<Option<u32>> = (0..4)
            .map(|n| param(&t, &format!("Ins{}_1", n + 1), 0x0C).map(|(b, pp)| self.val(b, pp, 0)))
            .collect();
        let var_sys = param(&t, "Var_1", 0x5A).map_or(true, |(b, pp)| self.val(b, pp, 0) == 1);
        let var_part = param(&t, "Var_1", 0x5B).map(|(b, pp)| self.val(b, pp, 0));
        let p1 = t.block("Part_1");
        for row in 0..16 {
            let part = first + row;
            if part >= nparts {
                break;
            }
            let y = ROW_Y + ROW_H * row as f32;
            let sel = part == self.part;
            let nr = r(NUM_X.0, y, NUM_X.1, 12.0);
            // the original: light blue numbers, the selected part's in yellow
            let ncol = if sel { Color32::from_rgb(0xFF, 0xFF, 0x00) } else { Color32::from_rgb(0xA6, 0xCA, 0xF0) };
            p.text(o + nr.center().to_vec2(), Align2::CENTER_CENTER, (part + 1).to_string(), font.clone(), ncol);
            let vr = r(VOICE_X.0, y, VOICE_X.1, 12.0);
            if sel {
                p.rect_filled(vr.translate(o.to_vec2()), 0.0, Color32::from_rgb(0xCC, 0xFF, 0x66));
            }
            let get = |lo: u8| p1.and_then(|b| b.params.iter().find(|pp| pp.lo == lo).map(|pp| self.val(b, pp, part) as u8)).unwrap_or(0);
            let (msb, lsb, prg, mode) = (get(1), get(2), get(3), get(7));
            let name = self.voice_name(msb, lsb, prg).map(str::to_string).unwrap_or_else(|| format!("{msb}/{lsb}/{}", prg as u32 + 1));
            text(&p, vr, &name, &font, Color32::BLACK);
            let vresp = ui.interact(vr.translate(o.to_vec2()), ui.id().with(("xgvoice", part)), Sense::click());
            if vresp.clicked() || ui.interact(nr.translate(o.to_vec2()), ui.id().with(("xgnum", part)), Sense::click()).clicked() {
                self.part = part;
            }
            if vresp.double_clicked() {
                self.panel.menu = Some((Menu::Voice(part), o + vec2(vr.left(), vr.bottom())));
                self.voice_filter.clear();
            }
            // Insertion 1–4 / Variation part
            for c in 0..5 {
                let cr = r(COL_X + 16.0 * c as f32, y, 15.0, 12.0);
                let on = if c < 4 { ins_part[c] == Some(part as u32) } else { !var_sys && var_part == Some(part as u32) };
                if on {
                    self.spr(&p, o, if c < 4 { BM_I[c] } else { BM_V }, cr.min);
                }
                let usable = if c < 4 { c < ins_n } else { !var_sys };
                if usable && ui.interact(cr.translate(o.to_vec2()), ui.id().with(("xgins", part, c)), Sense::click()).clicked() {
                    let (key, lo) = if c < 4 { (format!("Ins{}_1", c + 1), 0x0C) } else { ("Var_1".to_string(), 0x5B) };
                    if let Some((b, pp)) = param(&t, &key, lo) {
                        let v = if on { 127 } else { part as u32 };
                        let (b, pp) = (b.clone(), pp.clone());
                        self.set_val(&b, &pp, 0, v, &mut ch);
                    }
                }
            }
            // drum mode: Normal / Drum / Drum S1..
            let dr = r(DR_X, y, 15.0, 12.0);
            match mode {
                1 => {
                    self.spr(&p, o, BM_DR, dr.min);
                }
                m @ 2..=5 => {
                    self.spr(&p, o, BM_D[m as usize - 2], dr.min);
                }
                _ => {}
            }
            if ui.interact(dr.translate(o.to_vec2()), ui.id().with(("xgdr", part)), Sense::click()).clicked() {
                if let Some((b, pp)) = p1.and_then(|b| b.params.iter().find(|pp| pp.lo == 7).map(|pp| (b.clone(), pp.clone()))) {
                    let next = if mode as u32 >= pp.max() { 0 } else { mode as u32 + 1 };
                    self.set_val(&b, &pp, part, next, &mut ch);
                }
            }
        }
        self.detail_btn(ui, &p, o, r(262.0, 330.0, 44.0, 15.0), Detail::Part, "xgd_part", true);

        // ---- Assignable ----
        let (_, blo, slo) = self.panel.presets[self.panel.preset].clone();
        let bparam = part_param(&t, blo).map(|(b, pp)| (b.clone(), pp.clone()));
        let sparam = part_param(&t, slo).map(|(b, pp)| (b.clone(), pp.clone()));
        for row in 0..16 {
            let part = first + row;
            if part >= nparts {
                break;
            }
            let y = A_Y + ROW_H * row as f32;
            let br = r(A_BTN_X, y, 16.0, 11.0);
            let bon = bparam.as_ref().map_or(false, |(b, pp)| self.val(b, pp, part) != 0);
            self.spr(&p, o, if bon { BM_A_ON } else { BM_A_OFF }, br.min);
            p.text(o + br.center().to_vec2(), Align2::CENTER_CENTER, (part + 1).to_string(), small.clone(), Color32::BLACK);
            if ui.interact(br.translate(o.to_vec2()), ui.id().with(("xgab", part)), Sense::click()).clicked() {
                if let Some((b, pp)) = &bparam {
                    self.set_val(b, pp, part, if bon { 0 } else { 1 }, &mut ch);
                }
            }
            let sr = r(A_SLD_X, y, 87.0, 10.0);
            self.spr(&p, o, if bon { BM_A_TRACK_ON } else { BM_A_TRACK }, sr.min);
            if let Some((b, pp)) = &sparam {
                let (lo, hi) = (pp.min(), pp.max().max(pp.min() + 1));
                let v = self.val(b, pp, part);
                let x = sr.left() + (v.saturating_sub(lo)) as f32 / (hi - lo) as f32 * (87.0 - 15.0);
                self.spr(&p, o, BM_A_THUMB, pos2(x, sr.top()));
                let resp = ui.interact(sr.translate(o.to_vec2()), ui.id().with(("xgas", part)), Sense::click_and_drag());
                if resp.is_pointer_button_down_on() {
                    if let Some(pt) = resp.interact_pointer_pos() {
                        let f = ((pt.x - o.x - sr.left() - 7.0) / (87.0 - 15.0)).clamp(0.0, 1.0);
                        self.part = part;
                        self.set_val(b, pp, part, lo + (f * (hi - lo) as f32).round() as u32, &mut ch);
                    }
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    resp.on_hover_text(format!("{} {}: {}", pp.name, part + 1, param_display(pp, v)));
                }
            }
        }
        for k in 0..4 {
            let pr = r(PRESET.0, PRESET.1 + 19.0 * k as f32, 50.0, 15.0);
            self.spr(&p, o, if k == self.panel.preset { BM_PRESET_SEL } else { BM_PRESET }, pr.min);
            p.text(o + pr.center().to_vec2(), Align2::CENTER_CENTER, &self.panel.presets[k].0, font.clone(), Color32::WHITE);
            if ui.interact(pr.translate(o.to_vec2()), ui.id().with(("xgpre", k)), Sense::click()).clicked() {
                self.panel.preset = k;
            }
        }
        if ui.interact(r(ASSIGN.0, ASSIGN.1, 50.0, 15.0).translate(o.to_vec2()), ui.id().with("xgassign"), Sense::click()).clicked() {
            self.panel.assign_dlg = true;
        }

        // ---- Insertion 1–4 and Variation types ----
        let boxes = [
            (FxSlot::Insertion(0), r(249.0, 54.0, 72.0, 11.0), r(326.0, 52.0, 44.0, 15.0)),
            (FxSlot::Insertion(1), r(249.0, 78.0, 72.0, 11.0), r(326.0, 76.0, 44.0, 15.0)),
            (FxSlot::Insertion(2), r(402.0, 54.0, 72.0, 11.0), r(479.0, 52.0, 44.0, 15.0)),
            (FxSlot::Insertion(3), r(402.0, 78.0, 72.0, 11.0), r(479.0, 76.0, 44.0, 15.0)),
            (FxSlot::Variation, r(556.0, 54.0, 89.0, 11.0), r(608.0, 35.0, 44.0, 15.0)),
        ];
        for (k, (slot, br, dr)) in boxes.into_iter().enumerate() {
            let have = match slot {
                FxSlot::Insertion(n) => (n as usize) < ins_n,
                _ => true,
            };
            if !have {
                continue;
            }
            let (m, l) = self.fx_cur(&t, slot);
            let name = t.fx_name(m, l);
            text(&p, br, &name, &font, Color32::BLACK);
            if ui.interact(br.translate(o.to_vec2()), ui.id().with(("xgfx", k)), Sense::click()).clicked() {
                self.panel.menu = Some((Menu::FxType(slot), o + vec2(br.left(), br.bottom())));
            }
            self.detail_btn(ui, &p, o, dr, Detail::Fx(slot), &format!("xgd_fx{k}"), !name.trim().is_empty());
        }
        // INS / SYS
        let sw = r(550.0, 73.0, 79.0, 24.0);
        self.spr(&p, o, if var_sys { BM_SYS } else { BM_INS }, sw.min);
        if ui.interact(sw.translate(o.to_vec2()), ui.id().with("xginssys"), Sense::click()).clicked() {
            if let Some((b, pp)) = param(&t, "Var_1", 0x5A).map(|(b, pp)| (b.clone(), pp.clone())) {
                self.set_val(&b, &pp, 0, if var_sys { 0 } else { 1 }, &mut ch);
            }
        }

        // ---- Variation unit (system mode) ----
        if var_sys {
            self.spr(&p, o, BM_VAR_UNIT, pos2(526.0, 73.0 + BG_Y));
        }
        let (vm, vl) = self.fx_cur(&t, FxSlot::Variation);
        text(&p, r(546.0, 125.0, 89.0, 11.0), &t.fx_name(vm, vl), &font, if var_sys { Color32::BLACK } else { Color32::GRAY });
        // the coloured unit bitmap has its own active DETAIL
        if var_sys {
            self.detail_hit(ui, o, r(608.0, 106.0, 44.0, 15.0), Detail::Fx(FxSlot::Variation), "xgd_var");
        }
        if !var_sys {
            self.spr(&p, o, BM_VAR_TOP_GREY, pos2(528.0, 5.0 + BG_Y));
            // the top unit stays usable; only its switch and type are drawn again
            self.spr(&p, o, BM_INS, sw.min);
            text(&p, r(556.0, 54.0, 89.0, 11.0), &t.fx_name(vm, vl), &font, Color32::BLACK);
        }
        // positions: the DLL's knob / fader cap bitmaps found in the original
        let units: [(&'static str, u8, Rect, Option<Rect>, u8); 12] = [
            ("Var_1", 0x57, r(536.0, 156.0, 30.0, 30.0), Some(r(540.0, 143.0, 22.0, 10.0)), 0),
            ("Var_1", 0x56, r(576.0, 156.0, 13.0, 54.0), Some(r(572.0, 143.0, 20.0, 10.0)), 1),
            ("Var_1", 0x58, r(603.0, 156.0, 13.0, 54.0), Some(r(599.0, 143.0, 20.0, 10.0)), 1),
            ("Var_1", 0x59, r(630.0, 156.0, 13.0, 54.0), Some(r(626.0, 143.0, 20.0, 10.0)), 1),
            ("Cho_1", 0x2D, r(447.0, 279.0, 30.0, 30.0), Some(r(451.0, 266.0, 22.0, 10.0)), 0),
            ("Cho_1", 0x2C, r(488.0, 279.0, 13.0, 54.0), Some(r(484.0, 266.0, 20.0, 10.0)), 1),
            ("Cho_1", 0x2E, r(515.0, 279.0, 13.0, 54.0), Some(r(511.0, 266.0, 20.0, 10.0)), 1),
            ("Rev_1", 0x0D, r(588.0, 279.0, 30.0, 30.0), Some(r(592.0, 266.0, 22.0, 10.0)), 0),
            ("Rev_1", 0x0C, r(629.0, 279.0, 13.0, 54.0), Some(r(625.0, 266.0, 20.0, 10.0)), 1),
            ("System", 0x04, r(16.0, 384.0, 30.0, 30.0), Some(r(20.0, 372.0, 22.0, 10.0)), 0),
            ("System", 0x00, r(55.0, 384.0, 30.0, 30.0), Some(r(53.0, 372.0, 35.0, 10.0)), 0),
            ("System", 0x06, r(94.0, 384.0, 30.0, 30.0), Some(r(98.0, 372.0, 22.0, 10.0)), 0),
        ];
        for (key, lo, cr, er, kind) in units {
            let Some((b, pp)) = param(&t, key, lo).map(|(b, pp)| (b.clone(), pp.clone())) else { continue };
            let enabled = key != "Var_1" || var_sys;
            let v = self.val(&b, &pp, 0);
            let (lo_v, hi_v) = (pp.min(), pp.max().max(pp.min() + 1));
            let f = (v.saturating_sub(lo_v)) as f32 / (hi_v - lo_v) as f32;
            if !enabled && kind == 1 {
                // the original leaves the greyed variation unit's caps at the bottom
                self.spr(&p, o, BM_CAP_OFF, pos2(cr.left(), cr.top() + 54.0 - 13.0));
            }
            if enabled {
                if kind == 0 {
                    self.knob(&p, o, cr, f, false);
                } else {
                    self.spr(&p, o, BM_CAP, pos2(cr.left(), cr.top() + ((1.0 - f) * (54.0 - 13.0)).ceil()));
                }
                if let Some(er) = er {
                    text(&p, er, &short(&pp, v), &font, Color32::BLACK);
                }
                if let Some(nv) = self.drag_ctl(ui, o, cr, Knob::Param(key, lo), v, lo_v, hi_v) {
                    self.set_val(&b, &pp, 0, nv, &mut ch);
                }
            }
        }
        // chorus / reverb types
        for (k, (slot, br, dr)) in [
            (FxSlot::Chorus, r(444.0, 248.0, 89.0, 11.0), r(496.0, 229.0, 44.0, 15.0)),
            (FxSlot::Reverb, r(556.0, 248.0, 89.0, 11.0), r(608.0, 229.0, 44.0, 15.0)),
        ]
        .into_iter()
        .enumerate()
        {
            let (m, l) = self.fx_cur(&t, slot);
            let name = t.fx_name(m, l);
            text(&p, br, &name, &font, Color32::BLACK);
            if ui.interact(br.translate(o.to_vec2()), ui.id().with(("xgfx2", k)), Sense::click()).clicked() {
                self.panel.menu = Some((Menu::FxType(slot), o + vec2(br.left(), br.bottom())));
            }
            self.detail_btn(ui, &p, o, dr, Detail::Fx(slot), &format!("xgd_cr{k}"), !name.trim().is_empty());
        }
        self.detail_btn(ui, &p, o, r(88.0, 354.0, 44.0, 15.0), Detail::Block("System"), "xgd_sys", t.block("System").is_some());

        // ---- Drum ----
        let dmode = p1.and_then(|b| b.params.iter().find(|pp| pp.lo == 7).map(|pp| self.val(b, pp, self.part))).unwrap_or(0);
        // the drum setup of the selected part (setup 1 when it plays none)
        let dname = match dmode {
            0 | 1 | 2 => "DRUM".to_string(),
            m => format!("DRUM{}", m - 1),
        };
        text(&p, r(154.0, 381.0, 73.0, 11.0), &dname, &font, Color32::BLACK);
        self.detail_btn(ui, &p, o, r(192.0, 354.0, 44.0, 15.0), Detail::Block("DrumSet_1"), "xgd_drum", t.block("DrumSet_1").is_some());

        // ---- EQ ----
        if let Some(eq) = t.block("Eq Blk").cloned() {
            let get = |me: &Self, lo: u8| eq.params.iter().find(|pp| pp.lo == lo).map(|pp| (pp.clone(), me.val(&eq, pp, 0)));
            if let Some((tp, tv)) = get(self, 0) {
                const EQ_TYPES: [&str; 5] = ["FLAT", "JAZZ", "POPS", "ROCK", "CLASSIC"];
                let name = EQ_TYPES.get(tv as usize).map_or_else(|| param_display(&tp, tv), |s| s.to_string());
                text(&p, r(258.0, 381.0, 68.0, 11.0), &name, &font, Color32::BLACK);
            }
            let band = self.panel.eq_band;
            let gains: Vec<i32> = (0..5).map(|k| get(self, 1 + 4 * k).map_or(0, |(pp, v)| v as i32 + pp.disp_offset)).collect();
            // band frequency as 0..1 over its parameter range
            let freqs: Vec<f32> = (0..5)
                .map(|k| get(self, 2 + 4 * k).map_or(0.5, |(pp, v)| (v.saturating_sub(pp.min())) as f32 / (pp.max().max(pp.min() + 1) - pp.min()) as f32))
                .collect();
            // 15 columns: each band lifts / lowers the columns around its
            // frequency; flat = empty, as in the original
            let sel_col = (freqs[band] * 14.0).round() as usize;
            for c in 0..15 {
                let fx = c as f32 / 14.0;
                let g: f32 = (0..5).map(|k| gains[k] as f32 * (-((fx - freqs[k]) * 6.0).powi(2)).exp()).sum();
                let x = EQ_COLS.0 + 10.0 * c as f32;
                let h = (g.abs().min(12.0) / 12.0 * 40.0).round();
                if h >= 1.0 {
                    let col = if g > 0.0 { Color32::from_rgb(0x33, 0xDD, 0xCC) } else { Color32::from_rgb(0xCC, 0x55, 0x44) };
                    p.rect_filled(r(x, EQ_COLS.1 + BG_Y - h, 8.0, h).translate(o.to_vec2()), 0.0, col);
                }
                if c == sel_col {
                    p.rect_filled(r(x, EQ_COLS.1 + BG_Y, 8.0, 2.0).translate(o.to_vec2()), 0.0, Color32::from_rgb(0xFF, 0xCC, 0x33));
                }
            }
            text(&p, r(511.0, 372.0, 22.0, 10.0), &(band + 1).to_string(), &font, Color32::BLACK);
            self.knob(&p, o, r(507.0, 384.0, 30.0, 30.0), band as f32 / 4.0, true);
            if let Some(nb) = self.drag_ctl(ui, o, r(507.0, 384.0, 30.0, 30.0), Knob::EqBand, band as u32, 0, 4) {
                self.panel.eq_band = nb as usize;
            }
            if let Some((gp, gv)) = get(self, 1 + 4 * band as u8) {
                let (lo_v, hi_v) = (gp.min(), gp.max().max(gp.min() + 1));
                text(&p, r(552.0, 372.0, 22.0, 10.0), &short(&gp, gv), &font, Color32::BLACK);
                self.knob(&p, o, r(548.0, 384.0, 30.0, 30.0), (gv - lo_v) as f32 / (hi_v - lo_v) as f32, false);
                if let Some(nv) = self.drag_ctl(ui, o, r(548.0, 384.0, 30.0, 30.0), Knob::EqGain, gv, lo_v, hi_v) {
                    self.set_val(&eq, &gp, 0, nv, &mut ch);
                }
            }
            if let Some((fp, fv)) = get(self, 2 + 4 * band as u8) {
                let (lo_v, hi_v) = (fp.min(), fp.max().max(fp.min() + 1));
                let fr = r(340.0, 419.0, 157.0, 7.0);
                let x = fr.left() + (fv - lo_v) as f32 / (hi_v - lo_v) as f32 * (157.0 - 12.0);
                self.spr(&p, o, BM_FREQ_THUMB, pos2(x, fr.top()));
                text(&p, r(286.0, 413.0, 41.0, 10.0), param_display(&fp, fv).trim_end_matches(" kHz"), &font, Color32::BLACK);
                let resp = ui.interact(fr.translate(o.to_vec2()).expand(2.0), ui.id().with("xgeqf"), Sense::click_and_drag());
                if resp.is_pointer_button_down_on() {
                    if let Some(pt) = resp.interact_pointer_pos() {
                        let f = ((pt.x - o.x - fr.left() - 6.0) / (157.0 - 12.0)).clamp(0.0, 1.0);
                        self.set_val(&eq, &fp, 0, lo_v + (f * (hi_v - lo_v) as f32).round() as u32, &mut ch);
                    }
                }
            }
        }
        self.detail_btn(ui, &p, o, r(543.0, 354.0, 44.0, 15.0), Detail::Block("Eq Blk"), "xgd_eq", t.block("Eq Blk").is_some());

        self.menus(ui, &t, &mut ch);
        self.table = Some(t);
        ch
    }

    /// A DETAIL button: the original draws the active one (bitmap 8860) over
    /// the background's greyed one when the unit has parameters to show.
    #[allow(clippy::too_many_arguments)]
    fn detail_btn(&mut self, ui: &mut egui::Ui, p: &egui::Painter, o: Pos2, at: Rect, d: Detail, id: &str, on: bool) {
        if !on {
            return;
        }
        self.spr(p, o, BM_DETAIL, at.min);
        self.detail_hit(ui, o, at, d, id);
    }

    fn detail_hit(&mut self, ui: &mut egui::Ui, o: Pos2, at: Rect, d: Detail, id: &str) {
        if ui.interact(at.translate(o.to_vec2()), ui.id().with(id), Sense::click()).clicked() {
            self.panel.detail = Some(d);
        }
    }

    /// Vertical drag (1 step per pixel) and the wheel on a knob / fader.
    #[allow(clippy::too_many_arguments)]
    fn drag_ctl(&mut self, ui: &mut egui::Ui, o: Pos2, at: Rect, k: Knob, v: u32, lo: u32, hi: u32) -> Option<u32> {
        let resp = ui.interact(at.translate(o.to_vec2()), ui.id().with(("xgk", format!("{k:?}"))), Sense::click_and_drag());
        let mut out = None;
        if resp.drag_started() {
            self.panel.drag = Some((k, 0.0, v));
        }
        if let Some((dk, acc, v0)) = self.panel.drag.as_mut() {
            if *dk == k && resp.dragged() {
                *acc -= resp.drag_delta().y;
                out = Some((*v0 as f32 + *acc).round().clamp(lo as f32, hi as f32) as u32);
            }
        }
        if resp.drag_stopped() {
            self.panel.drag = None;
        }
        if resp.hovered() {
            let w = ui.input(|i| i.raw_scroll_delta.y);
            if w != 0.0 {
                out = Some((v as i64 + w.signum() as i64).clamp(lo as i64, hi as i64) as u32);
            }
        }
        out.filter(|&n| n != v)
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, p: &egui::Painter, o: Pos2, t: &XgTable, ch: &mut Vec<Change>) {
        const TIPS: [&str; 10] = [
            "Open XG Editor Parameter File (.SYX)",
            "Save XG Editor Parameter File (.SYX)",
            "XG Editor Setup",
            "Model Setup",
            "Transmit XG System On",
            "Transmit XG Parameter",
            "Receive XG Bulk",
            "Load XG Parameter (from the song)",
            "Insert XG Parameter (into the song)",
            "Record XG Parameter (while the song plays)",
        ];
        let tex = self.panel.sprites.as_ref().and_then(|s| s.get(&BM_TOOLS)).map(|t| t.id());
        for (k, x) in TOOL_X.iter().enumerate() {
            let br = r(*x, 3.0, 22.0, 21.0).translate(o.to_vec2());
            // no MIDI input in the browser
            let ready = !(k == 6 && cfg!(target_arch = "wasm32"));
            let tip = if ready { TIPS[k] } else { "Receive XG Bulk (not in the browser version: no MIDI input)" };
            let resp = ui.interact(br, ui.id().with(("xgtool", k)), Sense::click()).on_hover_text(tip);
            let down = k == 9 && self.recording;
            if down {
                p.rect_filled(br, 0.0, Color32::from_gray(0xA0));
                p.rect_stroke(br, 0.0, Stroke::new(1.0_f32, Color32::from_gray(0x40)));
            } else if resp.hovered() && ready {
                p.rect_stroke(br, 0.0, Stroke::new(1.0_f32, Color32::from_gray(0x80)));
            }
            if let Some(id) = tex {
                let uv = Rect::from_min_max(pos2(k as f32 / 10.0, 0.0), pos2((k + 1) as f32 / 10.0, 1.0));
                let tint = if ready { Color32::WHITE } else { Color32::from_white_alpha(110) };
                let at = br.min + vec2(3.0, 3.0) + if down { vec2(1.0, 1.0) } else { vec2(0.0, 0.0) };
                p.image(id, Rect::from_min_size(at, vec2(16.0, 15.0)), uv, tint);
            }
            if !(resp.clicked() && ready) {
                continue;
            }
            match k {
                0 => self.tool = Some(XgTool::Open),
                1 => self.tool = Some(XgTool::Save),
                2 => self.tool = Some(XgTool::Setup),
                3 => self.panel.menu = Some((Menu::Model, br.left_bottom())),
                4 => self.sends.push(([0, 0, 0x7E], vec![0])),
                5 => self.sends.extend(self.mem.changes(t, false)),
                6 => self.tool = Some(XgTool::Bulk),
                7 => self.tool = Some(XgTool::Load),
                8 => self.tool = Some(XgTool::Insert),
                _ => self.recording = !self.recording,
            }
        }
        let _ = ch;
    }

    /// The open drop-down list (effect types, voices, models).
    fn menus(&mut self, ui: &mut egui::Ui, t: &XgTable, ch: &mut Vec<Change>) {
        let Some((menu, at)) = self.panel.menu.clone() else { return };
        let mut close = false;
        let area = egui::Area::new(ui.id().with("xgmenu")).order(egui::Order::Foreground).fixed_pos(at).show(ui.ctx(), |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_max_height(300.0);
                match &menu {
                    Menu::FxType(slot) => {
                        let (m, l) = self.fx_cur(t, *slot);
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            for (tm, tl, _) in t.types(*slot) {
                                if ui.selectable_label((*tm, *tl) == (m, l), t.fx_name(*tm, *tl)).clicked() {
                                    if (*tm, *tl) != (m, l) {
                                        self.mem.clear_fx_params(*slot);
                                        self.mem.set(slot.type_addr(), &[*tm, *tl]);
                                        ch.push((slot.type_addr(), vec![*tm, *tl]));
                                    }
                                    close = true;
                                }
                            }
                        });
                    }
                    Menu::Voice(part) => {
                        let Some(p1) = t.block("Part_1") else { return };
                        let (h, mm) = p1.hm(*part);
                        let cur = self.mem.read([h, mm, 1], &[0, 0, 0]);
                        ui.text_edit_singleline(&mut self.voice_filter).request_focus();
                        let f = self.voice_filter.to_lowercase();
                        let mut pick = None;
                        egui::ScrollArea::vertical().show(ui, |ui| {
                            let mut cat = "";
                            for v in self.voices.iter().filter(|v| f.is_empty() || v.name.to_lowercase().contains(&f)) {
                                if v.category != cat {
                                    ui.weak(&v.category);
                                    cat = &v.category;
                                }
                                let on = [v.msb, v.lsb, v.program] == cur[..];
                                if ui.selectable_label(on, format!("{:3}  {}", v.program as u32 + 1, v.name)).clicked() {
                                    pick = Some([v.msb, v.lsb, v.program]);
                                }
                            }
                        });
                        if let Some(d) = pick {
                            self.mem.set([h, mm, 1], &d);
                            ch.push(([h, mm, 1], d.to_vec()));
                            close = true;
                        }
                    }
                    Menu::Model => {
                        let mut sel = None;
                        // the original's model tree: MU Series, Sound Card, …
                        egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                            let mut shown = vec![false; self.models.len()];
                            for (g, names) in &self.tree {
                                let idx: Vec<usize> = names.iter().filter_map(|n| self.models.iter().position(|m| &m.0 == n)).collect();
                                if idx.is_empty() {
                                    continue;
                                }
                                ui.label(egui::RichText::new(g).small().strong());
                                for i in idx {
                                    shown[i] = true;
                                    if ui.selectable_label(i == self.sel_model, format!("   {}", self.models[i].0)).clicked() {
                                        sel = Some(i);
                                    }
                                }
                            }
                            for (i, m) in self.models.iter().enumerate().filter(|(i, _)| !shown[*i]) {
                                if ui.selectable_label(i == self.sel_model, &m.0).clicked() {
                                    sel = Some(i);
                                }
                            }
                        });
                        if let Some(i) = sel {
                            self.sel_model = i;
                            close = true;
                        }
                    }
                }
            });
        });
        let outside = ui.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|q| !area.response.rect.contains(q)));
        let esc = ui.input(|i| i.key_pressed(egui::Key::Escape));
        if close || esc || outside {
            let model_changed = menu == Menu::Model && close;
            self.panel.menu = None;
            if model_changed {
                // `self.table` is put back by the caller; reload it next frame
                self.reload_model = true;
            }
        }
    }

    /// The XG Editor Assignable Setup dialog (Preset 1–4: name, button, slider).
    pub fn assign_dialog(&mut self, ui: &mut egui::Ui) -> bool {
        let Some(t) = self.table.as_ref() else { return false };
        let mut close = false;
        let rcv: Vec<(u8, String)> = t
            .blocks
            .iter()
            .filter(|b| b.key.starts_with("Part_"))
            .flat_map(|b| b.params.iter())
            .filter(|p| p.name.starts_with("Rcv") && p.max() <= 1 + p.min())
            .map(|p| (p.lo, p.name.clone()))
            .collect();
        let sld: Vec<(u8, String)> = t
            .blocks
            .iter()
            .filter(|b| b.key.starts_with("Part_"))
            .flat_map(|b| b.params.iter())
            .filter(|p| !p.name.is_empty() && p.max() > p.min() + 1 && !(1..=3).contains(&p.lo))
            .map(|p| (p.lo, p.name.clone()))
            .collect();
        egui::Grid::new("xgassign_grid").num_columns(4).spacing([8.0, 6.0]).show(ui, |ui| {
            ui.label("");
            ui.strong("Preset Name");
            ui.strong("Button");
            ui.strong("Slider");
            ui.end_row();
            for (k, (name, b, s)) in self.panel.presets.iter_mut().enumerate() {
                ui.label((k + 1).to_string());
                ui.add(egui::TextEdit::singleline(name).desired_width(80.0));
                let bn = rcv.iter().find(|x| x.0 == *b).map_or("-", |x| x.1.as_str());
                egui::ComboBox::from_id_source(("xgab", k)).width(130.0).selected_text(bn).show_ui(ui, |ui| {
                    for (lo, n) in &rcv {
                        ui.selectable_value(b, *lo, n);
                    }
                });
                let sn = sld.iter().find(|x| x.0 == *s).map_or("-", |x| x.1.as_str());
                egui::ComboBox::from_id_source(("xgas", k)).width(150.0).height(300.0).selected_text(sn).show_ui(ui, |ui| {
                    for (lo, n) in &sld {
                        ui.selectable_value(s, *lo, n);
                    }
                });
                ui.end_row();
            }
        });
        ui.add_space(6.0);
        if ui.button("  OK  ").clicked() {
            close = true;
        }
        !close
    }

    /// The parameter list behind a DETAIL button.
    pub fn detail_ui(&mut self, ui: &mut egui::Ui, track_names: &[(u8, String)]) -> Vec<Change> {
        let mut ch = Vec::new();
        let Some(d) = self.panel.detail.clone() else { return ch };
        let Some(t) = self.table.take() else { return ch };
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match d {
            Detail::Fx(slot) => self.fx_slot_ui(ui, &t, slot, &mut ch),
            Detail::Part => self.parts_ui(ui, &t, track_names, &mut ch),
            Detail::Block(key) => {
                if let Some(i) = t.blocks.iter().position(|b| b.key == key) {
                    self.block = i;
                    self.inst = if key == "DrumSet_1" { self.inst } else { 0 };
                }
                self.blocks_ui(ui, &t, &mut ch);
            }
        });
        self.table = Some(t);
        ch
    }
}

/// Short value for the small edit boxes ("127", "0.0", "C", "+3").
fn short(p: &TblParam, v: u32) -> String {
    let s = param_display(p, v);
    let s = s.trim_end_matches(" cent").trim_end_matches(" dB").trim_end_matches(" semitones");
    // zero without a sign, as the original shows it
    s.strip_prefix('+').filter(|r| r.trim_start_matches(['0', '.']).is_empty()).unwrap_or(s).to_string()
}

/// A knob's red pointer, -135° .. +135° over the range.
impl XgEditor {
    /// A knob at `at` (30×30) for the value fraction `f`: the original's
    /// bitmaps (7 positions, the EQ band knob 5 with dots), else a drawn line.
    fn knob(&self, p: &egui::Painter, o: Pos2, at: Rect, f: f32, five: bool) {
        let f = f.clamp(0.0, 1.0);
        let id = if five { BM_KNOB5[(f * 4.0).round() as usize] } else { BM_KNOB[(f * 6.0).round() as usize] };
        if !self.spr(p, o, id, at.min) {
            knob(p, o + at.center().to_vec2(), f);
        }
    }
}

fn knob(p: &egui::Painter, c: Pos2, f: f32) {
    let a = (-135.0 + 270.0 * f.clamp(0.0, 1.0)).to_radians();
    let d = vec2(a.sin(), -a.cos());
    p.line_segment([c + d * 4.0, c + d * 11.0], Stroke::new(2.0_f32, Color32::from_rgb(0xE0, 0x10, 0x10)));
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;

    /// Run one frame of the panel with these input events.
    fn frame(ctx: &egui::Context, ed: &mut XgEditor, events: Vec<egui::Event>) -> Vec<Change> {
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(CLIENT[0], CLIENT[1]))),
            events,
            ..Default::default()
        };
        let mut ch = Vec::new();
        let _ = ctx.run(input, |ctx| {
            egui::CentralPanel::default().frame(egui::Frame::none()).show(ctx, |ui| ch = ed.panel_ui(ui));
        });
        ch
    }

    fn click(ctx: &egui::Context, ed: &mut XgEditor, x: f32, y: f32) -> Vec<Change> {
        let pos = pos2(x, y);
        let btn = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        let mut ch = frame(ctx, ed, vec![egui::Event::PointerMoved(pos)]);
        ch.extend(frame(ctx, ed, vec![btn(true)]));
        ch.extend(frame(ctx, ed, vec![btn(false)]));
        ch.extend(frame(ctx, ed, vec![]));
        ch
    }

    #[test]
    fn main_window_controls() {
        let mut ed = XgEditor::new();
        if ed.table.is_none() {
            return; // no XGworks data on this machine
        }
        let ctx = egui::Context::default();
        frame(&ctx, &mut ed, vec![]);
        // Variation INS / SYS switch: insertion (0) by default -> system (1)
        let ch = click(&ctx, &mut ed, 591.0, 87.0);
        assert!(ch.contains(&([2, 1, 0x5A], vec![1])), "{ch:?}");
        // Insertion 2 on part 5
        let ch = click(&ctx, &mut ed, COL_X + 16.0 + 7.0, ROW_Y + ROW_H * 4.0 + 6.0);
        assert!(ch.contains(&([3, 1, 0x0C], vec![4])), "{ch:?}");
        // part 1 drum mode: Normal -> Drum
        let ch = click(&ctx, &mut ed, DR_X + 7.0, ROW_Y + 6.0);
        assert!(ch.contains(&([8, 0, 7], vec![1])), "{ch:?}");
        // the assignable button of part 2 (Rcv Vol) turns off
        let ch = click(&ctx, &mut ed, A_BTN_X + 8.0, A_Y + ROW_H + 5.0);
        assert!(ch.contains(&([8, 1, 0x39], vec![0])), "{ch:?}");
        // Preset 2 makes the sliders Pan; a click at the slider's left end pans part 1 hard left
        click(&ctx, &mut ed, PRESET.0 + 20.0, PRESET.1 + 19.0 + 7.0);
        assert_eq!(ed.panel.preset, 1);
        let ch = click(&ctx, &mut ed, A_SLD_X + 2.0, A_Y + 5.0);
        assert!(ch.iter().any(|c| c.0 == [8, 0, 0x0E] && c.1[0] <= 1), "{ch:?}");
        // the tab 17-32 shows parts 17..
        click(&ctx, &mut ed, TAB.0 + TAB.2 + 20.0, TAB.1 + 8.0);
        assert_eq!(ed.panel.page, 1);
        // a type box opens its list; a DETAIL button its parameters
        click(&ctx, &mut ed, 470.0, 255.0);
        assert!(matches!(ed.panel.menu, Some((Menu::FxType(FxSlot::Chorus), _))));
        click(&ctx, &mut ed, 5.0, 5.0); // outside closes it
        assert!(ed.panel.menu.is_none());
        click(&ctx, &mut ed, 620.0, 238.0);
        assert_eq!(ed.panel.detail, Some(Detail::Fx(FxSlot::Reverb)));
        // XG System On is sent, not written into the song
        click(&ctx, &mut ed, TOOL_X[4] + 10.0, 14.0);
        assert!(ed.sends.contains(&([0, 0, 0x7E], vec![0])));
        // the file / song tools are requests for the app; Record toggles
        for (k, tool) in [(0, XgTool::Open), (1, XgTool::Save), (2, XgTool::Setup), (6, XgTool::Bulk), (7, XgTool::Load), (8, XgTool::Insert)] {
            click(&ctx, &mut ed, TOOL_X[k] + 10.0, 14.0);
            assert_eq!(ed.tool.take(), Some(tool));
        }
        click(&ctx, &mut ed, TOOL_X[9] + 10.0, 14.0);
        assert!(ed.recording);
    }
}
