//! Sample Mapping window: assign your own samples to the Hypersonic 2 sample
//! banks (a SoundFont with automatic GM or a fixed preset, or a set of WAV
//! files). Sample elements of the original patches then play these sounds.

use crate::app::App;
use crate::platform::{self, Files};
use eframe::egui;
use hs2_core::sf2::{self, SoundFont};
use hs2_core::synth::SampleSource;
use std::sync::Arc;

#[derive(Clone)]
pub enum MapKind {
    Font { label: String, path: Option<String>, preset: Option<(u16, u16)> },
    Wavs { paths: Vec<String>, count: usize },
}

#[derive(Clone)]
pub struct MapEntry {
    pub bank: u32,
    pub kind: MapKind,
    pub source: SampleSource,
}

impl MapEntry {
    pub fn label(&self) -> String {
        match &self.kind {
            MapKind::Font { label, preset: None, .. } => format!("SF2 {label} (auto GM)"),
            MapKind::Font { label, preset: Some((b, p)), .. } => {
                let name = match &self.source {
                    SampleSource::Font { sf, .. } => sf.preset(*b, *p).map(|x| x.name.clone()).unwrap_or_default(),
                    _ => String::new(),
                };
                format!("SF2 {label}: {b}:{p} {name}")
            }
            MapKind::Wavs { count, .. } => format!("WAV × {count}"),
        }
    }
}

/// HS2 sample banks for the list: names from the content folder, else built in.
fn bank_list(app: &App) -> Vec<(u32, String)> {
    let mut v: Vec<(u32, String)> = if app.lib.sample_banks.len() > 3 {
        app.lib.sample_banks.iter().map(|b| (b.0, b.1.clone())).collect()
    } else {
        sf2::HS2_SAMPLE_BANKS.iter().map(|b| (b.0, b.1.to_string())).collect()
    };
    for e in &app.map {
        if !v.iter().any(|x| x.0 == e.bank) {
            v.push((e.bank, format!("Bank {:03}", e.bank)));
        }
    }
    v.sort_by_key(|x| x.0);
    v.dedup_by_key(|x| x.0);
    v
}

impl App {
    /// Push the mapping into the synth.
    pub fn apply_map(&mut self) {
        let m: Vec<(u32, SampleSource)> = self.map.iter().map(|e| (e.bank, e.source.clone())).collect();
        self.with(|s| s.sample_map = m);
    }

    fn font_for(&mut self, key: &str, data: Option<&[u8]>) -> Option<Arc<SoundFont>> {
        if let Some(f) = self.fonts.get(key) {
            return Some(f.clone());
        }
        let d = match data {
            Some(d) => d.to_vec(),
            None => platform::read(key)?,
        };
        let f = Arc::new(sf2::parse(&d)?);
        self.fonts.insert(key.to_string(), f.clone());
        Some(f)
    }

    pub fn map_set_font(&mut self, bank: u32, name: &str, data: Option<&[u8]>, preset: Option<(u16, u16)>) -> bool {
        let Some(sf) = self.font_for(name, data) else {
            self.status = format!("{}: not a SoundFont", platform::base(name));
            return false;
        };
        let path = platform::is_path(name).then(|| name.to_string());
        self.map.retain(|e| e.bank != bank);
        self.map.push(MapEntry { bank, kind: MapKind::Font { label: platform::base(name).to_string(), path, preset }, source: SampleSource::Font { sf, preset } });
        self.apply_map();
        true
    }

    pub fn map_set_wavs(&mut self, bank: u32, files: &Files) -> bool {
        let named: Vec<(String, Vec<u8>)> = files.iter().map(|(n, d)| (platform::base(n).to_string(), d.clone())).collect();
        let Some(p) = sf2::preset_from_wavs(&format!("bank {bank}"), &named) else {
            self.status = "No readable WAV files (16/24-bit PCM or 32-bit float)".into();
            return false;
        };
        let count = p.zones.len();
        let paths = files.iter().filter(|(n, _)| platform::is_path(n)).map(|(n, _)| n.clone()).collect();
        self.map.retain(|e| e.bank != bank);
        self.map.push(MapEntry { bank, kind: MapKind::Wavs { paths, count }, source: SampleSource::Wavs(Arc::new(p)) });
        self.apply_map();
        true
    }

    /// Settings lines for the mapping (only entries with file paths survive a restart).
    pub fn map_settings(&self) -> String {
        let mut t = String::new();
        for e in &self.map {
            match &e.kind {
                MapKind::Font { path: Some(p), preset, .. } => {
                    let (b, pr) = preset.map(|x| (x.0 as i32, x.1 as i32)).unwrap_or((-1, -1));
                    t += &format!("map={}|sf2|{}|{}|{}\n", e.bank, p, b, pr);
                }
                MapKind::Wavs { paths, .. } if !paths.is_empty() => {
                    t += &format!("map={}|wav|{}\n", e.bank, paths.join("*"));
                }
                _ => {}
            }
        }
        t
    }

    pub fn map_load_setting(&mut self, v: &str) {
        let f: Vec<&str> = v.split('|').collect();
        let Some(bank) = f.first().and_then(|b| b.parse::<u32>().ok()) else { return };
        match f.get(1) {
            Some(&"sf2") if f.len() >= 5 => {
                let preset = match (f[3].parse::<i32>(), f[4].parse::<i32>()) {
                    (Ok(b), Ok(p)) if b >= 0 && p >= 0 => Some((b as u16, p as u16)),
                    _ => None,
                };
                self.map_set_font(bank, f[2], None, preset);
            }
            Some(&"wav") if f.len() >= 3 => {
                let files: Files = f[2].split('*').filter_map(|p| Some((p.to_string(), platform::read(p)?))).collect();
                if !files.is_empty() {
                    self.map_set_wavs(bank, &files);
                }
            }
            _ => {}
        }
    }

    /// What a Sample element of `bank` plays (Osc page).
    pub fn map_describe(&self, bank: u32) -> Option<String> {
        self.map.iter().find(|e| e.bank == bank).map(|e| e.label())
    }

    pub fn map_window(&mut self, ctx: &egui::Context) {
        if !self.map_open {
            return;
        }
        let mut open = true;
        egui::Window::new("Sample Mapping").open(&mut open).default_size([620.0, 520.0]).show(ctx, |ui| {
            ui.label("Choose which samples the Sample elements of each Hypersonic 2 sample bank play. Unassigned banks use the global SoundFont (General MIDI program for the bank), else built-in substitute sounds.");
            ui.label("WAV sets: the root key comes from the file's sample chunk or its name (\"Piano C4.wav\" or \"Piano_060.wav\", C4 = MIDI 60). On drum banks each element picks one file by its Wave index.");
            ui.horizontal(|ui| {
                ui.label(format!("Global SoundFont: {}", self.sf_label()));
                if ui.button("Load…").clicked() {
                    self.pick("sf2", ctx, "SoundFont / WAV", &["sf2", "SF2", "wav", "WAV"]);
                }
            });
            ui.separator();
            let banks = bank_list(self);
            egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
                egui::Grid::new("mapgrid").striped(true).num_columns(3).show(ui, |ui| {
                    for (bank, name) in banks {
                        ui.label(format!("{bank:03} {name}"));
                        let entry = self.map.iter().find(|e| e.bank == bank).cloned();
                        match &entry {
                            Some(e) => {
                                if let (MapKind::Font { label, path, preset }, SampleSource::Font { sf, .. }) = (&e.kind, &e.source) {
                                    let cur = match preset {
                                        Some((b, p)) => sf.preset(*b, *p).map(|x| format!("{b}:{p} {}", x.name)).unwrap_or_default(),
                                        None => "Auto (GM)".into(),
                                    };
                                    let mut chosen: Option<Option<(u16, u16)>> = None;
                                    egui::ComboBox::from_id_source(("mappreset", bank)).width(230.0).selected_text(format!("{label}: {cur}")).show_ui(ui, |ui| {
                                        if ui.selectable_label(preset.is_none(), "Auto (GM)").clicked() {
                                            chosen = Some(None);
                                        }
                                        for pr in &sf.presets {
                                            if ui.selectable_label(*preset == Some((pr.bank, pr.program)), format!("{}:{} {}", pr.bank, pr.program, pr.name)).clicked() {
                                                chosen = Some(Some((pr.bank, pr.program)));
                                            }
                                        }
                                    });
                                    if let Some(c) = chosen {
                                        let key = path.clone().unwrap_or_else(|| label.clone());
                                        let sf = sf.clone();
                                        self.fonts.insert(key.clone(), sf);
                                        self.map_set_font(bank, &key, None, c);
                                    }
                                } else {
                                    ui.label(e.label());
                                }
                            }
                            None => {
                                ui.weak("— global / built-in —");
                            }
                        }
                        ui.horizontal(|ui| {
                            if ui.small_button("SF2…").clicked() {
                                self.map_target = bank;
                                self.pick("map_sf2", ctx, "SoundFont", &["sf2", "SF2"]);
                            }
                            if ui.small_button("WAV…").clicked() {
                                self.map_target = bank;
                                self.pick("map_wav", ctx, "WAV samples", &["wav", "WAV"]);
                            }
                            if entry.is_some() && ui.small_button("Clear").clicked() {
                                self.map.retain(|e| e.bank != bank);
                                self.apply_map();
                            }
                        });
                        ui.end_row();
                    }
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("Clear all").clicked() {
                    self.map.clear();
                    self.apply_map();
                }
                if ui.button("Save").clicked() {
                    self.save_settings();
                    self.status = "Sample mapping saved".into();
                }
            });
        });
        if !open {
            self.map_open = false;
            self.save_settings();
        }
    }
}
