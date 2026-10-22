//! XG parameter tables, reimplementing the data side of `XGEditor.dll`.
//!
//! XGworks ships the whole XG parameter map as INI-style text: one `<model>.tbl`
//! per tone generator (picked via `xgeditor.ini` → `prmfile=`) and voice names
//! in `*VoiceList*.ini`. Format, as decoded:
//!
//! - `[XGBLK]` lists block sections. A block has `adrHML=H1,H2,M1,M2,L` (the
//!   block repeats for H in H1..=H2, M in M1..=M2, starting at low address L)
//!   and one entry per parameter: `LL=size,min,max,def,offset,form,conv,option,name`
//!   (hex except name; `offset` is a signed hex display offset; `def` may be a
//!   `.`-separated list with one default per block instance, e.g. per part).
//!   `size>1` = multi-byte parameter whose following bytes appear as `size=0`
//!   entries carrying their own min/max/def; such bytes are 4-bit nibbles.
//!   `conv=4` marks 2-byte effect parameters (two 7-bit bytes).
//! - `[RevType]/[ChoType]/[VarType]/[InsType]`: `idx=MSBLSB,*ParamList` per
//!   effect block; `[EffectTypeList]`: `idx=MSBLSB,NAME,*Group,p1..p16` defaults;
//!   `[*ParamList]`: `NN=size,min,max,form,?,name` for effect params 1..16.

use std::collections::HashMap;

use crate::mixer::xg_param;
use crate::{EditTrack, Event};

// ---------------------------------------------------------------------------
// INI reader (comments `;`, Shift-JIS comment bytes are ignored)
// ---------------------------------------------------------------------------
pub struct Ini {
    pub sections: Vec<(String, Vec<(String, String)>)>,
}

impl Ini {
    pub fn parse(bytes: &[u8]) -> Ini {
        // names are ASCII; decode lossily as Latin-1 so SJIS comments don't fail
        let text: String = bytes.iter().map(|&b| b as char).collect();
        let mut sections: Vec<(String, Vec<(String, String)>)> = Vec::new();
        for raw in text.lines() {
            let line = match raw.find(';') {
                Some(i) => &raw[..i],
                None => raw,
            }
            .trim();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') {
                if let Some(end) = line.find(']') {
                    sections.push((line[1..end].to_string(), Vec::new()));
                }
            } else if let (Some(eq), Some(sec)) = (line.find('='), sections.last_mut()) {
                sec.1.push((line[..eq].trim().to_string(), line[eq + 1..].trim().to_string()));
            }
        }
        Ini { sections }
    }

    pub fn section(&self, name: &str) -> Option<&[(String, String)]> {
        self.sections.iter().find(|s| s.0 == name).map(|s| s.1.as_slice())
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.section(section)?.iter().find(|kv| kv.0 == key).map(|kv| kv.1.as_str())
    }
}

fn hex(s: &str) -> u32 {
    u32::from_str_radix(s.trim(), 16).unwrap_or(0)
}

fn signed_hex(s: &str) -> i32 {
    let s = s.trim();
    let (neg, digits) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let v = i32::from_str_radix(digits, 16).unwrap_or(0);
    if neg { -v } else { v }
}

// ---------------------------------------------------------------------------
// parameter model
// ---------------------------------------------------------------------------
#[derive(Clone, Debug)]
pub struct TblParam {
    /// Low address within the block instance.
    pub lo: u8,
    pub name: String,
    /// Per byte: (min, max, defaults — one per instance or a single value).
    pub bytes: Vec<(u8, u8, Vec<u8>)>,
    /// Display value = raw + `disp_offset`.
    pub disp_offset: i32,
    pub form: u8,
    pub conv: u8,
    pub option: u8,
}

impl TblParam {
    pub fn size(&self) -> usize {
        self.bytes.len()
    }
    /// Multi-byte params are nibble-coded (4 bits per byte), single bytes 7-bit.
    fn bits(&self) -> u32 {
        if self.bytes.len() > 1 { 4 } else { 7 }
    }
    pub fn combine(&self, bytes: &[u8]) -> u32 {
        bytes.iter().fold(0u32, |acc, &b| (acc << self.bits()) | b as u32)
    }
    pub fn split(&self, v: u32) -> Vec<u8> {
        let n = self.bytes.len();
        let mask = (1u32 << self.bits()) - 1;
        (0..n).map(|i| ((v >> (self.bits() * (n - 1 - i) as u32)) & mask) as u8).collect()
    }
    pub fn min(&self) -> u32 {
        self.combine(&self.bytes.iter().map(|b| b.0).collect::<Vec<_>>())
    }
    pub fn max(&self) -> u32 {
        self.combine(&self.bytes.iter().map(|b| b.1).collect::<Vec<_>>())
    }
    /// Default raw value for block instance `inst` (e.g. part number).
    pub fn default(&self, inst: usize) -> u32 {
        let b: Vec<u8> = self
            .bytes
            .iter()
            .map(|(_, _, d)| *d.get(inst).or(d.first()).unwrap_or(&0))
            .collect();
        self.combine(&b)
    }
    /// Effect-parameter placeholders (`effect#N`) are edited via the effect page.
    pub fn is_effect_slot(&self) -> bool {
        self.name.starts_with("effect#")
    }
}

#[derive(Clone, Debug)]
pub struct TblBlock {
    pub key: String,
    pub h: (u8, u8),
    pub m: (u8, u8),
    pub params: Vec<TblParam>,
}

impl TblBlock {
    pub fn instances(&self) -> usize {
        (self.h.1 - self.h.0 + 1) as usize * (self.m.1 - self.m.0 + 1) as usize
    }
    /// (H, M) for instance index `i` (M varies fastest).
    pub fn hm(&self, i: usize) -> (u8, u8) {
        let mc = (self.m.1 - self.m.0 + 1) as usize;
        (self.h.0 + (i / mc) as u8, self.m.0 + (i % mc) as u8)
    }
}

#[derive(Clone, Debug)]
pub struct FxParam {
    pub min: u32,
    pub max: u32,
    pub form: u8,
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct FxType {
    pub msb: u8,
    pub lsb: u8,
    pub name: String,
    pub defaults: [u32; 16],
}

#[derive(Clone, Debug, Default)]
pub struct XgTable {
    pub blocks: Vec<TblBlock>,
    /// Per effect block: (MSB, LSB, param-list name).
    pub rev_types: Vec<(u8, u8, String)>,
    pub cho_types: Vec<(u8, u8, String)>,
    pub var_types: Vec<(u8, u8, String)>,
    pub ins_types: Vec<(u8, u8, String)>,
    pub fx: Vec<FxType>,
    /// `*ParamList` → 16 params (None = unused slot).
    pub fx_params: HashMap<String, Vec<Option<FxParam>>>,
}

pub fn parse_tbl(bytes: &[u8]) -> XgTable {
    let ini = Ini::parse(bytes);
    let mut t = XgTable::default();
    let block_keys: Vec<String> = ini
        .section("XGBLK")
        .map(|s| s.iter().filter(|kv| kv.0 != "size").map(|kv| kv.1.clone()).collect())
        .unwrap_or_default();
    for key in block_keys {
        let Some(sec) = ini.section(&key) else { continue };
        let Some(adr) = sec.iter().find(|kv| kv.0 == "adrHML") else { continue };
        let a: Vec<u8> = adr.1.split(',').map(|x| hex(x) as u8).collect();
        if a.len() < 5 {
            continue;
        }
        let mut params: Vec<TblParam> = Vec::new();
        for (k, v) in sec {
            if k == "adrHML" || k == "size" {
                continue;
            }
            let f: Vec<&str> = v.splitn(9, ',').collect();
            if f.len() < 8 {
                continue;
            }
            let size = hex(f[0]) as usize;
            let defs: Vec<u8> = f[3].split('.').map(|d| hex(d) as u8).collect();
            let byte = (hex(f[1]) as u8, hex(f[2]) as u8, defs);
            if size == 0 {
                // continuation byte of the previous multi-byte parameter
                if let Some(p) = params.last_mut() {
                    p.bytes.push(byte);
                }
                continue;
            }
            params.push(TblParam {
                lo: hex(k) as u8,
                name: f.get(8).unwrap_or(&"").trim().to_string(),
                bytes: vec![byte],
                disp_offset: signed_hex(f[4]),
                form: hex(f[5]) as u8,
                conv: hex(f[6]) as u8,
                option: hex(f[7]) as u8,
            });
        }
        // effect params (conv 4) are 2 x 7-bit; their range lives in the effect lists
        t.blocks.push(TblBlock { key, h: (a[0], a[1]), m: (a[2], a[3]), params });
    }
    let types = |name: &str| -> Vec<(u8, u8, String)> {
        ini.section(name)
            .map(|s| {
                s.iter()
                    .filter(|kv| kv.0 != "size")
                    .filter_map(|kv| {
                        let (ty, list) = kv.1.split_once(',')?;
                        let v = hex(ty);
                        Some(((v >> 8) as u8, v as u8, list.trim().to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    };
    t.rev_types = types("RevType");
    t.cho_types = types("ChoType");
    t.var_types = types("VarType");
    t.ins_types = types("InsType");
    if let Some(s) = ini.section("EffectTypeList") {
        for (k, v) in s {
            if k == "size" {
                continue;
            }
            let f: Vec<&str> = v.split(',').collect();
            if f.len() < 3 {
                continue;
            }
            let ty = hex(f[0]);
            let mut defaults = [0u32; 16];
            for (i, d) in f.iter().skip(3).take(16).enumerate() {
                defaults[i] = hex(d);
            }
            t.fx.push(FxType { msb: (ty >> 8) as u8, lsb: ty as u8, name: f[1].trim().to_string(), defaults });
        }
    }
    for (name, sec) in &ini.sections {
        if !name.starts_with('*') {
            continue;
        }
        let mut list: Vec<Option<FxParam>> = vec![None; 16];
        for (k, v) in sec {
            let Ok(n) = k.parse::<usize>() else { continue }; // decimal 01..16
            let f: Vec<&str> = v.splitn(6, ',').collect();
            if n == 0 || n > 16 || f.len() < 6 || hex(f[0]) == 0 {
                continue;
            }
            list[n - 1] = Some(FxParam {
                min: hex(f[1]),
                max: hex(f[2]),
                form: hex(f[3]) as u8,
                name: f[5].trim().to_string(),
            });
        }
        t.fx_params.insert(name.clone(), list);
    }
    t
}

// ---------------------------------------------------------------------------
// effect blocks
// ---------------------------------------------------------------------------
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FxSlot {
    Reverb,
    Chorus,
    Variation,
    Insertion(u8),
}

impl FxSlot {
    pub const ALL: [FxSlot; 4] = [FxSlot::Reverb, FxSlot::Chorus, FxSlot::Variation, FxSlot::Insertion(0)];

    pub fn label(self) -> String {
        match self {
            FxSlot::Reverb => "Reverb".into(),
            FxSlot::Chorus => "Chorus".into(),
            FxSlot::Variation => "Variation".into(),
            FxSlot::Insertion(n) => format!("Insertion {}", n + 1),
        }
    }
    pub fn type_addr(self) -> [u8; 3] {
        match self {
            FxSlot::Reverb => [2, 1, 0x00],
            FxSlot::Chorus => [2, 1, 0x20],
            FxSlot::Variation => [2, 1, 0x40],
            FxSlot::Insertion(n) => [3, n, 0x00],
        }
    }
    /// Address and byte width of effect parameter `k` (0-based, 0..16).
    pub fn param_addr(self, k: usize, wide: bool) -> ([u8; 3], usize) {
        let [h, m, base] = self.type_addr();
        let k8 = k as u8;
        match (self, k < 10) {
            (FxSlot::Variation, true) => ([h, m, 0x42 + 2 * k8], 2),
            (FxSlot::Insertion(_), true) if wide => ([h, m, 0x30 + 2 * k8], 2),
            (_, true) => ([h, m, base + 2 + k8], 1),
            (FxSlot::Reverb, false) => ([h, m, 0x10 + k8 - 10], 1),
            (FxSlot::Chorus, false) => ([h, m, 0x30 + k8 - 10], 1),
            (FxSlot::Variation, false) => ([h, m, 0x70 + k8 - 10], 1),
            (FxSlot::Insertion(_), false) => ([h, m, 0x20 + k8 - 10], 1),
        }
    }
}

impl XgTable {
    pub fn types(&self, slot: FxSlot) -> &[(u8, u8, String)] {
        match slot {
            FxSlot::Reverb => &self.rev_types,
            FxSlot::Chorus => &self.cho_types,
            FxSlot::Variation => &self.var_types,
            FxSlot::Insertion(_) => &self.ins_types,
        }
    }
    pub fn fx_type(&self, msb: u8, lsb: u8) -> Option<&FxType> {
        self.fx.iter().find(|f| f.msb == msb && f.lsb == lsb)
    }
    pub fn fx_name(&self, msb: u8, lsb: u8) -> String {
        self.fx_type(msb, lsb).map_or_else(|| format!("{msb:02X}{lsb:02X}"), |f| f.name.clone())
    }
    pub fn block(&self, key: &str) -> Option<&TblBlock> {
        self.blocks.iter().find(|b| b.key == key)
    }
}

// ---------------------------------------------------------------------------
// XG parameter memory: byte-addressed, defaults from the table, overlaid by
// edits / SysEx found in the song.
// ---------------------------------------------------------------------------
#[derive(Clone, Debug, Default)]
pub struct XgMemory {
    pub bytes: HashMap<[u8; 3], u8>,
}

impl XgMemory {
    pub fn get(&self, addr: [u8; 3]) -> Option<u8> {
        self.bytes.get(&addr).copied()
    }
    pub fn set(&mut self, addr: [u8; 3], data: &[u8]) {
        for (i, &b) in data.iter().enumerate() {
            self.bytes.insert(offset(addr, i), b & 0x7F);
        }
    }
    /// Forget an effect's parameter bytes (they revert to the type defaults).
    pub fn clear_fx_params(&mut self, slot: FxSlot) {
        for k in 0..16 {
            for wide in [false, true] {
                let (a, w) = slot.param_addr(k, wide);
                for i in 0..w {
                    self.bytes.remove(&offset(a, i));
                }
            }
        }
    }
    /// Read `n` bytes at `addr`, falling back to `default` for unknown bytes.
    pub fn read(&self, addr: [u8; 3], default: &[u8]) -> Vec<u8> {
        (0..default.len()).map(|i| self.get(offset(addr, i)).unwrap_or(default[i])).collect()
    }

    /// Load every XG Parameter Change found in the song (time order; a
    /// System On clears what came before it).
    pub fn from_song(tracks: &[EditTrack]) -> XgMemory {
        let mut evs: Vec<&Event> = tracks.iter().flat_map(|t| t.others.iter()).collect();
        evs.sort_by_key(|e| e.t());
        let mut m = XgMemory::default();
        for e in evs {
            if let Event::Sysex { status: 0xF0, data, .. } = e {
                if let Some((addr, d)) = crate::mixer::parse_xg_change(data) {
                    if addr == [0, 0, 0x7E] {
                        m.bytes.clear();
                    } else {
                        // a new effect type resets that effect's parameters
                        for slot in [FxSlot::Reverb, FxSlot::Chorus, FxSlot::Variation, FxSlot::Insertion(0), FxSlot::Insertion(1)] {
                            if addr == slot.type_addr() {
                                m.clear_fx_params(slot);
                            }
                        }
                        m.set(addr, d);
                    }
                }
            }
        }
        m
    }
}

/// Address `addr + i` within the 7-bit low-address space.
pub fn offset(addr: [u8; 3], i: usize) -> [u8; 3] {
    let lo = addr[2] as usize + i;
    [addr[0], addr[1].wrapping_add((lo >> 7) as u8), (lo & 0x7F) as u8]
}

/// Effect parameter values as 7-bit pairs (`conv=4` 2-byte params).
pub fn split14(v: u32) -> Vec<u8> {
    vec![((v >> 7) & 0x7F) as u8, (v & 0x7F) as u8]
}
pub fn combine14(b: &[u8]) -> u32 {
    ((b[0] as u32) << 7) | b.get(1).copied().unwrap_or(0) as u32
}

/// Write XG Parameter Changes into `track`'s setup (tick 0, after an XG System
/// On if present), replacing earlier setup SysEx for the same addresses.
pub fn apply_xg_changes(track: &mut EditTrack, dev: u8, changes: &[([u8; 3], Vec<u8>)]) {
    let limit = track.notes.iter().map(|n| n.tick).min().unwrap_or(u32::MAX);
    let addrs: Vec<[u8; 3]> = changes.iter().map(|c| c.0).collect();
    track.others.retain(|e| match e {
        Event::Sysex { t, status: 0xF0, data } if *t <= limit => {
            !crate::mixer::parse_xg_change(data).map_or(false, |(a, _)| addrs.contains(&a))
        }
        _ => true,
    });
    let mut pos = track
        .others
        .iter()
        .rposition(|e| matches!(e, Event::Sysex { t: 0, status: 0xF0, data }
            if crate::mixer::parse_xg_change(data).map_or(false, |(a, _)| a == [0, 0, 0x7E])))
        .map_or(0, |i| i + 1);
    // keep tick-0 setup SysEx ordered; insert after any existing tick-0 SysEx block
    while pos < track.others.len() && matches!(track.others[pos], Event::Sysex { t: 0, .. }) {
        pos += 1;
    }
    for (k, (addr, data)) in changes.iter().enumerate() {
        let b = xg_param(dev, *addr, data);
        track.others.insert(pos + k, Event::Sysex { t: 0, status: 0xF0, data: b[1..].to_vec() });
    }
}

// ---------------------------------------------------------------------------
// voice list (*VoiceList*.ini)
// ---------------------------------------------------------------------------
#[derive(Clone, Debug)]
pub struct Voice {
    pub msb: u8,
    pub lsb: u8,
    pub program: u8,
    pub name: String,
    pub category: String,
}

/// Flatten the voice list: Melody (MSB 0, grouped by program range), SFX
/// (MSB 64), SFX Kit (126) and Drum Kit (127). Model-exclusive and A/D
/// voices are not listed.
pub fn parse_voice_list(bytes: &[u8]) -> Vec<Voice> {
    let ini = Ini::parse(bytes);
    let mut out = Vec::new();
    let entries = |sec: &str| -> Vec<Vec<String>> {
        ini.section(sec)
            .map(|s| {
                s.iter()
                    .filter(|kv| kv.0.chars().all(|c| c.is_ascii_digit()))
                    .map(|kv| kv.1.splitn(3, ',').map(|x| x.trim().to_string()).collect())
                    .collect()
            })
            .unwrap_or_default()
    };
    for top in entries("vce_list") {
        let (Some(msb), Some(cat)) = (top.first().and_then(|x| x.parse::<u8>().ok()), top.get(1)) else { continue };
        match msb {
            0 => {
                // groups: lo,hi,GroupName → [GroupName] entries prog,lsb,name
                for g in entries(cat) {
                    let Some(group) = g.get(2) else { continue };
                    for v in entries(group) {
                        if let (Some(p), Some(l), Some(n)) = (v.first(), v.get(1), v.get(2)) {
                            if let (Ok(p), Ok(l)) = (p.parse(), l.parse()) {
                                out.push(Voice { msb: 0, lsb: l, program: p, name: n.clone(), category: group.clone() });
                            }
                        }
                    }
                }
            }
            64 | 126 | 127 => {
                for v in entries(cat) {
                    if let (Some(p), Some(n)) = (v.first().and_then(|x| x.parse().ok()), v.get(2)) {
                        out.push(Voice { msb, lsb: 0, program: p, name: n.clone(), category: cat.clone() });
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Tone-generator models from `xgeditor.ini`: (name, prmfile, voicefile).
pub fn parse_models(bytes: &[u8]) -> Vec<(String, String, String)> {
    let ini = Ini::parse(bytes);
    let names: Vec<String> = ini
        .section("TG_Model_List")
        .map(|s| s.iter().filter(|kv| kv.0 != "line" && kv.0 != "def").map(|kv| kv.1.clone()).collect())
        .unwrap_or_default();
    names
        .into_iter()
        .filter_map(|n| {
            let prm = ini.get(&n, "prmfile")?.to_string();
            let vce = ini.get(&n, "voicefile").unwrap_or("").to_string();
            Some((n, prm, vce))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const TBL: &str = "[XGBLK]\nsize=2\n00=System\n01=Part_1\n\
[System]\nadrHML=00,00,00,00,00\nsize=7\n\
00=4,0000,00,00,-400,1,3,0,Master Tune (cent)\t;c\n01=0,0000,07,04,-400,1,0,0,\n02=0,0000,0F,00,-400,1,0,0,\n03=0,0000,0F,00,-400,1,0,0,\n\
04=1,0,7F,7F,0,0,0,0,Master Volume\n\
[Part_1]\nadrHML=08,08,00,3F,00\nsize=2\n01=1,00,7f,00.00.00.00.00.00.00.00.00.7F,0,0,0,1,Bnk Sel Msb\n03=1,00,7f,00,+1,0,0,1,Prg Num\n\
[VarType]\nsize=2\n00=0000,*NoEffect\n01=0500,*DelayLCR_var\n\
[EffectTypeList]\nsize=1\n0A=0500,DELAY L.C.R,*DelayLCR,d05,683,1388,1388,4a,64,0a,00,00,20,00,3c,1c,40,2e,40\n\
[*DelayLCR_var]\n01=1,01,3715,8,0,Lch Delay (ms)\n10=1,01,7f,9,0,Dry/Wet\n11=0,00,00,0,0,\n";

    #[test]
    fn parses_blocks_and_effects() {
        let t = parse_tbl(TBL.as_bytes());
        let sys = t.block("System").unwrap();
        let tune = &sys.params[0];
        assert_eq!(tune.size(), 4);
        assert_eq!((tune.min(), tune.max(), tune.default(0)), (0, 0x7FF, 0x400));
        assert_eq!(tune.disp_offset, -0x400);
        assert_eq!(tune.split(0x400), vec![0, 4, 0, 0]);
        let part = t.block("Part_1").unwrap();
        assert_eq!(part.instances(), 64);
        assert_eq!(part.hm(9), (8, 9));
        assert_eq!(part.params[0].default(9), 0x7F); // drum part bank
        assert_eq!(part.params[0].default(0), 0);
        assert_eq!(part.params[1].disp_offset, 1);
        assert_eq!(t.var_types[1], (5, 0, "*DelayLCR_var".into()));
        assert_eq!(t.fx_type(5, 0).unwrap().defaults[0], 0xd05);
        let l = &t.fx_params["*DelayLCR_var"];
        assert_eq!(l[0].as_ref().unwrap().max, 0x3715);
        assert_eq!(l[9].as_ref().unwrap().name, "Dry/Wet");
        assert!(l[10].is_none());
    }

    #[test]
    fn effect_addresses() {
        assert_eq!(FxSlot::Variation.param_addr(0, false), ([2, 1, 0x42], 2));
        assert_eq!(FxSlot::Variation.param_addr(10, false), ([2, 1, 0x70], 1));
        assert_eq!(FxSlot::Reverb.param_addr(9, false), ([2, 1, 0x0B], 1));
        assert_eq!(FxSlot::Insertion(1).param_addr(2, true), ([3, 1, 0x34], 2));
        assert_eq!(offset([2, 1, 0x7F], 1), [2, 2, 0]);
        assert_eq!(combine14(&split14(0x1388)), 0x1388);
    }

    #[test]
    fn memory_and_song_roundtrip() {
        let mut t = EditTrack { name: "t".into(), ..Default::default() };
        apply_xg_changes(&mut t, 0, &[([2, 1, 0x40], vec![5, 0]), ([0, 0, 4], vec![100])]);
        apply_xg_changes(&mut t, 0, &[([2, 1, 0x40], vec![5, 0])]);
        assert_eq!(t.others.len(), 2);
        let m = XgMemory::from_song(std::slice::from_ref(&t));
        assert_eq!(m.read([2, 1, 0x40], &[0, 0]), vec![5, 0]);
        assert_eq!(m.get([0, 0, 4]), Some(100));
    }
}
