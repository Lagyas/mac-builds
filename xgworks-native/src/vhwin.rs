//! Vocal Harmony window (Yamaha PSR-SX900 / SX700; after the user's
//! midi-file-editor), laid out like the Lyrics and Chords windows: the chords
//! with the harmony notes under them on the left (from the XF chords, from
//! chord names in markers / texts, or found in the notes), the preview and the
//! position on the right; under them the harmony channel (check, fix velocity
//! 1 / CC 7 = 1), harmony notes computed from the chords, the VH SysEx block
//! (type, part, vocoder, dry / send levels, mic sends, vocal effect) and the
//! harmony heard aloud while playing. The VH type names come from
//! midi-file-editor.html in the XGworks data folder at run time (they are
//! Yamaha's names, so they are not built in).

use super::*;
use eframe::egui::Color32;
use xgworks_core::listev::{note_name, TimeMap};
use xgworks_core::vh::{self, GenMode, VhSettings};

/// Where the chords for the harmony come from.
pub const CHORD_SOURCES: [&str; 3] = ["XF chords", "Chord names in markers / texts", "Found in the notes"];

/// One chord of the chosen source: tick, name, pitch classes (None = N.C.).
type ChordRow = (u32, String, Option<Vec<i32>>);

/// A chord name as typed in a marker or text ("Am7", "Bbmaj7", "F#-", "C/E"),
/// as XF chord data.
fn chord_from_name(w: &str) -> Option<Vec<u8>> {
    let w = w.trim().trim_matches(|c: char| matches!(c, '(' | ')' | '[' | ']' | ',' | ';'));
    if w.is_empty() || !w.starts_with(|c: char| matches!(c, 'A'..='G')) {
        return None;
    }
    if let Some(d) = lyricwin::xf_chord_data(w) {
        return Some(d);
    }
    let (main, bass) = match w.split_once('/') {
        Some((m, b)) => (m, Some(b)),
        None => (w, None),
    };
    let rl = 1 + main.chars().nth(1).filter(|c| *c == '#' || *c == 'b').map_or(0, |_| 1);
    let (root, suf) = main.split_at(rl);
    let suf = match suf {
        "maj7" | "Maj7" | "maj" | "Δ" | "Δ7" => "M7",
        "min" | "mi" | "-" => "m",
        "min7" | "mi7" | "-7" => "m7",
        "maj9" | "Maj9" => "M7(9)",
        "+" | "aug" | "+5" => "aug",
        "o" | "°" | "dim" => "dim",
        "o7" | "°7" => "dim7",
        "sus" => "sus4",
        "dom7" => "7",
        x => x,
    };
    lyricwin::xf_chord_data(&format!("{root}{suf}{}", bass.map(|b| format!("/{b}")).unwrap_or_default()))
}

/// The chords of `src` (0 XF, 1 chord names in markers / texts / cue points, 2 found in the notes).
fn chord_rows(tracks: &[EditTrack], tm: &TimeMap, src: u8, ch: u8) -> Vec<ChordRow> {
    const ROOTS: [&str; 12] = ["C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"];
    let mut v: Vec<ChordRow> = match src {
        0 => tracks
            .iter()
            .flat_map(|t| t.others.iter())
            .filter_map(|e| match e {
                Event::Meta { t, mtype: 0x7F, data } => Some((*t, lyricwin::xf_chord_name(data)?, vh::xf_pcs(data)?)),
                _ => None,
            })
            .collect(),
        1 => tracks
            .iter()
            .flat_map(|t| t.others.iter())
            .filter_map(|e| match e {
                // text, marker, cue point: every word a chord name (so a title is not taken)
                Event::Meta { t, mtype: 1 | 6 | 7, data } => {
                    let text = xgworks_core::textenc::decode(data);
                    let words: Vec<&str> = text.split_whitespace().collect();
                    let datas: Vec<Vec<u8>> = words.iter().filter_map(|w| chord_from_name(w)).collect();
                    (!words.is_empty() && datas.len() == words.len()).then(|| (*t, lyricwin::xf_chord_name(&datas[0]).unwrap_or_default(), vh::xf_pcs(&datas[0]).flatten()))
                }
                _ => None,
            })
            .collect(),
        _ => vh::detect_chords(tracks, tm, ch)
            .into_iter()
            .map(|(t, r, k)| (t, format!("{}{}", ROOTS[r as usize], vh::DETECT_KINDS[k].0), Some(vh::DETECT_KINDS[k].1.iter().map(|i| (r + i) % 12).collect())))
            .collect(),
    };
    v.sort_by_key(|c| c.0);
    v
}

/// The harmony channel's notes (track notes and format-0 events): (on, off, pitch).
fn channel_notes(tracks: &[EditTrack], ch: u8) -> Vec<(u32, u32, u8)> {
    let mut v: Vec<(u32, u32, u8)> = Vec::new();
    for t in tracks {
        if t.channel & 0x0F == ch {
            v.extend(t.notes.iter().map(|n| (n.tick, n.tick + n.dur, n.pitch)));
        }
        v.extend(t.others.iter().filter_map(|e| match e {
            Event::Note { t, ch: c, note, gate, .. } if c & 0x0F == ch => Some((*t, *t + gate, *note)),
            _ => None,
        }));
    }
    v.sort();
    v
}

fn names(ns: &[u8]) -> String {
    ns.iter().map(|&n| note_name(n).replace(' ', "")).collect::<Vec<_>>().join(" ")
}

pub struct VhState {
    ch: u8,
    s: VhSettings,
    bars: String,
    mode: GenMode,
    lead: u32,
    voices: usize,
    center: i32,
    /// Harmony heard while playing: (on, volume, velocity, choir voice).
    pub preview: bool,
    pub prev_vol: u8,
    pub prev_vel: u8,
    types: Vec<(String, u8, u8, String)>,
    report: String,
    /// Chord source (CHORD_SOURCES), list text size, selected row.
    pub src: u8,
    font: f32,
    sel: Option<usize>,
    /// The chord rows for (source, channel, song size).
    cache: Option<((u8, u8, usize), Vec<ChordRow>)>,
}

impl VhState {
    pub fn new(tracks: &[EditTrack]) -> VhState {
        let f = vh::analyze(tracks);
        let types = platform::read(&platform::xg4("midi-file-editor.html"))
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .and_then(|h| xgworks_core::soundset::js_rows(&h, "VH_TYPES"))
            .map(|rows| {
                rows.into_iter()
                    .filter_map(|r| Some((r.first()?.str()?.to_string(), r.get(1)?.num()?, r.get(2)?.num()?, r.get(3)?.str()?.to_string())))
                    .collect()
            })
            .unwrap_or_default();
        VhState {
            ch: f.channel.unwrap_or(vh::DEFAULT_CH),
            s: f.settings,
            bars: String::new(),
            mode: GenMode::Replace,
            lead: 10,
            voices: 4,
            center: 64,
            preview: false,
            prev_vol: 100,
            prev_vel: 90,
            types,
            src: if vh::xf_chords(tracks).is_empty() { 2 } else { 0 },
            font: 1.0,
            sel: None,
            cache: None,
            report: if f.has_any { "The song has VH settings (read)." .into() } else { "The song has no VH settings yet (defaults shown).".into() },
        }
    }
}

enum VhAct {
    Fix,
    Generate(Vec<(u32, u32)>, vh::ChordLine),
    Insert,
}

impl App {
    /// The playback plan with the harmony channel audible: CC 7 and note
    /// velocities raised, a choir voice (GM 53 Choir Aahs) before its first note.
    pub(super) fn vh_preview(&self, plan: Vec<(f64, usize, Vec<u8>)>) -> Vec<(f64, usize, Vec<u8>)> {
        let Some(v) = self.vh.as_ref().filter(|v| v.preview) else { return plan };
        let ch = v.ch;
        let mut out = Vec::with_capacity(plan.len() + 4);
        let mut first: Option<(f64, usize)> = None;
        for (ms, k, m) in plan {
            if m.first().is_some_and(|s| *s < 0xF0 && s & 0x0F == ch) {
                match (m[0] & 0xF0, m.get(1).copied(), m.get(2).copied()) {
                    (0xC0, _, _) | (0xB0, Some(0 | 32), _) => continue,
                    (0xB0, Some(7), _) => {
                        out.push((ms, k, vec![m[0], 7, v.prev_vol]));
                        continue;
                    }
                    (0x90, Some(n), Some(vel)) if vel > 0 => {
                        first.get_or_insert((ms, k));
                        out.push((ms, k, vec![m[0], n, v.prev_vel]));
                        continue;
                    }
                    _ => {}
                }
            }
            out.push((ms, k, m));
        }
        if let Some((ms, k)) = first {
            let at = (ms - 1.0).max(0.0);
            let pos = out.iter().position(|x| x.0 >= at).unwrap_or(out.len());
            for (i, m) in [vec![0xB0 | ch, 0, 0], vec![0xB0 | ch, 32, 0], vec![0xC0 | ch, 52], vec![0xB0 | ch, 7, v.prev_vol]].into_iter().enumerate() {
                out.insert(pos + i, (at, k, m));
            }
        }
        out
    }

    pub(super) fn vh_window(&mut self, ctx: &egui::Context) {
        let Some(mut st) = self.vh.take() else { return };
        if self.edit.is_empty() {
            return;
        }
        let tm = self.time_map();
        let pos = self.cur_tick();
        let chk = vh::check_channel(&self.edit, st.ch);
        let size = self.edit.iter().map(|t| t.notes.len() + t.others.len()).sum::<usize>();
        let key = (st.src, st.ch, size);
        if st.cache.as_ref().is_none_or(|c| c.0 != key) {
            st.cache = Some((key, chord_rows(&self.edit, &tm, st.src, st.ch)));
        }
        let rows = st.cache.as_ref().map(|c| c.1.clone()).unwrap_or_default();
        let harm = channel_notes(&self.edit, st.ch);
        // the harmony notes that start with each chord (until the next one; a lead counts in)
        let lead = st.lead.max(60);
        let under = |k: usize| -> Vec<u8> {
            let s = rows[k].0.saturating_sub(lead);
            let e = rows.get(k + 1).map_or(u32::MAX, |r| r.0.saturating_sub(lead));
            let mut v: Vec<u8> = harm.iter().filter(|n| n.0 >= s && n.0 < e).map(|n| n.2).collect();
            v.sort();
            v.dedup();
            v
        };
        let counts = [chord_rows(&self.edit, &tm, 0, st.ch).len(), chord_rows(&self.edit, &tm, 1, st.ch).len()];
        let mut act: Option<VhAct> = None;
        let mut locate: Option<u32> = None;
        let open = sub_window(ctx, "vh", "Vocal Harmony (PSR-SX900)", [900.0, 560.0], |ui| {
            // ---- top row: the harmony channel and its check
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new("Harmony channel").strong());
                egui::ComboBox::from_id_source("vh_ch").selected_text(format!("CH {}", st.ch + 1)).width(70.0).show_ui(ui, |ui| {
                    for c in 0..16u8 {
                        ui.selectable_value(&mut st.ch, c, format!("CH {}", c + 1));
                    }
                });
                let good = chk.notes == 0 || (chk.vel_ok() && chk.vol_ok());
                let col = if good { Color32::from_rgb(0x20, 0x90, 0x40) } else { Color32::from_rgb(0xC0, 0x60, 0x00) };
                ui.colored_label(col, format!("{} notes · velocity {} · CC 7 {}", chk.notes, if chk.vel_ok() { "1 OK" } else { "≠ 1" }, if chk.vol_ok() { "= 1 OK" } else { "needs fixing" }))
                    .on_hover_text(format!("Velocities found: {:?}. Harmony notes are silent control data for the keyboard: velocity 1 and one CC 7 = 1 before the first note.", chk.velocities));
                if ui.add_enabled(chk.notes > 0 && !good, egui::Button::new("Fix")).on_hover_text("Velocity 1, one CC 7 = 1 before the first note").clicked() {
                    act = Some(VhAct::Fix);
                }
                ui.separator();
                ui.label("Text size");
                if ui.button(" A− ").clicked() {
                    st.font = (st.font - 0.1).max(0.7);
                }
                if ui.button(" A+ ").clicked() {
                    st.font = (st.font + 0.1).min(2.5);
                }
                ui.label(format!("{:.0} %", st.font * 100.0));
            });
            ui.columns(2, |cols| {
                // ---- left: the chords and the harmony notes with them
                let ui = &mut cols[0];
                ui.horizontal(|ui| {
                    ui.label("Chords from");
                    let label = |k: usize| match k {
                        0 => format!("{} ({})", CHORD_SOURCES[0], counts[0]),
                        1 => format!("{} ({})", CHORD_SOURCES[1], counts[1]),
                        _ => CHORD_SOURCES[2].to_string(),
                    };
                    egui::ComboBox::from_id_source("vh_src").selected_text(label(st.src as usize)).width(250.0).show_ui(ui, |ui| {
                        for k in 0..3 {
                            if ui.selectable_value(&mut st.src, k as u8, label(k)).changed() {
                                st.sel = None;
                            }
                        }
                    })
                    .response
                    .on_hover_text("XF chords: the song's chord track (Chords window). Markers / texts: events whose words are all chord names (Am7, F#m, Bb/D, N.C.). Found in the notes: one chord per half bar from every channel except drums and the harmony channel.");
                });
                ui.label(egui::RichText::new("click = select + song position · right column: the harmony notes under the chord").small());
                egui::ScrollArea::vertical().id_source("vh_list").max_height(340.0).auto_shrink([false, false]).show(ui, |ui| {
                    if rows.is_empty() {
                        ui.weak(match st.src {
                            0 => "The song has no XF chords — choose another source, or write them in the Chords window.",
                            1 => "No marker / text with chord names.",
                            _ => "No notes to find chords in.",
                        });
                    }
                    egui::Grid::new("vh_grid").striped(true).num_columns(3).show(ui, |ui| {
                        let fs = 13.0 * st.font;
                        for (k, (t, name, _)) in rows.iter().enumerate() {
                            let (m, b, c) = tm.to_mbc(*t);
                            let r1 = ui.selectable_label(st.sel == Some(k), egui::RichText::new(format!("{m:04}:{b}:{c:03}")).monospace().size(fs));
                            let r2 = ui.selectable_label(st.sel == Some(k), egui::RichText::new(name).strong().size(fs * 1.15));
                            let r3 = ui.selectable_label(st.sel == Some(k), egui::RichText::new(names(&under(k))).monospace().size(fs).color(Color32::from_rgb(0x30, 0x70, 0xC0)));
                            if r1.union(r2).union(r3).clicked() {
                                st.sel = Some(k);
                                locate = Some(*t);
                            }
                            ui.end_row();
                        }
                    });
                });

                // ---- right: preview, position, then the tools
                let ui = &mut cols[1];
                ui.label(egui::RichText::new("Harmony preview").strong());
                let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 150.0), egui::Sense::hover());
                let p = ui.painter_at(rect);
                p.rect_filled(rect, 4.0, Color32::from_rgb(16, 20, 40));
                let now = rows.iter().rposition(|c| c.0 <= pos);
                let sounding: Vec<u8> = {
                    let mut v: Vec<u8> = harm.iter().filter(|n| n.0 <= pos + lead && n.1 > pos).map(|n| n.2).collect();
                    v.sort();
                    v.dedup();
                    v
                };
                let cur = now.map(|k| rows[k].1.as_str()).unwrap_or("");
                p.text(egui::pos2(rect.center().x, rect.top() + 42.0), egui::Align2::CENTER_CENTER, cur, egui::FontId::proportional(40.0 * st.font), Color32::from_rgb(255, 200, 40));
                p.text(
                    egui::pos2(rect.center().x, rect.top() + 88.0),
                    egui::Align2::CENTER_CENTER,
                    if sounding.is_empty() { "—".to_string() } else { names(&sounding) },
                    egui::FontId::monospace(18.0 * st.font),
                    Color32::from_rgb(120, 190, 255),
                );
                let next: Vec<&str> = rows.iter().skip(now.map_or(0, |k| k + 1)).take(4).map(|c| c.1.as_str()).collect();
                p.text(egui::pos2(rect.center().x, rect.bottom() - 22.0), egui::Align2::CENTER_CENTER, next.join("   "), egui::FontId::proportional(18.0 * st.font), Color32::WHITE);
                if let Some(t) = lyricwin::time_line(ui, &self.edit, &tm, pos, "vh_scrub") {
                    locate = Some(t);
                }
                ui.horizontal(|ui| {
                    ui.checkbox(&mut st.preview, "Harmony aloud while playing").on_hover_text("The harmony channel plays a choir (GM 53) at this volume / velocity; the song stays as it is");
                    ui.spacing_mut().slider_width = 70.0;
                    ui.add_enabled(st.preview, egui::Slider::new(&mut st.prev_vol, 1..=127).text("vol"));
                    ui.add_enabled(st.preview, egui::Slider::new(&mut st.prev_vel, 1..=127).text("vel"));
                });
                ui.separator();
                egui::ScrollArea::vertical().id_source("vh_tools").auto_shrink([false, false]).show(ui, |ui| {
                    egui::CollapsingHeader::new(egui::RichText::new("Compute harmony notes from the chords").strong()).default_open(true).show(ui, |ui| {
                        egui::Grid::new("vh_gen").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
                            ui.label("Bars");
                            ui.add(egui::TextEdit::singleline(&mut st.bars).hint_text("empty = whole song, or e.g. 54-77, 94-117").desired_width(240.0));
                            ui.end_row();
                            ui.label("Mode");
                            ui.vertical(|ui| {
                                ui.radio_value(&mut st.mode, GenMode::Replace, "replace the notes in those bars");
                                ui.radio_value(&mut st.mode, GenMode::Fill, "only where there are none");
                            });
                            ui.end_row();
                            ui.label("Lead");
                            ui.add(egui::DragValue::new(&mut st.lead).range(0..=240).suffix(" ticks before a chord change"));
                            ui.end_row();
                            ui.label("Voices / position");
                            ui.horizontal(|ui| {
                                ui.add(egui::DragValue::new(&mut st.voices).range(1..=4));
                                ui.add(egui::DragValue::new(&mut st.center).range(40..=84).prefix("centre "));
                                ui.weak(format!("({})", note_name(st.center as u8).replace(' ', "")));
                            });
                            ui.end_row();
                        });
                        let bars = vh::parse_bars(&st.bars);
                        if bars.is_none() {
                            ui.colored_label(Color32::from_rgb(0xC0, 0x40, 0x20), "Bars: write e.g. 54-77, 94-117");
                        }
                        if st.mode == GenMode::Replace && st.bars.trim().is_empty() && chk.notes > 0 {
                            ui.colored_label(Color32::from_rgb(0xC0, 0x60, 0x00), format!("Replaces all {} harmony notes of the song.", chk.notes));
                        }
                        let line: vh::ChordLine = rows.iter().map(|r| (r.0, r.2.clone())).collect();
                        if ui
                            .add_enabled(bars.is_some() && !rows.is_empty(), egui::Button::new(format!("Compute into CH {} ({})", st.ch + 1, ["from the XF chords", "from the marker / text chords", "from the chords found in the notes"][st.src as usize])))
                            .clicked()
                        {
                            act = Some(VhAct::Generate(bars.unwrap_or_default(), line));
                        }
                    });
                    egui::CollapsingHeader::new(egui::RichText::new("VH settings (SysEx)").strong()).default_open(true).show(ui, |ui| {
                        egui::Grid::new("vh_sx").num_columns(2).spacing([10.0, 4.0]).show(ui, |ui| {
                            ui.label("VH type");
                            let cur = st.types.iter().find(|t| t.1 == st.s.type_msb && t.2 == st.s.type_lsb).map(|t| t.0.clone()).unwrap_or_else(|| format!("{}/{}", st.s.type_msb, st.s.type_lsb));
                            if st.types.is_empty() {
                                ui.horizontal(|ui| {
                                    ui.add(egui::DragValue::new(&mut st.s.type_msb).range(0..=127).prefix("MSB "));
                                    ui.add(egui::DragValue::new(&mut st.s.type_lsb).range(0..=127).prefix("LSB "));
                                })
                                .response
                                .on_hover_text("Names: midi-file-editor.html in the XGworks folder");
                            } else {
                                egui::ComboBox::from_id_source("vh_type").selected_text(cur).width(200.0).show_ui(ui, |ui| {
                                    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                                        let mut group = String::new();
                                        for (name, m, l, g) in &st.types {
                                            if *g != group {
                                                ui.label(egui::RichText::new(g).small().strong());
                                                group = g.clone();
                                            }
                                            if ui.selectable_label(*m == st.s.type_msb && *l == st.s.type_lsb, format!("   {name}")).clicked() {
                                                st.s.type_msb = *m;
                                                st.s.type_lsb = *l;
                                            }
                                        }
                                    });
                                });
                            }
                            ui.end_row();
                            ui.label("VH Part Number");
                            ui.add(egui::DragValue::new(&mut st.s.part_number).range(0..=127));
                            ui.end_row();
                            ui.label("Vocoder Part");
                            ui.horizontal(|ui| {
                                for (k, n) in ["Off", "Upper", "Lower"].into_iter().enumerate() {
                                    ui.radio_value(&mut st.s.vocoder_part, k as u8, n);
                                }
                            });
                            ui.end_row();
                            for (label, v) in [
                                ("Lead Vocal Dry", &mut st.s.lead_dry),
                                ("Harmony Vocal Dry", &mut st.s.harm_dry),
                                ("Lead Vocal Send", &mut st.s.lead_send),
                                ("Harmony Vocal Send", &mut st.s.harm_send),
                                ("Mic Chorus Send", &mut st.s.mic_chorus),
                                ("Mic Reverb Send", &mut st.s.mic_reverb),
                            ] {
                                ui.label(label);
                                ui.add(egui::Slider::new(v, 0..=127));
                                ui.end_row();
                            }
                            ui.label("Vocal Effect");
                            ui.horizontal(|ui| {
                                ui.add(egui::DragValue::new(&mut st.s.vfx_msb).range(0..=127).prefix("type "));
                                ui.add(egui::DragValue::new(&mut st.s.vfx_lsb).range(0..=127).prefix("/ "));
                                ui.checkbox(&mut st.s.vfx_on, "on");
                            });
                            ui.end_row();
                        });
                        ui.horizontal(|ui| {
                            let found = vh::analyze(&self.edit);
                            let label = if found.has_any { "Update the VH SysEx" } else { "Insert the VH SysEx" };
                            if ui.button(label).on_hover_text("In place of the song's VH messages, else 100 ms after its last reset (first track)").clicked() {
                                act = Some(VhAct::Insert);
                            }
                            if ui.button("Defaults").on_hover_text("The settings of the reference PSR-SX900 file").clicked() {
                                st.s = VhSettings::default();
                            }
                        });
                        egui::CollapsingHeader::new("Messages").show(ui, |ui| {
                            for (label, data) in vh::build_msgs(&st.s, st.ch) {
                                ui.label(label);
                                ui.weak(egui::RichText::new(format!("F0 {}", data.iter().map(|b| format!("{b:02X}")).collect::<Vec<_>>().join(" "))).monospace().small());
                            }
                        });
                    });
                });
            });
            if !st.report.is_empty() {
                ui.separator();
                ui.label(&st.report);
            }
        });
        if let Some(t) = locate {
            self.locate(t);
        }
        if let Some(a) = act {
            self.tv_snapshot();
            let ch = st.ch;
            st.report = match a {
                VhAct::Fix => format!("CH {}: {} velocities set to 1, one CC 7 = 1 before the first note.", ch + 1, vh::fix_channel(&mut self.edit, ch)),
                VhAct::Generate(bars, line) => {
                    let (made, skipped) = vh::generate(&mut self.edit, &tm, &line, ch, &bars, st.mode, st.lead, st.voices, st.center);
                    vh::fix_channel(&mut self.edit, ch);
                    format!("{made} harmony notes computed on CH {}{} (velocity 1, CC 7 = 1).", ch + 1, if skipped > 0 { format!("; {skipped} chords skipped (notes there)") } else { String::new() })
                }
                VhAct::Insert => {
                    let (n, base, replaced) = vh::insert_sysex(&mut self.edit, &tm, &st.s, ch);
                    format!("{n} VH SysEx messages written from tick {base}{}.", if replaced > 0 { format!(", {replaced} old ones replaced") } else { String::new() })
                }
            };
            st.cache = None;
            self.status = st.report.clone();
            self.song_edited();
        }
        if open {
            self.vh = Some(st);
        }
        if self.playing {
            ctx.request_repaint();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harmony_heard_while_playing() {
        let mut app = App::new();
        let mut st = VhState::new(&[]);
        st.preview = true;
        app.vh = Some(st);
        let plan = vec![(10.0, 0, vec![0xCF, 5]), (20.0, 0, vec![0xBF, 7, 1]), (30.0, 0, vec![0x9F, 60, 1]), (30.0, 1, vec![0x90, 60, 80])];
        let out = app.vh_preview(plan);
        assert!(!out.iter().any(|m| m.2 == vec![0xCF, 5]), "the song's program on CH 16 is replaced");
        assert!(out.iter().any(|m| m.2 == vec![0xCF, 52]), "a choir");
        assert!(out.iter().any(|m| m.2 == vec![0x9F, 60, 90]), "heard");
        assert!(out.iter().any(|m| m.2 == vec![0x90, 60, 80]), "other channels untouched");
        app.vh.as_mut().unwrap().preview = false;
        assert_eq!(app.vh_preview(vec![(0.0, 0, vec![0x9F, 60, 1])]), vec![(0.0, 0, vec![0x9F, 60, 1])]);
    }

    #[test]
    fn chord_names_in_markers() {
        assert!(chord_from_name("Am7").is_some());
        assert!(chord_from_name("Bbmaj7").is_some());
        assert!(chord_from_name("F#-").is_some());
        assert!(chord_from_name("C/E").is_some());
        assert!(chord_from_name("Valcicek").is_none());
        assert!(chord_from_name("a").is_none());
        let tr = vec![EditTrack {
            others: vec![
                Event::Meta { t: 0, mtype: 6, data: b"Valcicek - Jan Nedved".to_vec() },
                Event::Meta { t: 480, mtype: 6, data: b"Am7".to_vec() },
                Event::Meta { t: 960, mtype: 1, data: b"G/B D".to_vec() },
            ],
            ..Default::default()
        }];
        let tm = TimeMap::new(&tr, 480);
        let rows = chord_rows(&tr, &tm, 1, 15);
        assert_eq!(rows.iter().map(|r| (r.0, r.1.as_str())).collect::<Vec<_>>(), vec![(480, "Am7"), (960, "G/B")]);
        assert_eq!(rows[0].2, Some(vec![9, 0, 4, 7]));
    }
}
