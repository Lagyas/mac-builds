//! Sound sets for the multi-standard sound picker (GM, GM2, GS, XG): every
//! program with its bank variations and the drum kits, in the 16 GM
//! categories.
//!
//! Nothing here ships voice data. The names come from the user's
//! installations at run time: GM / GM2 from XGworks' `Melody.ini`,
//! `GM2 Normal.ini` and `GM2 SFX Kit.ini`, XG from the XG Editor's voice list,
//! GS (SC-55 / SC-88 / SC-88Pro maps) from Midifile Optimizer's sound dialog
//! (a Delphi form resource in its EXE) when that is installed.

use crate::xgtbl::{Ini, Voice};

/// The 16 GM instrument families (program / 8).
pub const CATEGORIES: [&str; 16] = [
    "Piano", "Chromatic Perc.", "Organ", "Guitar", "Bass", "Strings", "Ensemble", "Brass",
    "Reed", "Pipe", "Synth Lead", "Synth Pad", "Synth SFX", "Ethnic", "Percussive", "SFX",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Standard {
    Gm,
    Gm2,
    /// GS with the map of an SC-55 (1), SC-88 (2), SC-88Pro (3) or SC-8850 (4).
    Gs(u8),
    Xg,
    /// A Yamaha arranger keyboard (bank / program, GM2 effects): 0 = PSR-SX900.
    Kbd(u8),
}

impl Standard {
    pub const ALL: [Standard; 8] = [Standard::Gm, Standard::Gm2, Standard::Gs(1), Standard::Gs(2), Standard::Gs(3), Standard::Gs(4), Standard::Kbd(0), Standard::Xg];

    pub fn name(self) -> &'static str {
        match self {
            Standard::Gm => "GM",
            Standard::Gm2 => "GM2",
            Standard::Gs(1) => "GS (SC-55)",
            Standard::Gs(2) => "GS (SC-88)",
            Standard::Gs(3) => "GS (SC-88Pro)",
            Standard::Gs(_) => "GS (SC-8850)",
            Standard::Xg => "XG",
            Standard::Kbd(_) => "Yamaha PSR-SX900",
        }
    }
}

/// One selectable sound: Bank Select MSB / LSB and Program Change.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sound {
    pub msb: u8,
    pub lsb: u8,
    pub program: u8,
    pub name: String,
}

#[derive(Clone, Debug, Default)]
pub struct SoundSet {
    /// Per program 0..127: its sounds, the capital tone first.
    pub tones: Vec<Vec<Sound>>,
    pub drums: Vec<Sound>,
    /// The instruments of each drum kit (same order as `drums`): (note, name).
    pub keys: Vec<Vec<(u8, String)>>,
}

impl SoundSet {
    fn empty() -> SoundSet {
        SoundSet { tones: vec![Vec::new(); 128], drums: Vec::new(), keys: Vec::new() }
    }

    /// Where a bank / program is in the set: (program, variation) or the drum kit index.
    pub fn find(&self, msb: u8, lsb: u8, program: u8) -> Option<(usize, usize)> {
        let v = self.tones.get(program as usize)?;
        v.iter().position(|s| s.msb == msb && s.lsb == lsb).map(|k| (program as usize, k))
    }

    pub fn find_drum(&self, msb: u8, lsb: u8, program: u8) -> Option<usize> {
        self.drums
            .iter()
            .position(|s| s.program == program && s.msb == msb && s.lsb == lsb)
            .or_else(|| self.drums.iter().position(|s| s.program == program))
    }

    /// The name for a bank / program (any variation of the program as a fallback).
    pub fn name(&self, msb: u8, lsb: u8, program: u8) -> Option<&str> {
        let v = self.tones.get(program as usize)?;
        v.iter().find(|s| s.msb == msb && s.lsb == lsb).or(v.first()).map(|s| s.name.as_str())
    }
}

/// GM: the 128 programs of XGworks' `Melody.ini` and the standard drum kit.
pub fn gm(melody_ini: &[u8]) -> SoundSet {
    let ini = Ini::parse(melody_ini);
    let mut s = SoundSet::empty();
    for (k, name) in ini.section("Melody").into_iter().flatten() {
        if let Ok(p) = k.parse::<u8>() {
            if p < 128 {
                s.tones[p as usize].push(Sound { msb: 0, lsb: 0, program: p, name: name.clone() });
            }
        }
    }
    s.drums.push(Sound { msb: 0, lsb: 0, program: 0, name: "Standard".into() });
    s
}

/// GM2: Bank MSB 121 with the variations (LSB) of `GM2 Normal.ini`, drum kits
/// (MSB 120) of `GM2 SFX Kit.ini`.
pub fn gm2(normal_ini: &[u8], kit_ini: &[u8]) -> SoundSet {
    let ini = Ini::parse(normal_ini);
    let mut s = SoundSet::empty();
    for (k, name) in ini.section("GM2 Normal").into_iter().flatten() {
        let Ok(p) = k.parse::<u8>() else { continue };
        if p >= 128 {
            continue;
        }
        let vars: Vec<(u8, String)> = ini
            .section(name.trim())
            .into_iter()
            .flatten()
            .filter_map(|(l, n)| Some((l.parse::<u8>().ok()?, n.trim().to_string())))
            .collect();
        if vars.is_empty() {
            s.tones[p as usize].push(Sound { msb: 121, lsb: 0, program: p, name: name.trim().to_string() });
        }
        for (l, n) in vars {
            s.tones[p as usize].push(Sound { msb: 121, lsb: l, program: p, name: n });
        }
    }
    for (k, name) in Ini::parse(kit_ini).section("GM2 SFX Kit").into_iter().flatten() {
        if let Ok(p) = k.parse::<u8>() {
            s.drums.push(Sound { msb: 120, lsb: 0, program: p, name: name.trim().to_string() });
            s.keys.push(ini_keys(kit_ini, name.trim()));
        }
    }
    s
}

/// XG: the XG Editor's voice list (drum kits = MSB 126 / 127).
pub fn xg(voices: &[Voice]) -> SoundSet {
    let mut s = SoundSet::empty();
    for v in voices {
        let snd = Sound { msb: v.msb, lsb: v.lsb, program: v.program, name: v.name.clone() };
        if v.msb >= 126 {
            s.drums.push(snd);
        } else if v.program < 128 {
            s.tones[v.program as usize].push(snd);
        }
    }
    for t in &mut s.tones {
        // the capital tone (bank 0 / 0) first, then by bank
        t.sort_by_key(|v| (v.msb != 0 || v.lsb != 0, v.msb, v.lsb));
    }
    s
}

/// GS (map 1 = SC-55, 2 = SC-88, 3 = SC-88Pro) from Midifile Optimizer's EXE:
/// the `SourceData<map>` / `DrumSet<map>` lists of its SC-88Pro sound
/// dialog. Variations are Bank Select MSB, the map is LSB.
pub fn gs_from_optimizer(exe: &[u8], map: u8) -> Option<SoundSet> {
    // the SC-8850 dialog has the fourth map
    let dfm = pe_rcdata(exe, if map >= 4 { "TSC8850PATCHES" } else { "TSC88PROPATCHES" })?;
    let tones = dfm_strings(dfm, &format!("SourceData{map}"))?;
    let drums = dfm_strings(dfm, &format!("DrumSet{map}"))?;
    let mut s = SoundSet::empty();
    for (p, line) in tones.iter().enumerate().take(128) {
        // "Piano 1\t8 Piano 1w\t16 Piano 1d": the capital tone, then "bank name"
        for (k, part) in line.split('\t').enumerate() {
            let (bank, name) = if k == 0 { (0, part) } else { split_num(part)? };
            // Optimizer marks some names with a trailing " :"
            let name = name.trim().trim_end_matches(':').trim_end();
            if !name.is_empty() {
                s.tones[p].push(Sound { msb: bank, lsb: map, program: p as u8, name: name.to_string() });
            }
        }
    }
    for line in drums {
        // "0 Standard 1\t27 High Q\t…": the kit's program and name, then its notes
        let mut it = line.split('\t');
        if let Some((p, name)) = it.next().and_then(split_num) {
            s.drums.push(Sound { msb: 0, lsb: map, program: p, name: name.trim().to_string() });
            s.keys.push(it.filter_map(split_num).map(|(n, k)| (n, k.trim().to_string())).filter(|k| !k.1.is_empty()).collect());
        }
    }
    (s.tones.iter().filter(|t| !t.is_empty()).count() >= 100).then_some(s)
}

/// A `const NAME = [...]` JSON array in a script (the user's
/// midi-file-editor.html): rows of numbers and strings.
pub fn js_rows(src: &str, name: &str) -> Option<Vec<Vec<JsVal>>> {
    let at = src.find(&format!("const {name} ="))?;
    let rest = &src[at..];
    let start = rest.find('[')?;
    let mut p = JsParser { s: rest[start..].as_bytes(), i: 0 };
    match p.value()? {
        JsVal::List(rows) => Some(rows.into_iter().filter_map(|r| if let JsVal::List(v) = r { Some(v) } else { None }).collect()),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum JsVal {
    Num(f64),
    Str(String),
    List(Vec<JsVal>),
}

impl JsVal {
    pub fn num(&self) -> Option<u8> {
        if let JsVal::Num(n) = self { (0.0..=255.0).contains(n).then_some(*n as u8) } else { None }
    }
    pub fn str(&self) -> Option<&str> {
        if let JsVal::Str(s) = self { Some(s) } else { None }
    }
}

struct JsParser<'a> {
    s: &'a [u8],
    i: usize,
}

impl JsParser<'_> {
    /// Whitespace and // or /* */ comments.
    fn ws(&mut self) {
        loop {
            while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
                self.i += 1;
            }
            match self.s.get(self.i..self.i + 2) {
                Some(b"//") => {
                    while self.i < self.s.len() && self.s[self.i] != b'\n' {
                        self.i += 1;
                    }
                }
                Some(b"/*") => {
                    self.i += 2;
                    while self.i + 1 < self.s.len() && &self.s[self.i..self.i + 2] != b"*/" {
                        self.i += 1;
                    }
                    self.i += 2;
                }
                _ => return,
            }
        }
    }
    fn value(&mut self) -> Option<JsVal> {
        self.ws();
        match *self.s.get(self.i)? {
            b'[' => {
                self.i += 1;
                let mut v = Vec::new();
                loop {
                    self.ws();
                    if *self.s.get(self.i)? == b']' {
                        self.i += 1;
                        return Some(JsVal::List(v));
                    }
                    v.push(self.value()?);
                    self.ws();
                    if *self.s.get(self.i)? == b',' {
                        self.i += 1;
                    }
                }
            }
            q @ (b'"' | b'\'') => {
                self.i += 1;
                let mut out = String::new();
                let start = self.i;
                let mut buf: Vec<u8> = Vec::new();
                loop {
                    let c = *self.s.get(self.i)?;
                    self.i += 1;
                    if c == q {
                        break;
                    }
                    if c == b'\\' {
                        let e = *self.s.get(self.i)?;
                        self.i += 1;
                        match e {
                            b'n' => buf.push(b'\n'),
                            b't' => buf.push(b'\t'),
                            b'u' => {
                                let h = std::str::from_utf8(self.s.get(self.i..self.i + 4)?).ok()?;
                                self.i += 4;
                                let ch = char::from_u32(u32::from_str_radix(h, 16).ok()?).unwrap_or('?');
                                let mut b = [0u8; 4];
                                buf.extend_from_slice(ch.encode_utf8(&mut b).as_bytes());
                            }
                            other => buf.push(other),
                        }
                    } else {
                        buf.push(c);
                    }
                }
                let _ = start;
                out.push_str(&String::from_utf8_lossy(&buf));
                Some(JsVal::Str(out))
            }
            _ => {
                let st = self.i;
                while self.i < self.s.len() && matches!(self.s[self.i], b'0'..=b'9' | b'-' | b'.' | b'e' | b'E' | b'+') {
                    self.i += 1;
                }
                std::str::from_utf8(&self.s[st..self.i]).ok()?.parse().ok().map(JsVal::Num)
            }
        }
    }
}

/// Yamaha PSR-SX900 from the user's midi-file-editor.html (`SX900_VOICES`:
/// MSB, LSB, program, category, name, type); drum kits = MSB 126 / 127.
pub fn sx900_from_mfe(html: &str) -> Option<SoundSet> {
    let rows = js_rows(html, "SX900_VOICES")?;
    let mut s = SoundSet::empty();
    for r in rows {
        let (Some(msb), Some(lsb), Some(p), Some(name)) = (r.first()?.num(), r.get(1)?.num(), r.get(2)?.num(), r.get(4).and_then(|v| v.str())) else { continue };
        if p >= 128 {
            continue;
        }
        let snd = Sound { msb, lsb, program: p, name: name.to_string() };
        if msb >= 126 {
            s.drums.push(snd);
        } else {
            s.tones[p as usize].push(snd);
        }
    }
    s.keys = vec![Vec::new(); s.drums.len()];
    (s.tones.iter().filter(|t| !t.is_empty()).count() >= 64).then_some(s)
}

/// SC-8850 (map 4) from the user's midi-file-editor.html (`SC8850_VOICES`:
/// MSB, LSB, program, name) when Optimizer is not installed; GS kits.
pub fn sc8850_from_mfe(html: &str) -> Option<SoundSet> {
    let rows = js_rows(html, "SC8850_VOICES")?;
    let mut s = SoundSet::empty();
    for r in rows {
        let (Some(msb), Some(lsb), Some(p), Some(name)) = (r.first()?.num(), r.get(1)?.num(), r.get(2)?.num(), r.get(3).and_then(|v| v.str())) else { continue };
        if p < 128 {
            s.tones[p as usize].push(Sound { msb, lsb, program: p, name: name.to_string() });
        }
    }
    for t in &mut s.tones {
        t.sort_by_key(|v| v.msb);
    }
    if let Some(kits) = js_rows(html, "GS_DRUM_KITS") {
        for k in kits {
            if let (Some(name), Some(p)) = (k.first().and_then(|v| v.str()), k.get(1).and_then(|v| v.num())) {
                s.drums.push(Sound { msb: 0, lsb: 4, program: p, name: name.to_string() });
            }
        }
    }
    s.keys = vec![Vec::new(); s.drums.len()];
    (s.tones.iter().filter(|t| !t.is_empty()).count() >= 100).then_some(s)
}

/// A drum kit's instruments from an XGworks kit file (`[<kit>]` with
/// `note=name`, as Drum Kit.ini / SFX Kit.ini / GM2 SFX Kit.ini).
pub fn ini_keys(ini: &[u8], kit: &str) -> Vec<(u8, String)> {
    Ini::parse(ini)
        .section(kit)
        .into_iter()
        .flatten()
        .filter_map(|(k, n)| Some((k.parse::<u8>().ok()?, n.trim().to_string())))
        .collect()
}

impl SoundSet {
    /// Fill the drum kits' instrument names from XGworks' kit files (XG / GM).
    pub fn fill_keys(&mut self, kit_files: &[Vec<u8>]) {
        self.keys = self
            .drums
            .iter()
            .map(|d| kit_files.iter().map(|f| ini_keys(f, &d.name)).find(|k| !k.is_empty()).unwrap_or_default())
            .collect();
    }

    /// The instrument names of the kit for this bank / program.
    pub fn kit_keys(&self, msb: u8, lsb: u8, program: u8) -> &[(u8, String)] {
        self.find_drum(msb, lsb, program).and_then(|k| self.keys.get(k)).map_or(&[], |v| v.as_slice())
    }
}

/// The standard a song is made for, from its resets and SysEx: XG (XG
/// System On / XG parameters), GS (GS Reset / Roland Data Set; the map from
/// Bank Select LSB 1-3, else SC-88Pro), GM2 or GM (GM System On level 2 / 1).
pub fn detect(tracks: &[crate::EditTrack]) -> Option<Standard> {
    use crate::Event;
    let (mut xg, mut gs, mut gm2, mut gm) = (false, false, false, false);
    let mut map = 0u8;
    for e in tracks.iter().flat_map(|t| t.others.iter()) {
        match e {
            Event::Sysex { data, .. } => {
                let d = data.strip_prefix(&[0xF0]).unwrap_or(data);
                if crate::mixer::parse_xg_change(d).is_some() || crate::xgtbl::parse_xg_bulk(d).is_some() {
                    xg = true;
                } else if crate::fxsysex::parse_gs(d).is_some() {
                    gs = true;
                } else if d.len() >= 4 && d[0] == 0x7E && d[2] == 0x09 {
                    match d[3] {
                        0x03 => gm2 = true,
                        0x01 => gm = true,
                        _ => {}
                    }
                }
            }
            Event::Chan { data, .. } if data.len() >= 3 && data[0] & 0xF0 == 0xB0 && data[1] == 32 && (1..=3).contains(&data[2]) => {
                map = map.max(data[2]);
            }
            _ => {}
        }
    }
    if xg {
        Some(Standard::Xg)
    } else if gs {
        Some(Standard::Gs(if map == 0 { 3 } else { map }))
    } else if gm2 {
        Some(Standard::Gm2)
    } else if gm {
        Some(Standard::Gm)
    } else {
        None
    }
}

fn split_num(s: &str) -> Option<(u8, &str)> {
    let s = s.trim_start();
    let end = s.find(|c: char| !c.is_ascii_digit())?;
    Some((s[..end].parse().ok()?, &s[end..]))
}

// ---------------------------------------------------------------------------
// PE named RCDATA resources and Delphi binary forms (TPF0)
// ---------------------------------------------------------------------------
fn u16_at(d: &[u8], o: usize) -> Option<usize> {
    Some(u16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?) as usize)
}
fn u32_at(d: &[u8], o: usize) -> Option<usize> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?) as usize)
}

/// The RCDATA resource called `name` (upper case, as Delphi stores forms).
pub fn pe_rcdata<'a>(exe: &'a [u8], name: &str) -> Option<&'a [u8]> {
    let pe = u32_at(exe, 0x3C)?;
    let nsec = u16_at(exe, pe + 6)?;
    let opt = pe + 24;
    let opt_size = u16_at(exe, pe + 20)?;
    let dd = if u16_at(exe, opt)? == 0x20B { opt + 112 } else { opt + 96 };
    let rsrc_rva = u32_at(exe, dd + 2 * 8)?;
    let secs: Vec<(usize, usize, usize)> = (0..nsec)
        .filter_map(|i| {
            let s = opt + opt_size + 40 * i;
            Some((u32_at(exe, s + 12)?, u32_at(exe, s + 8)?.max(u32_at(exe, s + 16)?), u32_at(exe, s + 20)?))
        })
        .collect();
    let off = |rva: usize| secs.iter().find(|s| s.0 <= rva && rva < s.0 + s.1).map(|s| rva - s.0 + s.2);
    let base = off(rsrc_rva)?;
    let entries = |o: usize| -> Vec<(usize, usize)> {
        let n = u16_at(exe, base + o + 12).unwrap_or(0) + u16_at(exe, base + o + 14).unwrap_or(0);
        (0..n).filter_map(|k| Some((u32_at(exe, base + o + 16 + 8 * k)?, u32_at(exe, base + o + 20 + 8 * k)?))).collect()
    };
    let res_name = |id: usize| -> Option<String> {
        let o = base + (id & 0x7FFF_FFFF);
        let n = u16_at(exe, o)?;
        let w: Vec<u16> = (0..n).filter_map(|k| u16_at(exe, o + 2 + 2 * k).map(|c| c as u16)).collect();
        Some(String::from_utf16_lossy(&w))
    };
    for (t, p1) in entries(0) {
        if t != 10 {
            continue;
        }
        for (id, p2) in entries(p1 & 0x7FFF_FFFF) {
            if id & 0x8000_0000 == 0 || res_name(id).as_deref() != Some(name) {
                continue;
            }
            let &(_, p3) = entries(p2 & 0x7FFF_FFFF).first()?;
            let (rva, size) = (u32_at(exe, base + p3)?, u32_at(exe, base + p3 + 4)?);
            return off(rva).and_then(|o| exe.get(o..o + size));
        }
    }
    None
}

/// The `Items.Strings` of the component `object_name` in a binary Delphi form.
pub fn dfm_strings(dfm: &[u8], object_name: &str) -> Option<Vec<String>> {
    let mut r = Dfm { d: dfm, i: 4 };
    if dfm.get(..4)? != b"TPF0" {
        return None;
    }
    let mut found = None;
    r.object(object_name, &mut found)?;
    found
}

struct Dfm<'a> {
    d: &'a [u8],
    i: usize,
}

enum Val {
    Str(String),
    List(Vec<Val>),
    Other,
}

impl Dfm<'_> {
    fn u8(&mut self) -> Option<u8> {
        let v = *self.d.get(self.i)?;
        self.i += 1;
        Some(v)
    }
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.d.get(self.i..self.i + n)?;
        self.i += n;
        Some(s)
    }
    fn n32(&mut self) -> Option<usize> {
        let v = u32_at(self.d, self.i)?;
        self.i += 4;
        Some(v)
    }
    fn sstr(&mut self) -> Option<String> {
        let n = self.u8()? as usize;
        Some(self.take(n)?.iter().map(|&b| b as char).collect())
    }
    fn value(&mut self) -> Option<Val> {
        Some(match self.u8()? {
            0 | 8 | 9 | 13 => Val::Other,
            1 => {
                let mut v = Vec::new();
                while *self.d.get(self.i)? != 0 {
                    v.push(self.value()?);
                }
                self.i += 1;
                Val::List(v)
            }
            2 => {
                self.take(1)?;
                Val::Other
            }
            3 => {
                self.take(2)?;
                Val::Other
            }
            4 | 15 => {
                self.take(4)?;
                Val::Other
            }
            5 => {
                self.take(10)?;
                Val::Other
            }
            16 | 17 | 19 => {
                self.take(8)?;
                Val::Other
            }
            6 | 7 => Val::Str(self.sstr()?),
            10 => {
                let n = self.n32()?;
                self.take(n)?;
                Val::Other
            }
            11 => {
                while !self.sstr()?.is_empty() {}
                Val::Other
            }
            12 | 20 => {
                let n = self.n32()?;
                Val::Str(String::from_utf8_lossy(self.take(n)?).into_owned())
            }
            18 => {
                let n = self.n32()?;
                let b = self.take(2 * n)?;
                let w: Vec<u16> = b.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
                Val::Str(String::from_utf16_lossy(&w))
            }
            14 => {
                if matches!(self.d.get(self.i)?, 2..=4) {
                    self.value()?;
                }
                while *self.d.get(self.i)? != 0 {
                    while *self.d.get(self.i)? != 0 {
                        self.sstr()?;
                        self.value()?;
                    }
                    self.i += 1;
                }
                self.i += 1;
                Val::Other
            }
            _ => return None,
        })
    }
    fn object(&mut self, want: &str, found: &mut Option<Vec<String>>) -> Option<()> {
        let flags = *self.d.get(self.i)?;
        if flags & 0xF0 == 0xF0 {
            self.i += 1;
            if flags & 2 != 0 {
                self.value()?;
            }
        }
        let _class = self.sstr()?;
        let name = self.sstr()?;
        while *self.d.get(self.i)? != 0 {
            let prop = self.sstr()?;
            let v = self.value()?;
            if name == want && prop == "Items.Strings" {
                if let Val::List(items) = v {
                    *found = Some(items.into_iter().filter_map(|x| if let Val::Str(s) = x { Some(s) } else { None }).collect());
                }
            }
        }
        self.i += 1;
        while *self.d.get(self.i)? != 0 {
            self.object(want, found)?;
        }
        self.i += 1;
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gm_and_gm2_from_ini() {
        let melody = b"[Melody]\r\n0=Grand Piano\r\n77=Shakuhachi\r\n";
        let s = gm(melody);
        assert_eq!(s.name(0, 0, 77), Some("Shakuhachi"));
        assert_eq!(s.drums.len(), 1);
        let normal = b"[GM2 Normal]\r\n0=GrandPno\r\n1=BritePno\r\n[GrandPno]\r\n0=GrandPno\r\n1=GrndPnoK\r\n";
        let kits = b"[GM2 SFX Kit]\r\n0=GM Stand\r\n8=GM Room\r\n";
        let s = gm2(normal, kits);
        assert_eq!(s.tones[0].len(), 2);
        assert_eq!(s.find(121, 1, 0), Some((0, 1)));
        assert_eq!(s.tones[1][0], Sound { msb: 121, lsb: 0, program: 1, name: "BritePno".into() });
        assert_eq!(s.find_drum(120, 0, 8), Some(1));
    }

    #[test]
    fn xg_groups_by_program() {
        let v = |msb, lsb, program, name: &str| Voice { msb, lsb, program, name: name.into(), category: String::new() };
        let s = xg(&[v(0, 1, 0, "Grand Piano KSP"), v(0, 0, 0, "Grand Piano"), v(127, 0, 0, "Standard Kit")]);
        assert_eq!(s.tones[0][0].name, "Grand Piano");
        assert_eq!(s.drums.len(), 1);
    }
}
