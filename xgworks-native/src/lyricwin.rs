//! Lyrics / karaoke editor (Window ▸ Lyrics): the lyric events (FF 05) of
//! one track as a list (position, text) with insert / edit / move /
//! delete, a karaoke preview that follows playback, and "assign syllables
//! to notes" from typed text. Markers as in Yamaha XF and .kar files: a
//! syllable starting with "/" begins a new line, "<" (XF) or "\" (.kar) a
//! new page; words end with a space, syllables of one word ("Hap-py") are
//! joined. Text is stored in the chosen 8-bit code page (default
//! Windows-1250, like XGW Next).

#![cfg_attr(target_arch = "wasm32", allow(dead_code))]

use eframe::egui::{self, Color32};
use xgworks_core::listev::TimeMap;
use xgworks_core::{EditTrack, Event};

pub const LYRIC: u8 = 0x05;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Enc {
    Cp1250,
    Cp1252,
    Utf8,
}

impl Enc {
    pub const ALL: [(Enc, &'static str); 3] = [(Enc::Cp1250, "Windows-1250 (Central European)"), (Enc::Cp1252, "Windows-1252 (Western)"), (Enc::Utf8, "UTF-8")];

    fn high(self) -> Option<&'static [u16; 128]> {
        match self {
            Enc::Cp1250 => Some(&CP1250),
            Enc::Cp1252 => Some(&CP1252),
            Enc::Utf8 => None,
        }
    }

    pub fn decode(self, b: &[u8]) -> String {
        match self.high() {
            None => String::from_utf8_lossy(b).into_owned(),
            Some(t) => b.iter().map(|&c| if c < 0x80 { c as char } else { char::from_u32(t[c as usize - 0x80] as u32).unwrap_or('?') }).collect(),
        }
    }

    pub fn encode(self, s: &str) -> Vec<u8> {
        match self.high() {
            None => s.as_bytes().to_vec(),
            Some(t) => s
                .chars()
                .map(|ch| {
                    let u = ch as u32;
                    if u < 0x80 {
                        u as u8
                    } else {
                        t.iter().position(|&x| x as u32 == u).map_or(b'?', |i| 0x80 + i as u8)
                    }
                })
                .collect(),
        }
    }
}

#[rustfmt::skip]
const CP1250: [u16; 128] = [
    0x20AC, 0x81, 0x201A, 0x83, 0x201E, 0x2026, 0x2020, 0x2021, 0x88, 0x2030, 0x0160, 0x2039, 0x015A, 0x0164, 0x017D, 0x0179,
    0x90, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x98, 0x2122, 0x0161, 0x203A, 0x015B, 0x0165, 0x017E, 0x017A,
    0x00A0, 0x02C7, 0x02D8, 0x0141, 0x00A4, 0x0104, 0x00A6, 0x00A7, 0x00A8, 0x00A9, 0x015E, 0x00AB, 0x00AC, 0x00AD, 0x00AE, 0x017B,
    0x00B0, 0x00B1, 0x02DB, 0x0142, 0x00B4, 0x00B5, 0x00B6, 0x00B7, 0x00B8, 0x0105, 0x015F, 0x00BB, 0x013D, 0x02DD, 0x013E, 0x017C,
    0x0154, 0x00C1, 0x00C2, 0x0102, 0x00C4, 0x0139, 0x0106, 0x00C7, 0x010C, 0x00C9, 0x0118, 0x00CB, 0x011A, 0x00CD, 0x00CE, 0x010E,
    0x0110, 0x0143, 0x0147, 0x00D3, 0x00D4, 0x0150, 0x00D6, 0x00D7, 0x0158, 0x016E, 0x00DA, 0x0170, 0x00DC, 0x00DD, 0x0162, 0x00DF,
    0x0155, 0x00E1, 0x00E2, 0x0103, 0x00E4, 0x013A, 0x0107, 0x00E7, 0x010D, 0x00E9, 0x0119, 0x00EB, 0x011B, 0x00ED, 0x00EE, 0x010F,
    0x0111, 0x0144, 0x0148, 0x00F3, 0x00F4, 0x0151, 0x00F6, 0x00F7, 0x0159, 0x016F, 0x00FA, 0x0171, 0x00FC, 0x00FD, 0x0163, 0x02D9,
];

#[rustfmt::skip]
const CP1252: [u16; 128] = {
    let mut t = [0u16; 128];
    let lo: [u16; 32] = [
        0x20AC, 0x81, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160, 0x2039, 0x0152, 0x8D, 0x017D, 0x8F,
        0x90, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022, 0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x9D, 0x017E, 0x0178,
    ];
    let mut i = 0;
    while i < 128 {
        t[i] = if i < 32 { lo[i] } else { 0x80 + i as u16 };
        i += 1;
    }
    t
};

/// (tick, index into `others`, raw text) of the track's lyric events, by time.
pub fn lyrics(t: &EditTrack) -> Vec<(u32, usize, Vec<u8>)> {
    let mut v: Vec<(u32, usize, Vec<u8>)> = t
        .others
        .iter()
        .enumerate()
        .filter_map(|(i, e)| match e {
            Event::Meta { t, mtype: LYRIC, data } => Some((*t, i, data.clone())),
            _ => None,
        })
        .collect();
    v.sort_by_key(|x| (x.0, x.1));
    v
}

/// Typed text → syllables with karaoke marks: words split at spaces,
/// syllables at "-", a new line starts with "/", an empty line = new page "<".
pub fn syllables(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut mark = "";
    for line in text.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        if words.is_empty() {
            if !out.is_empty() {
                mark = "<";
            }
            continue;
        }
        if !out.is_empty() && mark.is_empty() {
            mark = "/";
        }
        for w in words {
            let parts: Vec<&str> = w.split('-').filter(|p| !p.is_empty()).collect();
            for (k, p) in parts.iter().enumerate() {
                let end = if k + 1 == parts.len() { " " } else { "" };
                out.push(format!("{mark}{p}{end}"));
                mark = "";
            }
        }
    }
    out
}

/// Note start ticks of `melody` from `from` on (one per chord).
pub fn note_starts(melody: &EditTrack, from: u32) -> Vec<u32> {
    let mut v: Vec<u32> = melody.notes.iter().map(|n| n.tick).filter(|&t| t >= from).collect();
    v.sort();
    v.dedup();
    v
}

/// Put `syls` on the note starts; `replace` removes the lyrics already in
/// that range first. Returns how many were placed.
pub fn assign(track: &mut EditTrack, starts: &[u32], syls: &[String], enc: Enc, replace: bool) -> usize {
    let n = syls.len().min(starts.len());
    if n == 0 {
        return 0;
    }
    if replace {
        let (a, b) = (starts[0], starts[n - 1]);
        track.others.retain(|e| !matches!(e, Event::Meta { t, mtype: LYRIC, .. } if *t >= a && *t <= b));
    }
    for k in 0..n {
        track.others.push(Event::Meta { t: starts[k], mtype: LYRIC, data: enc.encode(&syls[k]) });
    }
    n
}

/// One karaoke page around `tick`: lines of (syllable, already sung).
fn page_at(items: &[(u32, String)], tick: u32) -> Vec<Vec<(String, bool)>> {
    if items.is_empty() {
        return Vec::new();
    }
    let cur = items.iter().rposition(|x| x.0 <= tick).unwrap_or(0);
    let is_page = |s: &str| s.starts_with('<') || s.starts_with('\\');
    let start = (0..=cur).rev().find(|&i| is_page(&items[i].1)).unwrap_or(0);
    let end = (cur + 1..items.len()).find(|&i| is_page(&items[i].1)).unwrap_or(items.len());
    let mut lines: Vec<Vec<(String, bool)>> = vec![Vec::new()];
    for (i, (t, s)) in items.iter().enumerate().take(end).skip(start) {
        let mut text = s.as_str();
        if i > start && text.starts_with('/') {
            lines.push(Vec::new());
        }
        text = text.trim_start_matches(['/', '<', '\\']);
        lines.last_mut().unwrap().push((text.to_string(), *t <= tick));
    }
    lines
}

pub enum LyricAction {
    /// The song was changed (after a snapshot for undo).
    Edit(Box<dyn FnOnce(&mut [EditTrack])>),
    Locate(u32),
    Status(String),
}

pub struct LyricsState {
    pub track: usize,
    enc: Enc,
    sel: Option<usize>,
    edit_text: String,
    melody: usize,
    from_bar: u32,
    replace: bool,
    typed: String,
}

impl LyricsState {
    pub fn new(track: usize) -> Self {
        LyricsState { track, enc: Enc::Cp1250, sel: None, edit_text: String::new(), melody: track, from_bar: 1, replace: true, typed: String::new() }
    }

    /// `visible` = track indices shown (display number = position + 1).
    #[allow(clippy::too_many_arguments)]
    pub fn ui(&mut self, ui: &mut egui::Ui, tracks: &[EditTrack], visible: &[usize], tm: &TimeMap, song_pos: u32, playing: bool, master: usize, file_name: &str) -> Vec<LyricAction> {
        let mut out = Vec::new();
        if tracks.is_empty() {
            ui.label("Open a song first.");
            return out;
        }
        self.track = self.track.min(tracks.len() - 1);
        let label = |i: usize| {
            let num = visible.iter().position(|&v| v == i).map_or(0, |k| k + 1);
            let has = tracks[i].others.iter().any(|e| matches!(e, Event::Meta { mtype: LYRIC, .. }));
            format!("{num} {}{}", tracks[i].name, if has { "  ♪" } else { "" })
        };
        ui.horizontal(|ui| {
            ui.label("Lyrics track");
            egui::ComboBox::from_id_source("lyr_trk").width(200.0).selected_text(label(self.track)).show_ui(ui, |ui| {
                for &i in visible {
                    if ui.selectable_label(i == self.track, label(i)).clicked() {
                        self.track = i;
                        self.sel = None;
                    }
                }
            });
            ui.label("Text encoding");
            egui::ComboBox::from_id_source("lyr_enc").selected_text(Enc::ALL.iter().find(|e| e.0 == self.enc).map_or("", |e| e.1)).show_ui(ui, |ui| {
                for (e, n) in Enc::ALL {
                    ui.selectable_value(&mut self.enc, e, n);
                }
            });
        });
        let items = lyrics(&tracks[self.track]);
        let texts: Vec<(u32, String)> = items.iter().map(|(t, _, d)| (*t, self.enc.decode(d))).collect();
        let ti = self.track;

        ui.columns(2, |cols| {
            // ---- list
            let ui = &mut cols[0];
            ui.horizontal(|ui| {
                if ui.button("Insert at song position").clicked() {
                    let (enc, txt) = (self.enc, if self.edit_text.is_empty() { "la ".to_string() } else { from_shown(&self.edit_text) });
                    out.push(LyricAction::Edit(Box::new(move |tr| tr[ti].others.push(Event::Meta { t: song_pos, mtype: LYRIC, data: enc.encode(&txt) }))));
                }
                let sel = self.sel.and_then(|k| items.get(k).map(|x| x.1));
                if ui.add_enabled(sel.is_some(), egui::Button::new("Move to song position")).clicked() {
                    let idx = sel.unwrap();
                    out.push(LyricAction::Edit(Box::new(move |tr| {
                        if let Some(Event::Meta { t, .. }) = tr[ti].others.get_mut(idx) {
                            *t = song_pos;
                        }
                    })));
                }
                if ui.add_enabled(sel.is_some(), egui::Button::new("Delete")).clicked() {
                    let idx = sel.unwrap();
                    out.push(LyricAction::Edit(Box::new(move |tr| {
                        tr[ti].others.remove(idx);
                    })));
                    self.sel = None;
                }
            });
            ui.horizontal(|ui| {
                ui.label("Text");
                let r = ui.add(egui::TextEdit::singleline(&mut self.edit_text).desired_width(180.0).hint_text("/ = new line, < = new page"));
                if r.changed() {
                    self.edit_text = to_shown(&from_shown(&self.edit_text)); // a typed space shows as ·
                }
                let sel = self.sel.and_then(|k| items.get(k).map(|x| x.1));
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if ui.add_enabled(sel.is_some(), egui::Button::new("Apply")).clicked() || (enter && sel.is_some()) {
                    let (idx, enc, txt) = (sel.unwrap(), self.enc, from_shown(&self.edit_text));
                    out.push(LyricAction::Edit(Box::new(move |tr| {
                        if let Some(Event::Meta { data, .. }) = tr[ti].others.get_mut(idx) {
                            *data = enc.encode(&txt);
                        }
                    })));
                }
            });
            #[cfg(target_arch = "wasm32")]
            let _ = (master, file_name);
            #[cfg(not(target_arch = "wasm32"))]
            ui.horizontal(|ui| {
                {
                    if ui.button("Export .lyrics…").on_hover_text("Lyrics (this track) and XF chords as text with bar:beat:tick (Midimusic Chord and Lyrics Protocol, Windows-1250)").clicked() {
                        let text = export_text(tracks, ti, tm, self.enc, file_name);
                        let stem = std::path::Path::new(file_name).file_stem().and_then(|x| x.to_str()).unwrap_or("song").to_string();
                        let mut dlg = rfd::FileDialog::new().set_file_name(format!("{stem}.lyrics")).add_filter("Lyrics", &["lyrics", "txt"]);
                        if let Some(d) = crate::platform::app_sub("Songs") {
                            dlg = dlg.set_directory(d);
                        }
                        if let Some(path) = dlg.save_file() {
                            out.push(LyricAction::Status(match std::fs::write(&path, Enc::Cp1250.encode(&text)) {
                                Ok(_) => format!("Exported to {}.", path.display()),
                                Err(e) => format!("Export failed: {e}"),
                            }));
                        }
                    }
                    if ui.button("Import .lyrics…").on_hover_text("Replace this track's lyrics (and the XF chords, when the file has Chd lines) from an edited .lyrics / .txt file").clicked() {
                        let mut dlg = rfd::FileDialog::new().add_filter("Lyrics", &["lyrics", "txt"]);
                        if let Some(d) = crate::platform::app_sub("Songs") {
                            dlg = dlg.set_directory(d);
                        }
                        if let Some(path) = dlg.pick_file() {
                            match std::fs::read(&path) {
                                Ok(bytes) => {
                                    let (lyr, chd, bad) = parse_text(&Enc::Cp1250.decode(&bytes), tm);
                                    if lyr.is_empty() && chd.is_empty() {
                                        out.push(LyricAction::Status("No Lyr / Chd lines in that file.".into()));
                                    } else {
                                        let (nl, nc, enc) = (lyr.len(), chd.len(), self.enc);
                                        out.push(LyricAction::Edit(Box::new(move |tr| apply_import(tr, ti, master, &lyr, &chd, enc))));
                                        out.push(LyricAction::Status(format!(
                                            "Imported {nl} lyrics{}{} (undo with Ctrl+Z).",
                                            if nc > 0 { format!(" and {nc} chords") } else { String::new() },
                                            if bad > 0 { format!("; {bad} lines not understood") } else { String::new() }
                                        )));
                                    }
                                }
                                Err(e) => out.push(LyricAction::Status(format!("Import failed: {e}"))),
                            }
                        }
                    }
                }
            });
            ui.label(egui::RichText::new(format!("{} lyric events · click = select · double-click = song position · · = space", items.len())).small());
            egui::ScrollArea::vertical().id_source("lyr_list").max_height(ui.available_height() - 4.0).show(ui, |ui| {
                egui::Grid::new("lyr_grid").striped(true).num_columns(2).show(ui, |ui| {
                    for (k, (t, s)) in texts.iter().enumerate() {
                        let (m, b, c) = tm.to_mbc(*t);
                        let pos = format!("{m:04}:{b}:{c:03}");
                        let r1 = ui.selectable_label(self.sel == Some(k), egui::RichText::new(pos).monospace());
                        let r2 = ui.selectable_label(self.sel == Some(k), shown_job(s, ui.visuals().text_color()));
                        let r = r1.union(r2);
                        if r.clicked() {
                            self.sel = Some(k);
                            self.edit_text = to_shown(s);
                        }
                        if r.double_clicked() {
                            out.push(LyricAction::Locate(*t));
                        }
                        ui.end_row();
                    }
                });
            });

            // ---- karaoke preview + assign
            let ui = &mut cols[1];
            ui.label(egui::RichText::new("Karaoke preview").strong());
            let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 150.0), egui::Sense::hover());
            let p = ui.painter_at(rect);
            p.rect_filled(rect, 4.0, Color32::from_rgb(16, 20, 40));
            let mut y = rect.top() + 14.0;
            let page = page_at(&texts, if playing || song_pos > 0 { song_pos } else { 0 });
            if page.is_empty() {
                p.text(rect.left_top() + egui::vec2(12.0, 20.0), egui::Align2::LEFT_CENTER, "No lyrics on this track.", egui::FontId::proportional(14.0), Color32::from_gray(150));
            }
            for line in page.iter().take(5) {
                let mut x = rect.left() + 12.0;
                for (s, sung) in line {
                    let col = if *sung { Color32::from_rgb(255, 200, 40) } else { Color32::WHITE };
                    let r = p.text(egui::pos2(x, y), egui::Align2::LEFT_TOP, s, egui::FontId::proportional(20.0), col);
                    x = r.right();
                }
                y += 26.0;
            }
            ui.add_space(6.0);
            ui.separator();
            ui.label(egui::RichText::new("Assign syllables to notes").strong());
            ui.label(egui::RichText::new("Type the words; split syllables with \"-\" (Hap-py birth-day). A new text line = new karaoke line, an empty line = new page.").small());
            ui.add(egui::TextEdit::multiline(&mut self.typed).desired_rows(5).desired_width(f32::INFINITY));
            ui.horizontal(|ui| {
                ui.label("Melody notes from");
                egui::ComboBox::from_id_source("lyr_mel").width(150.0).selected_text(label(self.melody.min(tracks.len() - 1))).show_ui(ui, |ui| {
                    for &i in visible {
                        ui.selectable_value(&mut self.melody, i, label(i));
                    }
                });
                ui.label("from bar");
                ui.add(egui::DragValue::new(&mut self.from_bar).range(1..=9999));
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.replace, "Replace lyrics in that range");
                if ui.button("Assign").clicked() {
                    let syls = syllables(&self.typed);
                    let starts = note_starts(&tracks[self.melody.min(tracks.len() - 1)], tm.from_mbc(self.from_bar, 1, 0));
                    if syls.is_empty() {
                        out.push(LyricAction::Status("Type the lyrics first.".into()));
                    } else if starts.is_empty() {
                        out.push(LyricAction::Status("The melody track has no notes from that bar.".into()));
                    } else {
                        let (enc, replace) = (self.enc, self.replace);
                        let n = syls.len().min(starts.len());
                        let rest = syls.len() - n;
                        out.push(LyricAction::Edit(Box::new(move |tr| {
                            assign(&mut tr[ti], &starts, &syls, enc, replace);
                        })));
                        out.push(LyricAction::Status(if rest > 0 {
                            format!("{n} syllables placed; {rest} left over (not enough notes).")
                        } else {
                            format!("{n} syllables placed.")
                        }));
                    }
                }
                if ui.button("Load current lyrics").clicked() {
                    self.typed = to_typed(&texts);
                }
            });
        });
        out
    }
}

/// Spaces made visible for editing: "·" (not stored, the karaoke shows spaces).
pub fn to_shown(s: &str) -> String {
    s.replace(' ', "\u{00B7}")
}

pub fn from_shown(s: &str) -> String {
    s.replace('\u{00B7}', " ")
}

/// A lyric for the list: in quotes, spaces as small grey dots.
fn shown_job(s: &str, col: Color32) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let fmt = |c: Color32| egui::TextFormat { color: c, ..Default::default() };
    job.append("\"", 0.0, fmt(Color32::GRAY));
    for ch in s.chars() {
        if ch == ' ' {
            job.append("\u{00B7}", 0.0, fmt(Color32::from_gray(150)));
        } else {
            job.append(&ch.to_string(), 0.0, fmt(col));
        }
    }
    job.append("\"", 0.0, fmt(Color32::GRAY));
    job
}

// ---------------------------------------------------------------------------
// Text export / import: "Midimusic Chord and Lyrics Protocol" (as the
// user's midi-file-editor): header, then one line per lyric / chord,
//   Lyr<TAB>bbb:bb:ttt<TAB>"text"      Chd<TAB>bbb:bb:ttt<TAB>Am7/D
// between [start chords+lyrics] and [end list], Windows-1250, CRLF.
// ---------------------------------------------------------------------------

const XF_TYPES: [&str; 34] = [
    "", "6", "maj7", "maj7(#11)", "add9", "maj7(9)", "6(9)", "aug", "m", "m6", "m7", "m7(b5)", "m(9)", "m7(9)", "m7(11)", "mMaj7",
    "mMaj7(9)", "dim", "dim7", "7", "7sus4", "7(b5)", "7(9)", "7(#11)", "7(13)", "7(b9)", "7(b13)", "7(#9)", "maj7aug", "7aug", "1+8",
    "1+5", "sus4", "1+2+5",
];
const LETTERS: [char; 7] = ['C', 'D', 'E', 'F', 'G', 'A', 'B'];
const ACC: [&str; 7] = ["bbb", "bb", "b", "", "#", "##", "###"];

fn xf_root(b: u8) -> Option<String> {
    let (n, f) = ((b & 0x0F) as usize, ((b >> 4) & 7) as usize);
    (b != 0x7F && (1..=7).contains(&n) && f <= 6).then(|| format!("{}{}", LETTERS[n - 1], ACC[f]))
}

fn xf_root_byte(name: &str) -> Option<u8> {
    let mut c = name.chars();
    let l = c.next()?.to_ascii_uppercase();
    let n = LETTERS.iter().position(|&x| x == l)? as u8 + 1;
    let acc: String = c.collect();
    let f = match acc.as_str() {
        "" => 3,
        "#" => 4,
        "##" => 5,
        "b" => 2,
        "bb" => 1,
        _ => return None,
    };
    Some(f << 4 | n)
}

/// "Am7/D" of an XF chord meta (FF 7F 43 7B 01 cr ct bn bt).
pub fn xf_chord_name(d: &[u8]) -> Option<String> {
    if d.len() != 7 || d[..3] != [0x43, 0x7B, 0x01] {
        return None;
    }
    if d[3] == 0x7F {
        return Some("N.C.".into());
    }
    let mut s = xf_root(d[3])? + XF_TYPES.get(d[4] as usize).copied().unwrap_or("");
    if d[5] != 0x7F {
        s += &format!("/{}", xf_root(d[5])?);
    }
    Some(s)
}

/// Chord name → XF chord data (7 bytes after FF 7F len).
pub fn xf_chord_data(name: &str) -> Option<Vec<u8>> {
    let t = name.trim();
    if t.eq_ignore_ascii_case("N.C.") || t.eq_ignore_ascii_case("NC") {
        return Some(vec![0x43, 0x7B, 0x01, 0x7F, 0, 0x7F, 0x7F]);
    }
    let (main, bass) = match t.split_once('/') {
        Some((m, b)) => (m, Some(b)),
        None => (t, None),
    };
    let rl = main.chars().take(1).count() + main.chars().nth(1).filter(|c| *c == '#' || *c == 'b').map_or(0, |_| 1);
    let root: String = main.chars().take(rl).collect();
    let cr = xf_root_byte(&root)?;
    let suffix: String = main.chars().skip(rl).collect();
    let ct = XF_TYPES.iter().position(|x| *x == suffix)? as u8;
    let (bn, bt) = match bass {
        Some(b) => (xf_root_byte(b.trim())?, 0),
        None => (0x7F, 0x7F),
    };
    Some(vec![0x43, 0x7B, 0x01, cr, ct, bn, bt])
}

fn pos3(tm: &TimeMap, t: u32) -> String {
    let (m, b, c) = tm.to_mbc(t);
    format!("{m:03}:{b:02}:{c:03}")
}

/// The export text (lyrics of `track`, XF chords of the whole song).
pub fn export_text(tracks: &[EditTrack], track: usize, tm: &TimeMap, enc: Enc, file: &str) -> String {
    let mut rows: Vec<(u32, String)> = lyrics(&tracks[track])
        .into_iter()
        .map(|(t, _, d)| (t, format!("Lyr\t{}\t\"{}\"", pos3(tm, t), enc.decode(&d).replace(['\r', '\n'], " "))))
        .collect();
    for tr in tracks {
        for e in &tr.others {
            if let Event::Meta { t, mtype: 0x7F, data } = e {
                if let Some(c) = xf_chord_name(data) {
                    rows.push((*t, format!("Chd\t{}\t{c}", pos3(tm, *t))));
                }
            }
        }
    }
    rows.sort_by_key(|r| r.0);
    let mut out = vec!["Midimusic Chord and Lyrics Protocol".to_string(), format!("File:  {file}"), String::new(), "[start chords+lyrics]".into()];
    out.extend(rows.into_iter().map(|r| r.1));
    out.push("[end list]".into());
    out.push(String::new());
    out.join("\r\n")
}

/// (lyrics (tick, text), chords (tick, xf data), lines not understood).
pub fn parse_text(text: &str, tm: &TimeMap) -> (Vec<(u32, String)>, Vec<(u32, Vec<u8>)>, usize) {
    let (mut lyr, mut chd, mut bad) = (Vec::new(), Vec::new(), 0);
    let pos = |p: &str| -> Option<u32> {
        let v: Vec<u32> = p.split(':').map(|x| x.trim().parse().ok()).collect::<Option<Vec<_>>>()?;
        (v.len() == 3).then(|| tm.from_mbc(v[0], v[1], v[2]))
    };
    for line in text.lines() {
        let parts: Vec<&str> = line.splitn(3, '\t').collect();
        if parts.len() != 3 {
            continue;
        }
        match (parts[0], pos(parts[1])) {
            ("Lyr", Some(t)) => {
                let s = parts[2].trim_end();
                let s = s.strip_prefix('"').and_then(|x| x.strip_suffix('"')).unwrap_or(s);
                lyr.push((t, s.to_string()));
            }
            ("Chd", Some(t)) => match xf_chord_data(parts[2]) {
                Some(d) => chd.push((t, d)),
                None => bad += 1,
            },
            ("Lyr", None) | ("Chd", None) => bad += 1,
            _ => {}
        }
    }
    (lyr, chd, bad)
}

/// Replace the lyrics of `track` (and, when given, the song's XF chords, put on `master`).
pub fn apply_import(tr: &mut [EditTrack], track: usize, master: usize, lyr: &[(u32, String)], chd: &[(u32, Vec<u8>)], enc: Enc) {
    if !lyr.is_empty() {
        tr[track].others.retain(|e| !matches!(e, Event::Meta { mtype: LYRIC, .. }));
        for (t, s) in lyr {
            tr[track].others.push(Event::Meta { t: *t, mtype: LYRIC, data: enc.encode(s) });
        }
    }
    if !chd.is_empty() {
        for t in tr.iter_mut() {
            t.others.retain(|e| !matches!(e, Event::Meta { mtype: 0x7F, data, .. } if xf_chord_name(data).is_some()));
        }
        for (t, d) in chd {
            tr[master].others.push(Event::Meta { t: *t, mtype: 0x7F, data: d.clone() });
        }
    }
}

/// Lyric events back to the typed form (for re-assigning).
fn to_typed(items: &[(u32, String)]) -> String {
    let mut s = String::new();
    for (_, t) in items {
        let mut t = t.as_str();
        if t.starts_with('<') || t.starts_with('\\') {
            s.push_str("\n\n");
            t = &t[1..];
        } else if let Some(r) = t.strip_prefix('/') {
            s.push('\n');
            t = r;
        }
        if t.ends_with(' ') {
            s.push_str(t);
        } else {
            s.push_str(t);
            s.push('-');
        }
    }
    s.trim().replace("- ", " ").replace("-\n", "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn syllables_and_marks() {
        let s = syllables("Hap-py birth-day\nto you\n\nNext page");
        assert_eq!(s, vec!["Hap", "py ", "birth", "day ", "/to ", "you ", "<Next ", "page "]);
    }

    #[test]
    fn text_export_import_round_trip() {
        let tm = TimeMap::new(&[], 480);
        let mut tr = vec![EditTrack::default(), EditTrack::default()];
        tr[1].others.push(Event::Meta { t: 480, mtype: LYRIC, data: Enc::Cp1250.encode("Žlu") });
        tr[1].others.push(Event::Meta { t: 960, mtype: LYRIC, data: Enc::Cp1250.encode("ťou ") });
        tr[0].others.push(Event::Meta { t: 0, mtype: 0x7F, data: xf_chord_data("Am7/D").unwrap() });
        let text = export_text(&tr, 1, &tm, Enc::Cp1250, "x.mid");
        assert!(text.contains("Lyr\t001:02:000\t\"Žlu\""), "{text}");
        assert!(text.contains("Chd\t001:01:000\tAm7/D"));
        let (l, c, bad) = parse_text(&text.replace("Žlu", "Žlu-"), &tm);
        assert_eq!((l.len(), c.len(), bad), (2, 1, 0));
        let mut tr2 = tr.clone();
        apply_import(&mut tr2, 1, 0, &l, &c, Enc::Cp1250);
        assert_eq!(Enc::Cp1250.decode(&lyrics(&tr2[1])[0].2), "Žlu-");
        assert_eq!(xf_chord_name(&xf_chord_data("F#m7(b5)").unwrap()).unwrap(), "F#m7(b5)");
        assert_eq!(from_shown(&to_shown("a b ")), "a b ");
    }

    #[test]
    fn code_pages_round_trip() {
        let t = "Žluťoučký kůň úpěl ďábelské ódy";
        assert_eq!(Enc::Cp1250.decode(&Enc::Cp1250.encode(t)), t);
        assert_eq!(Enc::Cp1250.encode("č")[0], 0xE8);
        assert_eq!(Enc::Cp1252.decode(&[0x80, 0xE9]), "€é");
    }

    #[test]
    fn assign_to_note_starts() {
        let mut mel = EditTrack::default();
        for (k, t) in [0u32, 480, 480, 960].iter().enumerate() {
            mel.notes.push(xgworks_core::EditNote { tick: *t, dur: 100, pitch: 60 + k as u8, vel: 100 });
        }
        let starts = note_starts(&mel, 0);
        assert_eq!(starts, vec![0, 480, 960]);
        let mut tr = EditTrack::default();
        let n = assign(&mut tr, &starts, &syllables("la la la la"), Enc::Cp1250, true);
        assert_eq!(n, 3);
        assert_eq!(lyrics(&tr).len(), 3);
        let page = page_at(&[(0, "a ".into()), (10, "/b ".into()), (20, "<c ".into())], 12);
        assert_eq!(page.len(), 2);
        assert!(page[1][0].1);
    }
}
