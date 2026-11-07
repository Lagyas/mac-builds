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
/// The text without diacritics (č → c, ů → u, ß → ss …) for displays and
/// keyboards that show only plain letters.
pub fn strip_diacritics(s: &str) -> String {
    const FROM: &str = "áàâäãåāăąčćçĉďđéèêëěēėęíìîïīįľĺłňńñóòôöõőøōřŕšśşșťţțúùûüůűūųýÿžźż\
ÁÀÂÄÃÅĀĂĄČĆÇĈĎĐÉÈÊËĚĒĖĘÍÌÎÏĪĮĽĹŁŇŃÑÓÒÔÖÕŐØŌŘŔŠŚŞȘŤŢȚÚÙÛÜŮŰŪŲÝŸŽŹŻ";
    const TO: &str = "aaaaaaaaaccccddeeeeeeeeiiiiiilllnnnoooooooorrsssstttuuuuuuuuyyzzz\
AAAAAAAAACCCCDDEEEEEEEEIIIIIILLLNNNOOOOOOOORRSSSSTTTUUUUUUUUYYZZZ";
    let map: Vec<(char, char)> = FROM.chars().zip(TO.chars()).collect();
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            'ß' => out.push_str("ss"),
            'Æ' => out.push_str("AE"),
            'æ' => out.push_str("ae"),
            'Œ' => out.push_str("OE"),
            'œ' => out.push_str("oe"),
            _ => out.push(map.iter().find(|m| m.0 == c).map_or(c, |m| m.1)),
        }
    }
    out
}

/// Re-encode every lyric and text event from `from` (as read now) to `to`;
/// a letter `to` cannot hold loses its diacritics (ů → u), never "?".
/// Returns how many events changed.
pub fn convert_song_lyrics(tracks: &mut [EditTrack], from: Enc, to: Enc) -> usize {
    let mut n = 0;
    for t in tracks.iter_mut() {
        for e in t.others.iter_mut() {
            if let Event::Meta { mtype: LYRIC | 0x01, data, .. } = e {
                let s = from.decode(data);
                let mut out = Vec::new();
                for ch in s.chars() {
                    let one = ch.to_string();
                    let b = to.encode(&one);
                    if b == b"?" && ch != '?' {
                        out.extend(to.encode(&strip_diacritics(&one)));
                    } else {
                        out.extend(b);
                    }
                }
                if out != *data {
                    *data = out;
                    n += 1;
                }
            }
        }
    }
    n
}

/// True when bytes with letters above ASCII are all valid UTF-8 (lyrics
/// written as UTF-8; Windows-1250 text is almost never valid UTF-8).
pub fn looks_utf8<'a>(texts: impl IntoIterator<Item = &'a [u8]>) -> bool {
    let mut high = false;
    for b in texts {
        if b.iter().any(|&c| c >= 0x80) {
            if std::str::from_utf8(b).is_err() {
                return false;
            }
            high = true;
        }
    }
    high
}

/// A text file's encoding: UTF-8 (with or without BOM) when it is valid
/// UTF-8 with letters above ASCII, else Windows-1250.
pub fn decode_file(bytes: &[u8]) -> String {
    let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(bytes);
    if b.len() != bytes.len() || looks_utf8([b]) {
        String::from_utf8_lossy(b).into_owned()
    } else {
        Enc::Cp1250.decode(b)
    }
}

/// Remove the diacritics of every lyric (and text event, as .KAR files keep
/// their lyrics there) in the song; returns how many events changed.
pub fn strip_song_diacritics(tracks: &mut [EditTrack], enc: Enc) -> usize {
    let mut n = 0;
    for t in tracks.iter_mut() {
        for e in t.others.iter_mut() {
            if let Event::Meta { mtype: LYRIC | 0x01, data, .. } = e {
                let s = enc.decode(data);
                let plain = strip_diacritics(&s);
                if plain != s {
                    *data = enc.encode(&plain);
                    n += 1;
                }
            }
        }
    }
    n
}

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
/// Line / page marks of one syllable: (starts a new line, starts a new
/// page, ends its line, ends its page, the text without marks). Understands
/// XF ("/" line, "<" page, at the start) and .kar files ("/" or "\\" at the
/// start, CR / LF anywhere, "/" at the end).
fn marks(s: &str) -> (bool, bool, bool, bool, String) {
    let new_page = s.starts_with('<') || s.starts_with('\\');
    let new_line = s.starts_with('/') || new_page;
    let t = s.trim_start_matches(['/', '<', '\\']);
    let end_page = t.ends_with("\r\n\r\n") || t.ends_with("\n\n");
    let end_line = t.ends_with('\r') || t.ends_with('\n') || t.ends_with('/') || end_page;
    let text: String = t.trim_end_matches(['\r', '\n', '/']).replace(['\r', '\n'], " ");
    (new_line, new_page, end_line, end_page, text)
}

/// One karaoke page around `tick`: lines of (syllable, already sung).
fn page_at(items: &[(u32, String)], tick: u32) -> Vec<Vec<(String, bool)>> {
    if items.is_empty() {
        return Vec::new();
    }
    // all pages, built from the marks
    let mut pages: Vec<Vec<Vec<(u32, String)>>> = vec![vec![Vec::new()]];
    let mut break_line = false;
    let mut break_page = false;
    for (t, raw) in items {
        let (nl, np, el, ep, text) = marks(raw);
        if (np || break_page) && pages.last().is_some_and(|p| p.iter().any(|l| !l.is_empty())) {
            pages.push(vec![Vec::new()]);
        } else if (nl || break_line) && pages.last().and_then(|p| p.last()).is_some_and(|l| !l.is_empty()) {
            pages.last_mut().unwrap().push(Vec::new());
        }
        pages.last_mut().unwrap().last_mut().unwrap().push((*t, text));
        break_line = el;
        break_page = ep;
    }
    // the page that holds the last syllable at / before `tick`
    let page = pages
        .iter()
        .rev()
        .find(|p| p.iter().flatten().any(|(t, _)| *t <= tick))
        .or(pages.first())
        .cloned()
        .unwrap_or_default();
    page.into_iter().filter(|l| !l.is_empty()).map(|l| l.into_iter().map(|(t, s)| (s, t <= tick)).collect()).collect()
}

/// A slider over the whole song (bar:beat shown); returns the new position
/// when it is moved. The Lyrics and Chords previews follow the song position.
pub fn time_line(ui: &mut egui::Ui, tracks: &[EditTrack], tm: &TimeMap, song_pos: u32, id: &str) -> Option<u32> {
    let end = tracks
        .iter()
        .flat_map(|t| t.notes.iter().map(|n| n.tick + n.dur).chain(t.others.iter().map(|e| e.t())))
        .max()
        .unwrap_or(0)
        .max(1);
    let mut v = song_pos.min(end) as f64;
    let mut moved = None;
    ui.horizontal(|ui| {
        ui.label("Position");
        ui.spacing_mut().slider_width = (ui.available_width() - 90.0).max(120.0);
        let r = ui.add(
            egui::Slider::new(&mut v, 0.0..=end as f64)
                .show_value(false)
                .step_by(1.0),
        );
        let (m, b, _) = tm.to_mbc(v as u32);
        ui.monospace(format!("{m:04}:{b}"));
        if r.changed() {
            moved = Some(v as u32);
        }
        let _ = id;
    });
    moved
}

/// Export / Import: what they handle.
pub const TXT_MODES: [&str; 3] = ["Lyrics + chords", "Lyrics", "Chords"];

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
    /// Text size in this window (list, karaoke preview), 0.7 … 2.5.
    font: f32,
    /// What Export / Import handle: 0 lyrics + chords, 1 lyrics, 2 chords.
    txt_mode: u8,
    /// The export file's encoding (Import finds UTF-8 by itself).
    file_enc: Enc,
    /// The file picked for Import (it arrives later on the web).
    pick: crate::platform::PickedFiles,
    /// An import waiting for "replace or add": (lyrics, chords, lines not understood).
    pending: Option<(Vec<(u32, String)>, Vec<(u32, Vec<u8>)>, usize)>,
    /// PSRUTI-style Fill Lyrics and how they go in.
    fill: FillSet,
    fill_method: u8,
}

/// PSRUTI's "Fill Lyrics": short strings on the beats that show the player
/// where the beats are. Defaults as PSRUTI's: measure ".-- ", beat "--- ",
/// end "---", a linefeed 30 ticks after the last beat.
#[derive(Clone, Debug, PartialEq)]
pub struct FillSet {
    pub measure: String,
    pub beat: String,
    pub end: String,
    pub linefeed: Option<u32>,
}

impl Default for FillSet {
    fn default() -> Self {
        FillSet { measure: ".-- ".into(), beat: "--- ".into(), end: "---".into(), linefeed: Some(30) }
    }
}

/// Where Fill Lyrics go (PSRUTI's three methods).
pub const FILL_METHODS: [&str; 3] = ["Not at lyrics", "Not in beats with lyrics", "Not in measures with lyrics"];

/// Insert Fill Lyrics on the beats of `track` up to `song_end`: on beat 1 the
/// measure string, on the last beat the end string, else the beat string
/// (beat and end both empty = beat 1 only), a linefeed after the measure.
/// A song without lyrics gets them from measure 2. Returns how many went in.
pub fn insert_fill(track: &mut EditTrack, tm: &TimeMap, song_end: u32, set: &FillSet, method: u8, enc: Enc) -> usize {
    let had: Vec<u32> = lyrics(track).into_iter().map(|(t, _, _)| t).collect();
    let any_in = |a: u32, b: u32| had.iter().any(|&t| t >= a && t < b);
    let mut m = if had.is_empty() { 2 } else { 1 };
    let mut n = 0;
    let only_one = set.beat.is_empty() && set.end.is_empty();
    loop {
        let ms = tm.from_mbc(m, 1, 0);
        if ms >= song_end {
            break;
        }
        let (bar, beat) = (tm.bar_len(ms).max(1), tm.beat_len(ms).max(1));
        let beats = (bar / beat).max(1);
        let me = ms + bar;
        let mut last = None;
        if !(method == 2 && any_in(ms, me)) {
            for k in 0..beats {
                let t = ms + k * beat;
                let skip = match method {
                    0 => had.contains(&t),
                    _ => any_in(t, t + beat),
                };
                if skip || (only_one && k > 0) {
                    continue;
                }
                let text = if k == 0 {
                    &set.measure
                } else if k + 1 == beats {
                    &set.end
                } else {
                    &set.beat
                };
                if text.is_empty() {
                    continue;
                }
                track.others.push(Event::Meta { t, mtype: LYRIC, data: enc.encode(text) });
                last = Some(ms + (beats - 1) * beat);
                n += 1;
            }
        }
        if let (Some(lb), Some(d)) = (last, set.linefeed) {
            if lb + d < me {
                track.others.push(Event::Meta { t: lb + d, mtype: LYRIC, data: b"\r".to_vec() });
            }
        }
        m += 1;
    }
    n
}

/// Delete Fill Lyrics: lyrics that are exactly one of the set's strings,
/// and a linefeed right after one of them. Returns how many went.
pub fn delete_fill(track: &mut EditTrack, set: &FillSet, enc: Enc) -> usize {
    let fills: Vec<&str> = [set.measure.as_str(), set.beat.as_str(), set.end.as_str()].into_iter().filter(|s| !s.is_empty()).collect();
    let mut lyr: Vec<(u32, usize)> = track
        .others
        .iter()
        .enumerate()
        .filter_map(|(i, e)| if let Event::Meta { t, mtype: LYRIC, .. } = e { Some((*t, i)) } else { None })
        .collect();
    lyr.sort();
    let mut gone = std::collections::HashSet::new();
    let mut prev_fill = false;
    for (_, i) in lyr {
        let Event::Meta { data, .. } = &track.others[i] else { continue };
        let s = enc.decode(data);
        let is_fill = fills.contains(&s.as_str());
        let is_lf = s == "\r";
        if is_fill || (is_lf && prev_fill) {
            gone.insert(i);
        }
        prev_fill = is_fill;
    }
    let mut k = 0;
    track.others.retain(|_| {
        let keep = !gone.contains(&k);
        k += 1;
        keep
    });
    gone.len()
}

impl LyricsState {
    /// The code page the lyrics are read in.
    pub fn enc(&self) -> Enc {
        self.enc
    }

    pub fn new(track: usize) -> Self {
        LyricsState {
            track,
            enc: Enc::Cp1250,
            sel: None,
            edit_text: String::new(),
            melody: track,
            from_bar: 1,
            replace: true,
            typed: String::new(),
            font: 1.0,
            txt_mode: 0,
            file_enc: Enc::Cp1250,
            pick: Default::default(),
            pending: None,
            fill: FillSet::default(),
            fill_method: 2,
        }
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
        ui.horizontal_wrapped(|ui| {
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
            if ui
                .add_enabled(self.enc != Enc::Cp1250, egui::Button::new("Convert to Windows-1250"))
                .on_hover_text("Write every lyric / text event in Windows-1250 (as read in the chosen encoding); letters 1250 lacks lose their diacritics (undo-able)")
                .clicked()
            {
                let from = self.enc;
                self.enc = Enc::Cp1250;
                out.push(LyricAction::Edit(Box::new(move |tr| {
                    convert_song_lyrics(tr, from, Enc::Cp1250);
                })));
                out.push(LyricAction::Status("Lyrics converted to Windows-1250 (undo with Ctrl+Z).".into()));
            }
            ui.separator();
            ui.label("Text size");
            if ui.button(" A− ").clicked() {
                self.font = (self.font - 0.1).max(0.7);
            }
            if ui.button(" A+ ").clicked() {
                self.font = (self.font + 0.1).min(2.5);
            }
            ui.label(format!("{:.0} %", self.font * 100.0));
        });
        let items = lyrics(&tracks[self.track]);
        if self.enc != Enc::Utf8 && looks_utf8(items.iter().map(|i| i.2.as_slice())) {
            ui.horizontal(|ui| {
                ui.colored_label(Color32::from_rgb(0xC0, 0x60, 0x00), "These lyrics look like UTF-8.");
                if ui.button("Read as UTF-8").clicked() {
                    self.enc = Enc::Utf8;
                }
            });
        }
        let texts: Vec<(u32, String)> = items.iter().map(|(t, _, d)| (*t, self.enc.decode(d))).collect();
        let ti = self.track;

        ui.columns(2, |cols| {
            // ---- list
            let ui = &mut cols[0];
            ui.horizontal(|ui| {
                if ui.button("Remove diacritics").on_hover_text("Every lyric and text event of the song without č ř ů … (undo-able)").clicked() {
                    let enc = self.enc;
                    out.push(LyricAction::Edit(Box::new(move |tr| {
                        strip_song_diacritics(tr, enc);
                    })));
                    out.push(LyricAction::Status("Lyrics: diacritics removed.".into()));
                }
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
            // Midimusic Chord and Lyrics Protocol (.txt), as midi-file-editor
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_source("lyr_txt_mode").selected_text(TXT_MODES[self.txt_mode as usize]).width(130.0).show_ui(ui, |ui| {
                    for (k, m) in TXT_MODES.iter().enumerate() {
                        ui.selectable_value(&mut self.txt_mode, k as u8, *m);
                    }
                });
                egui::ComboBox::from_id_source("lyr_file_enc")
                    .selected_text(if self.file_enc == Enc::Utf8 { "UTF-8" } else { "Win-1250" })
                    .width(80.0)
                    .show_ui(ui, |ui| {
                        ui.selectable_value(&mut self.file_enc, Enc::Cp1250, "Win-1250");
                        ui.selectable_value(&mut self.file_enc, Enc::Utf8, "UTF-8");
                    })
                    .response
                    .on_hover_text("The export file's encoding (Import recognises UTF-8 by itself)");
                if ui.button("Export…").on_hover_text("Midimusic Chord and Lyrics Protocol text (Lyr / Chd lines with bar:beat:tick)").clicked() {
                    let text = export_text(tracks, ti, tm, self.enc, file_name, self.txt_mode);
                    let stem = std::path::Path::new(file_name).file_stem().and_then(|x| x.to_str()).unwrap_or("song").to_string();
                    let suffix = ["_lyrics+chords", "_lyrics", "_chords"][self.txt_mode as usize];
                    if let Some(m) = crate::platform::save_song(&format!("{stem}{suffix}.txt"), "txt", "Midimusic lyrics (*.txt)", &self.file_enc.encode(&text)) {
                        out.push(LyricAction::Status(m));
                    }
                }
                if ui.button("Import…").on_hover_text("Lyrics / chords from a Midimusic lyrics text (.txt, .lyrics); then replace the old ones or add").clicked() {
                    crate::platform::pick_files(&self.pick, ui.ctx(), "Midimusic lyrics (*.txt, *.lyrics)", &["txt", "lyrics"]);
                }
                if ui.add_enabled(!items.is_empty(), egui::Button::new("Clear all lyrics")).on_hover_text("Delete every lyric of this track (undo-able)").clicked() {
                    let n = items.len();
                    out.push(LyricAction::Edit(Box::new(move |tr| tr[ti].others.retain(|e| !matches!(e, Event::Meta { mtype: LYRIC, .. })))));
                    out.push(LyricAction::Status(format!("{n} lyrics deleted (undo with Ctrl+Z).")));
                }
            });
            // a picked file: read it, then ask "replace or add" when the song has some already
            if let Some(f) = self.pick.borrow_mut().take().and_then(|v| v.into_iter().next()) {
                match f.read() {
                    Some(bytes) => {
                        let (mut lyr, mut chd, bad) = parse_text(&decode_file(&bytes), tm);
                        if self.txt_mode == 2 {
                            lyr.clear();
                        }
                        if self.txt_mode == 1 {
                            chd.clear();
                        }
                        if lyr.is_empty() && chd.is_empty() {
                            out.push(LyricAction::Status("No Lyr / Chd lines in that file.".into()));
                        } else {
                            self.pending = Some((lyr, chd, bad));
                        }
                    }
                    None => out.push(LyricAction::Status("Import: the file cannot be read.".into())),
                }
            }
            let has_chords = tracks.iter().any(|t| t.others.iter().any(|e| matches!(e, Event::Meta { mtype: 0x7F, data, .. } if xf_chord_name(data).is_some())));
            if let Some((lyr, chd, bad)) = self.pending.clone() {
                let clash = (!lyr.is_empty() && !items.is_empty()) || (!chd.is_empty() && has_chords);
                let mut go = if clash { None } else { Some(true) };
                if clash {
                    ui.horizontal(|ui| {
                        ui.colored_label(Color32::from_rgb(0xC0, 0x60, 0x00), format!("Import {} lyrics / {} chords: the song has some already.", lyr.len(), chd.len()));
                        if ui.button("Replace").clicked() {
                            go = Some(true);
                        }
                        if ui.button("Add").clicked() {
                            go = Some(false);
                        }
                        if ui.button("Cancel").clicked() {
                            self.pending = None;
                        }
                    });
                }
                if let Some(replace) = go {
                    self.pending = None;
                    let (nl, nc, enc) = (lyr.len(), chd.len(), self.enc);
                    out.push(LyricAction::Edit(Box::new(move |tr| apply_import(tr, ti, master, &lyr, &chd, enc, replace))));
                    out.push(LyricAction::Status(format!(
                        "Imported {nl} lyrics and {nc} chords{}{} (undo with Ctrl+Z).",
                        if replace { ", the old ones replaced" } else { ", added" },
                        if bad > 0 { format!("; {bad} lines not understood") } else { String::new() }
                    )));
                }
            }
            ui.label(egui::RichText::new(format!("{} lyric events · click = select + song position · · = space", items.len())).small());
            // a fixed height: filling the free height made the (resizable) window grow
            egui::ScrollArea::vertical().id_source("lyr_list").max_height(340.0).auto_shrink([false, false]).show(ui, |ui| {
                egui::Grid::new("lyr_grid").striped(true).num_columns(2).show(ui, |ui| {
                    for (k, (t, s)) in texts.iter().enumerate() {
                        let (m, b, c) = tm.to_mbc(*t);
                        let pos = format!("{m:04}:{b}:{c:03}");
                        let fs = 13.0 * self.font;
                        let r1 = ui.selectable_label(self.sel == Some(k), egui::RichText::new(pos).monospace().size(fs));
                        let mut job = shown_job(s, ui.visuals().text_color());
                        for sec in &mut job.sections {
                            sec.format.font_id.size = fs;
                        }
                        let r2 = ui.selectable_label(self.sel == Some(k), job);
                        let r = r1.union(r2);
                        if r.clicked() {
                            // the song position (and the preview) follow the clicked lyric
                            self.sel = Some(k);
                            self.edit_text = to_shown(s);
                            out.push(LyricAction::Locate(*t));
                        }
                        ui.end_row();
                    }
                });
            });

            // ---- karaoke preview + assign
            let ui = &mut cols[1];
            ui.label(egui::RichText::new("Karaoke preview").strong());
            let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 170.0), egui::Sense::hover());
            let p = ui.painter_at(rect);
            p.rect_filled(rect, 4.0, Color32::from_rgb(16, 20, 40));
            // the page at the song position (playing or not)
            let _ = playing;
            let page = page_at(&texts, song_pos);
            if page.is_empty() {
                p.text(rect.center(), egui::Align2::CENTER_CENTER, "No lyrics on this track.", egui::FontId::proportional(14.0), Color32::from_gray(150));
            }
            // lines centred; a line wider than the box wraps between syllables
            let font = egui::FontId::proportional(20.0 * self.font);
            let max_w = rect.width() - 24.0;
            let mut rows: Vec<Vec<(std::sync::Arc<egui::Galley>, bool)>> = Vec::new();
            for line in &page {
                let mut row: Vec<(std::sync::Arc<egui::Galley>, bool)> = Vec::new();
                let mut w = 0.0;
                for (s, sung) in line {
                    let g = p.layout_no_wrap(s.clone(), font.clone(), Color32::WHITE);
                    let gw = g.size().x;
                    if w + gw > max_w && !row.is_empty() {
                        rows.push(std::mem::take(&mut row));
                        w = 0.0;
                    }
                    w += gw;
                    row.push((g, *sung));
                }
                rows.push(row);
            }
            let line_h = 27.0 * self.font;
            let shown = rows.len().min(6);
            let mut y = rect.center().y - shown as f32 * line_h / 2.0;
            for row in rows.iter().take(shown) {
                let w: f32 = row.iter().map(|(g, _)| g.size().x).sum();
                let mut x = rect.center().x - w / 2.0;
                for (g, sung) in row {
                    let col = if *sung { Color32::from_rgb(255, 200, 40) } else { Color32::WHITE };
                    p.galley_with_override_text_color(egui::pos2(x, y), g.clone(), col);
                    x += g.size().x;
                }
                y += line_h;
            }
            // a time line to move through the song: the preview shows that page
            if let Some(t) = time_line(ui, tracks, tm, song_pos, "lyr_scrub") {
                out.push(LyricAction::Locate(t));
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
            ui.add_space(6.0);
            ui.separator();
            ui.label(egui::RichText::new("Fill Lyrics (beat marks, as PSRUTI)").strong());
            ui.horizontal(|ui| {
                ui.label("measure");
                ui.add(egui::TextEdit::singleline(&mut self.fill.measure).desired_width(50.0).font(egui::TextStyle::Monospace));
                ui.label("beat");
                ui.add(egui::TextEdit::singleline(&mut self.fill.beat).desired_width(50.0).font(egui::TextStyle::Monospace));
                ui.label("end");
                ui.add(egui::TextEdit::singleline(&mut self.fill.end).desired_width(50.0).font(egui::TextStyle::Monospace));
                let mut lf = self.fill.linefeed.is_some();
                if ui.checkbox(&mut lf, "linefeed").on_hover_text("A linefeed this many ticks after the measure's last beat").changed() {
                    self.fill.linefeed = lf.then_some(self.fill.linefeed.unwrap_or(30));
                }
                if let Some(d) = self.fill.linefeed.as_mut() {
                    ui.add(egui::DragValue::new(d).range(1..=240).suffix(" ticks"));
                }
                if ui.small_button("Default").clicked() {
                    self.fill = FillSet::default();
                }
            });
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_source("lyr_fill_m").selected_text(FILL_METHODS[self.fill_method as usize]).width(180.0).show_ui(ui, |ui| {
                    for (k, m) in FILL_METHODS.iter().enumerate() {
                        ui.selectable_value(&mut self.fill_method, k as u8, *m);
                    }
                });
                if ui.button("Insert Fill Lyrics").on_hover_text("On the beats of this track's song, where the chosen method allows").clicked() {
                    let (set, method, enc, tm) = (self.fill.clone(), self.fill_method, self.enc, tm.clone());
                    let end = tracks.iter().flat_map(|t| t.notes.iter().map(|n| n.tick + n.dur)).max().unwrap_or(0);
                    out.push(LyricAction::Edit(Box::new(move |tr| {
                        insert_fill(&mut tr[ti], &tm, end, &set, method, enc);
                    })));
                    out.push(LyricAction::Status("Fill Lyrics inserted (undo with Ctrl+Z).".into()));
                }
                if ui.button("Delete Fill Lyrics").on_hover_text("Lyrics that are exactly these strings, and their linefeeds").clicked() {
                    let (set, enc) = (self.fill.clone(), self.enc);
                    out.push(LyricAction::Edit(Box::new(move |tr| {
                        delete_fill(&mut tr[ti], &set, enc);
                    })));
                    out.push(LyricAction::Status("Fill Lyrics deleted (undo with Ctrl+Z).".into()));
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
pub fn export_text(tracks: &[EditTrack], track: usize, tm: &TimeMap, enc: Enc, file: &str, mode: u8) -> String {
    let mut rows: Vec<(u32, String)> = if mode == 2 {
        Vec::new()
    } else {
        lyrics(&tracks[track])
            .into_iter()
            .map(|(t, _, d)| (t, format!("Lyr\t{}\t\"{}\"", pos3(tm, t), enc.decode(&d).replace(['\r', '\n'], " "))))
            .collect()
    };
    for tr in tracks.iter().filter(|_| mode != 1) {
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
pub fn apply_import(tr: &mut [EditTrack], track: usize, master: usize, lyr: &[(u32, String)], chd: &[(u32, Vec<u8>)], enc: Enc, replace: bool) {
    if !lyr.is_empty() {
        if replace {
            tr[track].others.retain(|e| !matches!(e, Event::Meta { mtype: LYRIC, .. }));
        }
        for (t, s) in lyr {
            tr[track].others.push(Event::Meta { t: *t, mtype: LYRIC, data: enc.encode(s) });
        }
    }
    if !chd.is_empty() && replace {
        for t in tr.iter_mut() {
            t.others.retain(|e| !matches!(e, Event::Meta { mtype: 0x7F, data, .. } if xf_chord_name(data).is_some()));
        }
    }
    if !chd.is_empty() {
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
    fn fill_lyrics_like_psruti() {
        // 4/4, 480 ppqn: 3 bars of notes, lyrics in bar 2
        let n = |t| xgworks_core::EditNote { tick: t, dur: 100, pitch: 60, vel: 100 };
        let mut t = EditTrack { notes: vec![n(0), n(5760 - 100)], others: vec![Event::Meta { t: 1920 + 480, mtype: LYRIC, data: b"la ".to_vec() }], ..Default::default() };
        let tm = TimeMap::new(std::slice::from_ref(&t), 480);
        let set = FillSet::default();
        // not in measures with lyrics: bars 1 and 3, 4 beats each
        assert_eq!(insert_fill(&mut t, &tm, 5760, &set, 2, Enc::Cp1250), 8);
        let texts: Vec<(u32, String)> = { let mut v: Vec<_> = lyrics(&t).into_iter().map(|(t, _, d)| (t, Enc::Cp1250.decode(&d))).collect(); v.sort(); v };
        assert_eq!(texts[0], (0, ".-- ".to_string()));
        assert_eq!(texts[1], (480, "--- ".to_string()));
        assert_eq!(texts[3], (1440, "---".to_string()));
        assert_eq!(texts[4], (1470, "\r".to_string()), "linefeed 30 ticks after the last beat");
        // deleting leaves the song's own lyric
        assert_eq!(delete_fill(&mut t, &set, Enc::Cp1250), 10);
        assert_eq!(lyrics(&t).len(), 1);
        // not in beats with lyrics: bar 2 gets its free beats too
        assert_eq!(insert_fill(&mut t, &tm, 5760, &set, 1, Enc::Cp1250), 11);
    }

    #[test]
    fn utf8_lyrics_converted() {
        let mut t = vec![EditTrack { others: vec![Event::Meta { t: 0, mtype: LYRIC, data: "Řeka ů € ł ő".as_bytes().to_vec() }], ..Default::default() }];
        assert!(looks_utf8([t[0].others.iter().map(|e| if let Event::Meta { data, .. } = e { data.as_slice() } else { &[] }).next().unwrap()]));
        assert_eq!(convert_song_lyrics(&mut t, Enc::Utf8, Enc::Cp1250), 1);
        let Event::Meta { data, .. } = &t[0].others[0] else { panic!() };
        assert_eq!(Enc::Cp1250.decode(data), "Řeka ů € ł ő");
        assert!(!looks_utf8([data.as_slice()]), "Windows-1250 bytes are not UTF-8");
        // a letter 1252 lacks loses its diacritic, never "?"
        let mut t = vec![EditTrack { others: vec![Event::Meta { t: 0, mtype: LYRIC, data: "č".as_bytes().to_vec() }], ..Default::default() }];
        convert_song_lyrics(&mut t, Enc::Utf8, Enc::Cp1252);
        let Event::Meta { data, .. } = &t[0].others[0] else { panic!() };
        assert_eq!(data, b"c");
        assert_eq!(decode_file("\u{feff}Lyr".as_bytes()), "Lyr");
        assert_eq!(decode_file(&Enc::Cp1250.encode("žluť")), "žluť");
        assert_eq!(decode_file("žluť".as_bytes()), "žluť");
    }

    #[test]
    fn diacritics_removed() {
        assert_eq!(strip_diacritics("Příliš žluťoučký kůň úpěl ďábelské ódy. Straße"), "Prilis zlutoucky kun upel dabelske ody. Strasse");
        let mut t = vec![EditTrack { others: vec![Event::Meta { t: 0, mtype: LYRIC, data: Enc::Cp1250.encode("Řeka ") }], ..Default::default() }];
        assert_eq!(strip_song_diacritics(&mut t, Enc::Cp1250), 1);
        let Event::Meta { data, .. } = &t[0].others[0] else { panic!() };
        assert_eq!(data, b"Reka ");
    }

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
        let text = export_text(&tr, 1, &tm, Enc::Cp1250, "x.mid", 0);
        assert!(!export_text(&tr, 1, &tm, Enc::Cp1250, "x.mid", 1).contains("Chd"));
        assert!(text.contains("Lyr\t001:02:000\t\"Žlu\""), "{text}");
        assert!(text.contains("Chd\t001:01:000\tAm7/D"));
        let (l, c, bad) = parse_text(&text.replace("Žlu", "Žlu-"), &tm);
        assert_eq!((l.len(), c.len(), bad), (2, 1, 0));
        let mut tr2 = tr.clone();
        apply_import(&mut tr2, 1, 0, &l, &c, Enc::Cp1250, true);
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
        // .kar style: CR at the end of a line, "\\" new page
        let kar = [(0, "Ahoj ".to_string()), (5, "sve\r".to_string()), (10, "te ".to_string()), (20, "\\dal".to_string())];
        let p2 = page_at(&kar, 12);
        assert_eq!(p2.len(), 2, "{p2:?}");
        assert_eq!(p2[0][1].0, "sve");
        assert_eq!(page_at(&kar, 25).len(), 1);
    }
}
