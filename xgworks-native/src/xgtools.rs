//! XG Editor tool bar (XGEditor.dll): Open / Save (.SYX), XG Editor Setup,
//! Receive XG Bulk, Load XG Parameter, Insert XG Parameter and Record XG
//! Parameter, and where the editor's changes go: each part group to its
//! MIDI output port, then into the song's setup — or, while recording and
//! playing, into the recording track at the play position.

use super::{sub_window, App};
use crate::platform::{self, Instant};
use crate::player::{open_device, send_dev};
use crate::xged::{Change, XgTool};
use eframe::egui;
use xgworks_core::listev::TimeMap;
use xgworks_core::mixer;
use xgworks_core::xgtbl::{self, XgMemory, XgTable};
use xgworks_core::Event;

/// An open tool-bar dialog.
pub enum XgDlg {
    Setup,
    Load { from: [u32; 3], to: [u32; 3] },
    Insert { track: usize, at: [u32; 3], resets: [bool; 4] },
    Bulk(Bulk),
}

/// Receive XG Bulk: dump requests are sent one by one; each waits for its
/// answer (or the time-out) before the next goes out.
pub struct Bulk {
    request: bool,
    kind: usize,
    queue: Vec<[u8; 3]>,
    total: usize,
    waiting: Option<Instant>,
    got: XgMemory,
    received: usize,
    running: bool,
}

const BULK_KINDS: [&str; 5] = ["All", "System", "Effect", "Multi Part", "Drum Setup"];
const BULK_TIMEOUT_MS: u128 = 800;
/// Insert XG Parameter: GM System Off / GM On / GM2 On / XG On.
const RESETS: [(&str, &[u8]); 4] = [
    ("GM System Off", &[0x7E, 0x7F, 0x09, 0x02, 0xF7]),
    ("GM System On", &[0x7E, 0x7F, 0x09, 0x01, 0xF7]),
    ("GM2 System On", &[0x7E, 0x7F, 0x09, 0x03, 0xF7]),
    ("XG System On", &[0x43, 0x10, 0x4C, 0x00, 0x00, 0x7E, 0x00, 0xF7]),
];

/// Dump request addresses for a bulk type, from the model's table.
fn bulk_addrs(t: &XgTable, kind: usize) -> Vec<[u8; 3]> {
    let mut out = Vec::new();
    if kind == 0 || kind == 1 {
        out.push([0, 0, 0]);
    }
    if kind == 0 || kind == 2 {
        out.push([2, 1, 0]);
        if t.block("Eq Blk").is_some() {
            out.push([2, 0x40, 0]);
        }
        for b in t.blocks.iter().filter(|b| b.key.starts_with("Ins") && b.h.0 == 3) {
            out.extend((b.m.0..=b.m.1).map(|m| [3, m, 0]));
        }
    }
    if kind == 0 || kind == 3 {
        if let Some(b) = t.block("Part_1") {
            out.extend((0..b.instances()).map(|i| b.hm(i)).map(|(h, m)| [h, m, 0]));
        }
    }
    if kind == 0 || kind == 4 {
        if let Some(b) = t.block("DrumSet_1") {
            out.extend((0..b.instances()).map(|i| b.hm(i)).map(|(h, m)| [h, m, 0]));
        }
    }
    out.dedup();
    out
}

fn mbc_edit(ui: &mut egui::Ui, v: &mut [u32; 3]) {
    ui.add(egui::DragValue::new(&mut v[0]).range(1..=9999)).on_hover_text("Meas");
    ui.label(":");
    ui.add(egui::DragValue::new(&mut v[1]).range(1..=16)).on_hover_text("Beat");
    ui.label(":");
    ui.add(egui::DragValue::new(&mut v[2]).range(0..=1919)).on_hover_text("Clock");
}

impl App {
    fn xg_tm(&self) -> TimeMap {
        TimeMap::new(&self.edit, self.ppqn)
    }

    /// Send one XG Parameter Change to the port of its part group.
    pub(super) fn xg_send(&mut self, addr: [u8; 3], data: &[u8]) {
        let group = if addr[0] == 8 { (addr[1] / 16).min(3) as usize } else { 0 };
        let dev = self.port_device(self.xg.setup.ports[group]);
        if self.ensure_midi() && open_device(&self.midi, dev) {
            send_dev(&self.midi, dev, &mixer::xg_param(self.xg.setup.dev, addr, data));
        }
    }

    /// The editor's changes: sent, then written into the song.
    pub(super) fn xg_apply(&mut self, changes: &[Change]) {
        for (addr, data) in changes {
            self.xg_send(*addr, data);
        }
        if changes.is_empty() {
            return;
        }
        let dev = self.xg.setup.dev;
        if self.xg.recording && self.playing {
            let (Some(tick), Some(ti)) = (self.play_tick(), self.xg_rec_track()) else { return };
            for (addr, data) in changes {
                let b = mixer::xg_param(dev, *addr, data);
                self.edit[ti].others.push(Event::Sysex { t: tick as u32, status: 0xF0, data: b[1..].to_vec() });
            }
            self.extend_blocks(ti);
            self.song_edited();
        } else if !self.edit.is_empty() {
            // into the message the value comes from (else a new SysEx in track 1)
            xgtbl::apply_xg_changes_song(&mut self.edit, dev, changes);
        }
    }

    fn xg_rec_track(&self) -> Option<usize> {
        self.visible_tracks().get(self.xg.setup.rec_track.saturating_sub(1)).copied()
    }

    /// Tool-bar requests, picked files, dialogs and Play Monitor.
    pub(super) fn xg_tools(&mut self, ctx: &egui::Context) {
        match self.xg.tool.take() {
            Some(XgTool::Open) => platform::pick_files(&self.xg_pick, ctx, "XG Parameter File (*.SYX, *.XGP)", &["syx", "SYX", "xgp", "XGP"]),
            Some(XgTool::Save) => self.xg_save(),
            Some(XgTool::Setup) => self.xg_dlg = Some(XgDlg::Setup),
            Some(XgTool::Bulk) => {
                self.xg_dlg = Some(XgDlg::Bulk(Bulk { request: true, kind: 0, queue: Vec::new(), total: 0, waiting: None, got: XgMemory::default(), received: 0, running: false }))
            }
            Some(XgTool::Load) => {
                let tm = self.xg_tm();
                // up to the first note: the song's setup
                let first = self.edit.iter().flat_map(|t| t.notes.iter().map(|n| n.tick)).min().unwrap_or(0);
                let (m, b, c) = tm.to_mbc(first);
                self.xg_dlg = Some(XgDlg::Load { from: [1, 1, 0], to: [m, b, c] });
            }
            Some(XgTool::Insert) => {
                let track = self.visible_tracks().iter().position(|&v| v == self.sel_track).map_or(1, |k| k + 1);
                self.xg_dlg = Some(XgDlg::Insert { track, at: [1, 1, 0], resets: [false, false, false, true] });
            }
            None => {}
        }
        let picked = self.xg_pick.borrow_mut().take();
        if let Some(f) = picked.and_then(|v| v.into_iter().next()) {
            self.xg_open(&f.name, f.read().unwrap_or_default());
        }
        self.xg_dialog(ctx);
        if self.xg.setup.play_monitor && self.playing && self.show_xg {
            let now = ctx.input(|i| i.time);
            if now - self.xg_mon > 0.15 {
                self.xg_mon = now;
                if let Some(tick) = self.play_tick() {
                    self.xg.mem = XgMemory::from_song_range(&self.edit, 0, tick as u32, false);
                }
            }
        }
    }

    fn xg_open(&mut self, name: &str, bytes: Vec<u8>) {
        let msgs = xgtbl::split_syx(&bytes);
        let mut mem = self.xg.mem.clone();
        let used = msgs.iter().filter(|m| mem.apply_sysex(m)).count();
        if name.to_ascii_lowercase().ends_with(".xgp") || used == 0 {
            self.status = format!("{name}: this file format is not supported (XG SysEx .SYX only).");
            return;
        }
        let Some(t) = self.xg.table() else { return };
        let changes = mem.changes(t, false);
        if msgs.iter().any(|m| m.get(..6) == Some(&[0x43, 0x10, 0x4C, 0, 0, 0x7E][..]) || m.get(..4) == Some(&[0x7E, 0x7F, 0x09, 0x01][..])) {
            self.xg_send([0, 0, 0x7E], &[0]);
        }
        self.xg.mem = mem;
        self.xg_apply(&changes);
        self.status = format!("XG Editor: {name} loaded ({used} messages).");
    }

    fn xg_save(&mut self) {
        let Some(t) = self.xg.table() else { return };
        let bytes = xgtbl::to_syx(t, &self.xg.mem, self.xg.setup.dev);
        let stem = std::path::Path::new(&self.file_name).file_stem().and_then(|x| x.to_str()).unwrap_or("XGParam").to_string();
        let name = format!("{}.syx", if stem.is_empty() { "XGParam" } else { &stem });
        if let Some(msg) = platform::save_song(&name, "syx", "XG Parameter SYX File", &bytes) {
            self.status = msg;
        }
    }

    /// Received bulk dumps (native: the MIDI input), before the recorder
    /// drops idle input.
    pub(super) fn xg_bulk_tick(&mut self) {
        let Some(XgDlg::Bulk(bk)) = &mut self.xg_dlg else { return };
        if !bk.running {
            return;
        }
        let msgs: Vec<Vec<u8>> = std::mem::take(&mut *self.in_buf.lock().unwrap()).into_iter().map(|m| m.1).collect();
        for m in &msgs {
            if xgtbl::parse_xg_bulk(m).is_some() && bk.got.apply_sysex(m) {
                bk.received += 1;
                bk.waiting = None;
            }
        }
        if !bk.request {
            return;
        }
        if bk.waiting.is_some_and(|w| w.elapsed().as_millis() > BULK_TIMEOUT_MS) {
            bk.waiting = None; // no answer: the next one
        }
        if bk.waiting.is_none() {
            if bk.queue.is_empty() {
                bk.running = false;
                return;
            }
            let addr = bk.queue.remove(0);
            bk.waiting = Some(Instant::now());
            let group = if addr[0] == 8 { (addr[1] / 16).min(3) as usize } else { 0 };
            let dev = self.port_device(self.xg.setup.ports[group]);
            let req = xgtbl::xg_bulk_request(self.xg.setup.dev, addr);
            if self.ensure_midi() && open_device(&self.midi, dev) {
                send_dev(&self.midi, dev, &req);
            }
        }
    }

    fn xg_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut dlg) = self.xg_dlg.take() else { return };
        let keep;
        let mut act = 0; // 1 = OK / Exit
        let tracks: Vec<String> =
            self.visible_tracks().iter().enumerate().map(|(k, &i)| format!("{:2} {}", k + 1, self.edit[i].name)).collect();
        match &mut dlg {
            XgDlg::Setup => {
                let ports: Vec<String> = (0..self.port_map.len().max(1))
                    .map(|p| format!("Port {} - {}", p + 1, self.out_names.get(self.port_device(p as u8)).map_or("", |s| s.as_str())))
                    .collect();
                let su = &mut self.xg.setup;
                keep = sub_window(ctx, "xg_setup", "XG Editor Setup", [330.0, 330.0], |ui| {
                    ui.group(|ui| {
                        ui.label("MIDI Output Port");
                        egui::Grid::new("xg_ports").show(ui, |ui| {
                            for (g, port) in su.ports.iter_mut().enumerate() {
                                ui.label(format!("Part {}-{}", g * 16 + 1, g * 16 + 16));
                                egui::ComboBox::from_id_source(("xg_port", g))
                                    .width(200.0)
                                    .selected_text(ports.get(*port as usize).cloned().unwrap_or_else(|| format!("Port {}", *port + 1)))
                                    .show_ui(ui, |ui| {
                                        for (k, n) in ports.iter().enumerate() {
                                            ui.selectable_value(port, k as u8, n);
                                        }
                                    });
                                ui.end_row();
                            }
                        });
                    });
                    ui.group(|ui| {
                        ui.label("Inserting Data");
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut su.all, false, "Offset").on_hover_text("Only the parameters that are set");
                            ui.radio_value(&mut su.all, true, "All").on_hover_text("Every parameter (defaults included)");
                            ui.add_space(12.0);
                            ui.label("Interval");
                            ui.add(egui::DragValue::new(&mut su.interval).range(0..=480)).on_hover_text("Clocks between the inserted messages");
                        });
                    });
                    ui.horizontal(|ui| {
                        ui.label("Device Number");
                        let mut d = su.dev + 1;
                        if ui.add(egui::DragValue::new(&mut d).range(1..=16)).changed() {
                            su.dev = d - 1;
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Recording Track");
                        egui::ComboBox::from_id_source("xg_rec")
                            .width(170.0)
                            .selected_text(tracks.get(su.rec_track.saturating_sub(1)).cloned().unwrap_or_default())
                            .show_ui(ui, |ui| {
                                for (k, n) in tracks.iter().enumerate() {
                                    ui.selectable_value(&mut su.rec_track, k + 1, n);
                                }
                            });
                    });
                    ui.checkbox(&mut su.play_monitor, "Play Monitor").on_hover_text("While the song plays, the editor shows its XG settings");
                    ui.add_space(6.0);
                    if ui.button("  OK  ").clicked() {
                        act = 1;
                    }
                });
            }
            XgDlg::Load { from, to } => {
                keep = sub_window(ctx, "xg_load", "Load XG Parameter", [300.0, 130.0], |ui| {
                    egui::Grid::new("xg_load_g").show(ui, |ui| {
                        ui.label("From");
                        ui.horizontal(|ui| mbc_edit(ui, from));
                        ui.end_row();
                        ui.label("To");
                        ui.horizontal(|ui| mbc_edit(ui, to));
                        ui.end_row();
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button("  OK  ").clicked() {
                            act = 1;
                        }
                        if ui.button("Cancel").clicked() {
                            act = 2;
                        }
                    });
                });
            }
            XgDlg::Insert { track, at, resets } => {
                keep = sub_window(ctx, "xg_insert", "Insert XG Parameter", [300.0, 250.0], |ui| {
                    ui.group(|ui| {
                        ui.label("Destination");
                        ui.horizontal(|ui| {
                            ui.label("Track");
                            egui::ComboBox::from_id_source("xg_ins_trk")
                                .width(170.0)
                                .selected_text(tracks.get(track.saturating_sub(1)).cloned().unwrap_or_default())
                                .show_ui(ui, |ui| {
                                    for (k, n) in tracks.iter().enumerate() {
                                        ui.selectable_value(track, k + 1, n);
                                    }
                                });
                        });
                        ui.horizontal(|ui| {
                            ui.label("Meas : Beat : Clock");
                            mbc_edit(ui, at);
                        });
                    });
                    ui.group(|ui| {
                        for (k, (name, _)) in RESETS.iter().enumerate() {
                            ui.checkbox(&mut resets[k], *name);
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button("  OK  ").clicked() {
                            act = 1;
                        }
                        if ui.button("Cancel").clicked() {
                            act = 2;
                        }
                    });
                });
            }
            XgDlg::Bulk(bk) => {
                let has_in = self.midi_in.is_some();
                let table = self.xg.table();
                keep = sub_window(ctx, "xg_bulk", "Receive XG Bulk", [330.0, 250.0], |ui| {
                    ui.group(|ui| {
                        ui.label("Set Dump Request");
                        ui.horizontal(|ui| {
                            ui.radio_value(&mut bk.request, true, "Output");
                            ui.radio_value(&mut bk.request, false, "Not Output").on_hover_text("Only listen: start the dump on the instrument");
                            ui.label("Type");
                            ui.add_enabled_ui(bk.request && !bk.running, |ui| {
                                egui::ComboBox::from_id_source("xg_bulk_kind").selected_text(BULK_KINDS[bk.kind]).show_ui(ui, |ui| {
                                    for (k, n) in BULK_KINDS.iter().enumerate() {
                                        ui.selectable_value(&mut bk.kind, k, *n);
                                    }
                                });
                            });
                        });
                    });
                    ui.group(|ui| {
                        ui.label("Receive Bulk");
                        let f = if bk.total > 0 { (bk.total - bk.queue.len()) as f32 / bk.total as f32 } else { 0.0 };
                        ui.add(egui::ProgressBar::new(if bk.running || bk.total > 0 { f } else { 0.0 }).text(format!("{} received", bk.received)));
                        ui.horizontal(|ui| {
                            if ui.add_enabled(has_in && !bk.running && table.is_some(), egui::Button::new("Start")).clicked() {
                                bk.queue = if bk.request { table.map(|t| bulk_addrs(t, bk.kind)).unwrap_or_default() } else { Vec::new() };
                                bk.total = bk.queue.len();
                                bk.got = XgMemory::default();
                                bk.received = 0;
                                bk.waiting = None;
                                bk.running = true;
                            }
                            if ui.add_enabled(bk.running, egui::Button::new("Stop")).clicked() {
                                bk.running = false;
                                bk.queue.clear();
                            }
                        });
                        if !has_in {
                            ui.colored_label(egui::Color32::from_rgb(0xC0, 0x40, 0x20), "Choose a MIDI input first (Setup ▸ MIDI input).");
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.add_enabled(bk.received > 0 && !bk.running, egui::Button::new("Exit")).on_hover_text("Take the received data").clicked() {
                            act = 1;
                        }
                        if ui.button("Cancel").clicked() {
                            act = 2;
                        }
                    });
                });
                if bk.running {
                    ctx.request_repaint_after(std::time::Duration::from_millis(20));
                }
            }
        }
        if act == 1 {
            self.xg_dialog_ok(&dlg);
        }
        if keep && act == 0 {
            self.xg_dlg = Some(dlg);
        }
    }

    fn xg_dialog_ok(&mut self, dlg: &XgDlg) {
        match dlg {
            XgDlg::Setup => {}
            XgDlg::Load { from, to } => {
                let tm = self.xg_tm();
                let (a, b) = (tm.from_mbc(from[0], from[1], from[2]), tm.from_mbc(to[0], to[1], to[2]));
                self.xg.mem = XgMemory::from_song_range(&self.edit, a.min(b), a.max(b), false);
                self.status = format!("XG Editor: parameters loaded from {}:{}:{} - {}:{}:{}.", from[0], from[1], from[2], to[0], to[1], to[2]);
            }
            XgDlg::Insert { track, at, resets } => self.xg_insert(*track, *at, *resets),
            XgDlg::Bulk(bk) => {
                let mut mem = self.xg.mem.clone();
                for (a, v) in &bk.got.bytes {
                    mem.bytes.insert(*a, *v);
                }
                let changes = self.xg.table().map(|t| bk.got.changes(t, false)).unwrap_or_default();
                self.xg.mem = mem;
                // into the song like an edit (the instrument has them already)
                let dev = self.xg.setup.dev;
                if let Some(t0) = self.edit.first_mut() {
                    xgtbl::apply_xg_changes(t0, dev, &changes);
                }
                self.status = format!("XG Editor: {} bulk dumps received.", bk.received);
            }
        }
    }

    /// Insert XG Parameter: the resets, then the editor's parameters, one
    /// message every `interval` clocks from the position on.
    fn xg_insert(&mut self, track: usize, at: [u32; 3], resets: [bool; 4]) {
        let Some(&ti) = self.visible_tracks().get(track.saturating_sub(1)) else {
            self.status = "Insert XG Parameter: no such track.".into();
            return;
        };
        let Some(t) = self.xg.table() else { return };
        let changes = self.xg.mem.changes(t, self.xg.setup.all);
        let (dev, step) = (self.xg.setup.dev, self.xg.setup.interval);
        let mut tick = self.xg_tm().from_mbc(at[0], at[1], at[2]);
        let mut evs = Vec::new();
        for (k, (_, msg)) in RESETS.iter().enumerate().filter(|(k, _)| resets[*k]) {
            let mut d = msg.to_vec();
            if k == 3 {
                d[1] = 0x10 | (dev & 0x0F);
            }
            evs.push(Event::Sysex { t: tick, status: 0xF0, data: d });
            // the instrument needs time after a reset
            tick += step.max(self.ppqn as u32 / 4);
        }
        for (addr, data) in &changes {
            evs.push(Event::Sysex { t: tick, status: 0xF0, data: mixer::xg_param(dev, *addr, data)[1..].to_vec() });
            tick += step;
        }
        let n = evs.len();
        self.tv_snapshot();
        let tr = &mut self.edit[ti];
        tr.others.extend(evs);
        tr.others.sort_by_key(|e| e.t());
        self.extend_blocks(ti);
        self.song_edited();
        self.status = format!("Insert XG Parameter: {n} messages into track {track}.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sysex_at(app: &App, ti: usize, addr: [u8; 3]) -> Vec<u32> {
        app.edit[ti]
            .others
            .iter()
            .filter_map(|e| match e {
                Event::Sysex { t, data, .. } if mixer::parse_xg_change(data).is_some_and(|(a, _)| a == addr) => Some(*t),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn open_insert_load_and_bulk() {
        let mut app = App::new();
        let mid = std::fs::read("../xgworks-core/tests/fixtures/dream.mid").unwrap();
        app.load(&mid, "dream.mid");
        if app.xg.table().is_none() {
            eprintln!("no XG Editor data (XGworks folder) - skipped");
            return;
        }
        // Open .SYX: memory and the song's setup
        let mut syx = mixer::xg_reset(0);
        syx.extend(mixer::xg_param(0, [8, 0, 0x0B], &[33]));
        app.xg_open("t.syx", syx);
        assert_eq!(app.xg.mem.get([8, 0, 0x0B]), Some(33), "{}", app.status);
        // dream.mid sets part 1's volume with CC 7: the value goes there
        assert!(sysex_at(&app, 0, [8, 0, 0x0B]).is_empty());
        assert_eq!(xgtbl::XgMemory::from_song(&app.edit).get([8, 0, 0x0B]), Some(33));
        app.xg_open("t.xgp", vec![1, 2, 3]);
        assert!(app.status.contains("not supported"));
        // Insert XG Parameter: XG System On, then the parameters, at bar 2 of track 2
        app.xg.setup.interval = 10;
        app.xg_insert(2, [2, 1, 0], [false, false, false, true]);
        let ti = app.visible_tracks()[1];
        let bar2 = app.xg_tm().from_mbc(2, 1, 0);
        assert!(sysex_at(&app, ti, [0, 0, 0x7E]).contains(&bar2));
        let vol = sysex_at(&app, ti, [8, 0, 0x0B]);
        assert!(vol.iter().any(|&t| t > bar2), "{vol:?}");
        app.tv_undo_redo(true);
        assert!(!sysex_at(&app, ti, [0, 0, 0x7E]).contains(&bar2));
        // Load XG Parameter from a range
        app.xg.mem = XgMemory::default();
        app.xg_dialog_ok(&XgDlg::Load { from: [1, 1, 0], to: [1, 4, 479] });
        assert_eq!(app.xg.mem.get([8, 0, 0x0B]), Some(33));
        // Receive XG Bulk (listening only): one Multi Part dump
        app.xg_dlg = Some(XgDlg::Bulk(Bulk { request: false, kind: 0, queue: Vec::new(), total: 0, waiting: None, got: XgMemory::default(), received: 0, running: true }));
        let mut d = vec![0xF0, 0x43, 0x00, 0x4C, 0x00, 0x01, 0x08, 0x05, 0x0B, 77];
        let sum: u32 = d[4..].iter().map(|&b| b as u32).sum();
        d.push(((0x80 - (sum & 0x7F)) & 0x7F) as u8);
        d.push(0xF7);
        app.in_buf.lock().unwrap().push((Instant::now(), d));
        app.xg_bulk_tick();
        let dlg = app.xg_dlg.take().unwrap();
        let XgDlg::Bulk(bk) = &dlg else { unreachable!() };
        assert_eq!(bk.received, 1);
        app.xg_dialog_ok(&dlg);
        assert_eq!(app.xg.mem.get([8, 5, 0x0B]), Some(77));
        assert_eq!(sysex_at(&app, 0, [8, 5, 0x0B]), vec![0]);
    }

    #[test]
    fn dialogs_draw() {
        let mut app = App::new();
        let mid = std::fs::read("../xgworks-core/tests/fixtures/dream.mid").unwrap();
        app.load(&mid, "dream.mid");
        let ctx = egui::Context::default();
        for tool in [XgTool::Setup, XgTool::Load, XgTool::Insert, XgTool::Bulk] {
            app.xg.tool = Some(tool);
            for _ in 0..3 {
                let _ = ctx.run(egui::RawInput::default(), |ctx| app.xg_tools(ctx));
            }
            assert!(app.xg_dlg.is_some(), "{tool:?} closed");
            app.xg_dlg = None;
        }
    }

    #[test]
    fn setup_line_roundtrip() {
        let su = crate::xged::XgSetup { ports: [0, 0, 1, 3], all: true, interval: 5, dev: 2, rec_track: 4, play_monitor: true };
        assert_eq!(crate::xged::XgSetup::from_line(&su.to_line()), Some(su));
    }
}
