//! The XGworks editor/player app (Track View main window + Piano Roll, List,
//! Mixer and XG Editor windows), shared by the native and the web build.
//!
//! Loads into an editable tick-based model (`EditTrack`/`EditNote`), plays
//! over MIDI and saves back to `.XWS` or `.MID`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::platform::{self, Instant};
use crate::player::{self, open_device, send_dev, send_shared, PlayHandle, SharedOut};
use crate::{clip, icons, jobwin, listwin, lyricwin, markerwin, midiio, staff, staff_svg, mixwin, pianoroll, pluginwin, xfwin, xged};

use eframe::egui;
use xgworks_core::mixer::{self, PartMix};
use xgworks_core::{edit_model, edit_playback_plan, EditTrack, Event};

/// Command-line style options: `<file> [--roll N] [--list N] [--mixer] [--xg]`
/// (open views directly; handy for side-by-side checks against the original).
fn apply_args(app: &mut App, args: &[String]) {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--roll" => {
                if let Some(n) = it.next().and_then(|n| n.parse::<usize>().ok()) {
                    if let Some(&ti) = app.visible_tracks().get(n.saturating_sub(1)) {
                        app.open_roll(ti);
                    }
                }
            }
            "--staff" => {
                if let Some(n) = it.next().and_then(|n| n.parse::<usize>().ok()) {
                    if let Some(&ti) = app.visible_tracks().get(n.saturating_sub(1)) {
                        app.open_staff(ti);
                    }
                }
            }
            "--drum" => {
                if let Some(n) = it.next().and_then(|n| n.parse::<usize>().ok()) {
                    if let Some(&ti) = app.visible_tracks().get(n.saturating_sub(1)) {
                        app.open_drum(ti);
                    }
                }
            }
            "--list" => {
                if let Some(n) = it.next().and_then(|n| n.parse::<usize>().ok()) {
                    if let Some(&ti) = app.visible_tracks().get(n.saturating_sub(1)) {
                        app.open_list(vec![listwin::Span { track: ti, block: None }]);
                    }
                }
            }
            "--mixer" => app.show_mixer = true,
            "--lyrics" => app.open_lyrics(),
            "--chord" => app.open_chord_dialog(0),
            "--master" => app.open_master(),
            "--markers" => app.markers_win = Some(markerwin::MarkerState::new()),
            "--xg" => app.show_xg = true,
            "--autoplay" | "--importer" => {
                let files = it.next().and_then(|f| platform::read(std::path::Path::new(f))).map(|b| pluginwin::list_files(&b)).unwrap_or_default();
                if a == "--autoplay" {
                    app.autoplay.get_or_insert_with(pluginwin::AutoPlay::new).songs = files;
                } else {
                    app.importer.get_or_insert_with(pluginwin::Importer::new).files = files;
                }
            }
            path => {
                if let Some(bytes) = platform::read(std::path::Path::new(path)) {
                    let name = path.rsplit(['/', '\\']).next().unwrap_or("file").to_string();
                    app.load(&bytes, &name);
                }
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn native_main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title(crate::version::title()).with_inner_size([1000.0, 640.0]),
        ..Default::default()
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    eframe::run_native(crate::version::NAME, options, Box::new(move |cc| {
        listwin::install_font(&cc.egui_ctx);
        let mut app = App::new();
        apply_args(&mut app, &args);
        Ok(Box::new(app))
    }))
}

/// Web: run the app in canvas `canvas_id`; the page has filled the virtual
/// file system (`platform::add_file`) and requested Web MIDI before.
#[cfg(target_arch = "wasm32")]
pub async fn web_start(canvas_id: &str, args: Vec<String>) -> Result<(), wasm_bindgen::JsValue> {
    let runner = eframe::WebRunner::new();
    runner
        .start(canvas_id, eframe::WebOptions::default(), Box::new(move |cc| {
            listwin::install_font(&cc.egui_ctx);
            let mut app = App::new();
            apply_args(&mut app, &args);
            Ok(Box::new(app))
        }))
        .await?;
    std::mem::forget(runner); // keeps running for the page's lifetime
    Ok(())
}

struct App {
    out_names: Vec<String>,
    sel_out: usize,
    edit: Vec<EditTrack>,
    flags: Vec<(bool, bool)>, // (muted, solo)
    /// Per-track audibility shared with the player thread (live mute/solo).
    live_aud: Arc<Vec<AtomicBool>>,
    ppqn: u16,
    ms_per_tick: f64,
    sel_track: usize,
    total_ticks: u32,
    pmin: u8,
    pmax: u8,
    playing: bool,
    play_start: Option<Instant>,
    /// Song position (tick) while stopped; playing starts here.
    song_pos: u32,
    /// Tick the running playback started from, and the song's tempo map.
    play_from: u32,
    play_tm: Option<xgworks_core::listev::TimeMap>,
    /// Left / right locators (loop start / end), ticks.
    loc_l: u32,
    loc_r: u32,
    loop_on: bool,
    /// Numpad 2 jumps back this many bars (Preferences, 2..4).
    rewind_bars: u32,
    show_prefs: bool,
    /// The Transport window (the original's Control Panel), shown by default.
    show_transport: bool,
    player: Option<PlayHandle>,
    /// A song chosen in the Open dialog, loaded on the next frame.
    picked: platform::Picked,
    file_name: String,
    /// Where the song was opened from / saved to (Save overwrites it).
    file_path: Option<std::path::PathBuf>,
    /// Hash of the song as last loaded / saved (unsaved-changes check).
    saved_hash: u64,
    /// "Save changes?" is being asked before this command.
    ask_save: Option<fileops::After>,
    /// Exit confirmed: let the window close.
    quitting: bool,
    // ---- recording (record.rs)
    midi_in: Option<midiio::InConn>,
    in_dev: Option<usize>,
    in_buf: record::InBuf,
    thru: bool,
    thru_target: record::ThruTarget,
    metro: record::Metronome,
    rec_armed: bool,
    recording: bool,
    rec_track: usize,
    take_start: Option<Instant>,
    /// Count-in before the song position (ms), playback is shifted by it.
    count_in_ms: f64,
    status: String,
    /// One MIDI connection shared by the player thread and the mixer.
    midi: SharedOut,
    /// Song port -> output device (index 0 unused: port 1 is the toolbar's device).
    port_map: Vec<Option<usize>>,
    show_mixer: bool,
    show_xg: bool,
    open_rolls: Vec<(u64, pianoroll::RollState)>,
    next_roll_id: u64,
    clipboard: std::cell::RefCell<Vec<xgworks_core::EditNote>>,
    /// List windows and their shared event clipboard.
    open_lists: Vec<(u64, listwin::ListState)>,
    next_list_id: u64,
    list_clip: std::cell::RefCell<Vec<(u32, Event)>>,
    /// List windows' event areas (screen) and a drop between them.
    list_areas: std::cell::RefCell<Vec<(u64, egui::Rect)>>,
    list_drop: std::cell::RefCell<Option<(u64, Vec<(u32, Event)>)>>,
    list_names: xgworks_core::listev::ListNames,
    /// XGworks' inst.ini (Drum window rows), cp932 text.
    inst_ini: String,
    /// List font (name, px; name "" = default) and whether it must be (re)applied.
    list_font: std::cell::RefCell<(String, f32)>,
    list_font_changed: std::cell::Cell<bool>,
    /// Installed fonts for the font dialog (read when a list opens first).
    font_files: Vec<(String, String)>,
    /// List windows' "Display XG" settings (saved with the app settings).
    list_xg: std::cell::RefCell<xgworks_core::listev::XgDisp>,
    view_t0: f32,
    px_per_tick: f32,
    row_scroll: f32,
    /// Track View Trns cell being typed into: (track, text).
    trns_edit: Option<(usize, String)>,
    /// XF Information Editor (Job menu), open while Some.
    xf_dlg: Option<xfwin::XfDialog>,
    /// Lyrics / karaoke editor window, open while Some.
    lyrics: Option<lyricwin::LyricsState>,
    /// Markers window (Edit ▸ Add Marker / Marker List), open while Some.
    markers_win: Option<markerwin::MarkerState>,
    /// Chord lane: selected chords (track, index), Chord Insert dialog, drag start tick.
    chord_sel: Vec<(usize, usize)>,
    chord_dlg: Option<chordlane::ChordDlg>,
    chord_drag: Option<u32>,
    /// Plug-in windows (Auto Play, Importer), open while Some.
    autoplay: Option<pluginwin::AutoPlay>,
    importer: Option<pluginwin::Importer>,
    /// Song time (ms) at which the playback ends.
    play_len_ms: f64,
    /// The playback is the Importer's monitor (not the song).
    monitoring: bool,
    /// Job menu dialogs (Transpose, Quantize, ...).
    jobs: jobwin::JobState,
    /// Track View edit tool (toolbar / right-click menu).
    tv_tool: TvTool,
    tv_drag: Option<TvDrag>,
    /// Bond tool: the block clicked first.
    bond_first: Option<(usize, usize)>,
    /// Tools pop-up menu position.
    tv_menu: Option<egui::Pos2>,
    /// Paste chosen in the tools menu (done by the key handler).
    pending_paste: bool,
    /// Track number drag: (from row, to row).
    trk_drag: Option<(usize, usize)>,
    /// Port drop-down shown in a track's Port cell: (track, list open).
    port_combo: Option<(usize, bool)>,
    /// Tracks whose Port cells were range-selected by dragging (cyan).
    port_sel: Vec<usize>,
    /// Port column drag in progress: (first row, current row).
    port_drag: Option<(usize, usize)>,
    /// Track View block selection: (track, block index).
    blk_sel: Vec<(usize, usize)>,
    /// Paste line (white vertical line): (track, tick), set by clicking empty lane.
    paste_line: Option<(usize, u32)>,
    /// Block clipboard of the Track View.
    blk_clip: Vec<ClipBlock>,
    /// Track View undo / redo (whole-song snapshots).
    tv_undo: Vec<Vec<EditTrack>>,
    tv_redo: Vec<Vec<EditTrack>>,
    xg: xged::XgEditor,
    mix: Vec<PartMix>,
    /// The Mixer window (16 channel strips + master, like XGworks').
    mx: mixwin::Mixer,
    /// Per-channel audibility from the mixer's mute/solo, shared with the player.
    chan_aud: Arc<Vec<AtomicBool>>,
    /// Per track: whether it plays on the mixer's port (the mixer's channel
    /// mute / solo only applies to those tracks).
    mix_trk: Arc<Vec<AtomicBool>>,
    /// The mixer's output port (song port number; Mixer.hlp "Port").
    mix_port: u8,
    /// Saved port setup: device name per port (index 0 = port 1); None = no
    /// settings yet (then port n uses the n-th device, as XGworks does).
    port_cfg: Option<Vec<String>>,
    /// The settings text last written (saved again when it changes).
    saved_settings: String,
    /// Record Mixer Parameter: channel -> track receiving the recorded moves.
    rec_tracks: [Option<usize>; 16],
    last_frame: Instant,
    dev: u8,
    /// Web: when the MIDI output list was last checked (access is granted,
    /// and devices come and go, while the app runs).
    out_poll: f64,
}

impl App {
    fn new() -> Self {
        let mut app = App {
            out_names: Vec::new(),
            sel_out: 0,
            edit: Vec::new(),
            flags: Vec::new(),
            live_aud: Arc::new(Vec::new()),
            ppqn: 480,
            ms_per_tick: 500_000.0 / 480.0 / 1000.0,
            sel_track: 0,
            total_ticks: 1,
            pmin: 48,
            pmax: 72,
            playing: false,
            autoplay: None,
            importer: None,
            play_len_ms: 0.0,
            monitoring: false,
            play_start: None,
            song_pos: 0,
            play_from: 0,
            play_tm: None,
            loc_l: 0,
            loc_r: 0,
            loop_on: false,
            rewind_bars: 2,
            show_prefs: false,
            show_transport: true,
            player: None,
            picked: Default::default(),
            file_name: String::new(),
            file_path: None,
            saved_hash: 0,
            ask_save: None,
            quitting: false,
            midi_in: None,
            in_dev: None,
            in_buf: Default::default(),
            thru: true,
            thru_target: Default::default(),
            metro: Default::default(),
            rec_armed: false,
            recording: false,
            rec_track: 0,
            take_start: None,
            count_in_ms: 0.0,
            status: "Open an .XWS or .MID file.".into(),
            midi: Default::default(),
            port_map: vec![None; 16],
            show_mixer: false,
            show_xg: false,
            open_rolls: Vec::new(),
            next_roll_id: 0,
            clipboard: std::cell::RefCell::new(Vec::new()),
            open_lists: Vec::new(),
            next_list_id: 0,
            list_clip: std::cell::RefCell::new(Vec::new()),
            list_areas: Default::default(),
            list_drop: Default::default(),
            list_names: load_list_names(),
            inst_ini: platform::read(&platform::xg4("inst.ini")).map(|b| b.iter().map(|&c| if c < 0x80 { c as char } else { '?' }).collect()).unwrap_or_default(),
            list_xg: Default::default(),
            list_font: std::cell::RefCell::new((String::new(), 0.0)),
            list_font_changed: std::cell::Cell::new(false),
            font_files: Vec::new(),
            view_t0: 0.0,
            px_per_tick: 0.0,
            row_scroll: 0.0,
            trns_edit: None,
            port_combo: None,
            jobs: jobwin::JobState::new(&platform::read(&platform::xg4("JobDll.ini")).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default()),
            xf_dlg: None,
            lyrics: None,
            markers_win: None,
            chord_sel: Vec::new(),
            chord_dlg: None,
            chord_drag: None,
            tv_tool: TvTool::Arrow,
            tv_drag: None,
            bond_first: None,
            tv_menu: None,
            pending_paste: false,
            trk_drag: None,
            port_sel: Vec::new(),
            port_drag: None,
            blk_sel: Vec::new(),
            paste_line: None,
            blk_clip: Vec::new(),
            tv_undo: Vec::new(),
            tv_redo: Vec::new(),
            xg: xged::XgEditor::new(),
            mix: Vec::new(),
            mx: mixwin::Mixer::new(platform::xg4_exe()),
            chan_aud: Arc::new((0..16).map(|_| AtomicBool::new(true)).collect()),
            mix_trk: Arc::new(Vec::new()),
            mix_port: 0,
            port_cfg: None,
            saved_settings: String::new(),
            rec_tracks: [None; 16],
            last_frame: Instant::now(),
            dev: 0,
            out_poll: 0.0,
        };
        if let Some(text) = platform::load_settings() {
            app.saved_settings = text.clone();
            app.parse_settings(&text);
        }
        app.refresh_outputs();
        if !cfg!(target_arch = "wasm32") && !platform::xg4_found() {
            app.show_prefs = true;
            app.status = platform::data_help("XGworks data not found.").lines().next().unwrap_or("").to_string();
        }
        app
    }

    fn refresh_outputs(&mut self) {
        self.out_names = midiio::output_names();
        if self.sel_out >= self.out_names.len() {
            self.sel_out = 0;
        }
        self.resolve_ports();
    }

    /// Settings, in XGworks' own terms (its registry key `System`: "Midi Out
    /// Port Num", "Midi Out Dev%d"; `User Settings`: "MIX OUTPORT"), but with
    /// device names instead of indices so they survive devices coming and going.
    fn parse_settings(&mut self, text: &str) {
        let mut devs: Vec<String> = Vec::new();
        let mut num = 0usize;
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let (k, v) = (k.trim(), v.trim());
            if k == "Midi Out Port Num" {
                num = v.parse().unwrap_or(0);
            } else if let Some(n) = k.strip_prefix("Midi Out Dev").and_then(|n| n.parse::<usize>().ok()) {
                if n >= 1 && n <= 16 {
                    if devs.len() < n {
                        devs.resize(n, String::new());
                    }
                    devs[n - 1] = v.to_string();
                }
            } else if k == "Rewind Bars" {
                self.rewind_bars = v.parse::<u32>().unwrap_or(2).clamp(2, 4);
            } else if k == "Font" {
                if v != self.list_font.borrow().0 {
                    self.list_font.borrow_mut().0 = v.to_string();
                    self.list_font_changed.set(true);
                }
            } else if k == "Font Size" {
                let px = v.parse::<f32>().unwrap_or(0.0);
                if px != self.list_font.borrow().1 {
                    self.list_font.borrow_mut().1 = px;
                    self.list_font_changed.set(true);
                }
            } else if k == "XG Disp" {
                let b: Vec<bool> = v.chars().map(|c| c == '1').collect();
                if b.len() == 4 {
                    *self.list_xg.borrow_mut() = xgworks_core::listev::XgDisp { sysex: b[0], note: b[1], cc: b[2], prog: b[3] };
                }
            } else if k == "MIX OUTPORT" {
                self.mix_port = v.parse::<u8>().unwrap_or(0).min(15);
            }
        }
        devs.resize(num.max(devs.len()), String::new());
        self.port_cfg = Some(devs);
    }

    /// Map the saved device names onto the devices present now.
    fn resolve_ports(&mut self) {
        let find = |name: &str| self.out_names.iter().position(|n| !name.is_empty() && n == name);
        match &self.port_cfg {
            Some(cfg) => {
                if let Some(d) = cfg.first().and_then(|n| find(n)) {
                    self.sel_out = d;
                }
                for p in 1..self.port_map.len() {
                    self.port_map[p] = cfg.get(p).and_then(|n| find(n));
                }
            }
            None => {
                // XGworks' default: port n plays on the n-th output device
                for p in 1..self.port_map.len() {
                    self.port_map[p] = (p < self.out_names.len()).then_some(p);
                }
            }
        }
    }

    /// The settings text for the current state; ports whose device is absent
    /// now keep their saved name.
    fn settings_text(&self) -> String {
        let name = |p: usize, dev: Option<usize>| -> String {
            match dev.and_then(|d| self.out_names.get(d)) {
                Some(n) => n.clone(),
                None => self.port_cfg.as_ref().and_then(|c| c.get(p)).cloned().unwrap_or_default(),
            }
        };
        let mut devs = vec![name(0, Some(self.sel_out))];
        for p in 1..self.port_map.len() {
            devs.push(name(p, self.port_map[p]));
        }
        while devs.len() > 1 && devs.last().is_some_and(|d| d.is_empty()) {
            devs.pop();
        }
        let mut t = format!("[System]\r\nMidi Out Port Num={}\r\n", devs.len());
        for (k, d) in devs.iter().enumerate() {
            t += &format!("Midi Out Dev{}={d}\r\n", k + 1);
        }
        t += &format!("[User Settings]\r\nMIX OUTPORT={}\r\nRewind Bars={}\r\n", self.mix_port, self.rewind_bars);
        let x = *self.list_xg.borrow();
        let bit = |b: bool| if b { '1' } else { '0' };
        t += &format!("[List]\r\nXG Disp={}{}{}{}\r\n", bit(x.sysex), bit(x.note), bit(x.cc), bit(x.prog));
        let f = self.list_font.borrow();
        if !f.0.is_empty() {
            t += &format!("Font={}\r\nFont Size={}\r\n", f.0, f.1);
        }
        t
    }

    /// Write the settings when they changed (not before any device is known,
    /// which on the web is the case until MIDI access is granted).
    fn save_settings_if_changed(&mut self) {
        if self.out_names.is_empty() {
            return;
        }
        let t = self.settings_text();
        if t != self.saved_settings {
            platform::save_settings(&t);
            self.parse_settings(&t);
            self.saved_settings = t;
        }
    }

    /// Load a song file; `name` may be its full path (natively).
    fn load(&mut self, bytes: &[u8], name: &str) {
        let (model, ppqn) = edit_model(bytes);
        let path = std::path::Path::new(name);
        let short = path.file_name().and_then(|s| s.to_str()).unwrap_or(name).to_string();
        self.load_model(model, ppqn, &short);
        self.file_path = if path.is_absolute() { Some(path.to_path_buf()) } else { None };
        self.status = format!(
            "Loaded {short} — {} tracks, {} notes.",
            self.edit.iter().filter(|t| !t.conductor).count(),
            self.edit.iter().map(|t| t.notes.len()).sum::<usize>()
        );
    }

    /// Replace the song (open, new, close).
    fn load_model(&mut self, model: Vec<EditTrack>, ppqn: u16, name: &str) {
        self.stop();
        self.song_pos = 0;
        self.flags = model.iter().map(|t| (t.mute(), t.solo())).collect();
        self.trns_edit = None;
        self.blk_sel.clear();
        self.paste_line = None;
        self.tv_undo.clear();
        self.tv_redo.clear();
        self.live_aud = Arc::new((0..model.len()).map(|_| AtomicBool::new(true)).collect());
        self.mix_trk = Arc::new((0..model.len()).map(|_| AtomicBool::new(true)).collect());
        self.edit = model;
        self.mix = self.edit.iter().map(mixer::read_part_mix).collect();
        self.mx.load_song(&self.mixer_tracks());
        self.rec_tracks = [None; 16];
        self.xg.sync_from_song(&self.edit);
        self.open_rolls.clear();
        self.open_lists.clear();
        self.view_t0 = 0.0;
        self.px_per_tick = 0.0; // fit on first draw
        self.ppqn = ppqn.max(1);
        self.sel_track = self.edit.iter().position(|t| !t.notes.is_empty()).unwrap_or(0);
        self.rebuild_view();
        self.file_name = name.to_string();
        self.lyrics = None;
        self.mark_saved();
    }

    fn rebuild_view(&mut self) {
        let mut end = 0u32;
        let (mut pmin, mut pmax) = (127u8, 0u8);
        for t in &self.edit {
            for n in &t.notes {
                end = end.max(n.tick + n.dur);
                pmin = pmin.min(n.pitch);
                pmax = pmax.max(n.pitch);
            }
            for e in &t.others {
                end = end.max(e.t());
            }
        }
        if pmin > pmax {
            pmin = 48;
            pmax = 72;
        }
        self.pmin = pmin.saturating_sub(3);
        self.pmax = (pmax + 3).min(127);
        self.total_ticks = end.max(self.ppqn as u32 * 4);
        // tempo (first FF51 anywhere) for the playhead / duration
        let mut us = 500_000.0f64;
        'outer: for t in &self.edit {
            for e in &t.others {
                if let Event::Meta { mtype: 0x51, data, .. } = e {
                    if data.len() == 3 {
                        us = (((data[0] as u32) << 16) | ((data[1] as u32) << 8) | data[2] as u32) as f64;
                        break 'outer;
                    }
                }
            }
        }
        self.ms_per_tick = us / self.ppqn as f64 / 1000.0;
    }

    fn audible(&self) -> Vec<bool> {
        let any_solo = self.flags.iter().any(|f| f.1);
        self.flags.iter().map(|f| if any_solo { f.1 } else { !f.0 }).collect()
    }

    /// Publish the current mute/solo state to the player thread.
    fn sync_audible(&self) {
        for (a, v) in self.live_aud.iter().zip(self.audible()) {
            a.store(v, Ordering::Relaxed);
        }
        for (a, v) in self.chan_aud.iter().zip(self.mx.chan_audible()) {
            a.store(v, Ordering::Relaxed);
        }
        let md = self.mix_dev();
        for (a, t) in self.mix_trk.iter().zip(&self.edit) {
            a.store(self.port_device(t.port()) == md, Ordering::Relaxed);
        }
    }

    fn play(&mut self) {
        self.play_tracks(None);
    }

    /// Play the song, or other tracks (the Importer's monitor) when given.
    fn play_tracks(&mut self, other: Option<(Vec<EditTrack>, u16)>) {
        if (self.edit.is_empty() && other.is_none()) || self.out_names.is_empty() {
            self.status = "No file loaded or no MIDI output.".into();
            return;
        }
        self.finish_take(); // a loop restart ends the take so far
        self.halt();
        self.sync_audible();
        self.monitoring = other.is_some();
        let (tracks, ppqn) = match other {
            Some((t, p)) => (t, p),
            None => (self.edit.clone(), self.ppqn),
        };
        let from = if self.monitoring { 0 } else { self.song_pos };
        let tm = xgworks_core::listev::TimeMap::new(&tracks, ppqn);
        let msgs = transport::from_position(edit_playback_plan(&tracks, ppqn), tm.to_ms(from));
        let msgs = self.add_clicks(msgs, &tm, from);
        self.play_from = from;
        self.play_tm = Some(tm);
        self.play_len_ms = if self.recording { f64::INFINITY } else { msgs.last().map_or(0.0, |m| m.0) };

        if !self.ensure_midi() {
            self.status = "Could not open the MIDI output.".into();
            return;
        }
        // each track plays on the device of its Port
        let mut devs: Vec<usize> = tracks.iter().map(|t| self.port_device(t.port())).collect();
        let mut used: Vec<usize> = devs.clone();
        used.sort();
        used.dedup();
        let mut failed = Vec::new();
        for &d in &used {
            if !open_device(&self.midi, d) {
                failed.push(self.out_names.get(d).cloned().unwrap_or_default());
                for x in devs.iter_mut().filter(|x| **x == d) {
                    *x = self.sel_out;
                }
            }
        }
        let (aud, mixed) = if self.monitoring {
            let n = tracks.len();
            (Arc::new((0..n).map(|_| AtomicBool::new(true)).collect()), Arc::new((0..n).map(|_| AtomicBool::new(false)).collect()))
        } else {
            (self.live_aud.clone(), self.mix_trk.clone())
        };
        self.player = Some(player::start(self.midi.clone(), msgs, devs.clone(), aud, self.chan_aud.clone(), mixed));
        self.play_start = Some(Instant::now());
        self.playing = true;
        let mut names: Vec<&str> = devs.iter().map(|&d| self.out_names[d].as_str()).collect();
        names.sort();
        names.dedup();
        self.status = format!("Playing on {}…", names.join(", "));
        if !failed.is_empty() {
            self.status += &format!(" (could not open {}, using port 1)", failed.join(", "));
        }
    }

    /// Stop; the song position stays where playback was.
    fn stop(&mut self) {
        self.finish_take();
        if self.playing && !self.monitoring {
            self.song_pos = self.cur_tick().min(self.total_ticks);
        }
        self.halt();
    }

    /// Stop the player without moving the song position.
    fn halt(&mut self) {
        if let Some(p) = self.player.take() {
            p.stop();
        }
        self.playing = false;
        self.play_start = None;
    }

    /// Open the selected (default) output device and make it the default.
    fn ensure_midi(&mut self) -> bool {
        let ok = open_device(&self.midi, self.sel_out);
        if ok {
            self.midi.lock().unwrap().default = self.sel_out;
        }
        ok
    }

    /// Output device of a song port. Port 1 (index 0) is the device chosen in
    /// the toolbar; other ports use the device assigned to them (Port column),
    /// unassigned ones fall back to port 1, so a multi-port song still plays.
    fn port_device(&self, port: u8) -> usize {
        if port == 0 {
            return self.sel_out;
        }
        self.port_map.get(port as usize).copied().flatten().filter(|&d| d < self.out_names.len()).unwrap_or(self.sel_out)
    }

    /// Port index for a device picked in the Port column: the default device
    /// is port 1, others reuse their port or take the first free one.
    fn port_for_device(&mut self, dev: usize) -> u8 {
        if dev == self.sel_out {
            return 0;
        }
        if let Some(p) = (1..self.port_map.len()).find(|&p| self.port_map[p] == Some(dev)) {
            return p as u8;
        }
        let p = (1..self.port_map.len()).find(|&p| self.port_map[p].is_none()).unwrap_or(self.port_map.len() - 1);
        self.port_map[p] = Some(dev);
        p as u8
    }

    fn send(&mut self, bytes: &[u8]) {
        if self.ensure_midi() {
            send_shared(&self.midi, bytes);
        }
    }

}

#[path = "transport.rs"]
mod transport;
#[path = "fileops.rs"]
mod fileops;
#[path = "record.rs"]
mod record;
#[path = "transport_win.rs"]
mod transport_win;
#[path = "chordlane.rs"]
mod chordlane;

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let picked = self.picked.borrow_mut().take();
        if let Some((name, bytes)) = picked {
            self.load(&bytes, &name);
        }
        track_key_focus(ctx);
        self.file_keys(ctx);
        self.save_question(ctx);
        self.transport_keys(ctx);
        self.loop_check();
        self.record_tick();
        self.prefs_window(ctx);
        self.transport_window(ctx);
        if cfg!(target_arch = "wasm32") && !self.playing {
            let now = ctx.input(|i| i.time);
            if now - self.out_poll > 1.0 {
                self.out_poll = now;
                let names = midiio::output_names();
                if names != self.out_names {
                    self.midi.lock().unwrap().conns.clear(); // indices changed
                    self.out_names = names;
                    if self.sel_out >= self.out_names.len() {
                        self.sel_out = 0;
                    }
                    self.resolve_ports();
                }
            }
            ctx.request_repaint_after(std::time::Duration::from_secs(1));
        }
        if self.playing {
            if let Some(st) = self.play_start {
                if st.elapsed().as_secs_f64() * 1000.0 > self.play_len_ms + 400.0 {
                    self.halt();
                    self.song_pos = 0;
                    self.status = "Finished.".into();
                    self.autoplay_next();
                }
            }
            ctx.request_repaint();
        }

        egui::TopBottomPanel::top("bar").show(ctx, |ui| {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                self.file_menu(ui);
                if ui.button("Open").on_hover_text("Open (Ctrl+O)").clicked() {
                    let c = ui.ctx().clone();
                    self.file_command(fileops::After::Open, &c);
                }
                let has = !self.edit.is_empty();
                if ui.add_enabled(has, egui::Button::new("Save")).on_hover_text("Save (Ctrl+S)").clicked() {
                    self.save_current();
                }
                ui.separator();
                // Track View edit tools, with the original's toolbar icons
                ui.spacing_mut().item_spacing.x = 0.0;
                for (k, (t, name, tip)) in TV_TOOLS.into_iter().enumerate() {
                    if icons::tool_button(ui, icons::MAIN_BAR, 5 + k, self.tv_tool == t, name, tip).clicked() {
                        self.tv_tool = t;
                        self.bond_first = None;
                    }
                }
                ui.spacing_mut().item_spacing.x = 8.0;
                ui.separator();
                egui::ComboBox::from_id_source("out")
                    .selected_text(self.out_names.get(self.sel_out).cloned().unwrap_or_else(|| "— no output —".into()))
                    .show_ui(ui, |ui| {
                        for (i, n) in self.out_names.iter().enumerate() {
                            ui.selectable_value(&mut self.sel_out, i, n);
                        }
                    });
                if ui.button("⟳").on_hover_text("Refresh outputs").clicked() {
                    self.refresh_outputs();
                }
                ui.separator();
                if ui.add(egui::Button::new(egui::RichText::new("▶ Transport").strong())).on_hover_text("Show / hide the Transport window (Alt+F7)").clicked() {
                    self.show_transport = !self.show_transport;
                }
                ui.end_row();
                ui.separator();
                ui.add_enabled_ui(has, |ui| {
                    ui.menu_button("Job", |ui| {
                        for (j, label) in jobwin::JOB_MENU {
                            if ui.button(label).clicked() {
                                self.jobs.open = Some(j);
                                ui.close_menu();
                            }
                        }
                        ui.separator();
                        if ui.button("XF Information Editor...").clicked() {
                            self.xf_dlg = Some(xfwin::XfDialog::new(xgworks_core::xfinfo::read(&self.edit)));
                            ui.close_menu();
                        }
                    });
                });
                ui.menu_button("Plug-in", |ui| {
                    if ui.button("Auto Play").clicked() {
                        self.autoplay.get_or_insert_with(pluginwin::AutoPlay::new);
                        ui.close_menu();
                    }
                    if ui.button("Importer").clicked() {
                        self.importer.get_or_insert_with(pluginwin::Importer::new);
                        ui.close_menu();
                    }
                });
                ui.menu_button("Edit", |ui| {
                    if ui.add_enabled(!self.tv_undo.is_empty(), egui::Button::new("Undo                   Ctrl+Z")).clicked() {
                        self.tv_undo_redo(true);
                        ui.close_menu();
                    }
                    if ui.add_enabled(!self.tv_redo.is_empty(), egui::Button::new("Redo                   Ctrl+Y")).clicked() {
                        self.tv_undo_redo(false);
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.add_enabled(has, egui::Button::new("Insert Chord…")).on_hover_text("XF chord at the song position (or click the C lane with the pencil)").clicked() {
                        let t = self.cur_tick();
                        self.open_chord_dialog(t);
                        ui.close_menu();
                    }
                    if ui.add_enabled(has, egui::Button::new("Add Marker / Marker List…")).clicked() {
                        self.markers_win.get_or_insert_with(markerwin::MarkerState::new);
                        ui.close_menu();
                    }
                });
                self.setup_menu(ui);
                ui.menu_button("Window", |ui| {
                    let t = self.sel_track;
                    let has_t = t < self.edit.len();
                    if ui.add_enabled(has_t, egui::Button::new("Event List (List)      F2")).clicked() {
                        self.list_from_selection();
                        ui.close_menu();
                    }
                    if ui.add_enabled(has_t, egui::Button::new("Piano Roll             F3")).clicked() {
                        self.open_roll(t);
                        ui.close_menu();
                    }
                    if ui.add_enabled(has_t, egui::Button::new("Staff (notation)       F4")).clicked() {
                        self.open_staff(t);
                        ui.close_menu();
                    }
                    if ui.add_enabled(has_t, egui::Button::new("Drum                   F5")).clicked() {
                        self.open_drum(t);
                        ui.close_menu();
                    }
                    if ui.add_enabled(has, egui::Button::new("Mixer                  F6")).clicked() {
                        self.show_mixer = true;
                        ui.close_menu();
                    }
                    if ui.add_enabled(has, egui::Button::new("XG Editor              F7")).clicked() {
                        self.show_xg = true;
                        self.xg.sync_from_song(&self.edit);
                        ui.close_menu();
                    }
                    if ui.button("Transport              Alt+F7").clicked() {
                        self.show_transport = !self.show_transport;
                        ui.close_menu();
                    }
                    if ui.add_enabled(has, egui::Button::new("Master Track           F8")).clicked() {
                        self.open_master();
                        ui.close_menu();
                    }
                    if ui.add_enabled(has, egui::Button::new("Lyrics / Karaoke       F9")).clicked() {
                        self.open_lyrics();
                        ui.close_menu();
                    }
                    ui.separator();
                    ui.label(egui::RichText::new("For the track selected in the Track View").small());
                });
                if ui.button("⚙").on_hover_text("Preferences").clicked() {
                    self.show_prefs = true;
                }
                ui.menu_button("Help", |ui| {
                    ui.label(egui::RichText::new(crate::version::about()).monospace());
                    ui.separator();
                    if ui.button("Copy version info").clicked() {
                        ui.output_mut(|o| o.copied_text = crate::version::about());
                        ui.close_menu();
                    }
                });
                ui.end_row();
                self.editors_bar(ui);
            });
            ui.add_space(4.0);
        });

        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.label(&self.status);
                if !self.edit.is_empty() {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.weak("double-click a track = Piano Roll · Ctrl+wheel = zoom · wheel/Shift = scroll");
                    });
                }
            });
            ui.add_space(2.0);
        });

        self.sync_audible(); // mute/solo from the track view or mixer, applied live

        let tv_keys = keys_active(ctx, egui::LayerId::background());
        egui::CentralPanel::default().show(ctx, |ui| with_keys(ctx, tv_keys, || self.track_view(ui)));

        // ---- separate OS windows ----
        let dt = self.last_frame.elapsed().as_secs_f32();
        self.last_frame = Instant::now();
        let play_tick = self.play_tick();
        self.mx.follow(play_tick, dt);
        if self.show_mixer && !self.edit.is_empty() {
            let mut acts = Vec::new();
            let ports = self.out_names.clone();
            let port = self.mix_dev();
            self.mx.port_no = self.mix_port;
            let playing = self.playing;
            let (mx, xg) = (&mut self.mx, &self.xg);
            let voice = |m: u8, l: u8, p: u8| xg.voice_name(m, l, p).map(str::to_string);
            self.show_mixer = fixed_window(ctx, "mixer", "Mixer", mixwin::CLIENT, |ui| {
                acts = mx.ui(ui, &ports, port, xg.voices(), &voice, playing);
            });
            self.mixer_actions(acts);
            if self.playing {
                ctx.request_repaint();
            }
        }
        if self.show_xg && !self.edit.is_empty() {
            let names: Vec<(u8, String)> = self.edit.iter().map(|t| (t.channel & 0x0F, t.name.clone())).collect();
            let mut changes = Vec::new();
            self.show_xg = sub_window(ctx, "xged", "XG Editor", [600.0, 640.0], |ui| changes = self.xg.ui(ui, &names));
            if !changes.is_empty() {
                for (addr, data) in &changes {
                    self.send(&mixer::xg_param(self.dev, *addr, data));
                }
                let dev = self.dev;
                if let Some(t0) = self.edit.first_mut() {
                    xgworks_core::xgtbl::apply_xg_changes(t0, dev, &changes);
                }
            }
        }
        // piano rolls (each its own window; the track can be switched inside)
        if !self.open_rolls.is_empty() {
            let markers = self.markers();
            let names: Vec<String> = self.edit.iter().map(|t| t.name.clone()).collect();
            let play_tick = self.play_tick();
            let visible = self.visible_tracks();
            // monitoring: each track's channel on its port's device
            let outs: Vec<(usize, u8)> = self.edit.iter().map(|t| (self.port_device(t.port()), t.channel & 0x0F)).collect();
            let mut rolls = std::mem::take(&mut self.open_rolls);
            for (_, st) in rolls.iter_mut() {
                if st.drum.is_some() {
                    st.drum = Some(self.drum_map(st.track));
                }
                if st.staff.is_some() {
                    st.staff = Some(self.staff_info(st.track));
                }
            }
            let mut edited = false;
            rolls.retain_mut(|(id, st)| {
                let title = if st.drum.is_some() {
                    format!("Drum:{}", *id + 1)
                } else if st.staff.is_some() {
                    format!("Staff:{}", *id + 1)
                } else {
                    format!("Piano Roll:{}", *id + 1)
                };
                let (dev, ch) = outs.get(st.track).copied().unwrap_or((self.sel_out, 0));
                let midi = self.midi.clone();
                let monitor = move |note: u8, vel: u8| {
                    if open_device(&midi, dev) {
                        send_dev(&midi, dev, &[0x90 | ch, note & 0x7F, vel.clamp(1, 127)]);
                        send_dev(&midi, dev, &[0x90 | ch, note & 0x7F, 0]);
                    }
                };
                let cx = pianoroll::RollCtx {
                    visible: &visible,
                    ppqn: self.ppqn as u32,
                    total_ticks: self.total_ticks,
                    markers: &markers,
                    track_names: &names,
                    play_tick,
                    clipboard: &self.clipboard,
                    monitor: &monitor,
                };
                let edit = &mut self.edit;
                sub_window(ctx, ("roll", *id), &title, [1000.0, 620.0], |ui| edited |= st.ui(ui, edit, &cx))
            });
            for (_, st) in rolls.iter_mut().filter(|(_, st)| st.export_req) {
                st.export_req = false;
                if let (Some(info), Some(t)) = (st.staff, self.edit.get(st.track)) {
                    let title = format!("{} — {}", std::path::Path::new(&self.file_name).file_stem().and_then(|x| x.to_str()).unwrap_or("Song"), t.name);
                    let ttf = platform::read(&platform::xg4("XGmusic.ttf"));
                    let notes: Vec<xgworks_core::EditNote> = t.notes.iter().map(|n| xgworks_core::EditNote { pitch: (n.pitch as i32 + info.written as i32).clamp(0, 127) as u8, ..n.clone() }).collect();
                    let mut plain = info;
                    plain.written = 0;
                    let svg = staff_svg::export(&title, &notes, &plain, self.ppqn as u32, ttf.as_deref());
                    let name = format!("{}.svg", title.replace(['/', '\\', ':'], "-"));
                    if let Some((msg, path)) = platform::save_song_to(&name, "svg", "SVG (print / PDF)", svg.as_bytes()) {
                        self.status = msg;
                        if let Some(p) = path {
                            platform::open_with_default_app(&p);
                        }
                    }
                }
            }
            self.open_rolls = rolls;
            if edited {
                self.rebuild_view();
            }
        }
        // list windows
        if !self.open_lists.is_empty() && self.font_files.is_empty() {
            self.font_files = platform::font_files();
        }
        if self.list_font_changed.replace(false) {
            // the chosen font (by family name), else MS Gothic / built-in
            let (name, px) = self.list_font.borrow().clone();
            let bytes = if name == listwin::BUILTIN_FONT {
                Some(Vec::new()).filter(|_| false)
            } else {
                if self.font_files.is_empty() && !name.is_empty() {
                    self.font_files = platform::font_files();
                }
                self.font_files.iter().find(|f| f.0 == name).and_then(|f| platform::read(std::path::Path::new(&f.1)))
            };
            if name == listwin::BUILTIN_FONT {
                listwin::set_list_font_builtin(ctx, px);
            } else {
                listwin::set_list_font(ctx, bytes, px);
            }
        }
        if !self.open_lists.is_empty() {
            // drags read last frame's areas of all windows; this frame's are appended
            let n0 = self.list_areas.borrow().len();
            let cx = listwin::ListCtx { font: &self.list_font, font_changed: &self.list_font_changed, fonts: &self.font_files, areas: &self.list_areas, dropped: &self.list_drop, names: &self.list_names, xg: &self.list_xg, tbl: self.xg.table(), ppqn: self.ppqn as u32, clipboard: &self.list_clip };
            let mut lists = std::mem::take(&mut self.open_lists);
            let mut edited = false;
            let mut out = Vec::new();
            lists.retain_mut(|(id, st)| {
                let title = if st.master { "Master Track".to_string() } else { format!("List:{} - {}", *id + 1, st.title(&self.edit)) };
                let edit = &mut self.edit;
                sub_window(ctx, ("list", *id), &title, [1100.0, 640.0], |ui| edited |= st.ui(ui, edit, &cx, &mut out))
            });
            self.open_lists = lists;
            {
                let mut a = self.list_areas.borrow_mut();
                let k = n0.min(a.len());
                let now = a.split_off(k);
                *a = now;
            }
            for o in out {
                let listwin::ListOut::Send(ti, bytes) = o;
                let dev = self.edit.get(ti).map_or(self.sel_out, |t| self.port_device(t.port()));
                if open_device(&self.midi, dev) {
                    send_dev(&self.midi, dev, &bytes);
                }
            }
            if edited {
                // an event moved past its block's end makes the block longer (List.hlp)
                let mut tr: Vec<usize> = self.open_lists.iter().flat_map(|(_, st)| st.spans.iter().map(|s| s.track)).collect();
                tr.sort();
                tr.dedup();
                for ti in tr {
                    self.extend_blocks(ti);
                }
                self.song_edited();
            }
        }
        self.plugin_windows(ctx);
        self.lyrics_window(ctx);
        self.markers_window(ctx);
        self.chord_dialog(ctx);
        // XF Information Editor
        if let Some(d) = &mut self.xf_dlg {
            match xfwin::ui(ctx, d) {
                Some(true) => {
                    let info = d.info.clone();
                    self.xf_dlg = None;
                    self.tv_snapshot();
                    xgworks_core::xfinfo::write(&mut self.edit, &info);
                    self.song_edited();
                    self.status = "XF information written.".into();
                }
                Some(false) => self.xf_dlg = None,
                None => {}
            }
        }
        // Job menu dialog
        if self.jobs.open.is_some() && !self.edit.is_empty() {
            let tm = xgworks_core::listev::TimeMap::new(&self.edit, self.ppqn);
            let vis = self.visible_tracks();
            let selected: Vec<(usize, u32, u32)> = self.blk_sel.iter()
                .filter_map(|&(ti, bi)| self.edit.get(ti)?.blocks.get(bi).map(|b| (ti, b.start, b.end.max(b.start + 1))))
                .collect();
            let cx = jobwin::JobCtx { tm: &tm, ppqn: self.ppqn as u32, visible: &vis, selected, names: &self.list_names };
            let (undo, redo) = (&mut self.tv_undo, &mut self.tv_redo);
            let mut snap = |t: &[EditTrack]| {
                undo.push(t.to_vec());
                if undo.len() > 50 {
                    undo.remove(0);
                }
                redo.clear();
            };
            if let Some(msg) = jobwin::ui(ctx, &mut self.jobs, &cx, &mut self.edit, &mut snap) {
                self.status = msg;
                for ti in 0..self.edit.len() {
                    self.extend_blocks(ti); // moved events keep their blocks long enough
                }
                self.song_edited();
            }
        }
        self.save_settings_if_changed();
    }
}

/// ws.ini names + voice lists of the original (for the List window).
/// `note=name` lines of one section of an INI text (inst.ini kits).
fn ini_section(text: &str, name: &str) -> Vec<(u8, String)> {
    let head = format!("[{name}]");
    let mut on = false;
    let mut out = Vec::new();
    for l in text.lines() {
        let l = l.trim();
        if l.starts_with('[') {
            on = l.eq_ignore_ascii_case(&head);
            continue;
        }
        if on {
            if let Some((k, v)) = l.split_once('=') {
                if let Ok(n) = k.trim().parse::<u8>() {
                    out.push((n, v.trim().to_string()));
                }
            }
        }
    }
    out
}

fn load_list_names() -> xgworks_core::listev::ListNames {
    let ws = platform::read(&platform::xg4("ws.ini")).unwrap_or_default();
    xgworks_core::listev::ListNames::load(&ws, |name| platform::read(&platform::xg4(name)))
}

/// A fixed-size native window without panel margins (pixel-exact layouts).
fn fixed_window(ctx: &egui::Context, id: impl std::hash::Hash, title: &str, size: [f32; 2], mut body: impl FnMut(&mut egui::Ui)) -> bool {
    if ctx.embed_viewports() {
        return embedded_window(ctx, id, title, size, false, body);
    }
    let mut open = true;
    ctx.show_viewport_immediate(
        egui::ViewportId::from_hash_of(id),
        egui::ViewportBuilder::default().with_title(title).with_inner_size(size).with_resizable(false),
        |ctx, _class| {
            egui::CentralPanel::default().frame(egui::Frame::none()).show(ctx, |ui| body(ui));
            transport::note_space(ctx);
            if ctx.input(|i| i.viewport().close_requested()) {
                open = false;
            }
        },
    );
    open
}

/// Show `body` in its own native window; returns false once the user closes it.
fn sub_window(
    ctx: &egui::Context,
    id: impl std::hash::Hash,
    title: &str,
    size: [f32; 2],
    mut body: impl FnMut(&mut egui::Ui),
) -> bool {
    if ctx.embed_viewports() {
        return embedded_window(ctx, id, title, size, true, body);
    }
    let mut open = true;
    ctx.show_viewport_immediate(
        egui::ViewportId::from_hash_of(id),
        egui::ViewportBuilder::default().with_title(title).with_inner_size(size),
        |ctx, _class| {
            egui::CentralPanel::default().show(ctx, |ui| body(ui));
            transport::note_space(ctx);
            if ctx.input(|i| i.viewport().close_requested()) {
                open = false;
            }
        },
    );
    open
}

/// Where there is only one OS window (the web page), the sub-windows are
/// windows inside it. Keys go to one window only — the one last clicked or
/// opened — as they would with separate OS windows.
fn embedded_window(
    ctx: &egui::Context,
    id: impl std::hash::Hash,
    title: &str,
    size: [f32; 2],
    resizable: bool,
    mut body: impl FnMut(&mut egui::Ui),
) -> bool {
    let wid = egui::Id::new(("subwin", id));
    let layer = egui::LayerId::new(egui::Order::Middle, wid);
    let seen = egui::Id::new("subwin_seen").with(wid);
    if !ctx.data(|d| d.get_temp::<bool>(seen).unwrap_or(false)) {
        ctx.data_mut(|d| d.insert_temp(seen, true));
        set_key_focus(ctx, layer); // a new window takes the keys
        ctx.move_to_top(layer);
    }
    let mut open = true;
    let screen = ctx.screen_rect();
    let size = egui::vec2(size[0].min(screen.width() - 20.0), size[1].min(screen.height() - 60.0));
    let mut win = egui::Window::new(title).id(wid).open(&mut open).resizable(resizable).constrain(true);
    win = if resizable {
        win.default_size(size)
    } else {
        win.frame(egui::Frame::window(&ctx.style()).inner_margin(0.0)).fixed_size(size)
    };
    let active = keys_active(ctx, layer);
    win.show(ctx, |ui| with_keys(ctx, active, || body(ui)));
    if !open {
        ctx.data_mut(|d| d.remove::<bool>(seen));
    }
    open
}

fn key_focus_id() -> egui::Id {
    egui::Id::new("xgw_key_focus")
}

fn set_key_focus(ctx: &egui::Context, layer: egui::LayerId) {
    ctx.data_mut(|d| d.insert_temp(key_focus_id(), layer));
}

/// A click into a window (or the main view) gives it the keys; popups and
/// menus don't take them from the window they belong to.
fn track_key_focus(ctx: &egui::Context) {
    if !ctx.embed_viewports() {
        return;
    }
    let press = ctx.input(|i| if i.pointer.any_pressed() { i.pointer.interact_pos() } else { None });
    if let Some(layer) = press.and_then(|p| ctx.layer_id_at(p)) {
        if matches!(layer.order, egui::Order::Background | egui::Order::Middle) {
            set_key_focus(ctx, layer);
        }
    }
}

/// Whether `layer` gets keyboard input this frame (always, with OS windows;
/// the main view also when the focused window has been closed).
fn keys_active(ctx: &egui::Context, layer: egui::LayerId) -> bool {
    if !ctx.embed_viewports() {
        return true;
    }
    let cur = ctx.data(|d| d.get_temp::<egui::LayerId>(key_focus_id())).unwrap_or(egui::LayerId::background());
    if cur == layer {
        return true;
    }
    layer == egui::LayerId::background() && !ctx.memory(|m| m.areas().is_visible(&cur))
}

/// Run `f` with the keyboard events hidden unless `active`.
fn with_keys<R>(ctx: &egui::Context, active: bool, f: impl FnOnce() -> R) -> R {
    if active {
        return f();
    }
    let is_key = |e: &egui::Event| {
        matches!(e, egui::Event::Key { .. } | egui::Event::Text(_) | egui::Event::Copy | egui::Event::Cut | egui::Event::Paste(_))
    };
    let (keys, down) = ctx.input_mut(|i| {
        let (keys, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut i.events).into_iter().partition(is_key);
        i.events = rest;
        (keys, std::mem::take(&mut i.keys_down))
    });
    let r = f();
    ctx.input_mut(|i| {
        i.events.extend(keys);
        i.keys_down = down;
    });
    r
}

// ---------------------------------------------------------------------------
// Track View (main window), drawn like XGworks' Track View (compared against
// F:\rea\orig_shots\04_trackview_max.png): purple header, grey rows with white
// text, dark-blue arrangement area with grid, blocks as blue bars with a note
// icon and the block name (selected track red), markers on the ruler, and the
// Chord / Style lanes at the bottom. The unnamed conductor track is hidden.
// ---------------------------------------------------------------------------
const ROW_H: f32 = 20.0;
const TV_HEAD: Color32 = Color32::from_rgb(0x66, 0x00, 0x66);
const TV_ROW: Color32 = Color32::from_rgb(0x82, 0x82, 0x82);
const TV_LANE: Color32 = Color32::from_rgb(0x00, 0x40, 0x80);
const TV_GRID: Color32 = Color32::from_rgb(0x20, 0x60, 0xA0);
const TV_BLOCK: Color32 = Color32::from_rgb(0x66, 0x66, 0xFF);
const TV_BLOCK_SEL: Color32 = Color32::from_rgb(0xFF, 0x00, 0x00);
const TV_RULER_TXT: Color32 = Color32::from_rgb(0xFF, 0xCC, 0x66);
const TV_MARKER: Color32 = Color32::from_rgb(0x00, 0xDD, 0xEE);
const TV_PORT_SEL: Color32 = Color32::from_rgb(0, 200, 200);
const TV_CHORD_BG: Color32 = Color32::from_rgb(0xC8, 0xC8, 0xFF);
use egui::Color32;

/// (title, width) of the Track View columns, as in the original.
const TV_COLS: [(&str, f32); 10] = [
    ("Trk", 26.0), ("Port", 54.0), ("Ch/Pan", 56.0), ("Rec", 26.0), ("Name", 92.0),
    ("S", 24.0), ("M", 24.0), ("Trns/Vol", 60.0), ("Type", 52.0), ("Ins", 40.0),
];

const CHORD_TYPES: [&str; 35] = [
    "", "6", "M7", "M7(#11)", "add9", "M7(9)", "6(9)", "aug", "m", "m6", "m7", "m7b5", "madd9", "m7(9)",
    "m7(11)", "mM7", "mM7(9)", "dim", "dim7", "7", "7sus4", "7b5", "7(9)", "7(#11)", "7(13)", "7(b9)",
    "7(b13)", "7(#9)", "M7aug", "7aug", "1+8", "1+5", "sus4", "1+2+5", "cc",
];

/// The blocks a track shows: its stored blocks, or one implicit block over
/// all its data (rounded up to the bar) when the file had none.
fn track_blocks(t: &EditTrack, bar: u32) -> Vec<xgworks_core::EditBlock> {
    if !t.blocks.is_empty() || (t.notes.is_empty() && t.others.is_empty()) {
        return t.blocks.clone();
    }
    let end = t.notes.iter().map(|n| n.tick + n.dur).chain(t.others.iter().map(|e| e.t())).max().unwrap_or(0);
    vec![xgworks_core::EditBlock { name: t.name.clone(), start: 0, end: (end + bar - 1) / bar * bar, raw: Vec::new() }]
}

/// Which block owns an event at tick `t`: the last block starting at or
/// before it (else the first), the same rule `write_xws` saves with.
fn block_owner(blocks: &[xgworks_core::EditBlock], t: u32) -> usize {
    blocks.iter().rposition(|b| b.start <= t).unwrap_or(0)
}

/// A block on the Track View clipboard, times relative to the copied selection.
#[derive(Clone)]
struct ClipBlock {
    track: usize,
    name: String,
    start: u32,
    end: u32,
    notes: Vec<xgworks_core::EditNote>,
    others: Vec<Event>,
}

impl ClipBlock {
    fn take(t: &EditTrack, bi: usize, t0: u32, track: usize) -> Self {
        let b = &t.blocks[bi];
        let back = 0u32.wrapping_sub(t0);
        ClipBlock {
            track,
            name: b.name.clone(),
            start: b.start - t0,
            end: b.end.max(b.start) - t0,
            notes: t.notes.iter().filter(|n| block_owner(&t.blocks, n.tick) == bi)
                .map(|n| xgworks_core::EditNote { tick: n.tick.saturating_sub(t0), ..n.clone() }).collect(),
            others: t.others.iter().filter(|e| block_owner(&t.blocks, e.t()) == bi).map(|e| e.shifted(back)).collect(),
        }
    }

    /// Delete block `bi` and the events it owns.
    fn remove(t: &mut EditTrack, bi: usize) {
        let blocks = t.blocks.clone();
        t.notes.retain(|n| block_owner(&blocks, n.tick) != bi);
        t.others.retain(|e| block_owner(&blocks, e.t()) != bi);
        t.blocks.remove(bi);
    }

    /// Insert at tick `at` (+ its relative start); returns the new block index.
    fn paste(&self, t: &mut EditTrack, at: u32) -> usize {
        let off = at;
        for n in &self.notes {
            t.notes.push(xgworks_core::EditNote { tick: n.tick + off, ..n.clone() });
        }
        for e in &self.others {
            t.others.push(e.shifted(off));
        }
        t.notes.sort_by_key(|n| (n.tick, n.pitch));
        t.others.sort_by_key(|e| e.t());
        let blk = xgworks_core::EditBlock { name: self.name.clone(), start: self.start + off, end: self.end + off, raw: Vec::new() };
        t.blocks.push(blk);
        t.blocks.sort_by_key(|b| b.start);
        t.blocks.iter().position(|b| b.start == self.start + off && b.name == self.name).unwrap_or(0)
    }
}

/// Port column text: the driver's short name from XG4 DrvShortName.ini
/// [Midi Out] ("YAMAHA CBX A Driver= CBX A"), else the full name (clipped).
fn short_port_name(name: &str) -> String {
    static SHORT: std::sync::OnceLock<Vec<(String, String)>> = std::sync::OnceLock::new();
    let map = SHORT.get_or_init(|| {
        let txt = platform::read(&platform::xg4("DrvShortName.ini")).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
        txt.lines()
            .skip_while(|l| !l.starts_with("[Midi Out]"))
            .skip(1)
            .take_while(|l| !l.starts_with('['))
            .filter_map(|l| l.split_once('=').map(|(a, b)| (a.trim().to_string(), b.trim().to_string())))
            .collect()
    });
    map.iter().find(|(a, _)| a.eq_ignore_ascii_case(name.trim())).map_or_else(|| name.to_string(), |(_, b)| b.clone())
}

/// Track View "Ins" column text (transposing instruments are named from
/// XG4 Trk.ini [TransInst], entries "name,shift").
fn ins_label(t: &EditTrack) -> String {
    static TRANS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    let [kind, group, ..] = t.inst_type().to_be_bytes();
    if kind != 1 {
        return t.inst_label();
    }
    let names = TRANS.get_or_init(|| {
        let txt = platform::read(&platform::xg4("Trk.ini")).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
        let mut v = Vec::new();
        for l in txt.lines().skip_while(|l| !l.starts_with("[TransInst]")).skip(1) {
            if l.starts_with('[') {
                break;
            }
            if let Some((_, rest)) = l.split_once('=') {
                v.push(rest.split(',').next().unwrap_or("").trim().to_string());
            }
        }
        v
    });
    names.get(group as usize).cloned().unwrap_or_else(|| t.inst_label())
}

/// XF chord root byte: low nibble 1..7 = C..B, high nibble 3 = natural (lower = flats).
fn xf_root(b: u8) -> String {
    let n = ["", "C", "D", "E", "F", "G", "A", "B"].get((b & 0x0F) as usize).copied().unwrap_or("");
    let acc = match b >> 4 {
        0 => "bbb",
        1 => "bb",
        2 => "b",
        4 => "#",
        5 => "##",
        6 => "###",
        _ => "",
    };
    format!("{n}{acc}")
}

/// XF chord meta (FF 7F 43 7B 01 cr ct bn bt) → "Am7/D".
fn xf_chord(data: &[u8]) -> Option<String> {
    if data.len() < 7 || data[0..3] != [0x43, 0x7B, 0x01] {
        return None;
    }
    let mut s = format!("{}{}", xf_root(data[3]), CHORD_TYPES.get(data[4] as usize).copied().unwrap_or(""));
    if data[5] != 0x7F {
        s = format!("{s}/{}", xf_root(data[5]));
    }
    Some(s)
}

impl App {
    /// Indices of the tracks shown in the Track View (conductor hidden).
    fn visible_tracks(&self) -> Vec<usize> {
        (0..self.edit.len()).filter(|&i| !self.edit[i].conductor).collect()
    }

    fn track_view(&mut self, ui: &mut egui::Ui) {
        if self.edit.is_empty() {
            ui.centered_and_justified(|ui| ui.weak("Open an .XWS or .MID file."));
            return;
        }
        self.tv_keys(ui.ctx());
        let aud = self.audible();
        let vis = self.visible_tracks();
        let bar = self.ppqn as u32 * 4;
        let full = ui.available_rect_before_wrap();
        let hdr_w: f32 = TV_COLS.iter().map(|c| c.1).sum();
        let tl = egui::Rect::from_min_max(egui::pos2(full.left() + hdr_w + 4.0, full.top()), full.max);
        if self.px_per_tick <= 0.0 {
            self.px_per_tick = (tl.width() - 20.0) / self.total_ticks.max(1) as f32; // fit
        }
        // wheel: Ctrl = zoom, Shift / horizontal = scroll time, plain = scroll rows
        if ui.rect_contains_pointer(full) {
            let (scroll, zoom, ctrl, shift, mouse) =
                ui.input(|i| (i.raw_scroll_delta, i.zoom_delta(), i.modifiers.ctrl, i.modifiers.shift, i.pointer.hover_pos()));
            if ctrl && zoom != 1.0 {
                let mx = mouse.map_or(0.0, |p| (p.x - tl.left()).max(0.0));
                let at = self.view_t0 + mx / self.px_per_tick;
                self.px_per_tick = (self.px_per_tick * zoom).clamp(0.002, 2.0);
                self.view_t0 = (at - mx / self.px_per_tick).max(0.0);
            } else if shift || scroll.x != 0.0 {
                let d = if scroll.x != 0.0 { scroll.x } else { scroll.y };
                self.view_t0 = (self.view_t0 - d / self.px_per_tick).clamp(0.0, self.total_ticks as f32);
            } else if scroll.y != 0.0 {
                self.row_scroll = (self.row_scroll - scroll.y / ROW_H).max(0.0);
            }
        }
        let (t0, ppt) = (self.view_t0, self.px_per_tick);
        let x_of = |t: u32| tl.left() + (t as f32 - t0) * ppt;
        let p = ui.painter_at(full);
        let font = egui::FontId::monospace(11.0);
        let bold = |s: &str| egui::RichText::new(s);
        let _ = bold;

        // ---- header row + ruler
        let head = egui::Rect::from_min_size(full.min, egui::vec2(hdr_w, ROW_H));
        p.rect_filled(head, 0.0, TV_HEAD);
        let mut x = full.left();
        for (title, w) in TV_COLS {
            p.text(egui::pos2(x + 3.0, head.center().y), egui::Align2::LEFT_CENTER, title, font.clone(), Color32::WHITE);
            x += w;
            p.line_segment([egui::pos2(x, head.top()), egui::pos2(x, head.bottom())], egui::Stroke::new(1.0_f32, Color32::BLACK));
        }
        let ruler = egui::Rect::from_min_max(egui::pos2(tl.left(), full.top()), egui::pos2(tl.right(), full.top() + ROW_H));
        p.rect_filled(ruler, 0.0, TV_LANE);
        let step = ((36.0 / (bar as f32 * ppt)).ceil() as u32).max(1);
        let markers = self.markers();
        let mut b = (t0 as u32) / bar;
        while x_of(b * bar) <= ruler.right() {
            let xx = x_of(b * bar);
            if b % step == 0 && xx >= ruler.left() {
                p.text(egui::pos2(xx + 2.0, ruler.bottom() - 1.0), egui::Align2::LEFT_BOTTOM, format!("{}", b + 1), font.clone(), TV_RULER_TXT);
            }
            b += 1;
        }
        for (t, name) in &markers {
            let xx = x_of(*t);
            if xx >= ruler.left() && xx <= ruler.right() {
                p.add(egui::Shape::convex_polygon(
                    vec![egui::pos2(xx - 4.0, ruler.bottom() - 6.0), egui::pos2(xx + 4.0, ruler.bottom() - 6.0), egui::pos2(xx, ruler.bottom())],
                    TV_MARKER, egui::Stroke::NONE));
                p.text(egui::pos2(xx, ruler.top()), egui::Align2::CENTER_TOP, name, egui::FontId::monospace(10.0), TV_RULER_TXT);
            }
        }
        self.draw_playhead(&p, ruler, t0, ppt);
        let ruler_resp = ui.interact(ruler, ui.id().with("tv_ruler"), egui::Sense::click());
        self.ruler_input(ui, &ruler_resp, |x| ((x - tl.left()) / ppt + t0).max(0.0) as u32, |t| tl.left() + (t as f32 - t0) * ppt);

        // ---- track rows (pool rows after the used tracks, like the original)
        let bottom_lanes = 2.0 * ROW_H * 2.0;
        let rows_area = egui::Rect::from_min_max(egui::pos2(full.left(), head.bottom()), egui::pos2(full.right(), full.bottom() - bottom_lanes));
        let rows_fit = (rows_area.height() / ROW_H).floor() as usize;
        let total_rows = (vis.len() + 30).max(rows_fit);
        self.row_scroll = self.row_scroll.min(total_rows.saturating_sub(rows_fit) as f32);
        let first = self.row_scroll as usize;
        let rp = ui.painter_at(rows_area);
        rp.rect_filled(egui::Rect::from_min_max(egui::pos2(tl.left(), rows_area.top()), rows_area.max), 0.0, TV_LANE);
        // vertical grid every bar (thinned)
        let gstep = ((10.0 / (bar as f32 * ppt)).ceil() as u32).max(1);
        let mut b = (t0 as u32) / bar;
        while x_of(b * bar) <= rows_area.right() {
            if b % gstep == 0 {
                let xx = x_of(b * bar);
                rp.line_segment([egui::pos2(xx, rows_area.top()), egui::pos2(xx, rows_area.bottom())], egui::Stroke::new(1.0_f32, TV_GRID));
            }
            b += 1;
        }
        for r in 0..rows_fit.min(total_rows - first) {
            let row = first + r;
            let y = rows_area.top() + r as f32 * ROW_H;
            let hdr = egui::Rect::from_min_size(egui::pos2(full.left(), y), egui::vec2(hdr_w, ROW_H));
            let lane = egui::Rect::from_min_max(egui::pos2(tl.left(), y), egui::pos2(tl.right(), y + ROW_H));
            rp.line_segment([lane.left_bottom(), lane.right_bottom()], egui::Stroke::new(1.0_f32, TV_GRID));
            let ti = vis.get(row).copied();
            self.tv_row(&rp, ui, hdr, lane, row, ti, &aud, &x_of, bar);
            if ti.is_none() {
                // empty pool row: it can take a paste (the track is created then)
                let pool_ti = self.edit.len() + (row - vis.len());
                if let Some((pt, tick)) = self.paste_line {
                    if pt == pool_ti {
                        let xx = x_of(tick);
                        rp.line_segment([egui::pos2(xx, lane.top()), egui::pos2(xx, lane.bottom())], egui::Stroke::new(1.0_f32, Color32::WHITE));
                    }
                }
                let resp = ui.interact(lane, ui.id().with(("tv_pool", row)), egui::Sense::click());
                if let (true, Some(pos)) = (resp.clicked(), resp.interact_pointer_pos()) {
                    let tick = ((pos.x - lane.left()) / ppt + t0).max(0.0) as u32;
                    self.paste_line = Some((pool_ti, tick / bar * bar));
                    self.blk_sel.clear();
                }
            }
        }

        self.port_column(ui, &vis, rows_area, first, rows_fit.min(total_rows - first), full.left() + TV_COLS[0].1);
        self.trk_column(ui, &vis, rows_area, first, rows_fit.min(total_rows - first));
        let lanes = egui::Rect::from_min_max(egui::pos2(tl.left(), rows_area.top()), rows_area.max);
        self.tv_tools(ui, &vis, TvGeo { lanes, first, t0: self.view_t0, ppt: self.px_per_tick });

        // ---- Chord / Style lanes
        let cl = egui::Rect::from_min_max(egui::pos2(full.left(), full.bottom() - bottom_lanes), full.max);
        let lane_h = bottom_lanes / 2.0;
        for (k, label, kind) in [(0, "C", "CHORD"), (1, "S", "STYLE")] {
            let y = cl.top() + k as f32 * lane_h;
            let hdr = egui::Rect::from_min_size(egui::pos2(full.left(), y), egui::vec2(hdr_w, lane_h));
            p.rect_filled(hdr, 0.0, Color32::from_rgb(0x60, 0x60, 0xA0));
            p.text(egui::pos2(hdr.left() + 10.0, hdr.center().y), egui::Align2::CENTER_CENTER, label, font.clone(), Color32::WHITE);
            p.text(egui::pos2(hdr.left() + TV_COLS[..8].iter().map(|c| c.1).sum::<f32>() + 4.0, hdr.center().y),
                egui::Align2::LEFT_CENTER, kind, font.clone(), Color32::WHITE);
            let lane = egui::Rect::from_min_max(egui::pos2(tl.left(), y), egui::pos2(tl.right(), y + lane_h));
            p.rect_filled(lane, 0.0, TV_CHORD_BG);
            p.rect_stroke(lane, 0.0, egui::Stroke::new(1.0_f32, Color32::GRAY));
        }
        let chord_lane = egui::Rect::from_min_max(egui::pos2(tl.left(), cl.top()), egui::pos2(tl.right(), cl.top() + lane_h));
        let cp = ui.painter_at(chord_lane);
        let mut chords: Vec<(u32, String)> = self.edit.iter().flat_map(|t| t.others.iter()).filter_map(|e| match e {
            Event::Meta { t, mtype: 0x7F, data } => xf_chord(data).map(|c| (*t, c)),
            _ => None,
        }).collect();
        chords.sort_by_key(|c| c.0);
        let c_hdr = egui::Rect::from_min_size(egui::pos2(full.left(), cl.top()), egui::vec2(hdr_w, lane_h));
        let tick_of = |x: f32| ((x - tl.left()) / ppt + t0).max(0.0) as u32;
        self.chord_lane_input(ui, chord_lane, c_hdr, &x_of, &tick_of);
        // labels only where they fit (the original thins them out when zoomed out)
        let mut free_x = f32::MIN;
        for (t, c) in &chords {
            let xx = x_of(*t);
            if xx >= chord_lane.left() - 40.0 && xx <= chord_lane.right() && xx >= free_x {
                cp.line_segment([egui::pos2(xx, chord_lane.top() + 4.0), egui::pos2(xx, chord_lane.bottom() - 4.0)], egui::Stroke::new(1.0_f32, Color32::BLACK));
                let r = cp.text(egui::pos2(xx + 2.0, chord_lane.center().y), egui::Align2::LEFT_CENTER, c, egui::FontId::monospace(10.0), Color32::BLACK);
                free_x = r.right() + 4.0;
            }
        }
        ui.allocate_rect(full, egui::Sense::hover());
    }

    #[allow(clippy::too_many_arguments)]
    fn tv_row(&mut self, p: &egui::Painter, ui: &egui::Ui, hdr: egui::Rect, lane: egui::Rect, row: usize, ti: Option<usize>,
              aud: &[bool], x_of: &dyn Fn(u32) -> f32, bar: u32) {
        let font = egui::FontId::monospace(11.0);
        let sel = ti == Some(self.sel_track);
        // header cells
        p.rect_filled(hdr, 0.0, TV_ROW);
        let mut x = hdr.left();
        let mut cells: Vec<egui::Rect> = Vec::new();
        for (_, w) in TV_COLS {
            cells.push(egui::Rect::from_min_size(egui::pos2(x, hdr.top()), egui::vec2(w, ROW_H)));
            x += w;
        }
        p.rect_filled(cells[0].shrink(1.0), 0.0, Color32::from_gray(0xF0));
        p.text(cells[0].center(), egui::Align2::CENTER_CENTER, format!("{}", row + 1), font.clone(), Color32::BLACK);
        // cell text clipped to its column, like the original's truncated cells
        let clip_text = |c: egui::Rect, s: &str, col: Color32| {
            p.with_clip_rect(c.shrink(1.0)).text(c.left_center() + egui::vec2(2.0, 0.0), egui::Align2::LEFT_CENTER, s, font.clone(), col);
        };
        let dev = self.port_device(ti.map_or(0, |t| self.edit[t].port()));
        let port = short_port_name(self.out_names.get(dev).map_or("", |s| s.as_str()));
        // Port cells range-selected by dragging are cyan, as in the original
        let dragged = self.port_drag.is_some_and(|(a, b)| (a.min(b)..=a.max(b)).contains(&row));
        if ti.is_some() && (dragged || ti.is_some_and(|t| self.port_sel.contains(&t))) {
            p.rect_filled(cells[1].shrink(1.0), 0.0, TV_PORT_SEL);
        }
        clip_text(cells[1], &port, Color32::WHITE);
        for c in &cells[1..] {
            p.line_segment([c.left_top(), c.left_bottom()], egui::Stroke::new(1.0_f32, Color32::from_gray(0x40)));
        }
        p.line_segment([hdr.left_bottom(), hdr.right_bottom()], egui::Stroke::new(1.0_f32, Color32::from_gray(0x40)));
        let Some(ti) = ti else {
            p.text(cells[2].center(), egui::Align2::CENTER_CENTER, format!("{}", (row % 16) + 1), font.clone(), Color32::WHITE);
            let trns = if row % 16 == 9 { "Off" } else { "0" };
            p.text(cells[7].center(), egui::Align2::CENTER_CENTER, trns, font.clone(), Color32::WHITE);
            clip_text(cells[9], "Normal", Color32::WHITE);
            return;
        };
        let t = &self.edit[ti];
        p.text(cells[2].center(), egui::Align2::CENTER_CENTER, format!("{}", (t.channel & 0x0F) + 1), font.clone(), Color32::WHITE);
        let name_col = if aud[ti] { Color32::WHITE } else { Color32::from_gray(0xB0) };
        p.text(cells[4].left_center() + egui::vec2(2.0, 0.0), egui::Align2::LEFT_CENTER, &t.name, font.clone(), name_col);
        let (muted, solo) = self.flags[ti];
        if solo {
            p.rect_filled(cells[5].shrink(2.0), 0.0, Color32::from_rgb(214, 160, 40));
            p.text(cells[5].center(), egui::Align2::CENTER_CENTER, "S", font.clone(), Color32::BLACK);
        }
        if muted {
            p.rect_filled(cells[6].shrink(2.0), 0.0, Color32::from_rgb(200, 64, 56));
            p.text(cells[6].center(), egui::Align2::CENTER_CENTER, "M", font.clone(), Color32::WHITE);
        }
        let trns = match t.transpose() { None => "Off".to_string(), Some(v) => v.to_string() };
        p.text(cells[7].center(), egui::Align2::CENTER_CENTER, trns, font.clone(), Color32::WHITE);
        let ty = match t.track_type() { 2 => "WAVE", _ => "MIDI" };
        p.text(cells[8].center(), egui::Align2::CENTER_CENTER, ty, font.clone(), Color32::WHITE);
        clip_text(cells[9], &ins_label(t), Color32::WHITE);
        if sel {
            p.rect_stroke(cells[1].shrink(1.0), 0.0, egui::Stroke::new(1.0_f32, Color32::from_rgb(0, 220, 220)));
        }

        // blocks
        let blocks = track_blocks(t, bar);
        let lp = ui.painter_at(lane);
        let mut blk_rects: Vec<(usize, egui::Rect)> = Vec::new();
        for (bi, blk) in blocks.iter().enumerate() {
            let (x0, x1) = (x_of(blk.start), x_of(blk.end.max(blk.start + 1)).max(x_of(blk.start) + 12.0));
            if x1 < lane.left() || x0 > lane.right() {
                continue;
            }
            let r = egui::Rect::from_min_max(egui::pos2(x0, lane.top() + 4.0), egui::pos2(x1, lane.bottom() - 4.0));
            blk_rects.push((bi, r));
            let base = if self.blk_sel.contains(&(ti, bi)) { TV_BLOCK_SEL } else { TV_BLOCK };
            let col = if aud[ti] { base } else { base.linear_multiply(0.45) };
            lp.rect_filled(r, 0.0, col);
            lp.line_segment([r.left_top() + egui::vec2(0.0, 2.0), r.right_top() + egui::vec2(0.0, 2.0)], egui::Stroke::new(1.0_f32, Color32::WHITE.linear_multiply(0.6)));
            lp.rect_stroke(r, 0.0, egui::Stroke::new(1.0_f32, Color32::from_gray(0x0C)));
            lp.text(r.left_center() + egui::vec2(3.0, 0.0), egui::Align2::LEFT_CENTER,
                if blk.name.is_empty() || x1 - x0 < 40.0 { "♪".to_string() } else { format!("♪ {}", blk.name) },
                egui::FontId::monospace(11.0), Color32::WHITE);
        }
        if let Some((pt, tick)) = self.paste_line {
            if pt == ti {
                let xx = x_of(tick);
                lp.line_segment([egui::pos2(xx, lane.top()), egui::pos2(xx, lane.bottom())], egui::Stroke::new(1.0_f32, Color32::WHITE));
            }
        }
        self.draw_playhead(&lp, lane, self.view_t0, self.px_per_tick);

        // interaction: click selects, double-click opens the Piano Roll, S/M toggle
        let row_rect = egui::Rect::from_min_max(hdr.min, lane.max);
        let resp = ui.interact(row_rect, ui.id().with(("tv_row", ti)), egui::Sense::click());
        if resp.clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                if lane.contains(pos) {
                    // block: select (Ctrl toggles); empty lane: paste line at the bar
                    self.sel_track = ti;
                    let ctrl = ui.input(|i| i.modifiers.command);
                    match blk_rects.iter().rev().find(|(_, r)| r.contains(pos)) {
                        Some(&(bi, _)) => {
                            self.materialize_blocks(ti);
                            if ctrl {
                                if let Some(k) = self.blk_sel.iter().position(|&s| s == (ti, bi)) {
                                    self.blk_sel.remove(k);
                                } else {
                                    self.blk_sel.push((ti, bi));
                                }
                            } else {
                                self.blk_sel = vec![(ti, bi)];
                            }
                        }
                        None => {
                            let tick = ((pos.x - lane.left()) / self.px_per_tick + self.view_t0).max(0.0) as u32;
                            self.paste_line = Some((ti, tick / bar * bar));
                            if !ctrl {
                                self.blk_sel.clear();
                            }
                        }
                    }
                } else if cells[5].contains(pos) {
                    self.flags[ti].1 = !self.flags[ti].1;
                } else if cells[6].contains(pos) {
                    self.flags[ti].0 = !self.flags[ti].0;
                } else if cells[7].contains(pos) {
                    let cur = self.edit[ti].transpose().map_or("Off".to_string(), |v| v.to_string());
                    self.trns_edit = Some((ti, cur));
                    self.sel_track = ti;
                } else {
                    self.sel_track = ti;
                }
            }
        }
        let (muted, solo) = self.flags[ti];
        if (self.edit[ti].mute(), self.edit[ti].solo()) != (muted, solo) {
            self.edit[ti].set_solo_mute(solo, muted); // saved with the song, as XGworks does
        }
        if self.trns_edit.as_ref().is_some_and(|e| e.0 == ti) {
            self.trns_editor(ui.ctx(), cells[7], ti);
        }
        if let Some((ct, open)) = self.port_combo {
            if ct == ti {
                draw_combo(p, cells[1], &port);
                if open {
                    self.port_list(ui.ctx(), cells[1], ti);
                }
            }
        }
        if resp.double_clicked() {
            if let Some(pos) = resp.interact_pointer_pos() {
                if !cells[5].contains(pos) && !cells[6].contains(pos) && !cells[7].contains(pos) {
                    self.open_roll(ti);
                }
            }
        }
    }

    /// A track without stored blocks shows one implicit block; turn it into a
    /// real one so block indices stay valid for selection and editing.
    fn materialize_blocks(&mut self, ti: usize) {
        let bar = self.ppqn as u32 * 4;
        if self.edit[ti].blocks.is_empty() {
            self.edit[ti].blocks = track_blocks(&self.edit[ti], bar);
        }
    }

    /// Track View edit keys: Ctrl+C / Ctrl+X / Ctrl+V / Del / Ctrl+A / Ctrl+Z / Ctrl+Y.
    fn tv_keys(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.focused) || ctx.wants_keyboard_input() || self.trns_edit.is_some() {
            return;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F2)) {
            self.list_from_selection();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F3)) && self.sel_track < self.edit.len() {
            self.open_roll(self.sel_track);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F4)) {
            self.open_staff(self.sel_track);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F5)) {
            self.open_drum(self.sel_track);
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F6)) {
            self.show_mixer = !self.show_mixer;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F8)) {
            self.open_master();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F9)) {
            self.open_lyrics();
        }
        if ctx.input(|i| i.key_pressed(egui::Key::F7)) {
            self.show_xg = !self.show_xg;
            if self.show_xg {
                self.xg.sync_from_song(&self.edit);
            }
        }
        let k = clip::edit_keys(ctx);
        if k.all {
            let vis = self.visible_tracks();
            for &ti in &vis {
                self.materialize_blocks(ti);
            }
            self.blk_sel = vis.iter().flat_map(|&ti| (0..self.edit[ti].blocks.len()).map(move |bi| (ti, bi))).collect();
        }
        if (k.copy || k.cut) && !self.blk_sel.is_empty() {
            let t0 = self.blk_sel.iter().map(|&(ti, bi)| self.edit[ti].blocks[bi].start).min().unwrap_or(0);
            self.blk_clip = self.blk_sel.iter().map(|&(ti, bi)| ClipBlock::take(&self.edit[ti], bi, t0, ti)).collect();
            clip::announce(ctx, "blocks");
            self.status = format!("{} block(s) copied.", self.blk_clip.len());
        }
        if (k.cut || k.del) && !self.blk_sel.is_empty() {
            self.tv_snapshot();
            let mut sel = std::mem::take(&mut self.blk_sel);
            sel.sort_by(|a, b| b.cmp(a)); // remove from the back so indices stay valid
            for (ti, bi) in sel {
                ClipBlock::remove(&mut self.edit[ti], bi);
            }
            self.song_edited();
        }
        let menu_paste = std::mem::take(&mut self.pending_paste);
        if (k.paste || menu_paste) && !self.blk_clip.is_empty() {
            let Some((pt, at)) = self.paste_line else {
                self.status = "Click an empty place in the block area to set the paste line first.".into();
                return;
            };
            self.tv_snapshot();
            // pasting into an empty pool row creates the tracks up to it
            // (channel as the pool row shows it: row 1 = ch 1 ...)
            let mut row = self.visible_tracks().len(); // a new track takes the next row
            while self.edit.len() <= pt {
                self.edit.push(EditTrack { channel: (row % 16) as u8, ..Default::default() });
                row += 1;
            }
            // clip from one track goes to the paste line's track, else each to its own
            let one = self.blk_clip.iter().all(|c| c.track == self.blk_clip[0].track);
            let mut new_sel = Vec::new();
            for c in self.blk_clip.clone() {
                let ti = if one { pt } else { c.track };
                if ti >= self.edit.len() {
                    continue;
                }
                self.materialize_blocks(ti);
                let bi = c.paste(&mut self.edit[ti], at);
                new_sel.push((ti, bi));
            }
            self.blk_sel = new_sel;
            let len = self.blk_clip.iter().map(|c| c.end).max().unwrap_or(0);
            self.paste_line = Some((pt, at + len)); // repeated paste lays copies end to end
            self.song_edited();
        }
        if k.undo {
            self.tv_undo_redo(true);
        }
        if k.redo {
            self.tv_undo_redo(false);
        }
    }

    /// Song-level undo / redo (Track View, markers, lyrics, mixer insert …).
    fn tv_undo_redo(&mut self, undo: bool) {
        let (from, to) = if undo { (&mut self.tv_undo, &mut self.tv_redo) } else { (&mut self.tv_redo, &mut self.tv_undo) };
        if let Some(prev) = from.pop() {
            to.push(std::mem::replace(&mut self.edit, prev));
            self.blk_sel.clear();
            self.song_edited();
        }
    }

    fn tv_snapshot(&mut self) {
        self.tv_undo.push(self.edit.clone());
        if self.tv_undo.len() > 50 {
            self.tv_undo.remove(0);
        }
        self.tv_redo.clear();
    }

    /// Port column: a click shows the drop-down in the cell (a second click
    /// opens its list); dragging over several tracks selects their Port
    /// cells (cyan) and shows the drop-down in the bottom one, whose choice
    /// then applies to all of them (Track.hlp).
    fn port_column(&mut self, ui: &egui::Ui, vis: &[usize], rows: egui::Rect, first: usize, shown: usize, x: f32) {
        let col = egui::Rect::from_min_size(egui::pos2(x, rows.top()), egui::vec2(TV_COLS[1].1, shown as f32 * ROW_H));
        let resp = ui.interact(col, ui.id().with("tv_port_col"), egui::Sense::click_and_drag());
        let row_at = |y: f32| (first + ((y - rows.top()) / ROW_H).max(0.0) as usize).min(first + shown.saturating_sub(1));
        let pos = resp.interact_pointer_pos();
        if let Some(pos) = pos {
            if resp.drag_started() {
                // from where the button went down, not where the drag was recognised
                let y = ui.input(|i| i.pointer.press_origin()).map_or(pos.y, |o| o.y);
                let r = row_at(y);
                self.port_drag = Some((r, r));
                self.port_combo = None;
                self.port_sel.clear();
            } else if resp.dragged() {
                if let Some(d) = &mut self.port_drag {
                    d.1 = row_at(pos.y);
                }
            }
        }
        if resp.drag_stopped() {
            if let Some((a, b)) = self.port_drag.take() {
                let tracks: Vec<usize> = (a.min(b)..=a.max(b)).filter_map(|r| vis.get(r).copied()).collect();
                if let Some(&last) = tracks.last() {
                    self.port_combo = Some((last, false));
                    self.sel_track = last;
                    self.port_sel = if tracks.len() > 1 { tracks } else { Vec::new() };
                }
            }
        }
        if resp.clicked() {
            if let Some(&ti) = pos.and_then(|p| vis.get(row_at(p.y))) {
                match self.port_combo {
                    Some((ct, open)) if ct == ti => self.port_combo = Some((ti, !open)),
                    _ => {
                        self.port_combo = Some((ti, false));
                        self.port_sel.clear();
                    }
                }
                self.sel_track = ti;
            }
        }
        // a press elsewhere (outside the column and the open list) or Esc closes it
        let list_hit = ui.ctx().data(|d| d.get_temp::<egui::Rect>(egui::Id::new("port_list_rect")));
        let away = ui.input(|i| i.pointer.any_pressed() && i.pointer.interact_pos().is_some_and(|p| !col.contains(p) && !list_hit.is_some_and(|r| r.contains(p))));
        if self.port_combo.is_some() && (away || ui.input(|i| i.key_pressed(egui::Key::Escape))) {
            self.port_combo = None;
            self.port_sel.clear();
        }
    }

    /// The drop-down's list of output drivers under the Port cell; picking
    /// one routes the track (or all range-selected tracks) there.
    fn port_list(&mut self, ctx: &egui::Context, cell: egui::Rect, ti: usize) {
        let cur = self.port_device(self.edit[ti].port());
        let font = egui::FontId::monospace(11.0);
        let item_h = 15.0;
        let w = self.out_names.iter()
            .map(|n| ctx.fonts(|f| f.layout_no_wrap(n.clone(), font.clone(), Color32::BLACK).size().x))
            .fold(cell.width() - 2.0, f32::max) + 6.0;
        let rect = egui::Rect::from_min_size(cell.left_bottom() + egui::vec2(1.0, 0.0), egui::vec2(w, item_h * self.out_names.len() as f32 + 2.0));
        ctx.data_mut(|d| d.insert_temp(egui::Id::new("port_list_rect"), rect));
        let mut pick = None;
        egui::Area::new(egui::Id::new(("port_list", ti))).fixed_pos(rect.min).order(egui::Order::Foreground).show(ctx, |ui| {
            let (r, resp) = ui.allocate_exact_size(rect.size(), egui::Sense::click());
            let p = ui.painter();
            p.rect_filled(r, 0.0, Color32::WHITE);
            p.rect_stroke(r, 0.0, egui::Stroke::new(1.0_f32, Color32::BLACK));
            let hover = resp.hover_pos().map(|h| ((h.y - r.top() - 1.0) / item_h) as usize);
            for (d, name) in self.out_names.iter().enumerate() {
                let ir = egui::Rect::from_min_size(egui::pos2(r.left() + 1.0, r.top() + 1.0 + d as f32 * item_h), egui::vec2(r.width() - 2.0, item_h));
                let hi = hover.map_or(d == cur, |h| h == d);
                if hi {
                    p.rect_filled(ir, 0.0, Color32::from_rgb(0, 0, 128));
                }
                p.text(ir.left_center() + egui::vec2(2.0, 0.0), egui::Align2::LEFT_CENTER, name, font.clone(),
                    if hi { Color32::WHITE } else { Color32::BLACK });
            }
            if resp.clicked() {
                pick = hover.filter(|&h| h < self.out_names.len());
            }
        });
        if let Some(d) = pick {
            let p = self.port_for_device(d);
            let targets = if self.port_sel.is_empty() { vec![ti] } else { std::mem::take(&mut self.port_sel) };
            for &t in &targets {
                self.edit[t].set_port(p);
            }
            // the mixer's port now has other tracks: show their channel settings
            let mt = self.mixer_tracks();
            self.mx.chans = mixer::song_chan_mix(&mt, self.mx.ac_cc);
            self.mx.song_events(&mt);
            self.status = if targets.len() > 1 {
                format!("Tracks {}: Port {} = {}.", targets.len(), p + 1, self.out_names[d])
            } else {
                format!("{}: Port {} = {}.", self.edit[ti].name, p + 1, self.out_names[d])
            };
            self.port_combo = None;
            self.sel_track = ti;
        }
    }

    /// The Trns cell's spin box: type Off / -36..36, wheel or the arrows step
    /// by 1 (below -36 is Off). Enter or clicking elsewhere commits, Esc cancels.
    fn trns_editor(&mut self, ctx: &egui::Context, cell: egui::Rect, ti: usize) {
        let Some((_, mut text)) = self.trns_edit.take() else { return };
        let step = |v: Option<i8>, d: i32| -> Option<i8> {
            let n = v.map_or(-37, |x| x as i32) + d;
            if n < -36 { None } else { Some(n.min(36) as i8) }
        };
        let parse = |s: &str| -> Option<Option<i8>> {
            let s = s.trim();
            if s.eq_ignore_ascii_case("off") { Some(None) } else { s.parse::<i32>().ok().map(|v| Some(v.clamp(-36, 36) as i8)) }
        };
        let (mut done, mut cancel) = (false, false);
        let cur0 = self.edit[ti].transpose();
        egui::Area::new(egui::Id::new(("trns_edit", ti))).fixed_pos(cell.min).order(egui::Order::Foreground).show(ctx, |ui| {
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            ui.horizontal(|ui| {
                let te = ui.add_sized([cell.width() - 16.0, cell.height()],
                    egui::TextEdit::singleline(&mut text).font(egui::FontId::monospace(11.0)));
                te.request_focus();
                let cur = parse(&text).unwrap_or(cur0);
                let wheel = if ui.rect_contains_pointer(cell) { ui.input(|i| i.raw_scroll_delta.y) } else { 0.0 };
                let (up, dn) = ui.vertical(|ui| {
                    // spin buttons with painted arrows (the UI font has no ▲▼)
                    let b = |ui: &mut egui::Ui, up: bool| {
                        let (r, resp) = ui.allocate_exact_size(egui::vec2(16.0, cell.height() / 2.0), egui::Sense::click());
                        let p = ui.painter();
                        p.rect_filled(r, 0.0, Color32::from_gray(if resp.is_pointer_button_down_on() { 200 } else { 230 }));
                        p.rect_stroke(r, 0.0, egui::Stroke::new(1.0_f32, Color32::from_gray(120)));
                        let (c, s) = (r.center(), if up { 1.5 } else { -1.5 });
                        p.add(egui::Shape::convex_polygon(
                            vec![c + egui::vec2(-3.0, s), c + egui::vec2(3.0, s), c + egui::vec2(0.0, -s)],
                            Color32::BLACK, egui::Stroke::NONE));
                        resp.clicked()
                    };
                    (b(ui, true), b(ui, false))
                }).inner;
                let d = if up || wheel > 0.0 { 1 } else if dn || wheel < 0.0 { -1 } else { 0 };
                if d != 0 {
                    text = step(cur, d).map_or("Off".into(), |v| v.to_string());
                }
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                    cancel = true;
                } else if ui.input(|i| i.key_pressed(egui::Key::Enter))
                    || (ui.input(|i| i.pointer.any_pressed()) && !ui.rect_contains_pointer(cell.expand2(egui::vec2(18.0, 2.0))))
                {
                    done = true;
                }
            });
        });
        if cancel {
            return;
        }
        if done {
            if let Some(v) = parse(&text) {
                self.edit[ti].set_transpose(v);
                self.status = format!("{}: transpose {}.", self.edit[ti].name, v.map_or("Off".into(), |x| x.to_string()));
            }
            return;
        }
        self.trns_edit = Some((ti, text));
    }

    fn draw_playhead(&self, p: &egui::Painter, r: egui::Rect, t0: f32, ppt: f32) {
        if !self.edit.is_empty() {
            let tick = self.cur_tick() as f32;
            let x = r.left() + (tick - t0) * ppt;
            if x >= r.left() && x <= r.right() {
                p.line_segment([egui::pos2(x, r.top()), egui::pos2(x, r.bottom())],
                    egui::Stroke::new(1.5_f32, egui::Color32::from_rgb(224, 114, 74)));
            }
        }
    }

    /// Open a List window for the selected blocks (else the selected track).
    fn open_list(&mut self, spans: Vec<listwin::Span>) {
        if spans.is_empty() {
            return;
        }
        let mut st = listwin::ListState::new(spans);
        st.id = self.next_list_id;
        self.open_lists.push((self.next_list_id, st));
        self.next_list_id += 1;
    }

    fn list_from_selection(&mut self) {
        let mut spans: Vec<listwin::Span> = self.blk_sel.iter().map(|&(t, b)| listwin::Span { track: t, block: Some(b) }).collect();
        spans.sort_by_key(|s| (s.track, s.block));
        if spans.is_empty() && self.sel_track < self.edit.len() {
            spans.push(listwin::Span { track: self.sel_track, block: None });
        }
        self.open_list(spans);
    }

    /// The editors' bar: one button per window, icon + bold name (the
    /// original's toolbar window buttons, bitmap 2003 icons 11–17).
    fn editors_bar(&mut self, ui: &mut egui::Ui) {
        use icons::{editor_button, Vk, MAIN_BAR};
        let has = !self.edit.is_empty();
        let t = self.sel_track;
        ui.add_enabled_ui(has, |ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            let lists_open = self.open_lists.iter().any(|(_, st)| !st.master);
            if editor_button(ui, Some((MAIN_BAR, 11)), Vk::List, "Event List", lists_open, "List of the selected blocks, else the selected track (F2)").clicked() {
                self.list_from_selection();
            }
            let roll_open = self.open_rolls.iter().any(|(_, st)| st.drum.is_none() && st.staff.is_none());
            if editor_button(ui, Some((MAIN_BAR, 12)), Vk::PianoRoll, "Piano Roll", roll_open, "Piano Roll of the selected track (F3)").clicked() {
                self.open_roll(t);
            }
            let staff_open = self.open_rolls.iter().any(|(_, st)| st.staff.is_some());
            if editor_button(ui, Some((MAIN_BAR, 13)), Vk::Staff, "Staff", staff_open, "Notation of the selected track (F4)").clicked() {
                self.open_staff(t);
            }
            let drum_open = self.open_rolls.iter().any(|(_, st)| st.drum.is_some());
            if editor_button(ui, Some((MAIN_BAR, 14)), Vk::Drum, "Drum", drum_open, "Drum window of the selected track (F5)").clicked() {
                self.open_drum(t);
            }
            let master_open = self.open_lists.iter().any(|(_, st)| st.master);
            if editor_button(ui, Some((MAIN_BAR, 17)), Vk::Master, "Master Track", master_open, "Tempo, time / key signature, markers (F8)").clicked() {
                self.open_master();
            }
            if editor_button(ui, Some((MAIN_BAR, 15)), Vk::Mixer, "Mixer", self.show_mixer, "Mixer (F6)").clicked() {
                self.show_mixer = !self.show_mixer;
            }
            if editor_button(ui, Some((MAIN_BAR, 16)), Vk::Xg, "XG Editor", self.show_xg, "XG Editor (F7)").clicked() {
                self.show_xg = !self.show_xg;
                if self.show_xg {
                    self.xg.sync_from_song(&self.edit); // pick up mixer / song changes
                }
            }
            if editor_button(ui, None, Vk::Lyrics, "Lyrics", self.lyrics.is_some(), "Lyrics / Karaoke editor (F9)").clicked() {
                if self.lyrics.is_some() {
                    self.lyrics = None;
                } else {
                    self.open_lyrics();
                }
            }
            if editor_button(ui, None, Vk::Markers, "Markers", self.markers_win.is_some(), "Add Marker / Marker List").clicked() {
                if self.markers_win.is_some() {
                    self.markers_win = None;
                } else {
                    self.markers_win = Some(markerwin::MarkerState::new());
                }
            }
        });
    }

    /// The master track: the conductor track (XWS), else the first track (SMF).
    fn master_track(&self) -> Option<usize> {
        if self.edit.is_empty() {
            return None;
        }
        Some(self.edit.iter().position(|t| t.conductor).unwrap_or(0))
    }

    /// Master Track window (Window ▸ Master Track, F8 like the original).
    fn open_master(&mut self) {
        let Some(m) = self.master_track() else { return };
        if self.open_lists.iter().any(|(_, st)| st.master) {
            return;
        }
        let mut st = listwin::ListState::new_master(m);
        st.id = self.next_list_id;
        self.open_lists.push((self.next_list_id, st));
        self.next_list_id += 1;
    }

    /// Lyrics editor: on the first track with lyric events, else the selected one.
    fn open_lyrics(&mut self) {
        if self.edit.is_empty() || self.lyrics.is_some() {
            return;
        }
        let vis = self.visible_tracks();
        let t = vis
            .iter()
            .copied()
            .find(|&i| self.edit[i].others.iter().any(|e| matches!(e, Event::Meta { mtype: lyricwin::LYRIC, .. })))
            .unwrap_or(self.sel_track.min(self.edit.len() - 1));
        self.lyrics = Some(lyricwin::LyricsState::new(t));
    }

    /// Markers window and its actions.
    fn markers_window(&mut self, ctx: &egui::Context) {
        let Some(mut st) = self.markers_win.take() else { return };
        let Some(master) = self.master_track() else { return };
        let tm = self.time_map();
        let pos = self.cur_tick();
        let mut acts = Vec::new();
        let open = sub_window(ctx, "markers", "Markers", [460.0, 460.0], |ui| acts = st.ui(ui, &self.edit, master, &tm, pos));
        for a in acts {
            match a {
                markerwin::MarkerAction::Edit(f) => {
                    self.tv_snapshot();
                    f(&mut self.edit);
                    self.song_edited();
                }
                markerwin::MarkerAction::Locate(t) => self.locate(t),
            }
        }
        if open {
            self.markers_win = Some(st);
        }
    }

    /// Lyrics window and its actions.
    fn lyrics_window(&mut self, ctx: &egui::Context) {
        let Some(mut st) = self.lyrics.take() else { return };
        let vis = self.visible_tracks();
        let tm = self.time_map();
        let pos = self.cur_tick();
        let playing = self.playing;
        let mut acts = Vec::new();
        let master = self.master_track().unwrap_or(0);
        let fname = self.file_name.clone();
        let open = sub_window(ctx, "lyrics", "Lyrics / Karaoke", [900.0, 560.0], |ui| acts = st.ui(ui, &self.edit, &vis, &tm, pos, playing, master, &fname));
        for a in acts {
            match a {
                lyricwin::LyricAction::Edit(f) => {
                    self.tv_snapshot();
                    f(&mut self.edit);
                    let ti = st.track;
                    self.extend_blocks(ti);
                    self.song_edited();
                }
                lyricwin::LyricAction::Locate(t) => self.locate(t),
                lyricwin::LyricAction::Status(m) => self.status = m,
            }
        }
        if open {
            self.lyrics = Some(st);
        }
        if self.playing {
            ctx.request_repaint();
        }
    }

    /// Staff window for track `i` (Window ▸ Staff, F4 in the Track View).
    fn open_staff(&mut self, i: usize) {
        if i >= self.edit.len() {
            return;
        }
        self.sel_track = i;
        let info = self.staff_info(i);
        self.open_rolls.push((self.next_roll_id, pianoroll::RollState::new_staff(i, info)));
        self.next_roll_id += 1;
    }

    /// Key / time signature at the song start (master track) and the clef for the track.
    fn staff_info(&self, i: usize) -> staff::StaffInfo {
        let mut info = staff::StaffInfo::default();
        let metas = self.edit.iter().flat_map(|t| t.others.iter());
        let mut key_t = u32::MAX;
        let mut ts_t = u32::MAX;
        for e in metas {
            match e {
                Event::Meta { t, mtype: 0x59, data } if data.len() >= 2 && *t < key_t => {
                    key_t = *t;
                    info.sf = (data[0] as i8).clamp(-7, 7);
                }
                Event::Meta { t, mtype: 0x58, data } if data.len() >= 2 && data[0] > 0 && *t < ts_t => {
                    ts_t = *t;
                    info.ts_num = data[0] as u32;
                    info.ts_den = 1u32 << data[1].min(5);
                }
                _ => {}
            }
        }
        if let Some(t) = self.edit.get(i) {
            // guitar / bass (Track View Type: Gtr / Bass Tab) are written an octave up
            let kind = t.inst_type().to_be_bytes()[0];
            info.written = if matches!(kind, 3..=6) { 12 } else { 0 };
            let shown: Vec<xgworks_core::EditNote> = t.notes.iter().map(|n| xgworks_core::EditNote { pitch: (n.pitch as i32 + info.written as i32).clamp(0, 127) as u8, ..n.clone() }).collect();
            info.clef = if matches!(kind, 4 | 6) { staff::Clef::Bass } else { staff::clef_for(&shown) };
        }
        info
    }

    /// Drum window for track `i` (Window ▸ Drum, F5 in the Track View).
    fn open_drum(&mut self, i: usize) {
        if i >= self.edit.len() {
            return;
        }
        self.sel_track = i;
        let map = self.drum_map(i);
        self.open_rolls.push((self.next_roll_id, pianoroll::RollState::new_drum(i, map)));
        self.next_roll_id += 1;
    }

    /// Rows / names / colours of the Drum window: XGworks' inst.ini row
    /// order, names of the track's drum kit (bank MSB + program; channel 10
    /// defaults to the drum bank 127), else the Standard Kit's.
    fn drum_map(&self, i: usize) -> pianoroll::DrumMap {
        let inst = &self.inst_ini;
        let (msb, prog) = self.mix.get(i).map_or((0, 0), |m| (m.bank_msb, m.program));
        let msb = if msb == 0 && self.edit.get(i).is_some_and(|t| t.channel & 0x0F == 9) { 127 } else { msb };
        let std_kit = ini_section(inst, "Standard Kit");
        pianoroll::DrumMap::new(inst, |n| {
            self.list_names
                .drum(msb, prog, n)
                .map(|s| s.to_string())
                .or_else(|| std_kit.iter().find(|(k, _)| *k == n).map(|(_, v)| v.clone()))
        })
    }

    fn open_roll(&mut self, i: usize) {
        self.sel_track = i;
        if !self.open_rolls.iter().any(|(_, st)| st.track == i) {
            self.open_rolls.push((self.next_roll_id, pianoroll::RollState::new(i)));
            self.next_roll_id += 1;
        }
    }

    /// Marker meta events (FF 06) of the whole song: (tick, text).
    fn markers(&self) -> Vec<(u32, String)> {
        let mut m: Vec<(u32, String)> = self
            .edit
            .iter()
            .flat_map(|t| t.others.iter())
            .filter_map(|e| match e {
                Event::Meta { t, mtype: 0x06, data } => Some((*t, String::from_utf8_lossy(data).trim().to_string())),
                _ => None,
            })
            .collect();
        m.sort_by_key(|x| x.0);
        m.dedup();
        m
    }
}

// ---------------------------------------------------------------------------
// Mixer window glue (the window itself is mixwin.rs).
// ---------------------------------------------------------------------------
impl App {
    fn play_tick(&self) -> Option<f32> {
        self.play_start.map(|_| self.cur_tick() as f32)
    }

    fn mixer_actions(&mut self, acts: Vec<mixwin::MixAction>) {
        let mut song_changed = false;
        for a in acts {
            match a {
                mixwin::MixAction::Send(b) => {
                    let dev = self.mix_dev();
                    if open_device(&self.midi, dev) {
                        send_dev(&self.midi, dev, &b);
                    }
                    if self.mx.recording && self.playing && b[0] & 0xF0 != 0xF0 {
                        if let Some(tick) = self.play_tick() {
                            song_changed |= self.record_mixer_event(tick as u32, b);
                        }
                    }
                }
                mixwin::MixAction::SetPort(i) => {
                    // the mixer now controls the channels of that port's tracks
                    self.mix_port = self.port_for_device(i);
                    self.mx.load_song(&self.mixer_tracks());
                    self.status = format!("Mixer: Port {} = {}.", self.mix_port + 1, self.out_names.get(i).cloned().unwrap_or_default());
                }
                mixwin::MixAction::Insert(n) => {
                    let vis = self.visible_tracks();
                    let Some(&start) = vis.get(n.saturating_sub(1)) else {
                        self.status = format!("Track {n} does not exist.");
                        continue;
                    };
                    // channels beyond the last track get new (empty) tracks
                    while self.edit.len() < start + 16 {
                        let ch = (self.edit.len() - start) as u8;
                        let mut t = EditTrack { channel: ch, ..Default::default() };
                        t.set_port(self.mix_port);
                        self.edit.push(t);
                    }
                    mixer::insert_mixer_params(&mut self.edit, start, &self.mx.chans, self.mx.ac_cc, self.mx.master);
                    song_changed = true;
                    self.status = format!("Mixer parameters inserted into tracks {}–{}.", n, n + 15);
                }
                mixwin::MixAction::Status(s) => self.status = s,
            }
        }
        if song_changed {
            self.song_edited();
        }
    }

    /// Output device of the mixer's port.
    fn mix_dev(&self) -> usize {
        self.port_device(self.mix_port)
    }

    /// The tracks the mixer controls: those playing on its port's device.
    fn mixer_tracks(&self) -> Vec<EditTrack> {
        let md = self.mix_dev();
        self.edit.iter().filter(|t| self.port_device(t.port()) == md).cloned().collect()
    }

    /// Record Mixer Parameter: put a mixer move into an empty track reserved
    /// for its channel (the original picks a free track per channel).
    fn record_mixer_event(&mut self, tick: u32, data: Vec<u8>) -> bool {
        let ch = (data[0] & 0x0F) as usize;
        let ti = match self.rec_tracks[ch] {
            Some(t) => t,
            None => {
                let free = (0..self.edit.len()).find(|&i| {
                    let t = &self.edit[i];
                    !t.conductor && t.notes.is_empty() && t.others.is_empty() && !self.rec_tracks.contains(&Some(i))
                });
                let t = free.unwrap_or_else(|| {
                    self.edit.push(EditTrack::default());
                    self.edit.len() - 1
                });
                self.edit[t].channel = ch as u8;
                let port = self.mix_port;
                self.edit[t].set_port(port); // recorded for the mixer's port
                self.rec_tracks[ch] = Some(t);
                t
            }
        };
        self.edit[ti].others.push(Event::Chan { t: tick, data });
        true
    }

    /// Blocks of track `ti` grow (in whole bars) to cover the events they own.
    fn extend_blocks(&mut self, ti: usize) {
        let bar = self.ppqn as u32 * 4;
        let Some(t) = self.edit.get_mut(ti) else { return };
        if t.blocks.is_empty() {
            return;
        }
        let mut last = vec![0u32; t.blocks.len()];
        for tick in t.notes.iter().map(|n| n.tick).chain(t.others.iter().map(|e| e.t())) {
            let bi = block_owner(&t.blocks, tick);
            last[bi] = last[bi].max(tick + 1);
        }
        for (b, l) in t.blocks.iter_mut().zip(last) {
            if l > b.end {
                b.end = (l + bar - 1) / bar * bar;
            }
        }
    }

    /// Auto Play / Importer windows.
    fn plugin_windows(&mut self, ctx: &egui::Context) {
        if let Some(mut ap) = self.autoplay.take() {
            let playing = self.playing && ap.current.is_some();
            let lyrics = match (playing, self.play_tick()) {
                (true, Some(t)) => pluginwin::lyrics_page(&self.edit, t as u32),
                _ => Vec::new(),
            };
            let mut acts = Vec::new();
            let open = fixed_window(ctx, "autoplay", "Auto Play", [680.0, 340.0], |ui| pluginwin::dialog(ui, |ui| acts = ap.ui(ui, playing, &lyrics)));
            for a in acts {
                match a {
                    pluginwin::AutoAction::Play(i) => {
                        ap.current = Some(i);
                        self.autoplay_start(&mut ap);
                    }
                    pluginwin::AutoAction::Stop => {
                        ap.current = None;
                        self.stop();
                        self.status = "Stopped.".into();
                    }
                    pluginwin::AutoAction::Status(s) => self.status = s,
                }
            }
            if playing {
                ctx.request_repaint();
            }
            if open {
                self.autoplay = Some(ap);
            } else if ap.current.is_some() {
                self.stop();
            }
        }
        if let Some(mut im) = self.importer.take() {
            let tm = xgworks_core::listev::TimeMap::new(&self.edit, self.ppqn);
            let mut acts = Vec::new();
            let open = fixed_window(ctx, "importer", "Importer", [680.0, 360.0], |ui| pluginwin::dialog(ui, |ui| acts = im.ui(ui, &tm)));
            for a in acts {
                match a {
                    pluginwin::ImportAction::Insert(o, src, ppqn) => {
                        self.stop();
                        self.tv_snapshot();
                        if self.edit.is_empty() {
                            self.ppqn = ppqn;
                        }
                        let n = xgworks_core::importer::import(&mut self.edit, &src, ppqn, self.ppqn, &o);
                        self.song_edited();
                        self.status = format!("Imported {n} tracks.");
                    }
                    pluginwin::ImportAction::Monitor(tracks, ppqn) => {
                        self.play_tracks(Some((tracks, ppqn)));
                    }
                    pluginwin::ImportAction::Status(s) => self.status = s,
                }
            }
            if open {
                self.importer = Some(im);
            }
        }
    }

    /// Load and play the Auto Play window's current song.
    fn autoplay_start(&mut self, ap: &mut pluginwin::AutoPlay) {
        for _ in 0..ap.songs.len() {
            let Some(i) = ap.current else { return };
            let f = ap.songs[i].clone();
            match f.read() {
                Some(b) => {
                    self.load(&b, &f.name);
                    self.play();
                    return;
                }
                None => {
                    self.status = format!("Cannot read {}.", f.label());
                    ap.current = ap.next(); // skip it
                }
            }
        }
        ap.current = None;
    }

    /// A song ended: Auto Play goes on with the next one (endlessly; one
    /// selected song repeats).
    fn autoplay_next(&mut self) {
        if self.monitoring {
            return;
        }
        if let Some(mut ap) = self.autoplay.take() {
            if ap.current.is_some() {
                ap.current = ap.next();
                self.autoplay_start(&mut ap);
            }
            self.autoplay = Some(ap);
        }
    }

    /// After the mixer wrote into the song: keep the per-track state in step.
    fn song_edited(&mut self) {
        let n = self.edit.len();
        if self.flags.len() != n {
            self.flags.resize(n, (false, false));
            self.live_aud = Arc::new((0..n).map(|_| AtomicBool::new(true)).collect());
            self.mix_trk = Arc::new((0..n).map(|_| AtomicBool::new(true)).collect());
        }
        self.mix = self.edit.iter().map(mixer::read_part_mix).collect();
        self.mx.song_events(&self.mixer_tracks());
        self.rebuild_view();
    }
}

/// A Win95 drop-down (combo box) drawn in a cell: white field + arrow button.
fn draw_combo(p: &egui::Painter, cell: egui::Rect, text: &str) {
    let r = cell.shrink(1.0);
    p.rect_filled(r, 0.0, Color32::WHITE);
    p.rect_stroke(r, 0.0, egui::Stroke::new(1.0_f32, Color32::from_gray(0x40)));
    let b = egui::Rect::from_min_max(egui::pos2(r.right() - 14.0, r.top() + 1.0), egui::pos2(r.right() - 1.0, r.bottom() - 1.0));
    p.with_clip_rect(egui::Rect::from_min_max(r.min, egui::pos2(b.left(), r.bottom())))
        .text(r.left_center() + egui::vec2(2.0, 0.0), egui::Align2::LEFT_CENTER, text, egui::FontId::monospace(11.0), Color32::BLACK);
    p.rect_filled(b, 0.0, Color32::from_gray(0xC0));
    p.hline(b.x_range(), b.top() + 0.5, egui::Stroke::new(1.0_f32, Color32::WHITE));
    p.vline(b.left() + 0.5, b.y_range(), egui::Stroke::new(1.0_f32, Color32::WHITE));
    p.hline(b.x_range(), b.bottom() - 0.5, egui::Stroke::new(1.0_f32, Color32::from_gray(0x40)));
    p.vline(b.right() - 0.5, b.y_range(), egui::Stroke::new(1.0_f32, Color32::from_gray(0x40)));
    let c = b.center();
    p.add(egui::Shape::convex_polygon(
        vec![c + egui::vec2(-3.5, -1.5), c + egui::vec2(3.5, -1.5), c + egui::vec2(0.0, 2.0)],
        Color32::BLACK, egui::Stroke::NONE));
}

// ---------------------------------------------------------------------------
// Track View edit tools (Track.hlp "エディットツール"): arrow, pencil (new
// empty block by dragging over empty bars / drag a block's end to change its
// length), eraser (delete a block), scissors (split a block at a bar), bond
// (join two blocks of a track; Shift = all blocks between; drag a frame =
// all blocks inside). Track numbers can be dragged to reorder the tracks.
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq)]
enum TvTool {
    Arrow,
    Pencil,
    Eraser,
    Scissors,
    Bond,
}

const TV_TOOLS: [(TvTool, &str, &str); 5] = [
    (TvTool::Arrow, "Arrow", "Arrow tool"),
    (TvTool::Pencil, "Pencil", "Pencil tool: drag empty bars = new block, drag a block = change its length"),
    (TvTool::Eraser, "Eraser", "Eraser tool: click a block to delete it"),
    (TvTool::Scissors, "Scissors", "Scissors tool: click a block to split it at the bar"),
    (TvTool::Bond, "Bond", "Bond tool: click two blocks to join them (Shift: all between; drag: all inside)"),
];

#[derive(Clone, Copy)]
enum TvDrag {
    /// Pencil on empty bars: row, first tick, current tick.
    Create { row: usize, from: u32, cur: u32 },
    /// Pencil on a block: change its end.
    Resize { ti: usize, bi: usize, cur: u32 },
    /// Bond frame.
    Frame { from: egui::Pos2, cur: egui::Pos2 },
}

/// Geometry of the block section for the tools.
#[derive(Clone, Copy)]
struct TvGeo {
    lanes: egui::Rect,
    first: usize,
    t0: f32,
    ppt: f32,
}

impl TvGeo {
    fn row_at(&self, y: f32) -> usize {
        self.first + ((y - self.lanes.top()) / ROW_H).max(0.0) as usize
    }
    fn tick_at(&self, x: f32) -> u32 {
        ((x - self.lanes.left()) / self.ppt + self.t0).max(0.0) as u32
    }
    fn x_of(&self, t: u32) -> f32 {
        self.lanes.left() + (t as f32 - self.t0) * self.ppt
    }
    fn row_y(&self, row: usize) -> f32 {
        self.lanes.top() + (row as f32 - self.first as f32) * ROW_H
    }
}

impl App {
    /// The block of track `ti` under x (drawn extents, as the Track View draws them).
    fn block_at(&self, ti: usize, x: f32, g: &TvGeo) -> Option<usize> {
        let bar = self.ppqn as u32 * 4;
        let blocks = track_blocks(&self.edit[ti], bar);
        blocks.iter().enumerate().rev().find(|(_, b)| {
            let x0 = g.x_of(b.start);
            let x1 = g.x_of(b.end.max(b.start + 1)).max(x0 + 12.0);
            x >= x0 && x <= x1
        }).map(|(i, _)| i)
    }

    /// Block structure of `ti` changed: List windows showing its blocks by
    /// index would show the wrong ones, so those spans are dropped.
    fn blocks_changed(&mut self, ti: usize) {
        for (_, st) in &mut self.open_lists {
            st.spans.retain(|s| s.track != ti || s.block.is_none());
            st.target = st.target.min(st.spans.len().saturating_sub(1));
        }
        self.open_lists.retain(|(_, st)| !st.spans.is_empty());
        self.blk_sel.retain(|s| s.0 != ti);
    }

    fn tv_tools(&mut self, ui: &egui::Ui, vis: &[usize], g: TvGeo) {
        let bar = self.ppqn as u32 * 4;
        let ctx = ui.ctx().clone();
        // tool cursors over the blocks, from XGworks.exe
        if ctx.input(|i| i.pointer.hover_pos()).is_some_and(|p| g.lanes.contains(p)) && self.tv_menu.is_none() {
            use crate::cursors::{show, Cur};
            match self.tv_tool {
                TvTool::Arrow => {}
                TvTool::Pencil => show(&ctx, Cur::Pencil),
                TvTool::Eraser => show(&ctx, Cur::Eraser),
                TvTool::Scissors => show(&ctx, Cur::Scissors),
                TvTool::Bond => show(&ctx, Cur::Glue),
            }
        }
        // right click: the tools pop-up menu (any tool)
        if let Some(p) = ctx.input(|i| if i.pointer.secondary_clicked() { i.pointer.interact_pos() } else { None }) {
            if g.lanes.contains(p) {
                self.tv_menu = Some(p);
            }
        }
        self.tv_tool_menu(&ctx, vis, g);
        if self.tv_tool == TvTool::Arrow {
            return;
        }
        let resp = ui.interact(g.lanes, ui.id().with("tv_tools"), egui::Sense::click_and_drag());
        let press = ctx.input(|i| i.pointer.press_origin());
        let track_of = |row: usize| vis.get(row).copied();
        if resp.drag_started() {
            if let Some(p) = press {
                let row = g.row_at(p.y);
                match self.tv_tool {
                    TvTool::Pencil => {
                        let on = track_of(row).and_then(|ti| self.block_at(ti, p.x, &g).map(|bi| (ti, bi)));
                        self.tv_drag = Some(match on {
                            Some((ti, bi)) => {
                                self.materialize_blocks(ti);
                                TvDrag::Resize { ti, bi, cur: g.tick_at(p.x) }
                            }
                            None => TvDrag::Create { row, from: g.tick_at(p.x), cur: g.tick_at(p.x) },
                        });
                    }
                    TvTool::Bond => self.tv_drag = Some(TvDrag::Frame { from: p, cur: p }),
                    _ => {}
                }
            }
        }
        if let (Some(d), Some(p)) = (&mut self.tv_drag, resp.interact_pointer_pos()) {
            match d {
                TvDrag::Create { cur, .. } | TvDrag::Resize { cur, .. } => *cur = g.tick_at(p.x),
                TvDrag::Frame { cur, .. } => *cur = p,
            }
        }
        // preview
        let p = ui.painter_at(g.lanes);
        let white = egui::Stroke::new(1.0_f32, Color32::WHITE);
        match self.tv_drag {
            Some(TvDrag::Create { row, from, cur }) => {
                let (s, e) = (from.min(cur) / bar * bar, (from.max(cur) / bar + 1) * bar);
                let y = g.row_y(row);
                p.rect_stroke(egui::Rect::from_min_max(egui::pos2(g.x_of(s), y + 4.0), egui::pos2(g.x_of(e), y + ROW_H - 4.0)), 0.0, white);
            }
            Some(TvDrag::Resize { ti, bi, cur }) => {
                if let Some(b) = self.edit[ti].blocks.get(bi) {
                    let e = self.resize_end(ti, bi, cur);
                    let row = vis.iter().position(|&t| t == ti).unwrap_or(0);
                    let y = g.row_y(row);
                    p.rect_stroke(egui::Rect::from_min_max(egui::pos2(g.x_of(b.start), y + 4.0), egui::pos2(g.x_of(e), y + ROW_H - 4.0)), 0.0, white);
                }
            }
            Some(TvDrag::Frame { from, cur }) => {
                p.rect_stroke(egui::Rect::from_two_pos(from, cur), 0.0, white);
            }
            None => {}
        }
        if let Some((ti, bi)) = self.bond_first {
            if let (Some(row), Some(b)) = (vis.iter().position(|&t| t == ti), self.edit.get(ti).and_then(|t| t.blocks.get(bi))) {
                let y = g.row_y(row);
                p.rect_stroke(egui::Rect::from_min_max(egui::pos2(g.x_of(b.start), y + 3.0), egui::pos2(g.x_of(b.end).max(g.x_of(b.start) + 12.0), y + ROW_H - 3.0)), 0.0,
                    egui::Stroke::new(2.0_f32, Color32::YELLOW));
            }
        }
        if resp.drag_stopped() {
            match self.tv_drag.take() {
                Some(TvDrag::Create { row, from, cur }) => self.create_block(vis, row, from.min(cur) / bar * bar, (from.max(cur) / bar + 1) * bar),
                Some(TvDrag::Resize { ti, bi, cur }) => {
                    let e = self.resize_end(ti, bi, cur);
                    if self.edit[ti].blocks[bi].end != e {
                        self.tv_snapshot();
                        self.edit[ti].blocks[bi].end = e;
                        self.song_edited();
                    }
                }
                Some(TvDrag::Frame { from, cur }) => {
                    // all blocks inside the frame, per track, become one
                    let r = egui::Rect::from_two_pos(from, cur);
                    let (r0, r1) = (g.row_at(r.top()), g.row_at(r.bottom()));
                    let (t0, t1) = (g.tick_at(r.left()), g.tick_at(r.right()));
                    for row in r0..=r1 {
                        let Some(ti) = track_of(row) else { continue };
                        self.materialize_blocks(ti);
                        let inside: Vec<usize> = (0..self.edit[ti].blocks.len())
                            .filter(|&k| { let b = &self.edit[ti].blocks[k]; b.start >= t0 && b.end.max(b.start) <= t1 })
                            .collect();
                        if inside.len() >= 2 {
                            self.join_blocks(ti, &inside);
                        }
                    }
                }
                None => {}
            }
        }
        if resp.clicked() {
            let Some(pos) = resp.interact_pointer_pos() else { return };
            let Some(ti) = track_of(g.row_at(pos.y)) else { return };
            let Some(bi) = self.block_at(ti, pos.x, &g) else {
                if self.tv_tool == TvTool::Bond {
                    self.bond_first = None;
                }
                return;
            };
            self.materialize_blocks(ti);
            self.sel_track = ti;
            match self.tv_tool {
                TvTool::Eraser => {
                    self.tv_snapshot();
                    ClipBlock::remove(&mut self.edit[ti], bi);
                    self.blocks_changed(ti);
                    self.song_edited();
                    self.status = "Block deleted.".into();
                }
                TvTool::Scissors => {
                    // split at the bar nearest to the click (bar units, Track.hlp)
                    let b = self.edit[ti].blocks[bi].clone();
                    let at = ((g.tick_at(pos.x) + bar / 2) / bar) * bar;
                    if at > b.start && at < b.end {
                        self.tv_snapshot();
                        self.edit[ti].blocks[bi].end = at;
                        self.edit[ti].blocks.insert(bi + 1, xgworks_core::EditBlock { name: b.name.clone(), start: at, end: b.end, raw: Vec::new() });
                        self.blocks_changed(ti);
                        self.song_edited();
                        self.status = format!("Block split at bar {}.", at / bar + 1);
                    }
                }
                TvTool::Bond => match self.bond_first {
                    Some((ft, fb)) if ft == ti && fb != bi => {
                        let shift = ctx.input(|i| i.modifiers.shift);
                        let (a, z) = (fb.min(bi), fb.max(bi));
                        let set: Vec<usize> = if shift { (a..=z).collect() } else { vec![a, z] };
                        self.join_blocks(ti, &set);
                        self.bond_first = None;
                    }
                    _ => self.bond_first = Some((ti, bi)),
                },
                _ => {}
            }
        }
    }

    /// New end of block `bi` dragged with the pencil to `cur`: bar units, not
    /// before its last event (Track.hlp), at least one bar.
    fn resize_end(&self, ti: usize, bi: usize, cur: u32) -> u32 {
        let bar = self.ppqn as u32 * 4;
        let t = &self.edit[ti];
        let b = &t.blocks[bi];
        let last = t.notes.iter().filter(|n| block_owner(&t.blocks, n.tick) == bi).map(|n| n.tick + 1)
            .chain(t.others.iter().filter(|e| block_owner(&t.blocks, e.t()) == bi).map(|e| e.t() + 1))
            .max().unwrap_or(0);
        let min = ((last + bar - 1) / bar * bar).max(b.start + bar);
        ((cur + bar / 2) / bar * bar).max(min)
    }

    /// Pencil on empty bars of `row`: a new empty block [s, e) (a pool row
    /// gets its track created, as a paste there does).
    fn create_block(&mut self, vis: &[usize], row: usize, s: u32, e: u32) {
        self.tv_snapshot();
        let ti = match vis.get(row) {
            Some(&ti) => ti,
            None => {
                let mut r = vis.len();
                let mut ti = self.edit.len();
                while r <= row {
                    ti = self.edit.len();
                    self.edit.push(EditTrack { channel: (r % 16) as u8, ..Default::default() });
                    r += 1;
                }
                ti
            }
        };
        self.materialize_blocks(ti);
        let t = &mut self.edit[ti];
        t.blocks.push(xgworks_core::EditBlock { name: String::new(), start: s, end: e, raw: Vec::new() });
        t.blocks.sort_by_key(|b| b.start);
        self.blocks_changed(ti);
        self.sel_track = ti;
        self.song_edited();
        let bar = self.ppqn as u32 * 4;
        self.status = format!("New block: bars {}–{}.", s / bar + 1, e / bar);
    }

    /// Join blocks `set` (indices, ascending) of track `ti` into the first:
    /// it then reaches to the furthest end; a gap becomes empty bars. Blocks
    /// in between that are not in `set` stay as they are (Track.hlp).
    fn join_blocks(&mut self, ti: usize, set: &[usize]) {
        if set.len() < 2 {
            return;
        }
        self.tv_snapshot();
        let t = &mut self.edit[ti];
        let first = set[0];
        let end = set.iter().map(|&k| t.blocks[k].end.max(t.blocks[k].start)).max().unwrap_or(0);
        t.blocks[first].end = end;
        for &k in set[1..].iter().rev() {
            t.blocks.remove(k);
        }
        self.blocks_changed(ti);
        self.song_edited();
        self.status = format!("{} blocks joined.", set.len());
    }

    /// The tools pop-up menu (right click in the block section).
    fn tv_tool_menu(&mut self, ctx: &egui::Context, vis: &[usize], g: TvGeo) {
        let Some(at) = self.tv_menu else { return };
        let mut close = false;
        let r = egui::Area::new(egui::Id::new("tv_tool_menu")).fixed_pos(at).order(egui::Order::Foreground).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_min_width(120.0);
                for (t, name, _) in TV_TOOLS {
                    if ui.selectable_label(self.tv_tool == t, name).clicked() {
                        self.tv_tool = t;
                        self.bond_first = None;
                        close = true;
                    }
                }
                ui.separator();
                if ui.add_enabled(!self.blk_clip.is_empty(), egui::Button::new("Paste").frame(false)).clicked() {
                    // paste at the bar under the click, on that row's track
                    let bar = self.ppqn as u32 * 4;
                    let row = g.row_at(at.y);
                    let ti = vis.get(row).copied().unwrap_or(self.edit.len() + (row - vis.len().min(row)));
                    self.paste_line = Some((ti, g.tick_at(at.x) / bar * bar));
                    self.pending_paste = true;
                    close = true;
                }
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape))
            || (ctx.input(|i| i.pointer.primary_clicked()) && !r.response.contains_pointer())
        {
            close = true;
        }
        if close {
            self.tv_menu = None;
        }
    }

    /// Track numbers dragged up / down reorder the tracks (Track.hlp).
    fn trk_column(&mut self, ui: &egui::Ui, vis: &[usize], rows: egui::Rect, first: usize, shown: usize) {
        let col = egui::Rect::from_min_size(rows.min, egui::vec2(TV_COLS[0].1, shown as f32 * ROW_H));
        let resp = ui.interact(col, ui.id().with("tv_trk_col"), egui::Sense::drag());
        let row_at = |y: f32| first + ((y - rows.top()) / ROW_H).max(0.0) as usize;
        if resp.hovered() || resp.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        if resp.drag_started() {
            if let Some(p) = ui.input(|i| i.pointer.press_origin()) {
                let r = row_at(p.y);
                self.trk_drag = (r < vis.len()).then_some((r, r));
            }
        }
        if let (Some(d), Some(p)) = (&mut self.trk_drag, resp.interact_pointer_pos()) {
            d.1 = row_at(p.y).min(vis.len().saturating_sub(1));
        }
        if let Some((a, b)) = self.trk_drag {
            if a != b {
                // insertion mark where the track will go
                let y = rows.top() + (b as f32 - first as f32 + if b > a { 1.0 } else { 0.0 }) * ROW_H;
                ui.painter().hline(rows.x_range(), y, egui::Stroke::new(2.0_f32, Color32::WHITE));
            }
        }
        if resp.drag_stopped() {
            if let Some((a, b)) = self.trk_drag.take() {
                if a != b {
                    self.move_track(vis, a, b);
                }
            }
        }
    }

    /// Move the track shown in row `from` to row `to`; everything that refers
    /// to tracks by index follows.
    fn move_track(&mut self, vis: &[usize], from: usize, to: usize) {
        self.tv_snapshot();
        let mut order: Vec<usize> = vis.to_vec();
        let t = order.remove(from);
        order.insert(to, t);
        // new layout: hidden tracks keep their slots, visible ones in the new order
        let mut perm: Vec<usize> = (0..self.edit.len()).collect(); // new index -> old index
        for (slot, &old) in vis.iter().zip(&order) {
            perm[*slot] = old;
        }
        let mut new_of = vec![0usize; perm.len()]; // old index -> new index
        for (n, &o) in perm.iter().enumerate() {
            new_of[o] = n;
        }
        let old_edit = std::mem::take(&mut self.edit);
        let mut old: Vec<Option<EditTrack>> = old_edit.into_iter().map(Some).collect();
        self.edit = perm.iter().map(|&o| old[o].take().unwrap_or_default()).collect();
        self.flags = perm.iter().map(|&o| self.flags.get(o).copied().unwrap_or((false, false))).collect();
        let map = |i: usize| new_of.get(i).copied().unwrap_or(i);
        self.sel_track = map(self.sel_track);
        for (_, st) in &mut self.open_rolls {
            st.track = map(st.track);
        }
        for (_, st) in &mut self.open_lists {
            for s in &mut st.spans {
                s.track = map(s.track);
            }
        }
        for r in self.rec_tracks.iter_mut().flatten() {
            *r = map(*r);
        }
        self.blk_sel.clear();
        self.paste_line = None;
        self.bond_first = None;
        self.port_combo = None;
        self.port_sel.clear();
        self.song_edited();
        self.status = format!("Track {} moved to {}.", from + 1, to + 1);
    }
}
