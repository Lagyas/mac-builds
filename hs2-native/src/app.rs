//! The instrument window: Part area, Level strip, Display with its seven
//! pages, Hyper display, six Hyperknobs, wheels and the MIDI keyboard
//! (layout of the original front panel, manual pp. 16-30).

use crate::audio::{self, Audio, Shared};
use crate::midiin;
use crate::pages;
use crate::panel::{self, col, Act, Panel};
use crate::platform::{self, Files, Slot};
use eframe::egui::{self, Align2, Color32, Id, Key, Pos2, Rect, Rounding, Sense, Stroke, Vec2};
use hs2_core::library::Library;
use hs2_core::patch::{pp, Patch};
use hs2_core::synth::{FxSlot, Part, Synth, PARTS};
use hs2_core::{factory, sf2, state};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Load,
    Edit,
    Midi,
    Mix,
    Fx,
    Combi,
    Setup,
}

pub const PAGES: [(Page, &str); 7] = [
    (Page::Load, "LOAD"),
    (Page::Edit, "EDIT"),
    (Page::Midi, "MIDI"),
    (Page::Mix, "MIX"),
    (Page::Fx, "FX"),
    (Page::Combi, "COMBI"),
    (Page::Setup, "SETUP"),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Meter {
    Level,
    Polyphony,
    Velocity,
    Audio,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sub {
    Easy,
    Osc,
    Mod,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModView {
    Matrix,
    Lfo,
    Me,
    Fe,
    Ae,
    Osc,
    Pit,
    Trig,
    Misc,
    Knob,
}

/// A lightweight copy of a part for drawing (taken once per frame).
#[derive(Clone)]
pub struct PartView {
    pub patch: Option<Arc<Patch>>,
    pub mute: bool,
    pub link: bool,
    pub level: f32,
    pub pan: f32,
    pub sends: [f32; 4],
    pub patch_fx_on: bool,
    pub patch_fx: [Option<FxSlot>; 4],
    pub key_range: (i32, i32, i32),
    pub vel_range: (i32, i32, i32),
    pub semi: i32,
    pub cent: f32,
    pub max_voices: usize,
    pub lock: bool,
    pub knobs: [f32; 6],
    pub meter: f32,
    pub meter_vel: f32,
    pub activity: f32,
    pub poly: usize,
    pub output: usize,
}

impl PartView {
    fn of(p: &Part, poly: usize) -> PartView {
        PartView {
            patch: p.patch.clone(),
            mute: p.mute,
            link: p.link,
            level: p.level,
            pan: p.pan,
            sends: p.sends,
            patch_fx_on: p.patch_fx_on,
            patch_fx: p.patch_fx,
            key_range: p.key_range,
            vel_range: p.vel_range,
            semi: p.semi,
            cent: p.cent,
            max_voices: p.max_voices,
            lock: p.lock,
            knobs: p.knobs,
            meter: p.meter,
            meter_vel: p.meter_vel,
            activity: p.activity,
            poly,
            output: p.output,
        }
    }
    pub fn name(&self) -> &str {
        self.patch.as_ref().map(|p| p.display_name()).unwrap_or("")
    }
}

pub struct App {
    pub synth: Shared,
    pub audio: Option<Audio>,
    pub audio_err: Option<String>,
    pub sr: u32,
    pub midi: Option<midiin::Inputs>,
    pub mq: midiin::Queue,
    pub lib: Library,
    pub sel: usize,
    pub page: Page,
    pub parts: Vec<PartView>,
    pub global_fx: [FxSlot; 4],
    pub master: f32,
    pub hype: f32,
    pub master_tune: f32,
    pub tempo: f32,
    pub total_poly: usize,
    pub peak: [f32; 2],
    // load page
    pub search: String,
    pub results: Vec<(u32, usize)>,
    pub open_banks: HashSet<u32>,
    pub load_scroll: f32,
    pub load_sel: Option<(u32, usize)>,
    pub part_src: [Option<(u32, usize)>; PARTS],
    // edit page
    pub edit_elem: Option<usize>,
    pub sub: Sub,
    pub mod_view: ModView,
    pub step_edit: bool,
    pub matrix_scroll: usize,
    // fx page: (global?, slot)
    pub fx_sel: (bool, usize),
    pub meter_mode: Meter,
    pub hyper: Option<(String, f64)>,
    pub kb_base: i32,
    pub mouse_note: Option<(u8, bool)>,
    pub pc_keys: bool,
    pub pc_held: HashMap<Key, u8>,
    pub pc_oct: i32,
    pub pitch_wheel: f32,
    pub mod_wheel: f32,
    pub shown_notes: [bool; 128],
    pub status: String,
    pub content_dir: Option<String>,
    pub sf_name: Option<String>,
    pub picked_files: Slot<Files>,
    pub picked_kind: &'static str,
    pub picked_dir: Slot<String>,
    pub acc: HashMap<Id, f32>,
    pub chain: Vec<(String, Vec<u8>)>,
    pub chain_pos: usize,
    pub preload: usize,
    pub pgm_change: bool,
    pub combi_sel: usize,
    pub vel_curve: usize,
    pub rename: Option<(usize, String)>,
    pub about: bool,
    pub confirm_clear: bool,
    pub combi_name: String,
    pub map: Vec<crate::mapwin::MapEntry>,
    pub map_open: bool,
    pub map_target: u32,
    pub fonts: HashMap<String, Arc<sf2::SoundFont>>,
}

fn now() -> f64 {
    web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0)
}

pub fn note_name(n: i32) -> String {
    const N: [&str; 12] = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    format!("{}{}", N[n.rem_euclid(12) as usize], n.div_euclid(12) - 2)
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> App {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::light();
        cc.egui_ctx.set_style(style);
        let sr = audio::default_rate().unwrap_or(48000);
        let synth: Shared = Arc::new(Mutex::new(Synth::new(sr as f32)));
        let mut lib = Library::default();
        for (name, patches) in factory::banks() {
            let n = lib.banks.len() as u32 + 90;
            lib.banks.push(hs2_core::library::Bank { number: n, name: name.into(), user: false, patches: patches.into_iter().map(Arc::new).collect() });
        }
        let mq: midiin::Queue = Arc::new(Mutex::new(Vec::new()));
        let mut app = App {
            synth: synth.clone(),
            audio: None,
            audio_err: None,
            sr,
            midi: None,
            mq: mq.clone(),
            lib,
            sel: 0,
            page: Page::Load,
            parts: Vec::new(),
            global_fx: [FxSlot::new(1); 4],
            master: 0.7,
            hype: 0.0,
            master_tune: 0.0,
            tempo: 120.0,
            total_poly: 0,
            peak: [0.0; 2],
            search: String::new(),
            results: Vec::new(),
            open_banks: HashSet::new(),
            load_scroll: 0.0,
            load_sel: None,
            part_src: [None; PARTS],
            edit_elem: None,
            sub: Sub::Easy,
            mod_view: ModView::Matrix,
            step_edit: false,
            matrix_scroll: 0,
            fx_sel: (true, 0),
            meter_mode: Meter::Level,
            hyper: None,
            kb_base: 36,
            mouse_note: None,
            pc_keys: false,
            pc_held: HashMap::new(),
            pc_oct: 0,
            pitch_wheel: 0.0,
            mod_wheel: 0.0,
            shown_notes: [false; 128],
            status: String::new(),
            content_dir: None,
            sf_name: None,
            picked_files: Default::default(),
            picked_kind: "",
            picked_dir: Default::default(),
            acc: HashMap::new(),
            chain: Vec::new(),
            chain_pos: 0,
            preload: 0,
            pgm_change: true,
            combi_sel: 0,
            vel_curve: 1,
            rename: None,
            about: false,
            confirm_clear: false,
            combi_name: "Init Combi".into(),
            map: Vec::new(),
            map_open: false,
            map_target: 0,
            fonts: HashMap::new(),
        };
        // MIDI in (native: callback straight into the synth)
        let s2 = synth.clone();
        app.midi = Some(midiin::open(
            move |m: &[u8]| {
                if let Ok(mut s) = s2.lock() {
                    s.midi(m);
                }
            },
            &mq,
        ));
        #[cfg(not(target_arch = "wasm32"))]
        app.start_audio();
        app.load_settings();
        if app.parts_empty() {
            let first = app.lib.banks.first().map(|b| b.number);
            if let Some(b) = first {
                app.load_patch(0, b, 0);
                app.open_banks.insert(b);
            }
        }
        app.refresh();
        app
    }

    fn parts_empty(&self) -> bool {
        self.synth.lock().map(|s| s.parts.iter().all(|p| p.patch.is_none())).unwrap_or(true)
    }

    pub fn start_audio(&mut self) {
        if self.audio.is_some() {
            return;
        }
        match audio::start(self.synth.clone()) {
            Ok(a) => {
                if a.sample_rate != self.sr {
                    // re-create the synth at the device rate, keeping the setup
                    let mut s = self.synth.lock().unwrap();
                    let saved = state::save(&s, "x");
                    let sf = s.sf.clone();
                    let wt = s.wt.clone();
                    *s = Synth::new(a.sample_rate as f32);
                    s.sf = sf;
                    s.wt = wt;
                    state::load(&mut s, &saved);
                    self.sr = a.sample_rate;
                }
                self.status = format!("Audio: {} @ {} Hz", a.device, a.sample_rate);
                self.audio = Some(a);
            }
            Err(e) => self.audio_err = Some(e),
        }
    }

    // -----------------------------------------------------------------------
    // settings
    // -----------------------------------------------------------------------

    fn load_settings(&mut self) {
        let Some(t) = platform::load_settings() else { return };
        for l in t.trim_start_matches('\u{feff}').lines() {
            let Some((k, v)) = l.split_once('=') else { continue };
            match k.trim() {
                "content" if !v.trim().is_empty() => {
                    let dir = v.trim().to_string();
                    self.load_content_dir(&dir);
                }
                "soundfont" if !v.trim().is_empty() => {
                    if let Some(d) = platform::read(v.trim()) {
                        self.set_soundfont(v.trim().rsplit(['/', '\\']).next().unwrap_or("sf2"), &d, Some(v.trim()));
                    }
                }
                "map" => self.map_load_setting(v.trim()),
                "hype" => self.with(|s| s.hype = v.trim().parse().unwrap_or(0.0)),
                "master" => self.with(|s| s.master = v.trim().parse().unwrap_or(0.7)),
                _ => {}
            }
        }
    }

    pub fn save_settings(&self) {
        let mut t = String::new();
        if let Some(c) = &self.content_dir {
            t += &format!("content={c}\n");
        }
        if let Some(c) = self.sf_path() {
            t += &format!("soundfont={c}\n");
        }
        t += &format!("hype={}\nmaster={}\n", self.hype, self.master);
        t += &self.map_settings();
        platform::save_settings(&t);
    }

    fn sf_path(&self) -> Option<String> {
        self.sf_name.as_ref().and_then(|n| n.split_once('|').map(|(_, p)| p.to_string()))
    }

    pub fn sf_label(&self) -> String {
        self.sf_name.as_ref().map(|n| n.split('|').next().unwrap_or("").to_string()).unwrap_or_else(|| "(built-in sounds)".into())
    }

    // -----------------------------------------------------------------------
    // content
    // -----------------------------------------------------------------------

    pub fn load_content_dir(&mut self, dir: &str) {
        let files = platform::read_tree(dir);
        let n = self.add_content(&files);
        if n > 0 {
            self.content_dir = Some(dir.to_string());
        }
    }

    /// Add Hypersonic 2 content files (relative names). Returns patch banks added.
    pub fn add_content(&mut self, files: &Files) -> usize {
        let mut banks = 0;
        for (name, data) in files {
            let base = name.rsplit('/').next().unwrap_or(name);
            if let Some(hs2_core::bank::BankKind::Patch) = self.lib.add_file(base, data) {
                banks += 1;
            }
        }
        // combis (.fxb)
        for (name, data) in files {
            if name.to_lowercase().ends_with(".fxb") {
                self.lib.add_combi(data);
            }
        }
        self.lib.combis.sort_by(|a, b| a.name.cmp(&b.name));
        // user banks: folders of .hsp patches under Patches/
        let mut user: HashMap<String, Vec<Patch>> = HashMap::new();
        for (name, data) in files {
            let lower = name.to_lowercase();
            if lower.ends_with(".hsp") {
                let folder = name.rsplit('/').nth(1).unwrap_or("User").to_string();
                if let Some(p) = Patch::parse(data) {
                    user.entry(folder).or_default().push(p);
                }
            }
        }
        for (f, ps) in user {
            self.lib.add_user_bank(&f, ps);
        }
        let phrases = self.lib.phrases.clone();
        let nph: usize = phrases.iter().map(|b| b.1.len()).sum();
        self.with(|s| s.phrases = phrases);
        self.status = format!("Content: {} patches in {} banks, {} phrases, {} sample banks", self.lib.total_patches(), self.lib.banks.len(), nph, self.lib.sample_banks.len());
        banks
    }

    pub fn set_soundfont(&mut self, name: &str, data: &[u8], path: Option<&str>) {
        let sf = if name.to_lowercase().ends_with(".wav") {
            sf2::parse_wav(name, data, 60).map(|p| sf2::SoundFont { name: name.into(), presets: vec![p] })
        } else {
            sf2::parse(data)
        };
        match sf {
            Some(sf) => {
                let n = sf.presets.len();
                self.with(|s| s.sf = Some(Arc::new(sf)));
                self.sf_name = Some(match path {
                    Some(p) => format!("{name}|{p}"),
                    None => name.to_string(),
                });
                self.status = format!("SoundFont {name}: {n} presets");
            }
            None => self.status = format!("{name}: not a SoundFont / WAV"),
        }
    }

    // -----------------------------------------------------------------------
    // synth access
    // -----------------------------------------------------------------------

    pub fn with<R>(&self, f: impl FnOnce(&mut Synth) -> R) -> R {
        let mut s = self.synth.lock().unwrap();
        f(&mut s)
    }

    pub fn refresh(&mut self) {
        let s = self.synth.lock().unwrap();
        self.parts = s.parts.iter().enumerate().map(|(i, p)| PartView::of(p, s.polyphony(i))).collect();
        self.global_fx = s.global_fx;
        self.master = s.master;
        self.hype = s.hype;
        self.master_tune = s.master_tune;
        self.tempo = s.tempo;
        self.total_poly = s.total_polyphony();
        self.peak = s.peak;
    }

    pub fn load_patch(&mut self, part: usize, bank: u32, idx: usize) {
        let Some(p) = self.lib.bank(bank).and_then(|b| b.patches.get(idx)).cloned() else { return };
        let tables = Synth::tables_for(&p);
        let wt = self.synth.lock().unwrap().wt.clone();
        for t in tables {
            wt.get(t);
        }
        self.with(|s| s.set_patch(part, Some(p.clone())));
        self.part_src[part] = Some((bank, idx));
        self.show_hyper(format!("{}\n{}\nBank {} · Prog {}", p.display_name(), p.comment, bank, idx), 4.0);
        if self.edit_elem.map(|e| e >= p.elements.len()).unwrap_or(false) {
            self.edit_elem = None;
        }
    }

    /// Load a Hypersonic 2 combi (Load page).
    pub fn load_combi(&mut self, idx: usize) {
        let Some(c) = self.lib.combis.get(idx).cloned() else { return };
        self.with(|s| s.load_combi(&c));
        self.part_src = [None; PARTS];
        self.combi_name = c.name.clone();
        self.sel = 0;
        let parts: Vec<String> = c.parts.iter().map(|p| format!("{}: {}", p.part + 1, p.patch.display_name())).collect();
        self.show_hyper(format!("Combi {}
{}", c.name, parts.join(", ")), 4.0);
    }

    /// Apply an edit to the selected part's patch (copy, modify, swap in).
    pub fn edit(&mut self, f: impl FnOnce(&mut Patch)) {
        let part = self.sel;
        let Some(p) = self.parts[part].patch.clone() else { return };
        let mut q = (*p).clone();
        q.make_own();
        f(&mut q);
        let tables = Synth::tables_for(&q);
        let wt = self.synth.lock().unwrap().wt.clone();
        for t in tables {
            wt.get(t);
        }
        self.with(|s| s.update_patch(part, q));
    }

    pub fn show_hyper(&mut self, t: String, secs: f64) {
        self.hyper = Some((t, now() + secs));
    }

    pub fn acc(&mut self, id: Id) -> &mut f32 {
        self.acc.entry(id).or_insert(0.0)
    }

    pub fn note(&mut self, part: Option<usize>, n: u8, vel: u8, on: bool) {
        let ch = {
            let mut j = self.sel;
            while j > 0 && self.parts[j].link {
                j -= 1;
            }
            j
        };
        self.with(|s| match part {
            Some(p) => {
                if on {
                    s.note_on(p, n, vel)
                } else {
                    s.note_off(p, n)
                }
            }
            None => s.midi(&[if on { 0x90 } else { 0x80 } | ch as u8, n, if on { vel } else { 0 }]),
        });
        self.shown_notes[n as usize] = on;
    }

    pub fn cc_sel(&mut self, cc: u8, v: u8) {
        let ch = {
            let mut j = self.sel;
            while j > 0 && self.parts[j].link {
                j -= 1;
            }
            j
        };
        self.with(|s| s.midi(&[0xb0 | ch as u8, cc, v]));
    }

    pub fn pick(&mut self, kind: &'static str, ctx: &egui::Context, filter: &str, exts: &[&str]) {
        self.picked_kind = kind;
        platform::pick_files(&self.picked_files, ctx, filter, exts);
    }

    fn handle_picked(&mut self) {
        let added = platform::take_added();
        if !added.is_empty() {
            self.add_content(&added);
            if self.content_dir.is_none() {
                self.content_dir = Some("(browser folder)".into());
            }
        }
        let dir = self.picked_dir.borrow_mut().take();
        if let Some(dir) = dir {
            self.load_content_dir(&dir);
            self.save_settings();
        }
        let files = self.picked_files.borrow_mut().take();
        let Some(files) = files else { return };
        match self.picked_kind {
            "sf2" => {
                if let Some((n, d)) = files.into_iter().next() {
                    let path = platform::is_path(&n).then(|| n.clone());
                    self.set_soundfont(platform::base(&n), &d, path.as_deref());
                    self.save_settings();
                }
            }
            "map_sf2" => {
                if let Some((n, d)) = files.into_iter().next() {
                    let bank = self.map_target;
                    if self.map_set_font(bank, &n, Some(&d), None) {
                        self.status = format!("Bank {bank:03} -> {}", platform::base(&n));
                        self.save_settings();
                    }
                }
            }
            "map_wav" => {
                let bank = self.map_target;
                if self.map_set_wavs(bank, &files) {
                    self.status = format!("Bank {bank:03} -> {} WAV files", files.len());
                    self.save_settings();
                }
            }
            "content" => {
                self.add_content(&files);
            }
            "patch" => {
                for (n, d) in files {
                    if let Some(p) = Patch::parse(&d).or_else(|| hs2_core::bank::patches(&d).into_iter().next()) {
                        let part = self.sel;
                        self.with(|s| s.set_patch(part, Some(Arc::new(p))));
                        self.status = format!("Loaded {}", platform::base(&n));
                    }
                }
            }
            "combi" => {
                if let Some((n, d)) = files.into_iter().next() {
                    let r = self.with(|s| state::load(s, &d));
                    match r {
                        Some(name) => {
                            self.combi_name = name;
                            self.status = format!("Combi {n} loaded");
                        }
                        None => self.status = format!("{n}: not a combi of this synth"),
                    }
                }
            }
            "chain" => {
                for (n, d) in files {
                    self.chain.push((platform::base(&n).to_string(), d));
                }
            }
            _ => {}
        }
    }

    pub fn save_patch_file(&mut self) {
        let Some(p) = self.parts[self.sel].patch.clone() else { return };
        let mut q = (*p).clone();
        q.make_own();
        let name = format!("{}.hsp", q.display_name());
        if let Some(st) = platform::save_file(&name, "hsp", "Patch", &q.write()) {
            self.status = st;
        }
    }

    pub fn save_combi_file(&mut self) {
        let d = self.with(|s| state::save(s, &self.combi_name.clone()));
        let name = format!("{}.hs2", self.combi_name);
        if let Some(st) = platform::save_file(&name, "hs2", "Combi", &d) {
            self.status = st;
        }
    }

    /// Store the selected part's patch in the open (or first) user bank.
    pub fn save_to_user_bank(&mut self) {
        let Some(p) = self.parts[self.sel].patch.clone() else { return };
        let mut q = (*p).clone();
        q.make_own();
        let target = self.open_banks.iter().copied().filter(|b| self.lib.bank(*b).map(|x| x.user).unwrap_or(false)).min();
        let num = match target.or_else(|| self.lib.banks.iter().find(|b| b.user).map(|b| b.number)) {
            Some(n) => n,
            None => self.lib.add_user_bank("User Patches", Vec::new()),
        };
        let bank = self.lib.bank_mut(num).unwrap();
        bank.patches.push(Arc::new(q.clone()));
        let idx = bank.patches.len() - 1;
        self.part_src[self.sel] = Some((num, idx));
        self.open_banks.insert(num);
        // natively also write the file into the content folder
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(dir) = &self.content_dir {
            let bname = self.lib.bank(num).map(|b| b.name.clone()).unwrap_or_default();
            let folder = std::path::Path::new(dir).join("Patches").join(format!("{num:03} {bname}"));
            let _ = std::fs::create_dir_all(&folder);
            let _ = std::fs::write(folder.join(format!("{}.hsp", q.display_name())), q.write());
        }
        self.status = format!("Saved \"{}\" to user bank {num}", q.display_name());
    }

    // -----------------------------------------------------------------------
    // drawing
    // -----------------------------------------------------------------------

    fn chassis(&self, ui: &mut egui::Ui, pn: &Panel) {
        let p = ui.painter();
        let full = pn.r(0.0, 0.0, panel::W, panel::H);
        p.rect_filled(full, Rounding::same(10.0 * pn.s), Color32::from_rgb(20, 20, 22));
        p.rect_filled(pn.r(4.0, 4.0, panel::W - 4.0, 384.0), Rounding::same(8.0 * pn.s), col::CHASSIS);
        p.rect_filled(pn.r(4.0, 386.0, panel::W - 4.0, 488.0), Rounding::same(4.0 * pn.s), Color32::from_rgb(170, 175, 181));
        p.rect_filled(pn.r(4.0, 490.0, panel::W - 4.0, panel::H - 4.0), Rounding::same(8.0 * pn.s), col::CHASSIS);
        for (x, t) in [(65.0, "MUTE"), (105.0, "LINK"), (205.0, "PATCH"), (395.0, "")] {
            panel::text(ui, pn.p(x, 16.0), Align2::CENTER_CENTER, t, pn.f(8.0), col::DARK);
        }
        // logo + name (own branding)
        p.rect_filled(pn.r(16.0, 442.0, 314.0, 476.0), Rounding::same(3.0 * pn.s), col::DARK);
        panel::text(ui, pn.p(28.0, 455.0), Align2::LEFT_CENTER, "hs2 · synth", pn.f(14.0), Color32::from_rgb(220, 224, 230));
        for k in 0..3 {
            let y = 449.0 + k as f32 * 5.0;
            p.line_segment([pn.p(232.0, y), pn.p(300.0, y)], Stroke::new(1.0 * pn.s, Color32::from_rgb(90, 94, 100)));
        }
        panel::text(ui, pn.p(772.0, 480.0), Align2::RIGHT_CENTER, "HS2 · HYPERSYNTH  2", pn.f(13.0), Color32::from_rgb(50, 54, 60));
    }

    fn part_area(&mut self, ui: &mut egui::Ui, pn: &Panel) {
        let row_h = 21.75;
        let label = match self.meter_mode {
            Meter::Level => "LEVEL",
            Meter::Polyphony => "POLYPHONY",
            Meter::Velocity => "VELOCITY",
            Meter::Audio => "AUDIO LEVEL",
        };
        let lr = pn.r(340.0, 10.0, 452.0, 22.0);
        let lresp = ui.interact(lr, Id::new("meter-label"), Sense::click());
        panel::text(ui, lr.center(), Align2::CENTER_CENTER, label, pn.f(8.0), if lresp.hovered() { col::BLUE } else { col::DARK });
        if lresp.clicked() {
            self.meter_mode = match self.meter_mode {
                Meter::Level => Meter::Polyphony,
                Meter::Polyphony => Meter::Velocity,
                Meter::Velocity => Meter::Audio,
                Meter::Audio => Meter::Level,
            };
        }
        let mut drop_target: Option<usize> = None;
        for i in 0..PARTS {
            let y0 = 26.0 + i as f32 * row_h;
            let y1 = y0 + row_h - 3.0;
            let sel = i == self.sel;
            let row = pn.r(8.0, y0 - 1.0, 470.0, y1 + 1.0);
            if sel {
                ui.painter().rect_filled(row, Rounding::same(3.0 * pn.s), Color32::from_rgb(150, 172, 205));
            }
            panel::text(ui, pn.p(38.0, (y0 + y1) / 2.0), Align2::RIGHT_CENTER, &format!("{}", i + 1), pn.f(9.0), col::DARK);
            let pv = self.parts[i].clone();
            // mute (orange) doubling as MIDI activity LED
            let led = if pv.mute { col::ORANGE } else if pv.activity > 0.2 { Color32::from_rgb(120, 230, 255) } else { col::LED_OFF };
            let mr = panel::led_button(ui, pn, Id::new(("mute", i)), pn.r(50.0, y0 + 1.0, 78.0, y1 - 1.0), pv.mute || pv.activity > 0.2, led);
            if mr.clicked() {
                self.with(|s| s.parts[i].mute = !s.parts[i].mute);
            }
            // link arrow
            let lk = pn.r(84.0, y0, 120.0, y1);
            let lresp = ui.interact(lk, Id::new(("link", i)), Sense::click());
            let p = ui.painter();
            p.rect_filled(pn.r(84.0, y0, 330.0, y1), Rounding::same(3.0 * pn.s), col::LCD);
            p.line_segment([pn.p(120.0, y0), pn.p(120.0, y1)], Stroke::new(pn.s, col::LCD_LINE));
            if i > 0 {
                let c = col::LCD_TEXT;
                let ym = (y0 + y1) / 2.0;
                if pv.link {
                    // right-angled arrow: layered with the part above
                    p.line_segment([pn.p(100.0, y0 - 2.0), pn.p(100.0, ym)], Stroke::new(1.5 * pn.s, c));
                    p.line_segment([pn.p(100.0, ym), pn.p(114.0, ym)], Stroke::new(1.5 * pn.s, c));
                } else {
                    p.line_segment([pn.p(92.0, ym), pn.p(112.0, ym)], Stroke::new(1.0 * pn.s, col::LCD_DIM));
                }
                p.line_segment([pn.p(110.0, ym - 3.0), pn.p(114.0, ym)], Stroke::new(1.5 * pn.s, if pv.link { c } else { col::LCD_DIM }));
                p.line_segment([pn.p(110.0, ym + 3.0), pn.p(114.0, ym)], Stroke::new(1.5 * pn.s, if pv.link { c } else { col::LCD_DIM }));
                if lresp.clicked() {
                    self.with(|s| s.parts[i].link = !s.parts[i].link);
                }
            }
            // patch name
            let nr = pn.r(122.0, y0, 330.0, y1);
            let nresp = ui.interact(nr, Id::new(("pname", i)), Sense::click_and_drag());
            if let Some((ri, buf)) = &mut self.rename {
                if *ri == i {
                    let te = ui.put(nr, egui::TextEdit::singleline(buf).font(pn.f(9.0)).frame(false));
                    te.request_focus();
                    if te.lost_focus() || ui.input(|x| x.key_pressed(Key::Enter)) {
                        let name = buf.clone();
                        self.rename = None;
                        self.sel = i;
                        self.edit(|p| p.name = name);
                    }
                    continue;
                }
            }
            let name = pv.name().to_string();
            let layer = pv.patch.as_ref().map(|p| p.is_layer()).unwrap_or(false);
            panel::text(ui, nr.left_center() + Vec2::new(4.0 * pn.s, 0.0), Align2::LEFT_CENTER, &if layer { format!("≈ {name}") } else { name.clone() }, pn.f(9.5), col::LCD_TEXT);
            if nresp.clicked() {
                self.sel = i;
                if self.edit_elem.map(|e| e >= pv.patch.as_ref().map(|p| p.elements.len()).unwrap_or(0)).unwrap_or(false) {
                    self.edit_elem = None;
                }
                if let Some(p) = &pv.patch {
                    let t = format!("{}\n{}\n{}", p.display_name(), p.comment, p.controllers.replace('|', "\n"));
                    self.show_hyper(t, 6.0);
                }
            }
            if nresp.double_clicked() {
                if pv.patch.is_some() {
                    self.rename = Some((i, name.clone()));
                } else {
                    self.sel = i;
                    self.page = Page::Load;
                }
            }
            if nresp.drag_stopped() {
                if let Some(pos) = ui.input(|x| x.pointer.interact_pos()) {
                    let pp_ = pn.inv(pos);
                    let t = ((pp_.y - 26.0) / row_h).floor() as i32;
                    if (0..16).contains(&t) && t as usize != i {
                        let shift = ui.input(|x| x.modifiers.shift);
                        let t = t as usize;
                        self.with(|s| {
                            let src = s.parts[i].clone();
                            let patch = src.patch.clone();
                            s.set_patch(t, patch);
                            let d = &mut s.parts[t];
                            d.level = src.level;
                            d.pan = src.pan;
                            d.sends = src.sends;
                            d.key_range = src.key_range;
                            d.vel_range = src.vel_range;
                            d.semi = src.semi;
                            d.cent = src.cent;
                            if shift {
                                s.set_patch(i, None);
                            }
                        });
                        self.sel = t;
                    }
                }
            }
            nresp.context_menu(|ui| {
                if ui.button("Save").clicked() {
                    self.sel = i;
                    self.save_to_user_bank();
                    ui.close_menu();
                }
                if ui.button("Save as file…").clicked() {
                    self.sel = i;
                    self.save_patch_file();
                    ui.close_menu();
                }
                if ui.button("Find on Load Page").clicked() {
                    self.sel = i;
                    self.page = Page::Load;
                    if let Some((b, k)) = self.part_src[i] {
                        self.open_banks.insert(b);
                        self.load_sel = Some((b, k));
                    }
                    ui.close_menu();
                }
                if ui.button("Reset").clicked() {
                    if let Some((b, k)) = self.part_src[i] {
                        self.load_patch(i, b, k);
                    }
                    ui.close_menu();
                }
                if ui.button("Delete").clicked() {
                    self.with(|s| s.set_patch(i, None));
                    ui.close_menu();
                }
            });
            if ui.input(|x| x.pointer.any_released()) && nresp.hovered() {
                drop_target = Some(i);
            }
            // level strip
            let meter = match self.meter_mode {
                Meter::Level => None,
                Meter::Polyphony => Some((pv.poly as f32 / 16.0).min(1.0)),
                Meter::Velocity => Some(if pv.activity > 0.01 { pv.meter_vel } else { 0.0 }),
                Meter::Audio => Some((pv.meter * 2.0).min(1.0)),
            };
            let (nv, _) = panel::level_strip(ui, pn, Id::new(("level", i)), pn.r(338.0, y0, 452.0, y1), pv.level, meter, 0.75);
            if let Some(v) = nv {
                self.with(|s| s.parts[i].level = v);
                self.show_hyper(format!("Part {} Level {:.0}%", i + 1, v * 100.0), 1.5);
            }
        }
        let _ = drop_target;
    }

    fn page_buttons(&mut self, ui: &mut egui::Ui, pn: &Panel) {
        for (k, (pg, name)) in PAGES.iter().enumerate() {
            let y = 34.0 + k as f32 * 50.0;
            let r = panel::led_button(ui, pn, Id::new(("page", k)), pn.r(756.0, y, 788.0, y + 16.0), self.page == *pg, Color32::from_rgb(90, 220, 255));
            panel::text(ui, pn.p(772.0, y + 24.0), Align2::CENTER_CENTER, name, pn.f(7.5), col::DARK);
            if r.clicked() {
                self.page = *pg;
            }
        }
    }

    fn hyper_display(&mut self, ui: &mut egui::Ui, pn: &Panel) {
        let r = pn.r(16.0, 396.0, 314.0, 438.0);
        panel::lcd(ui, r, pn.s);
        let t = match &self.hyper {
            Some((t, until)) if *until > now() => t.clone(),
            _ => {
                self.hyper = None;
                match &self.parts[self.sel].patch {
                    Some(p) => {
                        let ctl = p.controllers.replace('|', "  ");
                        format!("{}\n{}", if p.comment.is_empty() { p.display_name() } else { &p.comment }, ctl)
                    }
                    None => format!("Part {} — double-click to load a patch", self.sel + 1),
                }
            }
        };
        let galley = ui.painter().layout(t, pn.f(9.0), col::LCD_TEXT, r.width() - 10.0 * pn.s);
        ui.painter().with_clip_rect(r.shrink(3.0 * pn.s)).galley(r.left_top() + Vec2::new(5.0, 4.0) * pn.s, galley, col::LCD_TEXT);
        let resp = ui.interact(r, Id::new("hyperdisp"), Sense::click());
        if resp.double_clicked() {
            self.rename = None;
        }
    }

    fn hyperknobs(&mut self, ui: &mut egui::Ui, pn: &Panel) {
        let pv = self.parts[self.sel].clone();
        for k in 0..6 {
            let x0 = 342.0 + k as f32 * 72.0;
            let br = pn.r(x0, 396.0, x0 + 68.0, 424.0);
            panel::lcd(ui, br, pn.s);
            let name = pv.patch.as_ref().map(|p| p.knob_names[k].clone()).unwrap_or_default();
            ui.painter().with_clip_rect(br).text(br.center(), Align2::CENTER_CENTER, if name.is_empty() { "-" } else { &name }, pn.f(8.5), col::LCD_TEXT);
            let rr = pn.r(x0 + 4.0, 438.0, x0 + 64.0, 470.0);
            let default = pv.patch.as_ref().map(|p| {
                let o = p.p[pp::KNOB[k] + 1];
                if o <= 0.0 || o >= 1.0 {
                    0.5
                } else {
                    o
                }
            });
            let (nv, resp) = panel::roller(ui, pn, Id::new(("hk", k)), rr, pv.knobs[k], default.unwrap_or(0.5));
            if let (Some(v), true) = (nv, pv.patch.is_some()) {
                let part = self.sel;
                self.with(|s| {
                    s.parts[part].knobs[k] = v;
                    s.parts[part].update_macros();
                });
                self.show_hyper(format!("{}: {:.0}%", if name.is_empty() { format!("Knob {}", k + 1) } else { name.clone() }, v * 100.0), 1.5);
            }
            resp.on_hover_text("Hyperknob: drag; Ctrl-click resets");
        }
    }

    fn wheels_and_keys(&mut self, ui: &mut egui::Ui, pn: &Panel) {
        // wheels
        for (k, label) in [(0usize, "PITCH"), (1, "MOD")] {
            let x0 = 40.0 + k as f32 * 34.0;
            panel::text(ui, pn.p(x0 + 9.0, 497.0), Align2::CENTER_CENTER, label, pn.f(7.0), col::DARK);
            let r = pn.r(x0, 503.0, x0 + 18.0, 566.0);
            let resp = ui.interact(r, Id::new(("wheel", k)), Sense::drag());
            let v = if k == 0 { self.pitch_wheel } else { self.mod_wheel };
            let p = ui.painter();
            p.rect_filled(r, Rounding::same(3.0 * pn.s), col::DARK);
            let yc = if k == 0 { r.center().y - v * r.height() * 0.45 } else { r.bottom() - v * r.height() };
            for j in 0..10 {
                let y = r.top() + (j as f32 + 0.5) * r.height() / 10.0;
                let sh = (1.0 - ((y - yc) / r.height()).abs() * 2.0).clamp(0.2, 1.0);
                p.line_segment([Pos2::new(r.left() + 2.0, y), Pos2::new(r.right() - 2.0, y)], Stroke::new(1.0f32, Color32::from_gray((80.0 + 140.0 * sh) as u8)));
            }
            p.line_segment([Pos2::new(r.left(), yc), Pos2::new(r.right(), yc)], Stroke::new(2.0 * pn.s, Color32::WHITE));
            if resp.dragged() {
                let d = -resp.drag_delta().y / (r.height() * if k == 0 { 0.45 } else { 1.0 });
                if k == 0 {
                    self.pitch_wheel = (self.pitch_wheel + d).clamp(-1.0, 1.0);
                    let v14 = ((self.pitch_wheel + 1.0) * 8191.5) as i32;
                    let ch = self.chan();
                    self.with(|s| s.midi(&[0xe0 | ch, (v14 & 0x7f) as u8, (v14 >> 7) as u8]));
                } else {
                    self.mod_wheel = (self.mod_wheel + d).clamp(0.0, 1.0);
                    self.cc_sel(1, (self.mod_wheel * 127.0) as u8);
                    self.show_hyper(format!("Mod Wheel {:.0}\n{}", self.mod_wheel * 127.0, self.parts[self.sel].patch.as_ref().map(|p| p.controllers.replace('|', "\n")).unwrap_or_default()), 2.0);
                }
            }
            if k == 0 && resp.drag_stopped() {
                self.pitch_wheel = 0.0;
                let ch = self.chan();
                self.with(|s| s.midi(&[0xe0 | ch, 0, 64]));
            }
        }
        // keyboard: 61 keys
        let kr = pn.r(104.0, 496.0, 776.0, 568.0);
        let p = ui.painter();
        p.rect_filled(kr, Rounding::same(2.0 * pn.s), col::DARK);
        let whites: Vec<i32> = (0..61).map(|k| self.kb_base + k).filter(|n| ![1, 3, 6, 8, 10].contains(&(n % 12))).collect();
        let ww = kr.width() / whites.len() as f32;
        let mut wrects = Vec::new();
        let edit_range = self.edit_key_range();
        for (k, &n) in whites.iter().enumerate() {
            let r = Rect::from_min_size(Pos2::new(kr.left() + k as f32 * ww, kr.top() + 2.0), Vec2::new(ww - 1.0, kr.height() - 4.0));
            let down = self.shown_notes.get(n as usize).copied().unwrap_or(false);
            p.rect_filled(r, Rounding::same(1.5 * pn.s), if down { col::CYAN } else { Color32::from_rgb(244, 244, 246) });
            if n % 12 == 0 {
                p.text(Pos2::new(r.center().x, r.bottom() - 7.0 * pn.s), Align2::CENTER_CENTER, format!("{}", n / 12 - 2), pn.f(7.0), Color32::from_gray(150));
            }
            if let Some((lo, hi)) = edit_range {
                if n >= lo && n <= hi {
                    p.rect_filled(Rect::from_min_size(Pos2::new(r.left(), kr.top()), Vec2::new(ww, 2.5 * pn.s)), Rounding::ZERO, col::RED);
                }
            }
            wrects.push((r, n));
        }
        let mut brects = Vec::new();
        for (k, &n) in whites.iter().enumerate() {
            let b = n + 1;
            if [1, 3, 6, 8, 10].contains(&(b % 12)) && b < self.kb_base + 61 {
                let x = kr.left() + (k as f32 + 1.0) * ww - ww * 0.3;
                let r = Rect::from_min_size(Pos2::new(x, kr.top() + 2.0), Vec2::new(ww * 0.6, kr.height() * 0.6));
                let down = self.shown_notes.get(b as usize).copied().unwrap_or(false);
                p.rect_filled(r, Rounding::same(1.0 * pn.s), if down { col::CYAN } else { Color32::from_rgb(20, 20, 24) });
                brects.push((r, b));
            }
        }
        let resp = ui.interact(kr, Id::new("keyboard"), Sense::click_and_drag());
        let shift = ui.input(|x| x.modifiers.shift);
        if shift && resp.dragged() {
            let d = resp.drag_delta().x;
            let acc = self.acc(Id::new("kbshift"));
            *acc += d;
            let steps = (*acc / (ww * 7.0)) as i32;
            if steps != 0 {
                *acc -= steps as f32 * ww * 7.0;
                self.kb_base = (self.kb_base - 12 * steps).clamp(0, 127 - 60);
            }
            return;
        }
        let pointer = ui.input(|x| x.pointer.interact_pos());
        let primary = ui.input(|x| x.pointer.primary_down());
        let secondary = ui.input(|x| x.pointer.secondary_down());
        let hit = |pos: Pos2| -> Option<(u8, u8)> {
            let find = |rs: &Vec<(Rect, i32)>| rs.iter().find(|(r, _)| r.contains(pos)).map(|(r, n)| (*n as u8, (((pos.y - r.top()) / r.height()) * 110.0 + 17.0).clamp(1.0, 127.0) as u8));
            find(&brects).or_else(|| find(&wrects))
        };
        let want = if (primary || secondary) && (resp.is_pointer_button_down_on() || self.mouse_note.is_some()) { pointer.and_then(hit) } else { None };
        let cur = self.mouse_note.map(|m| m.0);
        if want.map(|w| w.0) != cur {
            if let Some((n, solo)) = self.mouse_note.take() {
                self.note(if solo { Some(self.sel) } else { None }, n, 0, false);
            }
            if let Some((n, v)) = want {
                let solo = secondary || ui.input(|x| x.modifiers.command);
                self.note(if solo { Some(self.sel) } else { None }, n, v, true);
                self.mouse_note = Some((n, solo));
                if solo && self.page == Page::Edit && secondary {
                    self.next_elem_for_key(n as i32);
                }
            }
        }
    }

    fn chan(&self) -> u8 {
        let mut j = self.sel;
        while j > 0 && self.parts[j].link {
            j -= 1;
        }
        j as u8
    }

    fn edit_key_range(&self) -> Option<(i32, i32)> {
        if self.page != Page::Edit {
            return None;
        }
        let e = self.edit_elem?;
        let p = self.parts[self.sel].patch.as_ref()?;
        let el = p.elements.get(e)?;
        let (lo, hi, _) = el.key_range();
        Some((lo, if el.p[hs2_core::patch::ep::KEY_HI] == 0.0 { 127 } else { hi }))
    }

    /// Edit page: right-click on a key shows the next element that responds to it.
    fn next_elem_for_key(&mut self, n: i32) {
        let Some(p) = self.parts[self.sel].patch.clone() else { return };
        let start = self.edit_elem.map(|e| e + 1).unwrap_or(0);
        let len = p.elements.len();
        for k in 0..len {
            let i = (start + k) % len;
            let e = &p.elements[i];
            if e.kind() == hs2_core::ElemType::Fx {
                continue;
            }
            let (lo, hi, _) = e.key_range();
            let hi = if e.p[hs2_core::patch::ep::KEY_HI] == 0.0 { 127 } else { hi };
            if n >= lo && n <= hi {
                self.edit_elem = Some(i);
                return;
            }
        }
    }

    /// Play PC Keyboard (manual p. 30): Z-row/C = C3 white keys, S-row black,
    /// Q-row from C4, number row black; arrows transpose by octaves.
    fn pc_keyboard(&mut self, ctx: &egui::Context) {
        if !self.pc_keys || ctx.wants_keyboard_input() {
            return;
        }
        const LOW: [(Key, i32); 17] = [
            (Key::Z, -12), (Key::S, -11), (Key::X, -10), (Key::D, -9), (Key::C, 0), (Key::F, 1), (Key::V, 2), (Key::G, 3), (Key::B, 4),
            (Key::N, 5), (Key::J, 6), (Key::M, 7), (Key::K, 8), (Key::Comma, 9), (Key::L, 10), (Key::Period, 11), (Key::Slash, 12),
        ];
        const HIGH: [(Key, i32); 17] = [
            (Key::R, 12), (Key::Num5, 13), (Key::T, 14), (Key::Num6, 15), (Key::Y, 16), (Key::U, 17), (Key::Num8, 18), (Key::I, 19),
            (Key::Num9, 20), (Key::O, 21), (Key::Num0, 22), (Key::P, 23), (Key::OpenBracket, 24), (Key::E, 11), (Key::Num4, 10), (Key::W, 9), (Key::Num3, 8),
        ];
        let (pressed, released, left, right) = ctx.input(|i| {
            let mut p = Vec::new();
            let mut r = Vec::new();
            for (k, off) in LOW.iter().chain(HIGH.iter()) {
                if i.key_pressed(*k) {
                    p.push((*k, *off));
                }
                if i.key_released(*k) {
                    r.push(*k);
                }
            }
            (p, r, i.key_pressed(Key::ArrowLeft) && !i.modifiers.shift, i.key_pressed(Key::ArrowRight) && !i.modifiers.shift)
        });
        if left {
            self.pc_oct = (self.pc_oct - 1).max(-4);
        }
        if right {
            self.pc_oct = (self.pc_oct + 1).min(4);
        }
        for (k, off) in pressed {
            if self.pc_held.contains_key(&k) {
                continue;
            }
            let n = (60 + off + 12 * self.pc_oct).clamp(0, 127) as u8;
            self.pc_held.insert(k, n);
            self.note(None, n, 100, true);
        }
        for k in released {
            if let Some(n) = self.pc_held.remove(&k) {
                self.note(None, n, 0, false);
            }
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if ctx.wants_keyboard_input() {
            return;
        }
        let (up, down, del, shift) = ctx.input(|i| (i.key_pressed(Key::ArrowUp), i.key_pressed(Key::ArrowDown), i.key_pressed(Key::Delete), i.modifiers.shift));
        if shift && (up || down) {
            // browse the library from the highlighted part
            if let Some((b, k)) = self.part_src[self.sel] {
                if let Some((nb, nk)) = self.lib.step(b, k, if up { -1 } else { 1 }) {
                    let part = self.sel;
                    self.load_patch(part, nb, nk);
                }
            }
        } else if !shift && !self.pc_keys && (up || down) {
            self.sel = if up { (self.sel + PARTS - 1) % PARTS } else { (self.sel + 1) % PARTS };
        }
        if del && !self.pc_keys {
            if shift {
                let part = self.sel;
                self.with(|s| s.set_patch(part, None));
            } else {
                self.confirm_clear = false;
                let part = self.sel;
                if self.parts[part].patch.is_some() {
                    self.with(|s| s.set_patch(part, None));
                }
            }
        }
    }

    fn edge_menu(&mut self, ui: &mut egui::Ui, pn: &Panel) {
        let r = pn.r(0.0, 0.0, panel::W, panel::H);
        let resp = ui.interact(r, Id::new("edge"), Sense::click());
        resp.context_menu(|ui| {
            if ui.checkbox(&mut self.pc_keys, "Play PC Keyboard").clicked() {
                ui.close_menu();
            }
            if ui.button("All Notes Off").clicked() {
                self.with(|s| s.panic());
                self.shown_notes = [false; 128];
                ui.close_menu();
            }
            ui.separator();
            if ui.button("Reset/Clear All…").clicked() {
                self.confirm_clear = true;
                ui.close_menu();
            }
            if ui.button("About").clicked() {
                self.about = true;
                ui.close_menu();
            }
        });
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if self.confirm_clear {
            egui::Window::new("Reset/Clear All").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
                ui.label("Clear all parts and reset the instrument?");
                ui.horizontal(|ui| {
                    if ui.button("Reset").clicked() {
                        let sr = self.sr as f32;
                        self.with(|s| {
                            let sf = s.sf.clone();
                            let wt = s.wt.clone();
                            *s = Synth::new(sr);
                            s.sf = sf;
                            s.wt = wt;
                        });
                        self.part_src = [None; PARTS];
                        self.confirm_clear = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.confirm_clear = false;
                    }
                });
            });
        }
        if self.about {
            egui::Window::new("About").collapsible(false).resizable(false).anchor(Align2::CENTER_CENTER, Vec2::ZERO).show(ctx, |ui| {
                ui.heading(crate::version::NAME);
                ui.label("A synthesizer in the style of Hypersonic 2 (Steinberg / Wizoo, 2005).");
                ui.label("Reads Hypersonic 2 patch, phrase and tuning banks from your own content folder.");
                ui.label("Own synthesis engine; Sample elements play your SoundFont / WAV samples.");
                ui.label("Not affiliated with Steinberg or Wizoo.");
                ui.separator();
                ui.label(egui::RichText::new(crate::version::about()).monospace());
                if ui.button("Copy version info").clicked() {
                    ui.output_mut(|o| o.copied_text = crate::version::about());
                }
                if ui.button("Close").clicked() {
                    self.about = false;
                }
            });
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_picked();
        // web: MIDI polled here; audio starts with the first click (browser policy)
        for m in midiin::poll(&self.mq) {
            self.with(|s| s.midi(&m));
        }
        #[cfg(target_arch = "wasm32")]
        if self.audio.is_none() && self.audio_err.is_none() && ctx.input(|i| i.pointer.any_pressed() || !i.keys_down.is_empty()) {
            self.start_audio();
        }
        // program changes from MIDI
        let reqs = self.with(|s| std::mem::take(&mut s.program_requests));
        for (ch, bank, prog) in reqs {
            if !self.pgm_change {
                continue;
            }
            if ch == 15 && self.combi_sel == 2 && !self.chain.is_empty() {
                self.chain_pos = prog as usize % self.chain.len();
                let d = self.chain[self.chain_pos].1.clone();
                self.with(|s| state::load(s, &d));
                continue;
            }
            let b = bank as u32;
            if self.lib.program(b, prog as u32).is_some() {
                self.load_patch(ch, b, prog as usize);
            }
        }
        self.pc_keyboard(ctx);
        self.shortcuts(ctx);
        self.refresh();
        egui::CentralPanel::default().frame(egui::Frame::none().fill(Color32::from_rgb(20, 20, 22))).show(ctx, |ui| {
            let pn = Panel::fit(ui.max_rect());
            self.edge_menu(ui, &pn);
            self.chassis(ui, &pn);
            self.part_area(ui, &pn);
            self.page_buttons(ui, &pn);
            let dr = pn.r(476.0, 26.0, 748.0, 374.0);
            panel::lcd(ui, dr, pn.s);
            pages::show(self, ui, &pn, dr);
            self.hyper_display(ui, &pn);
            self.hyperknobs(ui, &pn);
            self.wheels_and_keys(ui, &pn);
            // status line (own addition, bottom of the hyper section)
            let st = if let Some(e) = &self.audio_err { format!("Audio: {e}") } else { self.status.clone() };
            ui.painter().with_clip_rect(pn.r(20.0, 462.0, 312.0, 475.0)).text(pn.p(28.0, 469.0), Align2::LEFT_CENTER, &st, pn.f(7.0), Color32::from_rgb(150, 156, 164));
            if self.pc_keys {
                panel::text(ui, pn.p(330.0, 480.0), Align2::LEFT_CENTER, &format!("PC keyboard  C{}", 3 + self.pc_oct), pn.f(7.5), col::BLUE);
            }
        });
        self.dialogs(ctx);
        self.map_window(ctx);
        ctx.request_repaint_after(std::time::Duration::from_millis(33));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_settings();
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn native_main() -> eframe::Result<()> {
    let opts = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1010.0, 720.0]).with_title(crate::version::title()).with_min_inner_size([606.0, 432.0]),
        ..Default::default()
    };
    eframe::run_native(crate::version::NAME, opts, Box::new(|cc| Ok(Box::new(App::new(cc)))))
}

#[cfg(target_arch = "wasm32")]
pub async fn web_start(canvas_id: &str) -> Result<(), wasm_bindgen::JsValue> {
    let opts = eframe::WebOptions::default();
    eframe::WebRunner::new().start(canvas_id, opts, Box::new(|cc| Ok(Box::new(App::new(cc))))).await
}

pub fn act_int(app: &mut App, id: Id, act: Act, v: i32, lo: i32, hi: i32, def: i32, px: f32) -> Option<i32> {
    let acc = app.acc(id);
    panel::apply_int(act, v, lo, hi, def, acc, px)
}
