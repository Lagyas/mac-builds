//! XGM Editor: the song's parts for a GM, GM2, GS or XG instrument, after
//! Midifile Optimizer's sound editing (not in the original XGworks). Per
//! channel: the sound (picked in Categories / Sounds / Variations like
//! Optimizer's sound dialog, auditioned while browsing, Solo), Volume, Pan,
//! Reverb / Chorus / Variation send, and for the active channel the filter
//! and EG. Changes go to MIDI Out and into the song: for XG as the XG Editor
//! writes them, for GM / GM2 / GS as Bank Select / Program Change / CC
//! messages in the part's track. The part values are the XG Editor's memory
//! (read from the song), so both editors show the same.

use super::*;
use crate::xged::Change;
use std::collections::HashMap;
use xgworks_core::drums::{self, DrumParam};
use xgworks_core::soundset::{self, Sound, SoundSet, Standard, CATEGORIES};

/// Part parameters shown per channel: (low address, label, default).
const STRIP: [(u8, &str, u8); 5] = [(0x0B, "Vol", 100), (0x0E, "Pan", 64), (0x13, "Rev", 40), (0x12, "Cho", 0), (0x14, "Var", 0)];
/// The active channel's sound shaping: (low address, label, CC).
const SHAPE: [(u8, &str, u8); 5] = [(0x18, "Cutoff", 74), (0x19, "Resonance", 71), (0x1A, "Attack", 73), (0x1B, "Decay", 75), (0x1C, "Release", 72)];

pub struct XgmState {
    pub std: Standard,
    page: usize,
    active: usize,
    sets: HashMap<Standard, Option<SoundSet>>,
    pick: Option<Picker>,
    drum: Option<DrumEd>,
    /// The effect window and its tab.
    pub fx_open: bool,
    pub fx_tab: usize,
}

/// The drum editor of one drum part (after Midifile Optimizer's).
struct DrumEd {
    part: usize,
    mode: DrumParam,
    solo: bool,
    saved_flags: Option<Vec<(bool, bool)>>,
    /// Mean velocities before the first Velo change (Reset goes back to them).
    velo_orig: HashMap<u8, u8>,
    /// A Velo drag in progress (one undo step per drag).
    velo_drag: bool,
}

impl Default for XgmState {
    fn default() -> Self {
        XgmState { std: Standard::Xg, page: 0, active: 0, sets: HashMap::new(), pick: None, drum: None, fx_open: false, fx_tab: 0 }
    }
}

/// The sound dialog of one part.
struct Picker {
    part: usize,
    cat: usize,
    prog: usize,
    var: usize,
    drums: bool,
    kit: usize,
    solo: bool,
    /// The sound when the dialog opened (Cancel sends it again).
    orig: [u8; 3],
    /// Track mute / solo flags while Solo is on.
    saved_flags: Option<Vec<(bool, bool)>>,
}

pub fn std_key(s: Standard) -> &'static str {
    match s {
        Standard::Gm => "GM",
        Standard::Gm2 => "GM2",
        Standard::Gs(1) => "SC55",
        Standard::Gs(2) => "SC88",
        Standard::Gs(3) => "SC88PRO",
        Standard::Gs(_) => "SC8850",
        Standard::Xg => "XG",
        Standard::Kbd(_) => "PSRSX900",
    }
}

pub fn std_from_key(k: &str) -> Option<Standard> {
    Standard::ALL.into_iter().find(|s| std_key(*s) == k)
}

/// Midifile Optimizer's EXE (GS sound names), if installed.
fn optimizer_exe() -> Option<Vec<u8>> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        for root in ["C:/Program Files (x86)", "C:/Program Files"] {
            let Ok(rd) = std::fs::read_dir(root) else { continue };
            for e in rd.flatten() {
                if e.file_name().to_string_lossy().to_lowercase().starts_with("midifile optimizer") {
                    if let Ok(b) = std::fs::read(e.path().join("MidifileOptimizer.exe")) {
                        return Some(b);
                    }
                }
            }
        }
    }
    platform::read(std::path::Path::new("optimizer/MidifileOptimizer.exe"))
}

/// The user's midi-file-editor.html (its voice databases: PSR-SX900,
/// SC-8850), in the XGworks data folder.
fn mfe() -> Option<String> {
    platform::read(&platform::xg4("midi-file-editor.html")).map(|b| String::from_utf8_lossy(&b).into_owned())
}

/// GS without Optimizer: the GM names as capital tones and the GS kits.
fn gs_fallback(gm: &SoundSet, map: u8) -> SoundSet {
    let mut s = gm.clone();
    for t in &mut s.tones {
        for v in t.iter_mut() {
            v.lsb = map;
        }
    }
    s.drums = [(0, "Standard"), (8, "Room"), (16, "Power"), (24, "Electronic"), (25, "TR-808"), (32, "Jazz"), (40, "Brush"), (48, "Orchestra"), (56, "SFX")]
        .into_iter()
        .map(|(p, n)| Sound { msb: 0, lsb: map, program: p, name: n.into() })
        .collect();
    s
}

/// The CC (or Program Change) of a Multi Part parameter for a GM / GS part.
fn chan_msgs(ch: u8, lo: u8, v: &[u8]) -> Vec<Vec<u8>> {
    match lo {
        // bank MSB / LSB / program, as many bytes as given
        1..=3 => {
            let mut out = Vec::new();
            for (k, &b) in v.iter().enumerate() {
                match lo as usize + k {
                    1 => out.push(vec![0xB0 | ch, 0, b]),
                    2 => out.push(vec![0xB0 | ch, 32, b]),
                    3 => out.push(vec![0xC0 | ch, b]),
                    _ => {}
                }
            }
            out
        }
        _ => STRIP
            .iter()
            .map(|s| (s.0, match s.0 { 0x0B => 7, 0x0E => 10, 0x13 => 91, 0x12 => 93, _ => 94 }))
            .chain(SHAPE.iter().map(|s| (s.0, s.2)))
            .find(|(l, _)| *l == lo)
            .map(|(_, cc)| vec![vec![0xB0 | ch, cc, v[0]]])
            .unwrap_or_default(),
    }
}

impl App {
    fn xgm_set(&mut self, std: Standard) -> Option<&SoundSet> {
        if !self.xgm.sets.contains_key(&std) {
            let rd = |f: &str| platform::read(&platform::xg4(f));
            let gm = rd("Melody.ini").map(|b| soundset::gm(&b));
            let set = match std {
                Standard::Gm => gm.map(|mut g| {
                    g.keys = vec![rd("Drum Kit.ini").map(|k| soundset::ini_keys(&k, "Standard Kit")).unwrap_or_default()];
                    g
                }),
                Standard::Gm2 => match (rd("GM2 Normal.ini"), rd("GM2 SFX Kit.ini")) {
                    (Some(n), Some(k)) => Some(soundset::gm2(&n, &k)),
                    _ => None,
                },
                Standard::Gs(map) => optimizer_exe()
                    .and_then(|e| soundset::gs_from_optimizer(&e, map))
                    .or_else(|| if map == 4 { mfe().and_then(|h| soundset::sc8850_from_mfe(&h)) } else { None })
                    .or_else(|| gm.map(|g| gs_fallback(&g, map))),
                Standard::Kbd(_) => mfe().and_then(|h| soundset::sx900_from_mfe(&h)),
                Standard::Xg => Some(soundset::xg(self.xg.voices())).filter(|s| s.tones.iter().any(|t| !t.is_empty())).map(|mut x| {
                    let kits: Vec<Vec<u8>> = ["Drum Kit.ini", "SFX Kit.ini"].iter().filter_map(|f| rd(f)).collect();
                    x.fill_keys(&kits);
                    x
                }),
            };
            self.xgm.sets.insert(std, set);
        }
        self.xgm.sets.get(&std).and_then(|s| s.as_ref())
    }

    /// The instrument choice (GM, GM2, GS maps, every XG model): the XGM
    /// Editor's header and Preferences.
    pub(super) fn xgm_inst_combo(&mut self, ui: &mut egui::Ui, id: &str) {
        let std = self.xgm.std;
        let models: Vec<String> = self.xg.models().iter().map(|m| m.0.clone()).collect();
        let model = self.xg.model_index();
        // the original's model groups (MU Series first), then any model not in them
        let mut groups: Vec<(String, Vec<usize>)> = self
            .xg
            .model_tree()
            .iter()
            .map(|(g, names)| (g.clone(), names.iter().filter_map(|n| models.iter().position(|m| m == n)).collect()))
            .filter(|g: &(String, Vec<usize>)| !g.1.is_empty())
            .collect();
        let rest: Vec<usize> = (0..models.len()).filter(|i| !groups.iter().any(|g| g.1.contains(i))).collect();
        if !rest.is_empty() {
            groups.push(("Other models".into(), rest));
        }
        let (mut new_std, mut new_model) = (None, None);
        egui::ComboBox::from_id_source(id).selected_text(self.xgm_inst_name()).width(170.0).show_ui(ui, |ui| {
            for s in Standard::ALL.into_iter().filter(|s| *s != Standard::Xg) {
                if ui.selectable_label(s == std, s.name()).clicked() {
                    new_std = Some(s);
                }
            }
            ui.separator();
            // every XG model of the XG Editor data, with its voices and effects
            egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                for (g, idx) in &groups {
                    ui.label(egui::RichText::new(format!("XG {g}")).small().strong());
                    for &i in idx {
                        if ui.selectable_label(std == Standard::Xg && i == model, format!("   {}", models[i])).clicked() {
                            new_std = Some(Standard::Xg);
                            new_model = Some(i);
                        }
                    }
                }
            });
        });
        self.xgm_choose(new_std, new_model);
    }

    fn xgm_choose(&mut self, std: Option<Standard>, model: Option<usize>) {
        if let Some(i) = model {
            self.xg.select_model(i);
            self.xg.sync_from_song(&self.edit);
            self.xgm.sets.remove(&Standard::Xg);
        }
        if let Some(s) = std {
            self.xgm.std = s;
            self.xgm.pick = None;
        }
    }

    /// "GS (SC-88Pro)", "XG MU128" …
    pub(super) fn xgm_inst_name(&self) -> String {
        match self.xgm.std {
            Standard::Xg => format!("XG {}", self.xg.models().get(self.xg.model_index()).map_or("", |m| m.0.as_str())),
            s => s.name().to_string(),
        }
    }

    /// The settings value: "SC88PRO", "XG:MU128" …
    pub(super) fn xgm_inst_key(&self) -> String {
        match self.xgm.std {
            Standard::Xg => format!("XG:{}", self.xg.models().get(self.xg.model_index()).map_or("", |m| m.0.as_str())),
            s => std_key(s).to_string(),
        }
    }

    pub(super) fn xgm_set_inst_key(&mut self, v: &str) {
        let (k, model) = v.split_once(':').unwrap_or((v, ""));
        if let Some(s) = std_from_key(k) {
            self.xgm.std = s;
        }
        if let Some(i) = self.xg.models().iter().position(|m| m.0 == model) {
            self.xg.select_model(i);
        }
    }

    fn xgm_val(&self, part: usize, lo: u8, default: u8) -> u8 {
        self.xg.mem.get([8, part as u8, lo]).unwrap_or(default)
    }

    /// A part's sound: (MSB, LSB, program); channel 10 defaults to a drum kit.
    fn xgm_sound(&self, part: usize) -> [u8; 3] {
        let drum_msb = match self.xgm.std {
            Standard::Xg | Standard::Kbd(_) => 127,
            Standard::Gm2 => 120,
            _ => 0,
        };
        let def = if part % 16 == 9 { drum_msb } else { 0 };
        [self.xgm_val(part, 1, def), self.xgm_val(part, 2, 0), self.xgm_val(part, 3, 0)]
    }

    fn xgm_is_drum(&self, part: usize) -> bool {
        let [msb, ..] = self.xgm_sound(part);
        part % 16 == 9 || (matches!(self.xgm.std, Standard::Xg | Standard::Kbd(_)) && msb >= 126) || (self.xgm.std == Standard::Gm2 && msb == 120)
    }

    /// Send a part change to MIDI Out (no song change): XG Parameter Change for
    /// XG, channel messages for GM / GM2 / GS.
    fn xgm_send(&mut self, changes: &[Change]) {
        if self.xgm.std == Standard::Xg {
            for (addr, data) in changes {
                self.xg_send(*addr, data);
            }
            return;
        }
        for (addr, data) in changes {
            let group = (addr[1] / 16).min(3) as usize;
            let dev = self.port_device(self.xg.setup.ports[group]);
            if self.ensure_midi() && open_device(&self.midi, dev) {
                for m in chan_msgs(addr[1] & 0x0F, addr[2], data) {
                    send_dev(&self.midi, dev, &m);
                }
            }
        }
    }

    /// Send and write part changes (the editor's memory too).
    fn xgm_apply(&mut self, changes: &[Change]) {
        if changes.is_empty() {
            return;
        }
        for (addr, data) in changes {
            self.xg.mem.set(*addr, data);
        }
        if self.xgm.std == Standard::Xg {
            self.xg_apply(changes);
        } else {
            self.xgm_send(changes);
            let dev = self.xg.setup.dev;
            xgworks_core::xgtbl::apply_part_changes_song(&mut self.edit, dev, changes, true);
        }
    }

    pub(super) fn xgm_window(&mut self, ctx: &egui::Context) {
        if !self.show_xgm || self.edit.is_empty() {
            return;
        }
        let mut changes: Vec<Change> = Vec::new();
        let mut open_pick = None;
        let std = self.xgm.std;
        let inst_name = self.xgm_inst_name();
        let title = format!("XGM Editor - {inst_name}");
        // what the song is made for, when that is not the chosen instrument
        let fam = |s: Standard| if let Standard::Gs(_) = s { Standard::Gs(0) } else { s };
        let song_std = soundset::detect(&self.edit).filter(|d| fam(*d) != fam(std));
        let mut new_std = None;
        let mut open_drums = false;
        let mut toggle: Option<(usize, bool)> = None;
        let have_set = self.xgm_set(std).is_some();
        let first = self.xgm.page * 16;
        // per channel: sound name, values, solo / mute of its tracks
        let rows: Vec<(String, [u8; 3], [u8; 5], bool, bool)> = (first..first + 16)
            .map(|part| {
                let snd = self.xgm_sound(part);
                let drum = self.xgm_is_drum(part);
                let name = self.xgm.sets.get(&std).and_then(|s| s.as_ref()).and_then(|set| {
                    if drum {
                        set.find_drum(snd[0], snd[1], snd[2]).map(|k| set.drums[k].name.clone())
                    } else {
                        set.name(snd[0], snd[1], snd[2]).map(str::to_string)
                    }
                });
                let vals = STRIP.map(|(lo, _, def)| self.xgm_val(part, lo, def));
                let (solo, mute) = self.xgm_part_flags(part);
                (name.unwrap_or_else(|| "—".into()), snd, vals, solo, mute)
            })
            .collect();
        let active = self.xgm.active;
        let shape = SHAPE.map(|(lo, _, _)| self.xgm_val(active, lo, 64));
        let active_name = rows.get(active.wrapping_sub(first)).map(|r| r.0.clone());
        let mut page = self.xgm.page;
        let mut new_active = None;
        let open = sub_window(ctx, "xgm", &title, [900.0, 560.0], |ui| {
            let panel = egui::Frame::none().fill(w::PANEL).inner_margin(egui::Margin::same(8.0)).rounding(4.0);
            panel.show(ui, |ui| {
                ui.visuals_mut().override_text_color = Some(w::TEXT);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("XGM").strong().size(18.0).color(w::ACCENT));
                    ui.label("Instrument");
                    ui.visuals_mut().override_text_color = None;
                    self.xgm_inst_combo(ui, "xgm_std");
                    ui.visuals_mut().override_text_color = Some(w::TEXT);
                    ui.separator();
                    if w::pill(ui, "DRUMS", self.xgm.drum.is_some()).on_hover_text("Drum editor of the selected drum channel (else channel 10)").clicked() {
                        open_drums = true;
                    }
                    if w::pill(ui, "EFFECTS", self.xgm.fx_open).on_hover_text("Reverb / Chorus / Variation, Delay, Insertion").clicked() {
                        self.xgm.fx_open = !self.xgm.fx_open;
                    }
                    ui.separator();
                    for k in 0..4 {
                        if w::pill(ui, &format!("{}-{}", k * 16 + 1, k * 16 + 16), page == k).clicked() {
                            page = k;
                        }
                    }
                });
                if let Some(d) = song_std {
                    ui.horizontal(|ui| {
                        ui.colored_label(w::ACCENT, format!("The song is made for {} (its resets / SysEx).", d.name()));
                        if w::pill(ui, &format!("Use {}", d.name()), false).clicked() {
                            new_std = Some(d);
                        }
                    });
                }
                if !have_set {
                    ui.colored_label(w::ACCENT, platform::data_help(&format!("No {} sound names found.", std.name())));
                }
                ui.add_space(6.0);
                // ---- 16 channel strips
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    for (k, (name, snd, vals, solo, mute)) in rows.iter().enumerate() {
                        let part = first + k;
                        let strip = egui::Frame::none()
                            .fill(if part == active { w::STRIP_ON } else { w::STRIP })
                            .rounding(3.0)
                            .inner_margin(egui::Margin::symmetric(2.0, 4.0));
                        strip.show(ui, |ui| {
                            ui.set_width(46.0);
                            ui.vertical_centered(|ui| {
                                ui.spacing_mut().item_spacing.y = 2.0;
                                let num = if part >= 16 { format!("{}-{}", part / 16 + 1, part % 16 + 1) } else { (part + 1).to_string() };
                                if ui.add(egui::Label::new(egui::RichText::new(num).strong().color(if part == active { w::ACCENT } else { w::TEXT })).sense(egui::Sense::click())).clicked() {
                                    new_active = Some(part);
                                }
                                let short: String = name.chars().take(7).collect();
                                let b = ui
                                    .add_sized([46.0, 30.0], egui::Button::new(egui::RichText::new(short).size(10.0)).wrap().fill(w::LCD))
                                    .on_hover_text(format!("{name}\nBank {} / {}  Program {}", snd[0], snd[1], snd[2] as u32 + 1));
                                if b.clicked() {
                                    new_active = Some(part);
                                    open_pick = Some(part);
                                }
                                for (i, (lo, label, def)) in STRIP.iter().enumerate().skip(1) {
                                    let mut v = vals[i] as i32;
                                    let text = if *lo == 0x0E { pan_text(v) } else { v.to_string() };
                                    if w::knob(ui, ("xgmk", part, *lo), &mut v, 0, 127, *def as i32, 30.0, label, &text).changed() {
                                        new_active = Some(part);
                                        changes.push(([8, part as u8, *lo], vec![v as u8]));
                                    }
                                }
                                let mut v = vals[0] as i32;
                                if w::fader(ui, ("xgmf", part), &mut v, 0, 127, 100, 96.0).changed() {
                                    new_active = Some(part);
                                    changes.push(([8, part as u8, 0x0B], vec![v as u8]));
                                }
                                ui.label(egui::RichText::new(vals[0].to_string()).monospace().size(10.0));
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 2.0;
                                    if w::small_toggle(ui, "S", *solo, w::SOLO).on_hover_text("Solo").clicked() {
                                        toggle = Some((part, true));
                                    }
                                    if w::small_toggle(ui, "M", *mute, w::MUTE).on_hover_text("Mute").clicked() {
                                        toggle = Some((part, false));
                                    }
                                });
                            });
                        });
                    }
                });
                ui.add_space(6.0);
                // ---- the active channel: sound shaping knobs
                egui::Frame::none().fill(w::STRIP).rounding(3.0).inner_margin(egui::Margin::same(6.0)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(egui::RichText::new(format!("Channel {}", active % 16 + 1)).strong().color(w::ACCENT));
                            ui.label(active_name.clone().unwrap_or_default());
                        });
                        ui.add_space(16.0);
                        for (i, (lo, label, _)) in SHAPE.iter().enumerate() {
                            let mut v = shape[i] as i32;
                            let text = (v - 64).to_string();
                            if w::knob(ui, ("xgms", *lo), &mut v, 0, 127, 64, 44.0, label, &text).changed() {
                                changes.push(([8, active as u8, *lo], vec![v as u8]));
                            }
                            ui.add_space(8.0);
                        }
                    });
                });
            });
        });
        self.show_xgm = open;
        self.xgm.page = page;
        if let Some(a) = new_active {
            self.xgm.active = a;
        }
        if let Some((part, solo)) = toggle {
            self.xgm_toggle_flag(part, solo);
        }
        self.xgm_choose(new_std, None);
        self.xgm_apply(&changes);
        if let Some(part) = open_pick {
            self.xgm_open_picker(part);
        }
        if open_drums {
            let a = self.xgm.active;
            let part = if self.xgm_is_drum(a) { a } else { a / 16 * 16 + 9 };
            self.xgm_open_drums(part);
        }
        self.xgm_picker(ctx);
        self.xgm_drums(ctx);
        self.xgm_effects(ctx);
        if !self.show_xgm {
            self.xgm_close_picker(false);
            self.xgm_close_drums();
            self.xgm.fx_open = false;
        }
    }

    /// The tracks that play a part (its port group and channel).
    fn xgm_part_tracks(&self, part: usize) -> Vec<usize> {
        let (group, ch) = ((part / 16) as u8, (part % 16) as u8);
        (0..self.edit.len())
            .filter(|&k| {
                let t = &self.edit[k];
                !t.conductor && t.port().saturating_sub(1).min(3) == group && t.channel & 0x0F == ch
            })
            .collect()
    }

    /// (solo, mute) of a part's tracks.
    fn xgm_part_flags(&self, part: usize) -> (bool, bool) {
        let ts = self.xgm_part_tracks(part);
        let any = |f: fn(&(bool, bool)) -> bool| !ts.is_empty() && ts.iter().all(|&k| self.flags.get(k).is_some_and(f));
        (any(|f| f.1), any(|f| f.0))
    }

    fn xgm_toggle_flag(&mut self, part: usize, solo: bool) {
        let (s, m) = self.xgm_part_flags(part);
        let on = if solo { !s } else { !m };
        for k in self.xgm_part_tracks(part) {
            if let Some(f) = self.flags.get_mut(k) {
                if solo {
                    f.1 = on;
                } else {
                    f.0 = on;
                }
            }
        }
    }

    pub(super) fn xgm_open_drums(&mut self, part: usize) {
        self.xgm_close_drums();
        self.xgm.active = part;
        self.xgm.drum = Some(DrumEd { part, mode: DrumParam::Velo, solo: false, saved_flags: None, velo_orig: HashMap::new(), velo_drag: false });
    }

    fn xgm_close_drums(&mut self) {
        if let Some(d) = self.xgm.drum.take() {
            if let Some(f) = d.saved_flags {
                self.flags = f;
            }
        }
    }

    /// A drum instrument's value in the current mode.
    fn drum_value(&self, part: usize, mode: DrumParam, key: &drums::DrumKey, nrpn: &HashMap<(u8, u8), u8>) -> u8 {
        if mode == DrumParam::Velo {
            return key.velocity;
        }
        let v = if self.xgm.std == Standard::Xg {
            mode.xg_lo().and_then(|lo| self.xg.mem.get([0x30, key.note, lo]))
        } else {
            mode.nrpn().and_then(|m| nrpn.get(&(m, key.note)).copied())
        };
        let _ = part;
        v.unwrap_or(mode.default())
    }

    /// Set a drum instrument's value: velocities in the song, or its drum
    /// setup (XG Parameter Change / NRPN) sent and written.
    fn drum_set(&mut self, part: usize, mode: DrumParam, note: u8, v: u8) {
        match mode {
            DrumParam::Velo => drums::set_key_velocity(&mut self.edit, part as u8, note, v),
            _ if self.xgm.std == Standard::Xg => {
                if let Some(lo) = mode.xg_lo() {
                    let ch = vec![([0x30, note, lo], vec![v])];
                    self.xg.mem.set(ch[0].0, &ch[0].1);
                    self.xg_apply(&ch);
                }
            }
            _ => {
                let Some(msb) = mode.nrpn() else { return };
                let group = (part / 16).min(3);
                let dev = self.port_device(self.xg.setup.ports[group]);
                if self.ensure_midi() && open_device(&self.midi, dev) {
                    for m in drums::nrpn_msgs((part % 16) as u8, msb, note, v) {
                        send_dev(&self.midi, dev, &m);
                    }
                }
                drums::set_nrpn(&mut self.edit, part as u8, msb, note, v);
            }
        }
    }

    /// The drum editor window: one slider per drum instrument the part plays,
    /// in the mode Velo / Lev / Pitch / Rev / Cho / Pan / LP; Solo, Reset.
    fn xgm_drums(&mut self, ctx: &egui::Context) {
        let Some(mut d) = self.xgm.drum.take() else { return };
        let part = d.part;
        let std = self.xgm.std;
        let snd = self.xgm_sound(part);
        let set = self.xgm_set(std).cloned();
        let kit = set.as_ref().and_then(|s| s.find_drum(snd[0], snd[1], snd[2]).map(|k| s.drums[k].name.clone())).unwrap_or_else(|| "Drums".into());
        let names: HashMap<u8, String> = set.as_ref().map(|s| s.kit_keys(snd[0], snd[1], snd[2]).iter().cloned().collect()).unwrap_or_default();
        let keys = drums::used_keys(&self.edit, part as u8);
        let nrpn = drums::nrpn_values(&self.edit, part as u8);
        let vals: Vec<u8> = keys.iter().map(|k| self.drum_value(part, d.mode, k, &nrpn)).collect();
        let mut sets: Vec<(u8, u8)> = Vec::new();
        let mut reset = false;
        let mut drag_start = false;
        let mut dragging = false;
        let title = format!("Drum Editor - {kit} (channel {})", part % 16 + 1);
        let keep = sub_window(ctx, "xgm_drums", &title, [470.0, 560.0], |ui| {
            // Optimizer-like: kit name, the mode row (active in yellow), then
            // one slider per instrument beside the orange display
            egui::Frame::none().fill(w::PANEL).inner_margin(egui::Margin::same(10.0)).rounding(4.0).show(ui, |ui| {
                ui.visuals_mut().override_text_color = Some(w::TEXT);
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new(&kit).size(20.0).strong().color(w::TEXT));
                });
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.add_space(26.0);
                    for m in DrumParam::ALL {
                        // GS has no drum filter NRPN
                        if m == DrumParam::Cutoff && matches!(std, Standard::Gs(_)) {
                            continue;
                        }
                        let col = if d.mode == m { w::YELLOW } else { w::TEXT };
                        if ui.add(egui::Label::new(egui::RichText::new(m.label()).strong().color(col)).sense(egui::Sense::click())).clicked() {
                            d.mode = m;
                        }
                    }
                    ui.add_space(14.0);
                    if w::pill(ui, "Solo", d.solo).on_hover_text("Play the drum part alone").clicked() {
                        d.solo = !d.solo;
                    }
                    if w::pill(ui, "Reset", false).on_hover_text("Velo: the velocities as they were; else the default for every instrument").clicked() {
                        reset = true;
                    }
                });
                ui.add_space(4.0);
                ui.painter().hline(ui.max_rect().x_range(), ui.cursor().top(), egui::Stroke::new(1.0_f32, w::STRIP_ON));
                ui.add_space(6.0);
                if keys.is_empty() {
                    ui.label("The song plays no notes on this channel.");
                }
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 0.0);
                    for (k, key) in keys.iter().enumerate() {
                        ui.horizontal(|ui| {
                            ui.set_height(24.0);
                            let mut v = vals[k] as i32;
                            let lo = if d.mode == DrumParam::Velo { 1 } else { 0 };
                            let resp = w::hslider(ui, ("drum", key.note), &mut v, lo, 127, 200.0);
                            if resp.drag_started() {
                                drag_start = true;
                            }
                            if resp.dragged() {
                                dragging = true;
                            }
                            if resp.changed() {
                                sets.push((key.note, v as u8));
                            }
                            let name = names.get(&key.note).cloned().unwrap_or_else(|| format!("Note {}", key.note));
                            w::lcd_row(ui, &vals[k].to_string(), &name, 210.0, k == 0, k + 1 == keys.len())
                                .on_hover_text(format!("note {}, played {}×", key.note, key.count));
                        });
                    }
                });
            });
        });
        // Solo: only the tracks on this channel play
        let ch = (part % 16) as u8;
        if d.solo && d.saved_flags.is_none() {
            d.saved_flags = Some(self.flags.clone());
            for (k, t) in self.edit.iter().enumerate() {
                if let Some(f) = self.flags.get_mut(k) {
                    f.1 = t.channel & 0x0F == ch;
                }
            }
        } else if !d.solo {
            if let Some(f) = d.saved_flags.take() {
                self.flags = f;
            }
        }
        if d.mode == DrumParam::Velo && !sets.is_empty() {
            if drag_start || !d.velo_drag {
                self.tv_snapshot();
            }
            for k in &keys {
                d.velo_orig.entry(k.note).or_insert(k.velocity);
            }
        }
        d.velo_drag = dragging;
        for (note, v) in sets {
            self.drum_set(part, d.mode, note, v);
        }
        if reset {
            if d.mode == DrumParam::Velo {
                if !d.velo_orig.is_empty() {
                    self.tv_snapshot();
                }
                for (note, v) in d.velo_orig.clone() {
                    self.drum_set(part, DrumParam::Velo, note, v);
                }
            } else {
                for k in &keys {
                    self.drum_set(part, d.mode, k.note, d.mode.default());
                }
            }
        }
        if !keys.is_empty() && (d.mode == DrumParam::Velo) {
            self.status = format!("Drum Editor: {} instruments on channel {}.", keys.len(), part % 16 + 1);
        }
        self.xgm.drum = Some(d);
        if !keep {
            self.xgm_close_drums();
        }
    }

    pub(super) fn xgm_open_picker(&mut self, part: usize) {
        self.xgm_close_picker(false);
        let snd = self.xgm_sound(part);
        let drums = self.xgm_is_drum(part);
        let std = self.xgm.std;
        let Some(set) = self.xgm_set(std) else { return };
        let (prog, var) = set.find(snd[0], snd[1], snd[2]).unwrap_or((snd[2] as usize, 0));
        let kit = set.find_drum(snd[0], snd[1], snd[2]).unwrap_or(0);
        self.xgm.pick = Some(Picker { part, cat: prog / 8, prog, var, drums, kit, solo: false, orig: snd, saved_flags: None });
    }

    /// Close the sound dialog: OK keeps (and writes) the sound, else the
    /// original is sent again. Solo is undone either way.
    fn xgm_close_picker(&mut self, ok: bool) {
        let Some(pk) = self.xgm.pick.take() else { return };
        if let Some(f) = pk.saved_flags {
            self.flags = f;
        }
        let cur = self.xgm_sound(pk.part);
        if ok {
            if cur != pk.orig {
                // the song gets the sound now (previewing only sent it)
                let ch = vec![([8, pk.part as u8, 1], cur.to_vec())];
                for (addr, data) in &ch {
                    self.xg.mem.set(*addr, data);
                }
                if self.xgm.std == Standard::Xg {
                    let dev = self.xg.setup.dev;
                    xgworks_core::xgtbl::apply_xg_changes_song(&mut self.edit, dev, &ch);
                } else {
                    let dev = self.xg.setup.dev;
                    xgworks_core::xgtbl::apply_part_changes_song(&mut self.edit, dev, &ch, true);
                }
                self.status = format!("Channel {}: sound set.", pk.part % 16 + 1);
            }
        } else if cur != pk.orig {
            let ch = vec![([8, pk.part as u8, 1], pk.orig.to_vec())];
            self.xg.mem.set(ch[0].0, &ch[0].1);
            self.xgm_send(&ch);
        }
    }

    /// Optimizer-style sound dialog: Categories / Sounds / Variations (or the
    /// drum kits), each click auditioned at once; Solo plays the part alone.
    fn xgm_picker(&mut self, ctx: &egui::Context) {
        let Some(mut pk) = self.xgm.pick.take() else { return };
        let std = self.xgm.std;
        let Some(set) = self.xgm_set(std).cloned() else { return };
        let mut chosen: Option<[u8; 3]> = None;
        let mut done: Option<bool> = None;
        let title = format!("{} - channel {}", self.xgm_inst_name(), pk.part % 16 + 1);
        let keep = sub_window(ctx, "xgm_pick", &title, [560.0, 450.0], |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut pk.drums, false, "Sounds");
                ui.selectable_value(&mut pk.drums, true, "Drum kits");
                ui.separator();
                ui.checkbox(&mut pk.solo, "Solo").on_hover_text("Play this part alone while choosing");
            });
            ui.separator();
            let h = 340.0;
            if pk.drums {
                egui::ScrollArea::vertical().id_source("xgm_kits").max_height(h).show(ui, |ui| {
                    for (k, d) in set.drums.iter().enumerate() {
                        if ui.selectable_label(k == pk.kit, format!("{:3}  {}", d.program as u32 + 1, d.name)).clicked() {
                            pk.kit = k;
                            chosen = Some([d.msb, d.lsb, d.program]);
                        }
                    }
                });
            } else {
                ui.columns(3, |cols| {
                    cols[0].strong("Categories");
                    egui::ScrollArea::vertical().id_source("xgm_cat").max_height(h).show(&mut cols[0], |ui| {
                        for (k, c) in CATEGORIES.iter().enumerate() {
                            if ui.selectable_label(k == pk.cat, *c).clicked() && k != pk.cat {
                                pk.cat = k;
                                pk.prog = k * 8;
                                pk.var = 0;
                                if let Some(s) = set.tones[pk.prog].first() {
                                    chosen = Some([s.msb, s.lsb, s.program]);
                                }
                            }
                        }
                    });
                    cols[1].strong("Sounds");
                    egui::ScrollArea::vertical().id_source("xgm_prog").max_height(h).show(&mut cols[1], |ui| {
                        for p in pk.cat * 8..pk.cat * 8 + 8 {
                            let Some(s) = set.tones[p].first() else { continue };
                            if ui.selectable_label(p == pk.prog, format!("{:3} {}", p + 1, s.name)).clicked() {
                                pk.prog = p;
                                pk.var = 0;
                                chosen = Some([s.msb, s.lsb, s.program]);
                            }
                        }
                    });
                    cols[2].strong("Variations");
                    egui::ScrollArea::vertical().id_source("xgm_var").max_height(h).show(&mut cols[2], |ui| {
                        for (k, s) in set.tones[pk.prog].iter().enumerate() {
                            if ui.selectable_label(k == pk.var, format!("{:3}/{:<3} {}", s.msb, s.lsb, s.name)).clicked() {
                                pk.var = k;
                                chosen = Some([s.msb, s.lsb, s.program]);
                            }
                        }
                    });
                });
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("   OK   ").clicked() {
                    done = Some(true);
                }
                if ui.button(" Cancel ").clicked() {
                    done = Some(false);
                }
            });
        });
        // Solo: only the tracks on this channel play while the dialog is open
        let ch = (pk.part % 16) as u8;
        if pk.solo && pk.saved_flags.is_none() {
            pk.saved_flags = Some(self.flags.clone());
            for (k, t) in self.edit.iter().enumerate() {
                if let Some(f) = self.flags.get_mut(k) {
                    f.1 = t.channel & 0x0F == ch;
                }
            }
        } else if !pk.solo {
            if let Some(f) = pk.saved_flags.take() {
                self.flags = f;
            }
        }
        if let Some(s) = chosen {
            let change = vec![([8, pk.part as u8, 1], s.to_vec())];
            self.xg.mem.set(change[0].0, &change[0].1);
            self.xgm_send(&change);
        }
        self.xgm.pick = Some(pk);
        if let Some(ok) = done {
            self.xgm_close_picker(ok);
        } else if !keep {
            self.xgm_close_picker(false);
        }
    }
}

/// Pan as L63 … C … R63.
pub(super) fn pan_text(v: i32) -> String {
    match v {
        0 => "Rnd".into(),
        64 => "C".into(),
        v if v < 64 => format!("L{}", 64 - v),
        v => format!("R{}", v - 64),
    }
}

/// The XGM Editor's controls, drawn in its own dark style (Optimizer-like
/// layout, no artwork taken from anywhere).
pub(super) mod w {
    use eframe::egui::{self, pos2, vec2, Color32, Response, Sense, Stroke};

    pub const PANEL: Color32 = Color32::from_rgb(0x3C, 0x41, 0x49);
    pub const STRIP: Color32 = Color32::from_rgb(0x4A, 0x50, 0x59);
    pub const STRIP_ON: Color32 = Color32::from_rgb(0x5A, 0x60, 0x6B);
    pub const LCD: Color32 = Color32::from_rgb(0xD8, 0x6A, 0x22);
    pub const ACCENT: Color32 = Color32::from_rgb(0xFF, 0x9A, 0x40);
    pub const TEXT: Color32 = Color32::from_rgb(0xE6, 0xE8, 0xEB);
    pub const SOLO: Color32 = Color32::from_rgb(0x3F, 0xB9, 0x50);
    pub const MUTE: Color32 = Color32::from_rgb(0xD8, 0x3A, 0x3A);
    pub const YELLOW: Color32 = Color32::from_rgb(0xFF, 0xE0, 0x30);
    const KNOB: Color32 = Color32::from_rgb(0xB8, 0xBD, 0xC4);
    const KNOB_DARK: Color32 = Color32::from_rgb(0x7A, 0x80, 0x88);
    const TRACK: Color32 = Color32::from_rgb(0x25, 0x28, 0x2D);

    /// A rotary knob (drag up / down or the wheel; double-click = default),
    /// with its label under it and the value shown while hovered or dragged.
    #[allow(clippy::too_many_arguments)]
    pub fn knob(ui: &mut egui::Ui, id: impl std::hash::Hash, v: &mut i32, min: i32, max: i32, default: i32, size: f32, label: &str, text: &str) -> Response {
        let id = ui.id().with(id);
        let lw = (label.chars().count() as f32 * 5.6).min(76.0);
        let (rect, mut resp) = ui.allocate_exact_size(vec2(size.max(30.0).max(lw), size + 22.0), Sense::click_and_drag());
        let old = *v;
        let span = (max - min).max(1) as f32;
        if resp.dragged() {
            let acc = ui.data_mut(|d| {
                let a = d.get_temp_mut_or(id, 0.0f32);
                *a += -resp.drag_delta().y * span / 120.0;
                let whole = a.trunc();
                *a -= whole;
                whole
            });
            *v = (*v + acc as i32).clamp(min, max);
        }
        if resp.hovered() {
            let s = ui.input(|i| i.raw_scroll_delta.y);
            if s != 0.0 {
                *v = (*v + s.signum() as i32 * ((span / 64.0).ceil() as i32).max(1)).clamp(min, max);
            }
        }
        if resp.double_clicked() {
            *v = default.clamp(min, max);
        }
        if *v != old {
            resp.mark_changed();
        }
        let p = ui.painter();
        let c = pos2(rect.center().x, rect.top() + size / 2.0);
        let r = size / 2.0 - 2.0;
        let f = (*v - min) as f32 / span;
        let a0 = -135f32.to_radians();
        let a = a0 + f * 270f32.to_radians();
        let pt = |ang: f32, rr: f32| c + vec2(ang.sin(), -ang.cos()) * rr;
        // value arc around the knob
        let arc = |from: f32, to: f32, col: Color32| {
            let n = 24;
            let pts: Vec<egui::Pos2> = (0..=n).map(|i| pt(from + (to - from) * i as f32 / n as f32, r)).collect();
            p.add(egui::Shape::line(pts, Stroke::new(3.0_f32, col)));
        };
        arc(a0, 135f32.to_radians(), TRACK);
        arc(a0, a, if resp.hovered() || resp.dragged() { ACCENT } else { super::w::LCD });
        p.circle_filled(c, r - 4.0, KNOB_DARK);
        p.circle_filled(c - vec2(1.0, 1.0), r - 5.0, KNOB);
        p.line_segment([pt(a, r * 0.2), pt(a, r - 5.0)], Stroke::new(2.0_f32, Color32::from_rgb(0x30, 0x30, 0x30)));
        let show = if resp.hovered() || resp.dragged() { text } else { label };
        p.text(pos2(c.x, rect.bottom() - 10.0), egui::Align2::CENTER_CENTER, show, egui::FontId::proportional(10.0), TEXT);
        resp.on_hover_text(format!("{label}: {text}"))
    }

    /// A vertical fader (drag; wheel; double-click = default).
    pub fn fader(ui: &mut egui::Ui, id: impl std::hash::Hash, v: &mut i32, min: i32, max: i32, default: i32, h: f32) -> Response {
        let _ = id;
        let (rect, mut resp) = ui.allocate_exact_size(vec2(26.0, h), Sense::click_and_drag());
        let old = *v;
        let span = (max - min).max(1) as f32;
        let cap = 14.0;
        let travel = h - cap;
        if resp.dragged() || resp.is_pointer_button_down_on() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let f = 1.0 - ((pos.y - rect.top() - cap / 2.0) / travel).clamp(0.0, 1.0);
                *v = min + (f * span).round() as i32;
            }
        }
        if resp.hovered() {
            let s = ui.input(|i| i.raw_scroll_delta.y);
            if s != 0.0 {
                *v = (*v + s.signum() as i32).clamp(min, max);
            }
        }
        if resp.double_clicked() {
            *v = default;
        }
        if *v != old {
            resp.mark_changed();
        }
        let p = ui.painter();
        let x = rect.center().x;
        p.rect_filled(egui::Rect::from_center_size(pos2(x, rect.center().y), vec2(4.0, h - 4.0)), 2.0, TRACK);
        let f = (*v - min) as f32 / span;
        let y = rect.top() + (1.0 - f) * travel;
        let capr = egui::Rect::from_min_size(pos2(x - 11.0, y), vec2(22.0, cap));
        p.rect_filled(capr, 2.0, KNOB);
        p.hline(capr.x_range(), capr.center().y, Stroke::new(2.0_f32, if resp.hovered() || resp.dragged() { ACCENT } else { Color32::from_rgb(0x30, 0x30, 0x30) }));
        resp
    }

    /// A horizontal slider with a metal thumb (drag, wheel; Optimizer's drum
    /// editor rows).
    pub fn hslider(ui: &mut egui::Ui, id: impl std::hash::Hash, v: &mut i32, min: i32, max: i32, width: f32) -> Response {
        let _ = id;
        let (rect, mut resp) = ui.allocate_exact_size(vec2(width, 22.0), Sense::click_and_drag());
        let old = *v;
        let span = (max - min).max(1) as f32;
        let thumb = 30.0;
        let travel = width - thumb;
        if resp.dragged() || resp.is_pointer_button_down_on() {
            if let Some(pos) = resp.interact_pointer_pos() {
                let f = ((pos.x - rect.left() - thumb / 2.0) / travel).clamp(0.0, 1.0);
                *v = min + (f * span).round() as i32;
            }
        }
        if resp.hovered() {
            let s = ui.input(|i| i.raw_scroll_delta.y);
            if s != 0.0 {
                *v = (*v + s.signum() as i32).clamp(min, max);
            }
        }
        if *v != old {
            resp.mark_changed();
        }
        let p = ui.painter();
        let y = rect.center().y;
        // the row's LED, the groove, the thumb
        p.rect_filled(egui::Rect::from_center_size(pos2(rect.left() - 9.0, y), vec2(6.0, 14.0)), 1.0, TRACK);
        p.rect_filled(egui::Rect::from_min_max(pos2(rect.left(), y - 2.0), pos2(rect.right(), y + 2.0)), 2.0, TRACK);
        let x = rect.left() + (*v - min) as f32 / span * travel;
        let tr = egui::Rect::from_min_size(pos2(x, rect.top() + 2.0), vec2(thumb, 18.0));
        p.rect_filled(tr, 2.0, KNOB);
        p.rect_stroke(tr, 2.0, Stroke::new(1.0_f32, KNOB_DARK));
        for dx in [-6.0, 6.0] {
            p.vline(tr.center().x + dx, tr.y_range().shrink(4.0), Stroke::new(1.0_f32, KNOB_DARK));
        }
        p.vline(tr.center().x, tr.y_range().shrink(3.0), Stroke::new(2.0_f32, if resp.hovered() || resp.dragged() { ACCENT } else { Color32::from_rgb(0xC0, 0x20, 0x20) }));
        resp
    }

    /// One row of the orange display: the value, then the name.
    pub fn lcd_row(ui: &mut egui::Ui, value: &str, name: &str, width: f32, first: bool, last: bool) -> Response {
        let (rect, resp) = ui.allocate_exact_size(vec2(width, 24.0), Sense::hover());
        let p = ui.painter();
        let r = egui::Rect::from_min_max(pos2(rect.left(), rect.top() - if first { 0.0 } else { 1.0 }), rect.max);
        let rounding = egui::Rounding { nw: if first { 3.0 } else { 0.0 }, ne: if first { 3.0 } else { 0.0 }, sw: if last { 3.0 } else { 0.0 }, se: if last { 3.0 } else { 0.0 } };
        p.rect_filled(r, rounding, LCD);
        let ink = Color32::from_rgb(0x2A, 0x10, 0x05);
        p.text(pos2(rect.left() + 34.0, rect.center().y), egui::Align2::RIGHT_CENTER, value, egui::FontId::proportional(13.0), ink);
        p.vline(rect.left() + 42.0, rect.y_range(), Stroke::new(1.0_f32, Color32::from_rgb(0xA8, 0x48, 0x10)));
        p.text(pos2(rect.left() + 50.0, rect.center().y), egui::Align2::LEFT_CENTER, name, egui::FontId::proportional(13.0), ink);
        resp
    }

    /// A small S / M switch.
    pub fn small_toggle(ui: &mut egui::Ui, text: &str, on: bool, color: Color32) -> Response {
        let (rect, resp) = ui.allocate_exact_size(vec2(20.0, 16.0), Sense::click());
        let p = ui.painter();
        p.rect_filled(rect, 2.0, if on { color } else { TRACK });
        p.text(rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(10.0), if on { Color32::BLACK } else { TEXT });
        resp
    }

    /// A header button that can be lit.
    pub fn pill(ui: &mut egui::Ui, text: &str, on: bool) -> Response {
        let galley = ui.painter().layout_no_wrap(text.to_string(), egui::FontId::proportional(12.0), TEXT);
        let (rect, resp) = ui.allocate_exact_size(vec2(galley.size().x + 16.0, 20.0), Sense::click());
        let p = ui.painter();
        p.rect_filled(rect, 3.0, if on { LCD } else if resp.hovered() { STRIP_ON } else { STRIP });
        p.text(rect.center(), egui::Align2::CENTER_CENTER, text, egui::FontId::proportional(12.0), if on { Color32::BLACK } else { TEXT });
        resp
    }

    /// A list of choices (types, categories) in the panel style.
    pub fn list(ui: &mut egui::Ui, id: impl std::hash::Hash, items: &[String], sel: Option<usize>, w: f32, h: f32) -> Option<usize> {
        let mut out = None;
        egui::Frame::none().fill(TRACK).rounding(2.0).inner_margin(egui::Margin::same(3.0)).show(ui, |ui| {
            ui.set_width(w);
            egui::ScrollArea::vertical().id_source(id).max_height(h).auto_shrink([false, false]).show(ui, |ui| {
                ui.set_width(w - 6.0);
                ui.vertical(|ui| for (k, it) in items.iter().enumerate() {
                    let on = sel == Some(k);
                    let t = egui::RichText::new(it).color(if on { Color32::BLACK } else { TEXT });
                    let r = ui.add(egui::SelectableLabel::new(on, t));
                    if r.clicked() {
                        out = Some(k);
                    }
                });
            });
        });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gs_picker_writes_channel_messages() {
        let mut app = App::new();
        let mid = std::fs::read("../xgworks-core/tests/fixtures/dream.mid").unwrap();
        app.load(&mid, "dream.mid");
        app.xgm.std = Standard::Gs(3);
        if app.xgm_set(Standard::Gs(3)).is_none() {
            eprintln!("no GM names (XGworks folder) - skipped");
            return;
        }
        app.xgm_open_picker(1);
        // a variation picked and kept
        let change = vec![([8u8, 1u8, 1u8], vec![8u8, 3, 33])];
        app.xg.mem.set(change[0].0, &change[0].1);
        app.xgm_close_picker(true);
        let m = xgworks_core::xgtbl::XgMemory::from_song(&app.edit);
        assert_eq!(m.read([8, 1, 1], &[0, 0, 0]), vec![8, 3, 33]);
        // the strip writes CC 7
        app.xgm_apply(&[([8, 1, 0x0B], vec![55])]);
        assert_eq!(xgworks_core::xgtbl::XgMemory::from_song(&app.edit).get([8, 1, 0x0B]), Some(55));
        // Cancel sends the original sound again and leaves the song
        app.xgm_open_picker(2);
        let before = format!("{:?}", app.edit);
        app.xg.mem.set([8, 2, 1], &[8, 3, 5]);
        app.xgm_close_picker(false);
        assert_eq!(format!("{:?}", app.edit), before);
        assert_ne!(app.xg.mem.read([8, 2, 1], &[0, 0, 0]), vec![8, 3, 5]);
    }

    #[test]
    fn drum_editor_velocity_and_setup() {
        let mut app = App::new();
        let mid = std::fs::read("../xgworks-core/tests/fixtures/dream.mid").unwrap();
        app.load(&mid, "dream.mid");
        let keys = drums::used_keys(&app.edit, 9);
        assert!(!keys.is_empty(), "dream.mid has drums on channel 10");
        let k = keys[0].clone();
        // Velo: the mean velocity of that instrument moves
        app.drum_set(9, DrumParam::Velo, k.note, 40);
        let after = drums::used_keys(&app.edit, 9).into_iter().find(|x| x.note == k.note).unwrap();
        assert!(after.velocity.abs_diff(40) <= 1, "{} -> {}", k.velocity, after.velocity);
        // GS: level as NRPN
        app.xgm.std = Standard::Gs(3);
        app.drum_set(9, DrumParam::Level, k.note, 90);
        assert_eq!(drums::nrpn_values(&app.edit, 9).get(&(0x1A, k.note)), Some(&90));
        // XG: drum setup as XG Parameter Change, read back from the song
        app.xgm.std = Standard::Xg;
        app.drum_set(9, DrumParam::Pitch, k.note, 70);
        assert_eq!(xgworks_core::xgtbl::XgMemory::from_song(&app.edit).get([0x30, k.note, 0]), Some(70));
        // the window draws in every mode
        app.show_xgm = true;
        app.xgm_open_drums(9);
        let ctx = egui::Context::default();
        for m in DrumParam::ALL {
            app.xgm.drum.as_mut().unwrap().mode = m;
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |_| {});
                app.xgm_window(ctx);
            });
        }
    }

    #[test]
    fn xg_effects_default_to_the_models_types() {
        let mut app = App::new();
        let mid = std::fs::read("../xgworks-core/tests/fixtures/dream.mid").unwrap();
        app.load(&mid, "dream.mid");
        let Some(t) = app.xg.table().cloned() else { return };
        use xgworks_core::xgtbl::FxSlot;
        // HALL1 / CHORUS1 when the song sets no type (not "NO EFFECT")
        assert_eq!(t.fx_name(app.xg.fx_default_type(&t, FxSlot::Reverb)[0], app.xg.fx_default_type(&t, FxSlot::Reverb)[1]), "HALL1");
        assert_eq!(t.fx_name(app.xg.fx_default_type(&t, FxSlot::Chorus)[0], app.xg.fx_default_type(&t, FxSlot::Chorus)[1]), "CHORUS1");
        // every XG model is offered and can be chosen
        assert!(app.xg.models().len() > 10);
        let mu50 = app.xg.models().iter().position(|m| m.0 == "MU50").unwrap();
        app.xgm_set_inst_key("XG:MU50");
        assert_eq!(app.xg.model_index(), mu50);
        assert_eq!(app.xgm_inst_key(), "XG:MU50");
        assert!(app.xgm_set(Standard::Xg).is_some());
    }

    #[test]
    fn xgm_window_draws() {
        let mut app = App::new();
        let mid = std::fs::read("../xgworks-core/tests/fixtures/dream.mid").unwrap();
        app.load(&mid, "dream.mid");
        app.show_xgm = true;
        let ctx = egui::Context::default();
        app.xgm.fx_open = true;
        for std in Standard::ALL {
            app.xgm.std = std;
            app.xgm_open_picker(0);
            app.xgm.fx_tab = 3;
            let _ = ctx.run(egui::RawInput::default(), |ctx| {
                egui::CentralPanel::default().show(ctx, |_| {});
                app.xgm_window(ctx);
            });
        }
    }
}
