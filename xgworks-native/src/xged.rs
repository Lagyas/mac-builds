//! XG Editor window (reimplements XGEditor.dll), driven by XGworks' own data
//! files: `xgeditor.ini` (models), `<model>.tbl` (parameter map) and the voice
//! list. The window only edits an `XgMemory`; every change is returned to the
//! caller, which sends it to the device and writes it into the song.

use std::path::PathBuf;

use eframe::egui;
use xgworks_core::xgtbl::{
    combine14, parse_models, parse_tbl, parse_voice_list, split14, FxSlot, TblBlock, TblParam, Voice, XgMemory,
    XgTable,
};
use xgworks_core::xgunits::{fx_display, fx_slot_desc, param_display, Enums};
use xgworks_core::EditTrack;

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Effects,
    Parts,
    Blocks,
}

pub struct XgEditor {
    data_dir: PathBuf,
    models: Vec<(String, String, String)>,
    sel_model: usize,
    table: Option<XgTable>,
    voices: Vec<Voice>,
    pub mem: XgMemory,
    tab: Tab,
    part: usize,
    block: usize,
    inst: usize,
    voice_filter: String,
    error: String,
    enums: Enums,
}

/// One parameter change: XG address + data bytes.
pub type Change = ([u8; 3], Vec<u8>);

impl XgEditor {
    pub fn new() -> Self {
        let mut ed = XgEditor {
            data_dir: crate::platform::xg4("XGEditor"),
            models: Vec::new(),
            sel_model: 0,
            table: None,
            voices: Vec::new(),
            mem: XgMemory::default(),
            tab: Tab::Effects,
            part: 0,
            block: 0,
            inst: 0,
            voice_filter: String::new(),
            error: String::new(),
            enums: Enums::default(),
        };
        ed.load_models();
        ed
    }

    fn load_models(&mut self) {
        // enum lists (Er Type, Pan Dir, …) live in XGworks' ws.ini, one level up
        let ws = self.data_dir.parent().map(|d| d.join("ws.ini"));
        self.enums = ws.and_then(|p| crate::platform::read(&p)).map(|b| Enums::parse(&b)).unwrap_or_default();
        self.models.clear();
        self.table = None;
        match crate::platform::read(&self.data_dir.join("xgeditor.ini")) {
            Some(b) => {
                self.models = parse_models(&b);
                self.sel_model = self.models.iter().position(|m| m.0 == "MU128").unwrap_or(0);
                self.load_model();
            }
            None => {
                self.error = format!("xgeditor.ini not found in {} — choose the XGworks XGEditor folder.", self.data_dir.display());
            }
        }
    }

    fn load_model(&mut self) {
        let Some((name, prm, vce)) = self.models.get(self.sel_model).cloned() else { return };
        match crate::platform::read(&self.data_dir.join(&prm)) {
            Some(b) => {
                self.table = Some(parse_tbl(&b));
                self.error.clear();
            }
            None => {
                self.table = None;
                self.error = format!("{name}: cannot read {prm}");
            }
        }
        self.voices = crate::platform::read(&self.data_dir.join(&vce)).map(|b| parse_voice_list(&b)).unwrap_or_default();
        self.block = 0;
        self.inst = 0;
    }

    /// Voice name from the model's voice list.
    pub fn voice_name(&self, msb: u8, lsb: u8, prg: u8) -> Option<&str> {
        self.voices.iter().find(|v| v.msb == msb && v.lsb == lsb && v.program == prg).map(|v| v.name.as_str())
    }

    /// The selected model's parameter table.
    pub fn table(&self) -> Option<&XgTable> {
        self.table.as_ref()
    }

    pub fn voices(&self) -> &[Voice] {
        &self.voices
    }

    /// Reload the parameter memory from the song's XG SysEx.
    pub fn sync_from_song(&mut self, tracks: &[EditTrack]) {
        self.mem = XgMemory::from_song(tracks);
    }

    /// Draw the editor; returns the changes made this frame.
    pub fn ui(&mut self, ui: &mut egui::Ui, track_names: &[(u8, String)]) -> Vec<Change> {
        let mut changes = Vec::new();
        ui.horizontal(|ui| {
            ui.label("Model");
            let cur = self.models.get(self.sel_model).map(|m| m.0.clone()).unwrap_or_else(|| "—".into());
            let mut sel = self.sel_model;
            egui::ComboBox::from_id_source("xg_model").selected_text(cur).show_ui(ui, |ui| {
                for (i, m) in self.models.iter().enumerate() {
                    ui.selectable_value(&mut sel, i, &m.0);
                }
            });
            if sel != self.sel_model {
                self.sel_model = sel;
                self.load_model();
            }
            #[cfg(not(target_arch = "wasm32"))]
            if ui.button("📁 Data folder…").on_hover_text(self.data_dir.display().to_string()).clicked() {
                if let Some(d) = rfd::FileDialog::new().set_directory(&self.data_dir).pick_folder() {
                    self.data_dir = d;
                    self.load_models();
                }
            }
            ui.separator();
            ui.selectable_value(&mut self.tab, Tab::Effects, "Effects");
            ui.selectable_value(&mut self.tab, Tab::Parts, "Multi Part");
            ui.selectable_value(&mut self.tab, Tab::Blocks, "All blocks");
        });
        if !self.error.is_empty() {
            ui.colored_label(egui::Color32::from_rgb(220, 90, 70), &self.error);
        }
        let Some(table) = self.table.take() else { return changes };
        ui.separator();
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.tab {
            Tab::Effects => self.effects_ui(ui, &table, &mut changes),
            Tab::Parts => self.parts_ui(ui, &table, track_names, &mut changes),
            Tab::Blocks => self.blocks_ui(ui, &table, &mut changes),
        });
        self.table = Some(table);
        changes
    }

    // -- effects ------------------------------------------------------------
    fn effects_ui(&mut self, ui: &mut egui::Ui, t: &XgTable, changes: &mut Vec<Change>) {
        let mut slots = vec![FxSlot::Reverb, FxSlot::Chorus, FxSlot::Variation];
        for n in 0..2u8 {
            if t.block(&format!("Ins{}_1", n + 1)).is_some() {
                slots.push(FxSlot::Insertion(n));
            }
        }
        for slot in slots {
            egui::CollapsingHeader::new(egui::RichText::new(slot.label()).strong())
                .default_open(!matches!(slot, FxSlot::Insertion(_)))
                .show(ui, |ui| self.fx_slot_ui(ui, t, slot, changes));
        }
    }

    fn fx_slot_ui(&mut self, ui: &mut egui::Ui, t: &XgTable, slot: FxSlot, changes: &mut Vec<Change>) {
        let types = t.types(slot);
        if types.is_empty() {
            ui.weak("No types for this model.");
            return;
        }
        let ta = slot.type_addr();
        let def_type = block_keys(slot)
            .first()
            .and_then(|k| t.block(k))
            .and_then(|b| b.params.first())
            .map(|p| p.bytes.iter().map(|b| *b.2.first().unwrap_or(&0)).collect::<Vec<u8>>())
            .unwrap_or_else(|| vec![0, 0]);
        let cur = self.mem.read(ta, &def_type);
        let (msb, lsb) = (cur[0], *cur.get(1).unwrap_or(&0));
        let mut sel = types.iter().position(|x| x.0 == msb && x.1 == lsb);
        ui.horizontal(|ui| {
            ui.label("Type");
            let text = t.fx_name(msb, lsb);
            egui::ComboBox::from_id_source(("fxtype", slot.label())).width(180.0).selected_text(text).show_ui(ui, |ui| {
                for (i, (m, l, _)) in types.iter().enumerate() {
                    ui.selectable_value(&mut sel, Some(i), t.fx_name(*m, *l));
                }
            });
        });
        if let Some(i) = sel {
            let (m, l, _) = &types[i];
            if (*m, *l) != (msb, lsb) {
                // the device resets the params to the type defaults on its own
                self.mem.clear_fx_params(slot);
                self.mem.set(ta, &[*m, *l]);
                changes.push((ta, vec![*m, *l]));
            }
        }
        let cur = self.mem.read(ta, &def_type);
        let (msb, lsb) = (cur[0], *cur.get(1).unwrap_or(&0));
        let list = types.iter().find(|x| x.0 == msb && x.1 == lsb).map(|x| x.2.clone());
        let defaults = t.fx_type(msb, lsb).map(|f| f.defaults).unwrap_or([0; 16]);
        if let Some(params) = list.as_ref().and_then(|l| t.fx_params.get(l)) {
            egui::Grid::new(("fxp", slot.label())).num_columns(2).striped(true).show(ui, |ui| {
                for (k, p) in params.iter().enumerate() {
                    let Some(p) = p else { continue };
                    let wide = matches!(slot, FxSlot::Insertion(_)) && p.max > 0x7F;
                    let (addr, w) = slot.param_addr(k, wide);
                    let def = defaults[k];
                    let def_bytes = if w == 2 { split14(def) } else { vec![(def & 0x7F) as u8] };
                    let bytes = self.mem.read(addr, &def_bytes);
                    let mut v = if w == 2 { combine14(&bytes) } else { bytes[0] as u32 };
                    ui.label(&p.name);
                    let (lo, hi) = (p.min.min(p.max), p.max.max(p.min));
                    let desc = fx_slot_desc(params, k);
                    ui.horizontal(|ui| {
                        if ui.add(egui::Slider::new(&mut v, lo..=hi).show_value(false)).changed() {
                            let data = if w == 2 { split14(v) } else { vec![v as u8] };
                            self.mem.set(addr, &data);
                            changes.push((addr, data));
                        }
                        let text = desc.map_or_else(|| v.to_string(), |d| fx_display(d, v, &self.enums));
                        ui.monospace(text).on_hover_text(format!("raw {v}"));
                    });
                    ui.end_row();
                }
            });
        }
        // the block's own (non-effect) parameters: return, pan, sends, connection…
        for key in block_keys(slot) {
            if let Some(b) = t.block(key) {
                let (h, m) = b.hm(0);
                params_grid(ui, &mut self.mem, b, 0, h, m, changes, |p| p.lo != ta[2] && p.conv != 2);
            }
        }
    }

    // -- multi part ---------------------------------------------------------
    fn parts_ui(&mut self, ui: &mut egui::Ui, t: &XgTable, track_names: &[(u8, String)], changes: &mut Vec<Change>) {
        ui.horizontal_wrapped(|ui| {
            ui.label("Part");
            for p in 0..16 {
                let names: Vec<&str> =
                    track_names.iter().filter(|(ch, _)| *ch as usize == p).map(|(_, n)| n.as_str()).collect();
                let r = ui.selectable_value(&mut self.part, p, format!("{}", p + 1));
                if !names.is_empty() {
                    r.on_hover_text(names.join(", "));
                }
            }
        });
        let Some(p1) = t.block("Part_1") else {
            ui.weak("This model has no Multi Part block.");
            return;
        };
        let (h, m) = p1.hm(self.part);
        // voice picker: bank MSB / LSB / program are consecutive (08 pp 01..03)
        let get = |mem: &XgMemory, lo: u8| -> u8 {
            let d = p1.params.iter().find(|p| p.lo == lo).map(|p| p.default(self.part) as u8).unwrap_or(0);
            mem.read([h, m, lo], &[d])[0]
        };
        let (msb, lsb, prg) = (get(&self.mem, 1), get(&self.mem, 2), get(&self.mem, 3));
        let cur_name = self
            .voices
            .iter()
            .find(|v| v.msb == msb && v.lsb == lsb && v.program == prg)
            .map(|v| v.name.clone())
            .unwrap_or_else(|| format!("{msb}/{lsb}/{}", prg as u32 + 1));
        ui.horizontal(|ui| {
            ui.strong("Voice");
            let mut picked: Option<(u8, u8, u8)> = None;
            egui::ComboBox::from_id_source("voice").width(260.0).selected_text(&cur_name).show_ui(ui, |ui| {
                ui.text_edit_singleline(&mut self.voice_filter);
                let f = self.voice_filter.to_lowercase();
                let mut last_cat = "";
                for v in self.voices.iter().filter(|v| f.is_empty() || v.name.to_lowercase().contains(&f)) {
                    if v.category != last_cat {
                        ui.weak(&v.category);
                        last_cat = &v.category;
                    }
                    let on = v.msb == msb && v.lsb == lsb && v.program == prg;
                    if ui.selectable_label(on, format!("{:3}  {}", v.program as u32 + 1, v.name)).clicked() {
                        picked = Some((v.msb, v.lsb, v.program));
                    }
                }
            });
            if let Some((a, b, c)) = picked {
                self.mem.set([h, m, 1], &[a, b, c]);
                changes.push(([h, m, 1], vec![a, b, c]));
            }
        });
        ui.separator();
        for key in ["Part_1", "Part_2", "Part_3", "Part_4", "PartAdd_2"] {
            if let Some(b) = t.block(key) {
                if b.instances() <= self.part {
                    continue;
                }
                let (h, m) = b.hm(self.part);
                egui::CollapsingHeader::new(key.replace('_', " ")).default_open(key == "Part_1").show(ui, |ui| {
                    params_grid(ui, &mut self.mem, b, self.part, h, m, changes, |p| !(1..=3).contains(&p.lo) || key != "Part_1");
                });
            }
        }
    }

    // -- generic block browser ------------------------------------------------
    fn blocks_ui(&mut self, ui: &mut egui::Ui, t: &XgTable, changes: &mut Vec<Change>) {
        if t.blocks.is_empty() {
            return;
        }
        self.block = self.block.min(t.blocks.len() - 1);
        ui.horizontal(|ui| {
            ui.label("Block");
            egui::ComboBox::from_id_source("blk").selected_text(&t.blocks[self.block].key).show_ui(ui, |ui| {
                for (i, b) in t.blocks.iter().enumerate() {
                    ui.selectable_value(&mut self.block, i, &b.key);
                }
            });
            let b = &t.blocks[self.block];
            if b.instances() > 1 {
                ui.label("Instance");
                ui.add(egui::DragValue::new(&mut self.inst).range(0..=b.instances() - 1));
                let (h, m) = b.hm(self.inst.min(b.instances() - 1));
                ui.weak(format!("addr {h:02X} {m:02X} xx"));
            }
        });
        let b = &t.blocks[self.block];
        let inst = self.inst.min(b.instances() - 1);
        let (h, m) = b.hm(inst);
        params_grid(ui, &mut self.mem, b, inst, h, m, changes, |_| true);
    }
}

fn block_keys(slot: FxSlot) -> Vec<&'static str> {
    match slot {
        FxSlot::Reverb => vec!["Rev_1"],
        FxSlot::Chorus => vec!["Cho_1"],
        FxSlot::Variation => vec!["Var_1"],
        FxSlot::Insertion(0) => vec!["Ins1_1"],
        FxSlot::Insertion(_) => vec!["Ins2_1"],
    }
}

/// Sliders for every editable parameter of block instance (h, m).
#[allow(clippy::too_many_arguments)]
fn params_grid(
    ui: &mut egui::Ui,
    mem: &mut XgMemory,
    b: &TblBlock,
    inst: usize,
    h: u8,
    m: u8,
    changes: &mut Vec<Change>,
    keep: impl Fn(&TblParam) -> bool,
) {
    egui::Grid::new(("grid", &b.key, h, m)).num_columns(2).striped(true).show(ui, |ui| {
        for p in b.params.iter().filter(|p| !p.is_effect_slot() && p.conv != 4 && !p.name.is_empty() && keep(p)) {
            let (lo, hi) = (p.min(), p.max());
            if hi <= lo {
                continue;
            }
            let addr = [h, m, p.lo];
            let def: Vec<u8> = p.split(p.default(inst));
            let mut v = p.combine(&mem.read(addr, &def));
            ui.label(&p.name);
            ui.horizontal(|ui| {
                if ui.add(egui::Slider::new(&mut v, lo..=hi).show_value(false)).changed() {
                    let data = p.split(v);
                    mem.set(addr, &data);
                    changes.push((addr, data));
                }
                ui.monospace(param_display(p, v)).on_hover_text(format!("raw {v}"));
            });
            ui.end_row();
        }
    });
}
