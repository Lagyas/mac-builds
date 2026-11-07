//! The XGM Editor's effect window (after Midifile Optimizer's Reverb /
//! Chorus / DSP windows): XG Reverb, Chorus, Variation and Insertion types
//! (categories and types, the type's parameters as knobs, return / pan /
//! sends), GS Reverb / Chorus / Delay macros and parameters and the SC-88Pro
//! Insertion Effect, GM2 Reverb / Chorus. Sent to MIDI Out and written into
//! the song (its own message when it has one).

use super::xgmwin::{pan_text, w};
use super::*;
use xgworks_core::fxsysex::{self as fx, GsParam};
use xgworks_core::soundset::Standard;
use xgworks_core::xgtbl::{combine14, split14, FxSlot};
use xgworks_core::xgunits::{fx_display, fx_slot_desc};

/// XG unit knobs: (label, address, default).
fn xg_unit(slot: FxSlot) -> &'static [(&'static str, [u8; 3], u8)] {
    match slot {
        FxSlot::Reverb => &[("Return", [2, 1, 0x0C], 64), ("Pan", [2, 1, 0x0D], 64)],
        FxSlot::Chorus => &[("Return", [2, 1, 0x2C], 64), ("Pan", [2, 1, 0x2D], 64), ("To Rev", [2, 1, 0x2E], 0)],
        FxSlot::Variation => &[("Return", [2, 1, 0x56], 64), ("Pan", [2, 1, 0x57], 64), ("To Rev", [2, 1, 0x58], 0), ("To Cho", [2, 1, 0x59], 0)],
        FxSlot::Insertion(_) => &[],
    }
}

/// "HALL1" -> "HALL", "WHITE ROOM" -> "WHITE": the category of a type name.
fn category(name: &str) -> String {
    let base = name.trim_end_matches(|c: char| c.is_ascii_digit() || c == ' ' || c == '*');
    base.split_whitespace().next().unwrap_or(base).to_string()
}

enum FxEdit {
    Xg(Vec<([u8; 3], Vec<u8>)>),
    Gs([u8; 3], u8),
    Gm2(u8, u8, u8),
}

impl App {
    fn xgm_fx_send(&mut self, msg: &[u8]) {
        let dev = self.port_device(self.xg.setup.ports[0]);
        if self.ensure_midi() && open_device(&self.midi, dev) {
            send_dev(&self.midi, dev, msg);
        }
    }

    pub(super) fn xgm_effects(&mut self, ctx: &egui::Context) {
        if !self.xgm.fx_open || self.edit.is_empty() {
            return;
        }
        let std = self.xgm.std;
        let mut edits: Vec<FxEdit> = Vec::new();
        let mut tab = self.xgm.fx_tab;
        let title = format!("XGM Effects - {}", self.xgm_inst_name());
        let gs = if matches!(std, Standard::Gs(_)) { fx::gs_values(&self.edit) } else { Default::default() };
        let gm2 = if matches!(std, Standard::Gm2 | Standard::Kbd(_)) { fx::gm2_values(&self.edit) } else { Default::default() };
        let xg_table = self.xg.table().cloned();
        let open = sub_window(ctx, "xgm_fx", &title, [980.0, 470.0], |ui| {
            egui::Frame::none().fill(w::PANEL).inner_margin(egui::Margin::same(8.0)).rounding(4.0).show(ui, |ui| {
                ui.visuals_mut().override_text_color = Some(w::TEXT);
                match std {
                    Standard::Xg => {
                        let Some(t) = xg_table.as_ref() else {
                            ui.label("No XG parameter table (XG Editor data) found.");
                            return;
                        };
                        let mut slots = vec![FxSlot::Reverb, FxSlot::Chorus, FxSlot::Variation];
                        for n in 0..2u8 {
                            if t.block(&format!("Ins{}_1", n + 1)).is_some() && !t.types(FxSlot::Insertion(n)).is_empty() {
                                slots.push(FxSlot::Insertion(n));
                            }
                        }
                        tab = tab.min(slots.len() - 1);
                        ui.horizontal(|ui| {
                            for (k, s) in slots.iter().enumerate() {
                                if w::pill(ui, &s.label().to_uppercase(), k == tab).clicked() {
                                    tab = k;
                                }
                            }
                        });
                        ui.add_space(6.0);
                        self.xgm_xg_unit(ui, t, slots[tab], &mut edits);
                    }
                    Standard::Gs(map) => {
                        let mut tabs = vec!["REVERB", "CHORUS"];
                        if map >= 2 {
                            tabs.push("DELAY");
                        }
                        if map >= 3 {
                            tabs.push("INSERTION");
                        }
                        tab = tab.min(tabs.len() - 1);
                        ui.horizontal(|ui| {
                            for (k, s) in tabs.iter().enumerate() {
                                if w::pill(ui, s, k == tab).clicked() {
                                    tab = k;
                                }
                            }
                        });
                        ui.add_space(6.0);
                        let get = |a: [u8; 3], d: u8| *gs.get(&a).unwrap_or(&d);
                        let macro_unit = |ui: &mut egui::Ui, params: &[GsParam], names: &[&str], edits: &mut Vec<FxEdit>| {
                            ui.horizontal_top(|ui| {
                                let items: Vec<String> = names.iter().map(|s| s.to_string()).collect();
                                let cur = get(params[0].addr, params[0].default) as usize;
                                if let Some(k) = w::list(ui, ("gsm", params[0].addr), &items, Some(cur), 170.0, 260.0) {
                                    edits.push(FxEdit::Gs(params[0].addr, k as u8));
                                }
                                ui.add_space(10.0);
                                knob_grid(ui, params[1..].iter().map(|p| (p.name.to_string(), p.addr, p.max as i32, p.default as i32, get(p.addr, p.default) as i32)), |a, v| {
                                    edits.push(FxEdit::Gs(a, v));
                                });
                            });
                        };
                        match tabs[tab] {
                            "REVERB" => macro_unit(ui, &fx::GS_REVERB, &fx::GS_REVERB_MACROS, &mut edits),
                            "CHORUS" => macro_unit(ui, &fx::GS_CHORUS, &fx::GS_CHORUS_MACROS, &mut edits),
                            "DELAY" => macro_unit(ui, &fx::GS_DELAY, &fx::GS_DELAY_MACROS, &mut edits),
                            _ => {
                                // SC-88Pro EFX: type, 20 parameters, sends, the parts that use it
                                ui.horizontal(|ui| {
                                    ui.label("EFX type");
                                    ui.visuals_mut().override_text_color = None;
                                    let mut m = get(fx::GS_EFX_TYPE, 0) as i32;
                                    let mut l = get([0x40, 0x03, 0x01], 0) as i32;
                                    if ui.add(egui::DragValue::new(&mut m).range(0..=127).hexadecimal(2, false, true)).on_hover_text("EFX type MSB (40 03 00)").changed() {
                                        edits.push(FxEdit::Gs(fx::GS_EFX_TYPE, m as u8));
                                    }
                                    if ui.add(egui::DragValue::new(&mut l).range(0..=127).hexadecimal(2, false, true)).on_hover_text("EFX type LSB (40 03 01)").changed() {
                                        edits.push(FxEdit::Gs([0x40, 0x03, 0x01], l as u8));
                                    }
                                    ui.weak("(00 00 = Thru; parameters as the SC-88Pro manual lists them for the type)");
                                });
                                knob_grid(ui, (0..20).map(|k| (format!("P{}", k + 1), fx::gs_efx_param(k), 127, 64, get(fx::gs_efx_param(k), 64) as i32)), |a, v| edits.push(FxEdit::Gs(a, v)));
                                ui.horizontal(|ui| {
                                    for (lo, name) in fx::GS_EFX_SENDS {
                                        let a = [0x40, 0x03, lo];
                                        let mut v = get(a, if lo == 0x17 { 40 } else { 0 }) as i32;
                                        let t = v.to_string();
                                        if w::knob(ui, ("efxs", lo), &mut v, 0, 127, 0, 38.0, name, &t).changed() {
                                            edits.push(FxEdit::Gs(a, v as u8));
                                        }
                                    }
                                    ui.add_space(12.0);
                                    ui.vertical(|ui| {
                                        ui.label("Parts with the effect");
                                        ui.horizontal(|ui| {
                                            ui.spacing_mut().item_spacing.x = 2.0;
                                            for part in 0..16u8 {
                                                let a = fx::gs_part_efx(part);
                                                let on = get(a, 0) != 0;
                                                if w::small_toggle(ui, &(part + 1).to_string(), on, w::LCD).clicked() {
                                                    edits.push(FxEdit::Gs(a, !on as u8));
                                                }
                                            }
                                        });
                                    });
                                });
                            }
                        }
                    }
                    // the keyboard takes the GM2 Reverb / Chorus messages
                    Standard::Gm2 | Standard::Kbd(_) => {
                        let tabs = ["REVERB", "CHORUS"];
                        tab = tab.min(1);
                        ui.horizontal(|ui| {
                            for (k, s) in tabs.iter().enumerate() {
                                if w::pill(ui, s, k == tab).clicked() {
                                    tab = k;
                                }
                            }
                        });
                        ui.add_space(6.0);
                        let slot = if tab == 0 { fx::GM2_REVERB } else { fx::GM2_CHORUS };
                        let get = |pp: u8, d: u8| *gm2.get(&(slot, pp)).unwrap_or(&d);
                        ui.horizontal_top(|ui| {
                            let (items, codes): (Vec<String>, Vec<u8>) = if slot == fx::GM2_REVERB {
                                fx::GM2_REVERB_TYPES.iter().map(|(c, n)| (n.to_string(), *c)).unzip()
                            } else {
                                fx::GM2_CHORUS_TYPES.iter().enumerate().map(|(c, n)| (n.to_string(), c as u8)).unzip()
                            };
                            let def = fx::GM2_PARAMS.iter().find(|p| p.0 == slot && p.1 == 0).map_or(0, |p| p.3);
                            let cur = codes.iter().position(|&c| c == get(0, def));
                            if let Some(k) = w::list(ui, ("gm2t", slot), &items, cur, 170.0, 200.0) {
                                edits.push(FxEdit::Gm2(slot, 0, codes[k]));
                            }
                            ui.add_space(10.0);
                            for (s, pp, name, d) in fx::GM2_PARAMS.iter().filter(|p| p.0 == slot && p.1 != 0) {
                                let mut v = get(*pp, *d) as i32;
                                let t = v.to_string();
                                if w::knob(ui, ("gm2k", s, pp), &mut v, 0, 127, *d as i32, 44.0, name, &t).changed() {
                                    edits.push(FxEdit::Gm2(*s, *pp, v as u8));
                                }
                            }
                        });
                    }
                    Standard::Gm => {
                        ui.label("GM has no effect settings: each channel's Reverb and Chorus send is on its strip in the XGM Editor.");
                    }
                }
            });
        });
        self.xgm.fx_open = open;
        self.xgm.fx_tab = tab;
        let dev = self.xg.setup.dev;
        for e in edits {
            match e {
                FxEdit::Xg(ch) => {
                    for (a, d) in &ch {
                        self.xg.mem.set(*a, d);
                    }
                    self.xg_apply(&ch);
                }
                FxEdit::Gs(a, v) => {
                    self.xgm_fx_send(&fx::gs_dt1(dev, a, &[v]));
                    fx::set_gs(&mut self.edit, dev, a, &[v]);
                }
                FxEdit::Gm2(slot, pp, v) => {
                    self.xgm_fx_send(&fx::gm2_global(0x7F, slot, pp, v));
                    fx::set_gm2(&mut self.edit, 0x7F, slot, pp, v);
                }
            }
        }
    }

    /// One XG effect unit: categories, types, the type's parameters, the unit's knobs.
    fn xgm_xg_unit(&mut self, ui: &mut egui::Ui, t: &xgworks_core::xgtbl::XgTable, slot: FxSlot, edits: &mut Vec<FxEdit>) {
        let types = t.types(slot);
        if types.is_empty() {
            ui.label("No types for this model.");
            return;
        }
        let ta = slot.type_addr();
        // the song's type, else the model's default (HALL1 …)
        let def = self.xg.fx_default_type(t, slot);
        let cur = self.xg.mem.read(ta, &def);
        let (msb, lsb) = (cur[0], *cur.get(1).unwrap_or(&0));
        let names: Vec<String> = types.iter().map(|x| t.fx_name(x.0, x.1)).collect();
        let mut cats: Vec<String> = Vec::new();
        for n in &names {
            let c = category(n);
            if !cats.contains(&c) {
                cats.push(c);
            }
        }
        let cur_k = types.iter().position(|x| x.0 == msb && x.1 == lsb);
        let cur_cat = cur_k.map(|k| category(&names[k]));
        let key = egui::Id::new(("xgm_fx_cat", slot.label()));
        let mut cat: Option<String> = ui.data(|d| d.get_temp(key)).or(cur_cat);
        ui.horizontal_top(|ui| {
            let ci = cat.as_ref().and_then(|c| cats.iter().position(|x| x == c));
            if let Some(k) = w::list(ui, ("xgcat", slot.label()), &cats, ci, 120.0, 300.0) {
                cat = Some(cats[k].clone());
            }
            let in_cat: Vec<usize> = (0..types.len()).filter(|&k| cat.as_ref().is_none_or(|c| category(&names[k]) == *c)).collect();
            let items: Vec<String> = in_cat.iter().map(|&k| names[k].clone()).collect();
            let sel = cur_k.and_then(|k| in_cat.iter().position(|&x| x == k));
            if let Some(i) = w::list(ui, ("xgtype", slot.label()), &items, sel, 150.0, 300.0) {
                let (m, l, _) = &types[in_cat[i]];
                if (*m, *l) != (msb, lsb) {
                    // the device resets the parameters to the type's defaults
                    self.xg.mem.clear_fx_params(slot);
                    edits.push(FxEdit::Xg(vec![(ta, vec![*m, *l])]));
                }
            }
            ui.add_space(10.0);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new(t.fx_name(msb, lsb)).strong().size(15.0).color(w::ACCENT));
                let list = types.iter().find(|x| x.0 == msb && x.1 == lsb).map(|x| x.2.clone());
                let defaults = t.fx_type(msb, lsb).map(|f| f.defaults).unwrap_or([0; 16]);
                if let Some(params) = list.as_ref().and_then(|l| t.fx_params.get(l)) {
                    let mut knobs = Vec::new();
                    for (k, p) in params.iter().enumerate() {
                        let Some(p) = p else { continue };
                        let wide = matches!(slot, FxSlot::Insertion(_)) && p.max > 0x7F;
                        let (addr, wd) = slot.param_addr(k, wide);
                        let def = defaults[k];
                        let def_bytes = if wd == 2 { split14(def) } else { vec![(def & 0x7F) as u8] };
                        let bytes = self.xg.mem.read(addr, &def_bytes);
                        let v = if wd == 2 { combine14(&bytes) } else { bytes[0] as u32 };
                        let text = fx_slot_desc(params, k).map_or_else(|| v.to_string(), |d| fx_display(d, v, self.xg.enums()));
                        // the unit stays in the hover text: "Lch Delay (ms)" -> "Lch Delay"
                        let short = p.name.split(" (").next().unwrap_or(&p.name).to_string();
                        knobs.push((k, short, addr, wd, p.min.min(p.max) as i32, p.max.max(p.min) as i32, def as i32, v as i32, format!("{text}  ({})", p.name)));
                    }
                    egui::Grid::new(("xgfxk", slot.label())).spacing([6.0, 4.0]).show(ui, |ui| {
                        for (i, (k, name, addr, wd, lo, hi, def, v, text)) in knobs.into_iter().enumerate() {
                            let mut v = v;
                            if w::knob(ui, ("xgfx", slot.label(), k), &mut v, lo, hi, def, 40.0, &name, &text).changed() {
                                let data = if wd == 2 { split14(v as u32) } else { vec![v as u8] };
                                edits.push(FxEdit::Xg(vec![(addr, data)]));
                            }
                            if i % 6 == 5 {
                                ui.end_row();
                            }
                        }
                    });
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    for (name, addr, def) in xg_unit(slot) {
                        let mut v = self.xg.mem.get(*addr).unwrap_or(*def) as i32;
                        let text = if *name == "Pan" { pan_text(v) } else { v.to_string() };
                        if w::knob(ui, ("xgu", addr[2]), &mut v, 0, 127, *def as i32, 44.0, name, &text).changed() {
                            edits.push(FxEdit::Xg(vec![(*addr, vec![v as u8])]));
                        }
                    }
                });
            });
        });
        ui.data_mut(|d| {
            if let Some(c) = cat {
                d.insert_temp(key, c);
            }
        });
    }
}

/// Knobs in rows of 8: (label, address, max, default, value); `set` gets the changes.
fn knob_grid(ui: &mut egui::Ui, knobs: impl Iterator<Item = (String, [u8; 3], i32, i32, i32)>, mut set: impl FnMut([u8; 3], u8)) {
    egui::Grid::new(ui.next_auto_id()).spacing([6.0, 4.0]).show(ui, |ui| {
        for (i, (name, a, max, def, v)) in knobs.enumerate() {
            let mut v = v;
            let t = v.to_string();
            if w::knob(ui, ("gsk", a), &mut v, 0, max, def, 40.0, &name, &t).changed() {
                set(a, v as u8);
            }
            if i % 8 == 7 {
                ui.end_row();
            }
        }
    });
}
