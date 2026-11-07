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
    /// The song's XG state: every XG Parameter Change, plus each part's setup
    /// channel messages (bank, program, volume, pan, sends, filter/EG, bend
    /// range) up to the track's first note, applied in time order. XG / GM
    /// System On reset the memory.
    pub fn from_song(tracks: &[EditTrack]) -> XgMemory {
        Self::from_song_range(tracks, 0, u32::MAX, true)
    }

    /// The XG state from the events in `from..=to`; `setup_only` takes channel
    /// messages only up to each track's first note (else all in the range).
    pub fn from_song_range(tracks: &[EditTrack], from: u32, to: u32, setup_only: bool) -> XgMemory {
        let mut evs: Vec<(&Event, u8)> = Vec::new();
        for t in tracks {
            let limit = if setup_only { first_notes(t) } else { [u32::MAX; 16] };
            let base = t.port().saturating_sub(1).min(3) * 16;
            for e in t.others.iter().filter(|e| (from..=to).contains(&e.t())) {
                match e {
                    Event::Sysex { status: 0xF0, .. } => evs.push((e, 0)),
                    Event::Chan { t, data } if data.len() >= 2 && *t <= limit[(data[0] & 0x0F) as usize] => evs.push((e, base + (data[0] & 0x0F))),
                    _ => {}
                }
            }
        }
        // a System On before the other messages of its tick (any track)
        evs.sort_by_key(|e| (e.0.t(), !matches!(e.0, Event::Sysex { data, .. } if is_reset(data))));
        let mut m = XgMemory::default();
        let mut rpn = [[0x7Fu8; 2]; 64];
        for (e, part) in evs {
            match e {
                Event::Sysex { data, .. } => {
                    m.apply_sysex(data);
                }
                Event::Chan { data, .. } => {
                    let p = part as usize;
                    let at = |lo: u8| [0x08, part, lo];
                    match (data[0] & 0xF0, data[1], data.get(2).copied().unwrap_or(0)) {
                        (0xC0, prg, _) => m.set(at(0x03), &[prg]),
                        (0xB0, 101, v) => rpn[p][0] = v,
                        (0xB0, 100, v) => rpn[p][1] = v,
                        (0xB0, 6, v) if rpn[p] == [0, 0] => m.set(at(0x23), &[(0x40 + v.min(24)) & 0x7F]),
                        (0xB0, cc, v) => {
                            if let Some(lo) = cc_part_param(cc) {
                                // CC 10 = 0 is hard left; XG pan 0 means random
                                m.set(at(lo), &[if cc == 10 { v.max(1) } else { v }]);
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        m
    }
}

impl XgMemory {
    /// Apply one SysEx (with or without the leading F0): XG Parameter Change,
    /// XG Bulk Dump, XG / GM System On (reset). Returns false if it is none
    /// of these.
    pub fn apply_sysex(&mut self, data: &[u8]) -> bool {
        let d = data.strip_prefix(&[0xF0]).unwrap_or(data);
        if is_gm_on(d) {
            self.bytes.clear();
        } else if let Some((addr, v)) = crate::mixer::parse_xg_change(d) {
            if addr == [0, 0, 0x7E] || addr == [0, 0, 0x7F] {
                self.bytes.clear();
            } else {
                // a new effect type resets that effect's parameters
                for slot in [FxSlot::Reverb, FxSlot::Chorus, FxSlot::Variation, FxSlot::Insertion(0), FxSlot::Insertion(1)] {
                    if addr == slot.type_addr() {
                        self.clear_fx_params(slot);
                    }
                }
                self.set(addr, v);
            }
        } else if let Some((addr, v)) = parse_xg_bulk(d) {
            self.set(addr, v);
        } else {
            return false;
        }
        true
    }

    /// The memory as XG Parameter Changes, one per table parameter (multi-byte
    /// parameters whole), in address order. With `all`, every parameter of the
    /// system, effect and part blocks is included (memory value, else the
    /// table default); drum setup, A/D and plug-in parts only when set.
    pub fn changes(&self, t: &XgTable, all: bool) -> Vec<([u8; 3], Vec<u8>)> {
        let mut out: Vec<([u8; 3], Vec<u8>)> = Vec::new();
        let mut covered = std::collections::HashSet::new();
        for b in &t.blocks {
            let full = all && ["System", "Rev", "Cho", "Var", "Eq", "Ins", "Part"].iter().any(|k| b.key.starts_with(k));
            for inst in 0..b.instances() {
                let (h, m) = b.hm(inst);
                for p in b.params.iter().filter(|p| p.size() > 0) {
                    let addr = [h, m, p.lo];
                    let at: Vec<[u8; 3]> = (0..p.size()).map(|i| offset(addr, i)).collect();
                    let set = at.iter().any(|a| self.bytes.contains_key(a));
                    if !(set || (full && !p.is_effect_slot())) || covered.contains(&addr) {
                        continue;
                    }
                    let data = at
                        .iter()
                        .zip(&p.bytes)
                        .map(|(a, (_, _, d))| self.get(*a).unwrap_or(*d.get(inst).or(d.first()).unwrap_or(&0)))
                        .collect();
                    covered.extend(at);
                    out.push((addr, data));
                }
            }
        }
        // bytes the table does not describe (another model's memory)
        out.extend(self.bytes.iter().filter(|(a, _)| !covered.contains(*a)).map(|(a, v)| (*a, vec![*v])));
        out.sort_by_key(|c| c.0);
        out
    }
}

/// The data of an XG Bulk Dump (`43 0n 4C sh sl ah am al data… cs F7`):
/// (start address, data); None if it is not one or the checksum is wrong.
pub fn parse_xg_bulk(d: &[u8]) -> Option<([u8; 3], &[u8])> {
    let d = d.strip_prefix(&[0xF0]).unwrap_or(d);
    if d.len() < 10 || d[0] != 0x43 || d[1] & 0xF0 != 0x00 || d[2] != 0x4C {
        return None;
    }
    let n = ((d[3] as usize) << 7) | d[4] as usize;
    let body = d.get(3..3 + 2 + 3 + n + 1)?; // count, address, data, checksum
    if body.iter().fold(0u32, |s, &b| s + b as u32) & 0x7F != 0 {
        return None;
    }
    Some(([d[5], d[6], d[7]], &d[8..8 + n]))
}

/// XG Bulk Dump Request for the block at `addr`.
pub fn xg_bulk_request(dev: u8, addr: [u8; 3]) -> Vec<u8> {
    vec![0xF0, 0x43, 0x20 | (dev & 0x0F), 0x4C, addr[0], addr[1], addr[2], 0xF7]
}

/// The SysEx messages of a .SYX file (each F0 … F7, without the F0).
pub fn split_syx(bytes: &[u8]) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    let mut i = 0;
    while let Some(s) = bytes[i..].iter().position(|&b| b == 0xF0).map(|k| i + k) {
        let Some(e) = bytes[s + 1..].iter().position(|&b| b == 0xF7).map(|k| s + 1 + k) else { break };
        out.push(bytes[s + 1..=e].to_vec());
        i = e + 1;
    }
    out
}

/// A .SYX file: XG System On, then the memory's parameter changes.
pub fn to_syx(t: &XgTable, m: &XgMemory, dev: u8) -> Vec<u8> {
    let mut out = crate::mixer::xg_reset(dev);
    for (addr, data) in m.changes(t, false) {
        out.extend(xg_param(dev, addr, &data));
    }
    out
}

/// XG Multi Part parameter (low address) that a setup controller sets.
/// Each channel's first note in a track (u32::MAX for none): a part's setup
/// is what comes before it. A format-0 track keeps the notes of its other
/// channels as events, so each channel gets its own limit.
pub(crate) fn first_notes(t: &EditTrack) -> [u32; 16] {
    let mut f = [u32::MAX; 16];
    let own = &mut f[(t.channel & 0x0F) as usize];
    *own = t.notes.iter().map(|n| n.tick).min().unwrap_or(u32::MAX);
    for e in &t.others {
        if let Event::Note { t, ch, .. } = e {
            let k = (ch & 0x0F) as usize;
            f[k] = f[k].min(*t);
        }
    }
    f
}

/// The CC that sets a Multi Part parameter (the reverse of `cc_part_param`).
fn part_param_cc(lo: u8) -> Option<u8> {
    (0..128u8).find(|&cc| cc_part_param(cc) == Some(lo))
}

/// The track that plays a part (port group, channel): the one on that
/// channel with notes, else one with events on it (format 0), else one set
/// to it, else the first track.
pub(crate) fn part_track(tracks: &[EditTrack], part: u8) -> Option<usize> {
    let (group, ch) = (part / 16, part & 0x0F);
    let on_port = |t: &EditTrack| t.port().saturating_sub(1).min(3) == group;
    let uses = |t: &EditTrack| {
        t.others.iter().any(|e| match e {
            Event::Note { ch: c, .. } => c & 0x0F == ch,
            Event::Chan { data, .. } => data.first().is_some_and(|s| s & 0x0F == ch),
            _ => false,
        })
    };
    tracks
        .iter()
        .position(|t| on_port(t) && t.channel & 0x0F == ch && !t.notes.is_empty())
        .or_else(|| tracks.iter().position(|t| on_port(t) && uses(t)))
        .or_else(|| tracks.iter().position(|t| on_port(t) && t.channel & 0x0F == ch))
        .or(if tracks.is_empty() { None } else { Some(0) })
}

fn cc_part_param(cc: u8) -> Option<u8> {
    Some(match cc {
        0 => 0x01,  // bank MSB
        32 => 0x02, // bank LSB
        7 => 0x0B,  // volume
        10 => 0x0E, // pan
        93 => 0x12, // chorus send
        91 => 0x13, // reverb send
        94 => 0x14, // variation send
        74 => 0x18, // filter cutoff
        71 => 0x19, // filter resonance
        73 => 0x1A, // EG attack
        75 => 0x1B, // EG decay
        72 => 0x1C, // EG release
        _ => return None,
    })
}

/// XG or GM System On / All Parameter Reset.
pub fn is_reset(d: &[u8]) -> bool {
    is_gm_on(d) || crate::mixer::parse_xg_change(d).is_some_and(|(a, _)| a == [0, 0, 0x7E] || a == [0, 0, 0x7F])
}

/// GM System On (`F0 7E dd 09 01 F7`).
fn is_gm_on(d: &[u8]) -> bool {
    d.len() >= 4 && d[0] == 0x7E && d[2] == 0x09 && d[3] == 0x01
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

/// Where a parameter byte's value in the song comes from.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Source {
    /// XG Parameter Change / Bulk Dump: the data byte at this index (without F0).
    Sysex(usize),
    /// A part's channel message (Program, Bank / CC, RPN 0 data entry).
    Chan,
}

/// Write XG Editor changes into the song where the values come from, as
/// `XgMemory::from_song` reads them: the message that sets the parameter last
/// (an XG Parameter Change or Bulk Dump in any track, or a part's Program /
/// Bank / Volume / Pan / send / filter / EG / Pitch Bend range message before
/// its first note) gets the new value. Values the song does not set (or a
/// channel message cannot carry) go into `tracks[0]` as XG Parameter Changes,
/// as `apply_xg_changes` does — except a part's Bank / Program, which go in
/// as Bank Select + Program Change in the part's track (a later Program
/// Change in the song would undo a SysEx bank).
pub fn apply_xg_changes_song(tracks: &mut [EditTrack], dev: u8, changes: &[([u8; 3], Vec<u8>)]) {
    apply_part_changes_song(tracks, dev, changes, false);
}

/// `apply_xg_changes_song` for a GM / GM2 / GS instrument (`chan_only`):
/// every new value goes in as a channel message (Bank Select, Program
/// Change, CC) in the part's track; values without one are left out.
pub fn apply_part_changes_song(tracks: &mut [EditTrack], dev: u8, changes: &[([u8; 3], Vec<u8>)], chan_only: bool) {
    // the setup messages in the order from_song_range reads them
    let mut evs: Vec<(usize, usize, u32, bool, u8)> = Vec::new();
    for (ti, t) in tracks.iter().enumerate() {
        let limit = first_notes(t);
        let base = t.port().saturating_sub(1).min(3) * 16;
        for (ei, e) in t.others.iter().enumerate() {
            match e {
                Event::Sysex { t, status: 0xF0, data } => evs.push((ti, ei, *t, is_reset(data), 0)),
                Event::Chan { t, data } if data.len() >= 2 && *t <= limit[(data[0] & 0x0F) as usize] => {
                    evs.push((ti, ei, *t, false, base + (data[0] & 0x0F)))
                }
                _ => {}
            }
        }
    }
    evs.sort_by_key(|e| (e.2, !e.3));
    let fx_types = [FxSlot::Reverb, FxSlot::Chorus, FxSlot::Variation, FxSlot::Insertion(0), FxSlot::Insertion(1)].map(|s| s.type_addr());
    // the last writer of every address
    let mut writer: HashMap<[u8; 3], (usize, usize, Source)> = HashMap::new();
    let mut rpn = [[0x7Fu8; 2]; 64];
    for &(ti, ei, _, reset, part) in &evs {
        match &tracks[ti].others[ei] {
            Event::Sysex { data, .. } => {
                if reset {
                    writer.clear();
                } else if let Some((addr, v)) = crate::mixer::parse_xg_change(data) {
                    if fx_types.contains(&addr) {
                        // a new effect type resets that effect's parameters
                        writer.retain(|a, _| {
                            let mut m = XgMemory::default();
                            m.set(*a, &[0]);
                            m.apply_sysex(data);
                            m.get(*a).is_some()
                        });
                    }
                    for i in 0..v.len() {
                        writer.insert(offset(addr, i), (ti, ei, Source::Sysex(6 + i)));
                    }
                } else if let Some((addr, v)) = parse_xg_bulk(data) {
                    for i in 0..v.len() {
                        writer.insert(offset(addr, i), (ti, ei, Source::Sysex(8 + i)));
                    }
                }
            }
            Event::Chan { data, .. } => {
                let (p, at) = (part as usize, |lo: u8| [0x08, part, lo]);
                match (data[0] & 0xF0, data[1], data.get(2).copied().unwrap_or(0)) {
                    (0xC0, _, _) => {
                        writer.insert(at(0x03), (ti, ei, Source::Chan));
                    }
                    (0xB0, 101, v) => rpn[p][0] = v,
                    (0xB0, 100, v) => rpn[p][1] = v,
                    (0xB0, 6, _) if rpn[p] == [0, 0] => {
                        writer.insert(at(0x23), (ti, ei, Source::Chan));
                    }
                    (0xB0, cc, _) => {
                        if let Some(lo) = cc_part_param(cc) {
                            writer.insert(at(lo), (ti, ei, Source::Chan));
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    let mut rest: Vec<([u8; 3], Vec<u8>)> = Vec::new();
    let mut bulks: Vec<(usize, usize)> = Vec::new();
    for (addr, data) in changes {
        for (i, &v) in data.iter().enumerate() {
            let a = offset(*addr, i);
            let done = match writer.get(&a) {
                Some(&(ti, ei, src)) => match (&mut tracks[ti].others[ei], src) {
                    (Event::Sysex { data: d, .. }, Source::Sysex(k)) if k < d.len() => {
                        d[k] = v & 0x7F;
                        if d[1] & 0xF0 == 0x00 {
                            bulks.push((ti, ei));
                        }
                        true
                    }
                    (Event::Chan { data: d, .. }, Source::Chan) => match d[0] & 0xF0 {
                        0xC0 => {
                            d[1] = v & 0x7F;
                            true
                        }
                        // Pitch Bend range: data entry 0..24 = XG 0x40..0x58
                        0xB0 if d[1] == 6 && d.len() >= 3 => {
                            let ok = (0x40..=0x58).contains(&v);
                            if ok {
                                d[2] = v - 0x40;
                            }
                            ok
                        }
                        0xB0 if d.len() >= 3 => {
                            d[2] = v & 0x7F;
                            true
                        }
                        _ => false,
                    },
                    _ => false,
                },
                None => false,
            };
            if !done {
                rest.push((a, vec![v]));
            }
        }
    }
    // a changed Bulk Dump gets its checksum again (over count, address, data)
    for (ti, ei) in bulks {
        if let Event::Sysex { data: d, .. } = &mut tracks[ti].others[ei] {
            let n = ((d[3] as usize) << 7) | d[4] as usize;
            if d.len() > 8 + n {
                let sum: u32 = d[3..8 + n].iter().map(|&b| b as u32).sum();
                d[8 + n] = ((0x80 - (sum & 0x7F)) & 0x7F) as u8;
            }
        }
    }
    let reset_t = evs.iter().filter(|e| e.3).map(|e| e.2).max().unwrap_or(0);
    // part values that go in as channel messages: (part, lo, value)
    let (chan, rest): (Vec<_>, Vec<_>) = rest.into_iter().partition(|(a, _)| {
        a[0] == 0x08 && a[1] < 64 && (matches!(a[2], 1..=3) || (chan_only && part_param_cc(a[2]).is_some()))
    });
    let mut parts: Vec<u8> = chan.iter().map(|(a, _)| a[1]).collect();
    parts.dedup();
    for part in parts {
        let vals: HashMap<u8, u8> = chan.iter().filter(|(a, _)| a[1] == part).map(|(a, v)| (a[2], v[0])).collect();
        let Some(ti) = part_track(tracks, part) else { continue };
        let ch = part & 0x0F;
        let first = first_notes(&tracks[ti])[ch as usize];
        let tick = reset_t.min(first).min(u32::MAX - 1);
        let mut msgs: Vec<Vec<u8>> = Vec::new();
        for (lo, cc) in [(1u8, 0u8), (2, 32)] {
            if let Some(&v) = vals.get(&lo) {
                msgs.push(vec![0xB0 | ch, cc, v]);
            }
        }
        // a bank goes right before the song's own Program Change
        let pc = writer.get(&[0x08, part, 0x03]).map(|&(t, e, _)| (t, e));
        if let (false, Some((pt, pe))) = (msgs.is_empty(), pc) {
            let at = tracks[pt].others[pe].t();
            for (k, m) in msgs.drain(..).enumerate() {
                tracks[pt].others.insert(pe + k, Event::Chan { t: at, data: m });
            }
        }
        if let Some(&v) = vals.get(&3) {
            msgs.push(vec![0xC0 | ch, v]);
        }
        let mut ccs: Vec<(u8, u8)> = vals.iter().filter_map(|(&lo, &v)| Some((part_param_cc(lo)?, v))).collect();
        ccs.sort();
        msgs.extend(ccs.into_iter().filter(|(cc, _)| *cc != 0 && *cc != 32).map(|(cc, v)| vec![0xB0 | ch, cc, v]));
        let t = &mut tracks[ti];
        let mut pos = t.others.iter().position(|e| e.t() > tick).unwrap_or(t.others.len());
        for m in msgs {
            t.others.insert(pos, Event::Chan { t: tick, data: m });
            pos += 1;
        }
    }
    let rest: Vec<([u8; 3], Vec<u8>)> = if chan_only { Vec::new() } else { rest };
    if let (false, Some(t0)) = (rest.is_empty(), tracks.first_mut()) {
        // bytes of one multi-byte parameter stay one message
        let mut merged: Vec<([u8; 3], Vec<u8>)> = Vec::new();
        for (a, v) in rest {
            match merged.last_mut() {
                Some((s, d)) if offset(*s, d.len()) == a && changes.iter().any(|c| c.0 == *s && c.1.len() > d.len()) => d.extend(v),
                _ => merged.push((a, v)),
            }
        }
        // after the song's last System On (a later reset would wipe them)
        if reset_t == 0 {
            apply_xg_changes(t0, dev, &merged);
        } else {
            let mut pos = t0.others.iter().position(|e| e.t() > reset_t).unwrap_or(t0.others.len());
            for (a, v) in merged {
                let b = xg_param(dev, a, &v);
                t0.others.insert(pos, Event::Sysex { t: reset_t, status: 0xF0, data: b[1..].to_vec() });
                pos += 1;
            }
        }
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

/// The XG Editor's model tree (`[TG Tree]`: MU Series, Sound Card, …), each
/// group's model names in its order; the entries point into `[TG_Model_List]`.
pub fn parse_model_tree(bytes: &[u8]) -> Vec<(String, Vec<String>)> {
    let ini = Ini::parse(bytes);
    let list: Vec<(String, String)> = ini.section("TG_Model_List").map(|s| s.to_vec()).unwrap_or_default();
    let name_of = |idx: &str| list.iter().find(|kv| kv.0 == idx).map(|kv| kv.1.clone());
    ini.section("TG Tree")
        .into_iter()
        .flatten()
        .filter(|kv| kv.0 != "size")
        .map(|(_, group)| {
            let models = ini
                .section(group)
                .into_iter()
                .flatten()
                .filter(|kv| kv.0 != "size")
                .filter_map(|kv| name_of(kv.1.split(',').next()?.trim()))
                .collect();
            (group.clone(), models)
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

    #[test]
    fn song_setup_channel_messages() {
        let cc = |t, d: &[u8]| Event::Chan { t, data: d.to_vec() };
        let t = EditTrack {
            name: "t".into(),
            channel: 2,
            notes: vec![crate::EditNote { tick: 480, dur: 10, pitch: 60, vel: 100 }],
            others: vec![
                Event::Sysex { t: 0, status: 0xF0, data: vec![0x43, 0x10, 0x4C, 0, 0, 0x7E, 0, 0xF7] },
                cc(10, &[0xB2, 0, 0]),
                cc(10, &[0xB2, 32, 1]),
                cc(10, &[0xC2, 24]),
                cc(10, &[0xB2, 7, 90]),
                cc(10, &[0xB2, 10, 0]),
                cc(10, &[0xB2, 101, 0]),
                cc(10, &[0xB2, 100, 0]),
                cc(10, &[0xB2, 6, 12]),
                cc(10, &[0xB9, 0, 127]), // format-0 track: another channel
                cc(960, &[0xB2, 7, 20]), // automation after the first note
            ],
            ..Default::default()
        };
        let m = XgMemory::from_song(std::slice::from_ref(&t));
        assert_eq!(m.get([8, 2, 1]), Some(0));
        assert_eq!(m.get([8, 2, 2]), Some(1));
        assert_eq!(m.get([8, 2, 3]), Some(24));
        assert_eq!(m.get([8, 2, 0x0B]), Some(90));
        assert_eq!(m.get([8, 2, 0x0E]), Some(1));
        assert_eq!(m.get([8, 2, 0x23]), Some(0x40 + 12));
        assert_eq!(m.get([8, 9, 1]), Some(127));
    }

    #[test]
    fn gm_gs_values_go_in_as_channel_messages() {
        let note = |t| crate::EditNote { tick: t, dur: 10, pitch: 60, vel: 100 };
        let drums = EditTrack { channel: 9, notes: vec![note(0)], ..Default::default() };
        // the bass has a Program Change but no bank yet
        let bass = EditTrack { channel: 1, notes: vec![note(960)], others: vec![Event::Chan { t: 480, data: vec![0xC1, 33] }], ..Default::default() };
        let pad = EditTrack { channel: 2, notes: vec![note(0)], ..Default::default() };
        let mut song = vec![drums, bass, pad];
        // GS SC-88Pro: the bass gets variation 8, the pad a sound, a volume and
        // a parameter no channel message carries (left out)
        let changes = [([8, 1, 1], vec![8, 3, 33]), ([8, 2, 1], vec![0, 3, 89]), ([8, 2, 0x0B], vec![90]), ([8, 2, 0x30], vec![1])];
        apply_part_changes_song(&mut song, 0, &changes, true);
        assert_eq!(
            song[1].others,
            vec![
                Event::Chan { t: 480, data: vec![0xB1, 0, 8] },
                Event::Chan { t: 480, data: vec![0xB1, 32, 3] },
                Event::Chan { t: 480, data: vec![0xC1, 33] },
            ]
        );
        assert_eq!(
            song[2].others,
            vec![
                Event::Chan { t: 0, data: vec![0xB2, 0, 0] },
                Event::Chan { t: 0, data: vec![0xB2, 32, 3] },
                Event::Chan { t: 0, data: vec![0xC2, 89] },
                Event::Chan { t: 0, data: vec![0xB2, 7, 90] },
            ]
        );
        assert!(song[0].others.is_empty(), "no XG SysEx for a GS instrument");
        let m = XgMemory::from_song(&song);
        assert_eq!(m.read([8, 1, 1], &[0, 0, 0]), vec![8, 3, 33]);
        // a second edit changes those messages instead of adding more
        apply_part_changes_song(&mut song, 0, &[([8, 2, 1], vec![8, 3, 90])], true);
        assert_eq!(song[2].others.len(), 4);
        assert_eq!(XgMemory::from_song(&song).read([8, 2, 1], &[0, 0, 0]), vec![8, 3, 90]);
    }

    #[test]
    fn format0_part_entering_late_keeps_its_setup() {
        // one track, channel 1 plays from the start; channel 2 gets its program
        // just before its own first note at bar 3
        let t = EditTrack {
            channel: 0,
            notes: vec![crate::EditNote { tick: 0, dur: 10, pitch: 60, vel: 100 }],
            others: vec![
                Event::Chan { t: 3840, data: vec![0xC1, 33] },
                Event::Note { t: 3840, ch: 1, note: 40, vel: 100, gate: 10 },
                Event::Chan { t: 5000, data: vec![0xC1, 40] }, // a change while it plays
            ],
            ..Default::default()
        };
        let m = XgMemory::from_song(std::slice::from_ref(&t));
        assert_eq!(m.get([8, 1, 3]), Some(33));
        let mut song = vec![t];
        apply_xg_changes_song(&mut song, 0, &[([8, 1, 3], vec![34])]);
        assert_eq!(song[0].others[0], Event::Chan { t: 3840, data: vec![0xC1, 34] });
        assert_eq!(song[0].others[2], Event::Chan { t: 5000, data: vec![0xC1, 40] });
    }

    #[test]
    fn changes_go_where_the_song_sets_them() {
        let cc = |t, d: &[u8]| Event::Chan { t, data: d.to_vec() };
        let reset = Event::Sysex { t: 0, status: 0xF0, data: vec![0x43, 0x10, 0x4C, 0, 0, 0x7E, 0, 0xF7] };
        let rev = Event::Sysex { t: 5, status: 0xF0, data: xg_param(0, [2, 1, 0x0C], &[64])[1..].to_vec() };
        // bulk dump of part 3 bytes 0..3
        let mut bulk = vec![0x43, 0x00, 0x4C, 0x00, 0x04, 0x08, 0x02, 0x00, 2, 0, 0, 24];
        let sum: u32 = bulk[3..].iter().map(|&b| b as u32).sum();
        bulk.push(((0x80 - (sum & 0x7F)) & 0x7F) as u8);
        bulk.push(0xF7);
        let t0 = EditTrack { name: "setup".into(), others: vec![reset, rev, Event::Sysex { t: 6, status: 0xF0, data: bulk }], ..Default::default() };
        let t1 = EditTrack {
            name: "bass".into(),
            channel: 1,
            notes: vec![crate::EditNote { tick: 480, dur: 10, pitch: 40, vel: 100 }],
            others: vec![cc(10, &[0xC1, 33]), cc(10, &[0xB1, 7, 100]), cc(10, &[0xB1, 101, 0]), cc(10, &[0xB1, 100, 0]), cc(10, &[0xB1, 6, 2]), cc(960, &[0xB1, 7, 20])],
            ..Default::default()
        };
        let mut song = vec![t0, t1];
        let n0 = song[0].others.len();
        apply_xg_changes_song(&mut song, 0, &[
            ([8, 1, 0x0B], vec![80]),   // volume: the CC 7 before the first note
            ([8, 1, 0x03], vec![5]),    // program change
            ([8, 1, 0x23], vec![0x4C]), // pitch bend range 12: the RPN data entry
            ([2, 1, 0x0C], vec![90]),   // reverb return: the XG SysEx
            ([8, 2, 0x03], vec![30]),   // part 3 program: inside the bulk dump
            ([8, 5, 0x0B], vec![70]),   // not in the song: a new SysEx
        ]);
        assert_eq!(song[1].others[1], cc(10, &[0xB1, 7, 80]));
        assert_eq!(song[1].others[0], cc(10, &[0xC1, 5]));
        assert_eq!(song[1].others[4], cc(10, &[0xB1, 6, 12]));
        assert_eq!(song[1].others[5], cc(960, &[0xB1, 7, 20]), "automation after the first note stays");
        assert_eq!(song[0].others.len(), n0 + 1);
        let m = XgMemory::from_song(&song);
        assert_eq!(m.get([8, 1, 0x0B]), Some(80));
        assert_eq!(m.get([8, 1, 0x03]), Some(5));
        assert_eq!(m.get([8, 1, 0x23]), Some(0x4C));
        assert_eq!(m.get([2, 1, 0x0C]), Some(90));
        assert_eq!(m.get([8, 2, 0x03]), Some(30), "bulk dump edited, checksum still right");
        assert_eq!(m.get([8, 5, 0x0B]), Some(70));
    }

    #[test]
    fn bulk_dump_and_syx() {
        // MU-style bulk dump of Multi Part 3, bytes 0..3 (element reserve, MSB, LSB, program)
        let data = [2u8, 0, 0, 24];
        let mut d = vec![0xF0, 0x43, 0x00, 0x4C, 0x00, 0x04, 0x08, 0x02, 0x00];
        d.extend(data);
        let sum: u32 = d[4..].iter().map(|&b| b as u32).sum();
        d.push(((0x80 - (sum & 0x7F)) & 0x7F) as u8);
        d.push(0xF7);
        assert_eq!(parse_xg_bulk(&d), Some(([8, 2, 0], &data[..])));
        let mut bad = d.clone();
        bad[9] ^= 1;
        assert_eq!(parse_xg_bulk(&bad), None);
        let mut file = crate::mixer::xg_reset(0);
        file.extend(&d);
        file.extend(xg_param(0, [8, 2, 0x0B], &[90]));
        let msgs = split_syx(&file);
        assert_eq!(msgs.len(), 3);
        let mut m = XgMemory::default();
        m.set([0, 0, 4], &[1]);
        assert!(msgs.iter().all(|x| m.apply_sysex(x)));
        assert_eq!(m.get([0, 0, 4]), None); // System On cleared it
        assert_eq!(m.get([8, 2, 3]), Some(24));
        assert_eq!(m.get([8, 2, 0x0B]), Some(90));
        assert_eq!(xg_bulk_request(1, [8, 2, 0]), vec![0xF0, 0x43, 0x21, 0x4C, 8, 2, 0, 0xF7]);
    }

    #[test]
    fn memory_as_changes() {
        let t = parse_tbl(TBL.as_bytes());
        let mut m = XgMemory::default();
        m.set([0, 0, 2], &[3]); // one byte of the 4-byte Master Tune
        m.set([8, 9, 3], &[5]);
        let c = m.changes(&t, false);
        assert_eq!(c, vec![([0, 0, 0], vec![0, 4, 3, 0]), ([8, 9, 3], vec![5])]);
        let all = m.changes(&t, true);
        assert!(all.contains(&([0, 0, 4], vec![0x7F])));
        assert!(all.contains(&([8, 9, 1], vec![0x7F]))); // per-part default
        assert_eq!(all.len(), 2 + 64 * 2);
        let syx = to_syx(&t, &m, 0);
        assert_eq!(split_syx(&syx).len(), 3);
    }
}
