//! The Auto Play and Importer plug-ins (AutoPlay.dll, Importer.dll), laid out
//! like the original windows (captured from XGworks: orig_shots/autoplay.png,
//! importer.png). Their lists save as the original's .ATP / .IMP files.

use eframe::egui;
use xgworks_core::importer::{self, ImportOpts};
use xgworks_core::{edit_model, EditTrack, Event};

use crate::platform::{self, PickedFile, PickedFiles};

/// The plug-in windows' Windows look: light, dialog grey, a margin.
pub fn dialog(ui: &mut egui::Ui, body: impl FnOnce(&mut egui::Ui)) {
    ui.style_mut().visuals = egui::Visuals::light();
    egui::Frame::none().fill(egui::Color32::from_gray(0xF0)).inner_margin(12.0).show(ui, |ui| {
        ui.set_min_size(ui.available_size());
        body(ui);
    });
}

/// The files of an .ATP / .IMP list.
pub fn list_files(text: &[u8]) -> Vec<PickedFile> {
    importer::read_list(&String::from_utf8_lossy(text))
        .into_iter()
        .map(|p| PickedFile { name: p.rsplit(['\\', '/']).next().unwrap_or("").to_string(), path: Some(p), bytes: None })
        .collect()
}

// ---------------------------------------------------------------------------
// Auto Play
// ---------------------------------------------------------------------------

/// Karaoke colours of AutoPlay.ini ([Word] sung text, [Other] the rest, [Bk]).
pub struct Colors {
    pub word: egui::Color32,
    pub other: egui::Color32,
    pub bk: egui::Color32,
}

impl Colors {
    pub fn from_ini(ini: &str) -> Colors {
        let get = |sec: &str, d: [u8; 3]| -> egui::Color32 {
            let mut on = false;
            let mut c = d;
            for l in ini.lines() {
                let l = l.trim();
                if l.starts_with('[') {
                    on = l.eq_ignore_ascii_case(&format!("[{sec}]"));
                } else if on {
                    if let Some((k, v)) = l.split_once('=') {
                        let v = v.trim().parse().unwrap_or(0);
                        match k.trim() {
                            "R" => c[0] = v,
                            "G" => c[1] = v,
                            "B" => c[2] = v,
                            _ => {}
                        }
                    }
                }
            }
            egui::Color32::from_rgb(c[0], c[1], c[2])
        };
        Colors { word: get("Word", [255, 153, 255]), other: get("Other", [255, 255, 255]), bk: get("Bk", [0, 112, 84]) }
    }
}

pub enum AutoAction {
    Play(usize),
    Stop,
    Status(String),
}

pub struct AutoPlay {
    pub songs: Vec<PickedFile>,
    sel: Option<usize>,
    pub random: bool,
    /// The song being played (index into `songs`).
    pub current: Option<usize>,
    drag: Option<usize>,
    picked: PickedFiles,
    list_picked: PickedFiles,
    colors: Colors,
    rng: std::cell::Cell<u64>,
}

impl AutoPlay {
    pub fn new() -> Self {
        let ini = platform::read(&platform::xg4("AutoPlay.ini")).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
        AutoPlay {
            songs: Vec::new(),
            sel: None,
            random: false,
            current: None,
            drag: None,
            picked: Default::default(),
            list_picked: Default::default(),
            colors: Colors::from_ini(&ini),
            rng: std::cell::Cell::new(
                web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map_or(0x9E37_79B9, |d| d.as_nanos() as u64) | 1,
            ),
        }
    }

    /// The song after the current one (in order, or at random); wraps around.
    pub fn next(&self) -> Option<usize> {
        let n = self.songs.len();
        if n == 0 {
            return None;
        }
        let cur = self.current.unwrap_or(n - 1);
        if self.random && n > 1 {
            let mut x = self.rng.get();
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.rng.set(x);
            let r = (x % (n as u64 - 1)) as usize;
            Some(if r >= cur { r + 1 } else { r })
        } else {
            Some((cur + 1) % n)
        }
    }

    /// The window body. `lyrics` = (current page lines, sung chars of each).
    pub fn ui(&mut self, ui: &mut egui::Ui, playing: bool, lyrics: &[(String, usize)]) -> Vec<AutoAction> {
        let mut out = Vec::new();
        if let Some(files) = self.picked.borrow_mut().take() {
            self.songs.extend(files);
        }
        if let Some(files) = self.list_picked.borrow_mut().take() {
            if let Some(text) = files.first().and_then(|f| f.read()) {
                self.songs = list_files(&text);
                self.sel = None;
            }
        }
        // the song list; while an XF song plays, its lyrics (karaoke wipe)
        let list_h = 250.0;
        egui::Frame::canvas(ui.style()).fill(if playing && !lyrics.is_empty() { self.colors.bk } else { egui::Color32::WHITE }).show(ui, |ui| {
            ui.set_min_size(egui::vec2(ui.available_width(), list_h));
            if playing && !lyrics.is_empty() {
                ui.add_space(20.0);
                for (line, sung) in lyrics {
                    let (a, b) = line.split_at(line.char_indices().nth(*sung).map_or(line.len(), |x| x.0));
                    let mut job = egui::text::LayoutJob::default();
                    let f = egui::FontId::proportional(26.0);
                    job.append(a, 0.0, egui::TextFormat::simple(f.clone(), self.colors.word));
                    job.append(b, 0.0, egui::TextFormat::simple(f, self.colors.other));
                    ui.label(job);
                }
                return;
            }
            egui::ScrollArea::vertical().max_height(list_h).show(ui, |ui| {
                let mut drop_at = None;
                for (i, s) in self.songs.iter().enumerate() {
                    let cur = self.current == Some(i);
                    let txt = if cur { egui::RichText::new(s.label()).strong() } else { egui::RichText::new(s.label()) };
                    let r = ui.selectable_label(self.sel == Some(i), txt).interact(egui::Sense::drag());
                    if r.clicked() {
                        self.sel = Some(i);
                    }
                    if r.double_clicked() {
                        out.push(AutoAction::Play(i)); // play from this song
                    }
                    if r.drag_started() {
                        self.drag = Some(i);
                    }
                    if self.drag.is_some() && ui.input(|x| x.pointer.hover_pos()).is_some_and(|p| r.rect.contains(p)) {
                        ui.painter().hline(r.rect.x_range(), r.rect.top(), egui::Stroke::new(2.0_f32, egui::Color32::BLACK));
                        drop_at = Some(i);
                    }
                }
                // drag a song up / down to reorder (AutoPlay.hlp)
                if ui.input(|x| x.pointer.any_released()) {
                    if let (Some(from), Some(to)) = (self.drag.take(), drop_at) {
                        if from != to {
                            let s = self.songs.remove(from);
                            self.songs.insert(to, s);
                            self.sel = Some(to);
                        }
                    }
                    self.drag = None;
                }
            });
        });
        ui.add_space(6.0);
        let btn = |ui: &mut egui::Ui, t: &str| ui.add_sized([90.0, 24.0], egui::Button::new(t)).clicked();
        ui.horizontal(|ui| {
            if playing && self.current.is_some() {
                if btn(ui, "Stop") {
                    out.push(AutoAction::Stop);
                }
            } else if btn(ui, "Play") {
                if let Some(i) = self.sel.or((!self.songs.is_empty()).then_some(0)) {
                    out.push(AutoAction::Play(i));
                }
            }
            if btn(ui, "Add") {
                platform::pick_files(&self.picked, ui.ctx(), "XGworks & SMF & XF File (*.XWS & *.MID)", &["xws", "XWS", "mid", "MID"]);
            }
            if btn(ui, "Delete") {
                if let Some(i) = self.sel.filter(|&i| i < self.songs.len()) {
                    self.songs.remove(i);
                    self.sel = None;
                }
            }
            if btn(ui, "All Clear") {
                self.songs.clear();
                self.sel = None;
            }
            if btn(ui, "Load...") {
                platform::pick_files(&self.list_picked, ui.ctx(), "Auto Play File (*.ATP)", &["atp", "ATP"]);
            }
            if btn(ui, "Save...") {
                let names: Vec<String> = self.songs.iter().map(|s| s.label().to_string()).collect();
                let text = importer::write_list("Juke Box List", &names);
                if let Some(msg) = platform::save_song("songs.atp", "atp", "Auto Play File", text.as_bytes()) {
                    out.push(AutoAction::Status(msg));
                }
            }
        });
        ui.checkbox(&mut self.random, "Random");
        out
    }
}

/// The lyrics page being sung at `tick`: XF lyric events (FF 05), "/" ends
/// a line, "<" a page. Returns the page's lines with the sung char counts.
pub fn lyrics_page(tracks: &[EditTrack], tick: u32) -> Vec<(String, usize)> {
    let mut syl: Vec<(u32, String)> = tracks
        .iter()
        .flat_map(|t| t.others.iter())
        .filter_map(|e| match e {
            Event::Meta { t, mtype: 0x05, data } => Some((*t, data.iter().map(|&b| b as char).collect())),
            _ => None,
        })
        .collect();
    if syl.is_empty() {
        return Vec::new();
    }
    syl.sort_by_key(|s| s.0);
    // split into pages of lines of (tick, text)
    let mut pages: Vec<Vec<Vec<(u32, String)>>> = vec![vec![vec![]]];
    for (t, s) in syl {
        let mut text = s.as_str();
        let new_page = text.starts_with('<');
        let new_line = text.starts_with('/');
        if new_page || new_line {
            text = &text[1..];
        }
        if new_page && pages.last().is_some_and(|p| p.iter().any(|l| !l.is_empty())) {
            pages.push(vec![vec![]]);
        } else if new_line && pages.last().and_then(|p| p.last()).is_some_and(|l| !l.is_empty()) {
            pages.last_mut().unwrap().push(vec![]);
        }
        let (clean, end_line) = match text.strip_suffix('/') {
            Some(c) => (c, true),
            None => (text, false),
        };
        pages.last_mut().unwrap().last_mut().unwrap().push((t, clean.to_string()));
        if end_line {
            pages.last_mut().unwrap().push(vec![]);
        }
    }
    let first = |p: &Vec<Vec<(u32, String)>>| p.iter().flat_map(|l| l.iter()).map(|s| s.0).next().unwrap_or(0);
    let page = pages.iter().rev().find(|p| first(p) <= tick).or(pages.first()).unwrap();
    page.iter()
        .filter(|l| !l.is_empty())
        .map(|l| {
            let text: String = l.iter().map(|s| s.1.as_str()).collect();
            let sung: usize = l.iter().filter(|s| s.0 <= tick).map(|s| s.1.chars().count()).sum();
            (text, sung)
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Importer
// ---------------------------------------------------------------------------

pub enum ImportAction {
    /// Insert these (already placed) tracks into the song.
    Insert(ImportOpts, Vec<EditTrack>, u16),
    /// Play the import target.
    Monitor(Vec<EditTrack>, u16),
    Status(String),
}

pub struct Importer {
    pub files: Vec<PickedFile>,
    sel: Option<usize>,
    /// The selected file, parsed: (tracks, ppqn, markers).
    src: Option<(usize, Vec<EditTrack>, u16, Vec<(u32, String)>)>,
    marker: usize,
    meas: String,
    beat: String,
    clock: String,
    offset: String,
    all: bool,
    master: [bool; 4],
    picked: PickedFiles,
    list_picked: PickedFiles,
}

impl Importer {
    pub fn new() -> Self {
        // the original's Importer.ini: [Importer] Marker=1, 0..3 = master event checks
        let ini = platform::read(&platform::xg4("Importer.ini")).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
        let get = |k: &str| ini.lines().find_map(|l| l.split_once('=').filter(|x| x.0.trim() == k).map(|x| x.1.trim() == "1"));
        Importer {
            files: Vec::new(),
            sel: None,
            src: None,
            marker: 0,
            meas: "001".into(),
            beat: "01".into(),
            clock: "000".into(),
            offset: "000".into(),
            all: !get("Marker").unwrap_or(true),
            master: [get("0").unwrap_or(false), get("1").unwrap_or(false), get("2").unwrap_or(false), get("3").unwrap_or(false)],
            picked: Default::default(),
            list_picked: Default::default(),
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, tm: &xgworks_core::listev::TimeMap) -> Vec<ImportAction> {
        let mut out = Vec::new();
        if let Some(files) = self.picked.borrow_mut().take() {
            self.files.extend(files);
        }
        if let Some(files) = self.list_picked.borrow_mut().take() {
            if let Some(text) = files.first().and_then(|f| f.read()) {
                self.files = list_files(&text);
                self.sel = None;
                self.src = None;
            }
        }
        if self.sel.is_none() && !self.files.is_empty() {
            self.sel = Some(0); // the original selects the first file
        }
        // parse the selected file (for its markers)
        if let Some(i) = self.sel {
            if self.src.as_ref().map_or(true, |s| s.0 != i) {
                self.src = self.files.get(i).and_then(|f| f.read()).map(|b| {
                    let (t, p) = edit_model(&b);
                    let m = importer::markers(&t);
                    (i, t, p, m)
                });
                self.marker = 0;
            }
        }
        let btn = |ui: &mut egui::Ui, t: &str| ui.add_sized([90.0, 24.0], egui::Button::new(t)).clicked();
        ui.horizontal_top(|ui| {
            egui::Frame::group(ui.style()).show(ui, |ui| ui.vertical(|ui| {
                ui.set_min_size(egui::vec2(470.0, 130.0));
                ui.label("File Name");
                egui::ScrollArea::vertical().id_source("imp_files").max_height(120.0).show(ui, |ui| {
                    for (i, f) in self.files.iter().enumerate() {
                        if ui.selectable_label(self.sel == Some(i), f.label()).clicked() {
                            self.sel = Some(i);
                        }
                    }
                });
            }));
            ui.vertical(|ui| {
                if btn(ui, "Add...") {
                    platform::pick_files(&self.picked, ui.ctx(), "XGworks & SMF & XF File (*.XWS & *.MID)", &["xws", "XWS", "mid", "MID"]);
                }
                if btn(ui, "Delete") {
                    if let Some(i) = self.sel.filter(|&i| i < self.files.len()) {
                        self.files.remove(i);
                        self.sel = None;
                        self.src = None;
                    }
                }
                if btn(ui, "Clear All") {
                    self.files.clear();
                    self.sel = None;
                    self.src = None;
                }
                if btn(ui, "Load...") {
                    platform::pick_files(&self.list_picked, ui.ctx(), "Importer File (*.IMP)", &["imp", "IMP"]);
                }
                if btn(ui, "Save...") {
                    let names: Vec<String> = self.files.iter().map(|s| s.label().to_string()).collect();
                    let text = importer::write_list("File List", &names);
                    if let Some(msg) = platform::save_song("files.imp", "imp", "Importer File", text.as_bytes()) {
                        out.push(ImportAction::Status(msg));
                    }
                }
            });
        });
        ui.horizontal_top(|ui| {
            egui::Frame::group(ui.style()).show(ui, |ui| ui.vertical(|ui| {
                ui.set_min_size(egui::vec2(170.0, 150.0));
                ui.label("Marker");
                ui.add_enabled_ui(!self.all, |ui| {
                    egui::ScrollArea::vertical().id_source("imp_markers").max_height(130.0).show(ui, |ui| {
                        if let Some((_, _, _, m)) = &self.src {
                            for (k, (_, name)) in m.iter().enumerate() {
                                if ui.selectable_label(self.marker == k, name).clicked() {
                                    self.marker = k;
                                }
                            }
                        }
                    });
                });
            }));
            ui.vertical(|ui| {
                egui::Grid::new("imp_loc").num_columns(4).show(ui, |ui| {
                    ui.label("Meas");
                    ui.label("Beat");
                    ui.label("Clock");
                    ui.label("Marker Offset");
                    ui.end_row();
                    ui.add(egui::TextEdit::singleline(&mut self.meas).desired_width(48.0));
                    ui.add(egui::TextEdit::singleline(&mut self.beat).desired_width(48.0));
                    ui.add(egui::TextEdit::singleline(&mut self.clock).desired_width(48.0));
                    ui.add(egui::TextEdit::singleline(&mut self.offset).desired_width(70.0));
                    ui.end_row();
                });
                ui.horizontal_top(|ui| {
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.set_min_size(egui::vec2(90.0, 100.0));
                            ui.label("Import Type");
                            ui.radio_value(&mut self.all, true, "All");
                            ui.radio_value(&mut self.all, false, "Marker");
                        });
                    });
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.vertical(|ui| {
                            ui.label("Master Track Event");
                            ui.checkbox(&mut self.master[0], "Marker");
                            ui.checkbox(&mut self.master[1], "Tempo");
                            ui.checkbox(&mut self.master[2], "Time Signature");
                            ui.checkbox(&mut self.master[3], "XF Chord Name");
                        });
                    });
                });
            });
            ui.vertical(|ui| {
                ui.add_space(20.0);
                let monitor = btn(ui, "Monitor");
                let insert = btn(ui, "Insert");
                if monitor || insert {
                    match self.opts(tm) {
                        Ok((o, src, ppqn)) => {
                            if insert {
                                out.push(ImportAction::Insert(o, src, ppqn));
                            } else {
                                // just the import target, from its start
                                let mut tmp = Vec::new();
                                importer::import(&mut tmp, &src, ppqn, ppqn, &ImportOpts { at: 0, ..o });
                                out.push(ImportAction::Monitor(tmp, ppqn));
                            }
                        }
                        Err(e) => out.push(ImportAction::Status(e)),
                    }
                }
            });
        });
        out
    }

    /// Import options from the window, with the source song.
    fn opts(&self, tm: &xgworks_core::listev::TimeMap) -> Result<(ImportOpts, Vec<EditTrack>, u16), String> {
        let (_, src, ppqn, markers) = self.src.as_ref().ok_or("Choose a song in the file list.")?;
        let num = |s: &str, what: &str| s.trim().parse::<u32>().map_err(|_| format!("{what}: a number"));
        let at = tm.from_mbc(num(&self.meas, "Meas")?.max(1), num(&self.beat, "Beat")?.max(1), num(&self.clock, "Clock")?);
        let span = if self.all {
            None
        } else {
            let m = markers.get(self.marker).ok_or("The song has no markers; use Import Type All.")?;
            let end = markers.iter().map(|x| x.0).find(|&t| t > m.0).unwrap_or(u32::MAX);
            Some((m.0, end))
        };
        Ok((ImportOpts { span, offset: num(&self.offset, "Marker Offset")?, at, master: self.master }, src.clone(), *ppqn))
    }
}
