//! SoundFont 2 loader (user-supplied samples for Sample elements) and WAV
//! import. Only what the voice needs: zones with key/velocity ranges, root
//! key, tuning, loop, attenuation and pan.

use std::sync::Arc;

#[derive(Clone, Debug)]
pub struct Zone {
    pub key: (u8, u8),
    pub vel: (u8, u8),
    pub data: Arc<Vec<f32>>,
    pub start: usize,
    pub end: usize,
    pub loop_start: usize,
    pub loop_end: usize,
    pub looped: bool,
    pub root: u8,
    /// Tuning in cents (coarse + fine + sample correction).
    pub tune: f32,
    pub rate: u32,
    /// Linear gain from initialAttenuation.
    pub gain: f32,
    /// -1..1
    pub pan: f32,
}

#[derive(Clone, Debug)]
pub struct Preset {
    pub name: String,
    pub bank: u16,
    pub program: u16,
    pub zones: Vec<Zone>,
}

impl Preset {
    pub fn find(&self, key: u8, vel: u8) -> impl Iterator<Item = &Zone> {
        self.zones.iter().filter(move |z| z.key.0 <= key && key <= z.key.1 && z.vel.0 <= vel && vel <= z.vel.1)
    }
}

#[derive(Clone, Debug, Default)]
pub struct SoundFont {
    pub name: String,
    pub presets: Vec<Preset>,
}

impl SoundFont {
    pub fn preset(&self, bank: u16, program: u16) -> Option<&Preset> {
        let exact = self.presets.iter().find(|p| p.bank == bank && p.program == program);
        exact.or_else(|| {
            if bank >= 128 {
                self.presets.iter().find(|p| p.bank >= 128)
            } else {
                self.presets.iter().find(|p| p.program == program).or(self.presets.first())
            }
        })
    }
}

fn u16le(d: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([d[o], d[o + 1]])
}
fn u32le(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(d[o..o + 4].try_into().unwrap())
}
fn i16le(d: &[u8], o: usize) -> i16 {
    i16::from_le_bytes([d[o], d[o + 1]])
}

/// RIFF chunks inside `d` (id, body).
fn chunks(d: &[u8]) -> Vec<(&[u8], &[u8])> {
    let mut out = Vec::new();
    let mut o = 0;
    while o + 8 <= d.len() {
        let id = &d[o..o + 4];
        let len = u32le(d, o + 4) as usize;
        let end = (o + 8 + len).min(d.len());
        out.push((id, &d[o + 8..end]));
        o = o + 8 + len + (len & 1);
    }
    out
}

#[derive(Clone, Copy)]
struct Gens {
    set: [bool; 61],
    v: [i16; 61],
}

impl Default for Gens {
    fn default() -> Self {
        Gens { set: [false; 61], v: [0; 61] }
    }
}

impl Gens {
    fn get(&self, g: usize) -> Option<i16> {
        self.set[g].then_some(self.v[g])
    }
    fn range(&self, g: usize) -> Option<(u8, u8)> {
        self.get(g).map(|v| {
            let [lo, hi] = v.to_le_bytes();
            (lo, hi)
        })
    }
}

const G_START: usize = 0;
const G_END: usize = 1;
const G_LSTART: usize = 2;
const G_LEND: usize = 3;
const G_START_C: usize = 4;
const G_END_C: usize = 12;
const G_PAN: usize = 17;
const G_INSTRUMENT: usize = 41;
const G_KEYRANGE: usize = 43;
const G_VELRANGE: usize = 44;
const G_LSTART_C: usize = 45;
const G_ATTEN: usize = 48;
const G_LEND_C: usize = 50;
const G_COARSE: usize = 51;
const G_FINE: usize = 52;
const G_SAMPLE: usize = 53;
const G_MODES: usize = 54;
const G_ROOT: usize = 58;

/// Generator lists per bag: (bag ranges, gens).
fn bags(bag: &[u8], gen: &[u8], first: usize, last: usize) -> Vec<Gens> {
    let mut out = Vec::new();
    for b in first..last {
        let g0 = u16le(bag, b * 4) as usize;
        let g1 = if (b + 1) * 4 + 2 <= bag.len() { u16le(bag, (b + 1) * 4) as usize } else { g0 };
        let mut gs = Gens::default();
        for g in g0..g1 {
            if g * 4 + 4 > gen.len() {
                break;
            }
            let op = u16le(gen, g * 4) as usize;
            if op < 61 {
                gs.set[op] = true;
                gs.v[op] = i16le(gen, g * 4 + 2);
            }
        }
        out.push(gs);
    }
    out
}

pub fn parse(d: &[u8]) -> Option<SoundFont> {
    if d.len() < 12 || &d[..4] != b"RIFF" || &d[8..12] != b"sfbk" {
        return None;
    }
    let (mut smpl, mut name) = (&[][..], String::new());
    let mut pd: std::collections::HashMap<&[u8], &[u8]> = Default::default();
    for (id, body) in chunks(&d[12..]) {
        if id != b"LIST" || body.len() < 4 {
            continue;
        }
        for (cid, cb) in chunks(&body[4..]) {
            match &body[..4] {
                b"INFO" if cid == b"INAM" => name = String::from_utf8_lossy(cb).trim_end_matches('\0').to_string(),
                b"sdta" if cid == b"smpl" => smpl = cb,
                b"pdta" => {
                    pd.insert(cid, cb);
                }
                _ => {}
            }
        }
    }
    let data: Arc<Vec<f32>> = Arc::new(smpl.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect());
    let (phdr, pbag, pgen) = (pd.get(&b"phdr"[..])?, pd.get(&b"pbag"[..])?, pd.get(&b"pgen"[..])?);
    let (inst, ibag, igen, shdr) = (pd.get(&b"inst"[..])?, pd.get(&b"ibag"[..])?, pd.get(&b"igen"[..])?, pd.get(&b"shdr"[..])?);

    struct Shdr {
        start: usize,
        end: usize,
        ls: usize,
        le: usize,
        rate: u32,
        key: u8,
        corr: i8,
    }
    let samples: Vec<Shdr> = shdr
        .chunks_exact(46)
        .map(|s| Shdr {
            start: u32le(s, 20) as usize,
            end: u32le(s, 24) as usize,
            ls: u32le(s, 28) as usize,
            le: u32le(s, 32) as usize,
            rate: u32le(s, 36),
            key: s[40],
            corr: s[41] as i8,
        })
        .collect();

    let n_inst = inst.len() / 22;
    let mut instruments: Vec<Vec<Gens>> = Vec::with_capacity(n_inst);
    for i in 0..n_inst.saturating_sub(1) {
        let b0 = u16le(inst, i * 22 + 20) as usize;
        let b1 = u16le(inst, (i + 1) * 22 + 20) as usize;
        instruments.push(bags(ibag, igen, b0, b1));
    }

    let n_pre = phdr.len() / 38;
    let mut presets = Vec::new();
    for p in 0..n_pre.saturating_sub(1) {
        let h = &phdr[p * 38..p * 38 + 38];
        let pname = String::from_utf8_lossy(&h[..20]).trim_end_matches('\0').to_string();
        let program = u16le(h, 20);
        let bank = u16le(h, 22);
        let b0 = u16le(h, 24) as usize;
        let b1 = u16le(&phdr[(p + 1) * 38..], 24) as usize;
        let pzones = bags(pbag, pgen, b0, b1);
        let pglobal = pzones.first().filter(|z| z.get(G_INSTRUMENT).is_none()).copied().unwrap_or_default();
        let mut zones = Vec::new();
        for pz in &pzones {
            let Some(ii) = pz.get(G_INSTRUMENT) else { continue };
            let Some(izones) = instruments.get(ii as usize) else { continue };
            let iglobal = izones.first().filter(|z| z.get(G_SAMPLE).is_none()).copied().unwrap_or_default();
            for iz in izones {
                let Some(si) = iz.get(G_SAMPLE) else { continue };
                let Some(s) = samples.get(si as usize) else { continue };
                let ig = |g: usize| iz.get(g).or(iglobal.get(g));
                let pg = |g: usize| pz.get(g).or(pglobal.get(g)).unwrap_or(0) as i32;
                let isum = |g: usize| ig(g).unwrap_or(0) as i32;
                let key = intersect(iz.range(G_KEYRANGE).or(iglobal.range(G_KEYRANGE)), pz.range(G_KEYRANGE).or(pglobal.range(G_KEYRANGE)));
                let vel = intersect(iz.range(G_VELRANGE).or(iglobal.range(G_VELRANGE)), pz.range(G_VELRANGE).or(pglobal.range(G_VELRANGE)));
                let off = |fine: usize, coarse: usize| (isum(fine) + pg(fine)) as i64 + 32768 * (isum(coarse) + pg(coarse)) as i64;
                let clampi = |v: i64| v.clamp(0, data.len() as i64) as usize;
                let start = clampi(s.start as i64 + off(G_START, G_START_C));
                let end = clampi(s.end as i64 + off(G_END, G_END_C)).max(start + 1).min(data.len());
                let ls = clampi(s.ls as i64 + off(G_LSTART, G_LSTART_C));
                let le = clampi(s.le as i64 + off(G_LEND, G_LEND_C));
                let root = ig(G_ROOT).filter(|&r| r >= 0).map(|r| r as u8).unwrap_or(if s.key > 127 { 60 } else { s.key });
                let tune = (isum(G_COARSE) + pg(G_COARSE)) as f32 * 100.0 + (isum(G_FINE) + pg(G_FINE)) as f32 + s.corr as f32;
                let atten = (isum(G_ATTEN) + pg(G_ATTEN)).max(0) as f32 / 10.0 * 0.4; // cB, EMU 0.4 factor
                let pan = ((isum(G_PAN) + pg(G_PAN)) as f32 / 500.0).clamp(-1.0, 1.0);
                zones.push(Zone {
                    key,
                    vel,
                    data: data.clone(),
                    start,
                    end,
                    loop_start: ls.max(start),
                    loop_end: le.min(end),
                    looped: ig(G_MODES).unwrap_or(0) & 1 == 1 && le > ls + 4,
                    root,
                    tune,
                    rate: s.rate.max(1),
                    gain: 10f32.powf(-atten / 20.0),
                    pan,
                });
            }
        }
        presets.push(Preset { name: pname, bank, program, zones });
    }
    presets.sort_by_key(|p| (p.bank, p.program));
    Some(SoundFont { name, presets })
}

fn intersect(a: Option<(u8, u8)>, b: Option<(u8, u8)>) -> (u8, u8) {
    let a = a.unwrap_or((0, 127));
    let b = b.unwrap_or((0, 127));
    (a.0.max(b.0), a.1.min(b.1))
}

/// A mono/stereo PCM WAV as a single-zone preset across the keyboard.
pub fn parse_wav(name: &str, d: &[u8], root: u8) -> Option<Preset> {
    if d.len() < 12 || &d[..4] != b"RIFF" || &d[8..12] != b"WAVE" {
        return None;
    }
    let (mut fmt, mut pcm) = (None, None);
    let mut lp = None;
    let mut unity = None;
    for (id, body) in chunks(&d[12..]) {
        if id == b"smpl" && body.len() >= 16 {
            let u = u32le(body, 12);
            if u > 0 && u < 128 {
                unity = Some(u as u8);
            }
        }
        match id {
            b"fmt " if body.len() >= 16 => fmt = Some((u16le(body, 0), u16le(body, 2), u32le(body, 4), u16le(body, 14))),
            b"data" => pcm = Some(body),
            b"smpl" if body.len() >= 60 && u32le(body, 28) > 0 => lp = Some((u32le(body, 44 + 8) as usize, u32le(body, 44 + 12) as usize)),
            _ => {}
        }
    }
    let (format, ch, rate, bits) = fmt?;
    let pcm = pcm?;
    let ch = ch.max(1) as usize;
    let frames: Vec<f32> = match (format, bits) {
        (1, 16) => pcm.chunks_exact(2 * ch).map(|f| (0..ch).map(|c| i16le(f, 2 * c) as f32).sum::<f32>() / (32768.0 * ch as f32)).collect(),
        (1, 24) => pcm
            .chunks_exact(3 * ch)
            .map(|f| (0..ch).map(|c| (i32::from_le_bytes([0, f[3 * c], f[3 * c + 1], f[3 * c + 2]]) >> 8) as f32).sum::<f32>() / (8388608.0 * ch as f32))
            .collect(),
        (1, 8) => pcm.chunks_exact(ch).map(|f| f.iter().map(|&b| b as f32 - 128.0).sum::<f32>() / (128.0 * ch as f32)).collect(),
        (3, 32) => pcm.chunks_exact(4 * ch).map(|f| (0..ch).map(|c| f32::from_le_bytes(f[4 * c..4 * c + 4].try_into().unwrap())).sum::<f32>() / ch as f32).collect(),
        _ => return None,
    };
    let n = frames.len();
    let (ls, le) = lp.unwrap_or((0, 0));
    Some(Preset {
        name: name.to_string(),
        bank: 0,
        program: 0,
        zones: vec![Zone {
            key: (0, 127),
            vel: (0, 127),
            data: Arc::new(frames),
            start: 0,
            end: n,
            loop_start: ls.min(n),
            loop_end: le.min(n),
            looped: le > ls + 4 && le <= n,
            root: unity.unwrap_or(root),
            tune: 0.0,
            rate,
            gain: 1.0,
            pan: 0.0,
        }],
    })
}

/// Root key from a sample file name: a MIDI number ("Piano_060.wav") or a
/// note name with C4 = 60 ("Piano C#4.wav", "Str_Db3.wav"); the last match wins.
pub fn root_from_name(name: &str) -> Option<u8> {
    let stem = name.rsplit_once('.').map(|x| x.0).unwrap_or(name);
    let b = stem.as_bytes();
    let mut found = None;
    let mut i = 0;
    while i < b.len() {
        let c = b[i].to_ascii_uppercase();
        let boundary = i == 0 || !b[i - 1].is_ascii_alphabetic();
        if boundary && (b'A'..=b'G').contains(&c) {
            let mut j = i + 1;
            let mut semi: i32 = [9, 11, 0, 2, 4, 5, 7][(c - b'A') as usize];
            if j < b.len() && (b[j] == b'#' || b[j] == b's') {
                semi += 1;
                j += 1;
            } else if j < b.len() && b[j] == b'b' && j + 1 < b.len() && (b[j + 1].is_ascii_digit() || b[j + 1] == b'-') {
                semi -= 1;
                j += 1;
            }
            let neg = j < b.len() && b[j] == b'-';
            let k = if neg { j + 1 } else { j };
            if k < b.len() && b[k].is_ascii_digit() && (k + 1 >= b.len() || !b[k + 1].is_ascii_digit()) {
                let oct = (b[k] - b'0') as i32 * if neg { -1 } else { 1 };
                let n = (oct + 1) * 12 + semi;
                if (0..128).contains(&n) {
                    found = Some(n as u8);
                }
                i = k + 1;
                continue;
            }
        }
        if b[i].is_ascii_digit() && (i == 0 || !b[i - 1].is_ascii_alphanumeric()) {
            let mut j = i;
            while j < b.len() && b[j].is_ascii_digit() {
                j += 1;
            }
            if j - i <= 3 && (j >= b.len() || !b[j].is_ascii_alphabetic()) {
                if let Ok(n) = stem[i..j].parse::<u32>() {
                    if n < 128 && j - i >= 2 {
                        found = Some(n as u8);
                    }
                }
            }
            i = j;
            continue;
        }
        i += 1;
    }
    found
}

/// A multisample from WAV files: each file's root comes from its smpl
/// chunk or file name; key ranges split half-way between neighbouring roots.
pub fn preset_from_wavs(name: &str, files: &[(String, Vec<u8>)]) -> Option<Preset> {
    let mut zones: Vec<Zone> = Vec::new();
    for (k, (fname, data)) in files.iter().enumerate() {
        let guess = root_from_name(fname).unwrap_or(60u8.saturating_add(k as u8).min(127));
        if let Some(p) = parse_wav(fname, data, guess) {
            zones.extend(p.zones);
        }
    }
    if zones.is_empty() {
        return None;
    }
    zones.sort_by_key(|z| z.root);
    let n = zones.len();
    for i in 0..n {
        let lo = if i == 0 { 0 } else { (zones[i - 1].root as u16 + zones[i].root as u16) / 2 + 1 };
        let hi = if i + 1 == n { 127 } else { (zones[i].root as u16 + zones[i + 1].root as u16) / 2 };
        zones[i].key = (lo.min(127) as u8, hi.min(127) as u8);
    }
    Some(Preset { name: name.to_string(), bank: 0, program: 0, zones })
}

/// Hypersonic 2 sample banks (number, name) of the Wave ROM content.
pub const HS2_SAMPLE_BANKS: [(u32, &str); 62] = [
    (0, "Kicks"), (1, "Snare+Sticks+Claps"), (2, "Toms"), (3, "HiHats"), (4, "Cymbals"), (5, "Acoustic Percussion"),
    (6, "Elektro Percussion"), (7, "GM Xtra Percussion"), (8, "Drumloops"), (9, "Grand Pianos"), (10, "Acoustic Pianos"),
    (12, "Electric Pianos"), (13, "Electric Pianos"), (15, "Clavinets"), (16, "Organs"), (17, "Organs"), (19, "Harpsichords"),
    (20, "Tuned Percussion"), (21, "Tuned Percussion"), (23, "Toms 2"), (24, "Drumloops 2"), (30, "Synth Basses"),
    (32, "Acoustic Basses"), (33, "Acoustic Basses"), (36, "Electric Basses"), (37, "Electric Basses"), (40, "Acoustic Guitars"),
    (45, "Electric Guitars"), (46, "Electric Guitars"), (50, "String Sections"), (51, "Solo Strings"), (57, "Orchestra Misc"),
    (58, "Orchestra Misc"), (59, "Orchestra Hits"), (60, "Vocal"), (61, "Vocal"), (70, "Brass Sections"), (75, "Solo Brass"),
    (76, "Solo Orchestral Brass"), (80, "Woodwind"), (81, "Woodwind"), (90, "Ethnic"), (91, "Ethnic"), (100, "Synth Textures"),
    (101, "Digital Polys"), (102, "Synth Bells"), (103, "Synth Atmos"), (104, "Arpeggios"), (105, "Noise"), (110, "Attack Partials"),
    (111, "Sustain Partials"), (112, "Color Partials"), (113, "Percussive Partials"), (123, "Synth FX"),
    (11, "Electric Pianos"), (14, "Electric Pianos"), (18, "Organs"), (22, "Tuned Percussion"), (31, "Synth Basses"),
    (34, "Acoustic Basses"), (35, "Acoustic Basses"), (38, "Electric Basses"),
];

/// Sample banks that hold one-shot drum / percussion sounds: a WAV set
/// assigned to them is chosen per element by its Wave index, not by key.
pub fn is_drum_bank(bank: u32) -> bool {
    matches!(bank, 0..=8 | 23 | 24)
}

/// Hypersonic 2 sample bank number → candidate General MIDI programs
/// (bank 128 = drum kit, program = GM drum note).
pub fn gm_for_bank(bank: u32) -> (u16, &'static [u16]) {
    match bank {
        0 => (128, &[36, 35]),
        1 => (128, &[38, 40, 37, 39]),
        2 | 23 => (128, &[41, 43, 45, 47, 48, 50]),
        3 => (128, &[42, 44, 46]),
        4 => (128, &[49, 57, 51, 59, 52, 55, 53]),
        5 | 7 => (128, &[60, 61, 62, 63, 64, 65, 66, 69, 70, 75, 76, 77, 54, 56, 58]),
        6 => (128, &[39, 54, 69, 70, 81]),
        8 | 24 => (128, &[36, 38, 42, 46]),
        9 | 10 => (0, &[0, 1, 2, 3]),
        11..=14 => (0, &[4, 5]),
        15 => (0, &[7]),
        16..=18 => (0, &[16, 17, 18, 19, 20]),
        19 => (0, &[6]),
        20..=22 => (0, &[11, 12, 13, 8, 9, 10, 14, 15]),
        30 | 31 => (0, &[38, 39, 87]),
        32..=35 => (0, &[32]),
        36..=39 => (0, &[33, 34, 35, 36, 37]),
        40..=44 => (0, &[24, 25]),
        45..=49 => (0, &[27, 26, 28, 29, 30]),
        50 => (0, &[48, 49, 44, 45]),
        51..=56 => (0, &[40, 41, 42, 43]),
        57 | 58 => (0, &[46, 47, 45]),
        59 => (0, &[55]),
        60..=69 => (0, &[52, 53, 54]),
        70..=74 => (0, &[61, 62, 63]),
        75 => (0, &[56, 57, 58, 59]),
        76..=79 => (0, &[60, 57]),
        80..=89 => (0, &[73, 71, 68, 70, 69, 72, 74, 75, 64, 65, 66, 67]),
        90..=99 => (0, &[104, 105, 106, 107, 108, 109, 110, 111, 78, 79]),
        100 => (0, &[88, 89, 90, 91, 92, 93, 94, 95]),
        101 => (0, &[80, 81, 84, 87, 90]),
        102 => (0, &[98, 88, 14]),
        103 => (0, &[99, 94, 95, 101]),
        104 => (0, &[81, 80]),
        105 => (0, &[122, 121, 127]),
        110..=113 => (0, &[80, 82, 86, 88]),
        _ => (0, &[96, 97, 98, 99, 100, 101, 102, 103]),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav16(samples: &[i16]) -> Vec<u8> {
        let mut d = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        d.extend_from_slice(&16u32.to_le_bytes());
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&44100u32.to_le_bytes());
        d.extend_from_slice(&88200u32.to_le_bytes());
        d.extend_from_slice(&2u16.to_le_bytes());
        d.extend_from_slice(&16u16.to_le_bytes());
        d.extend_from_slice(b"data");
        d.extend_from_slice(&((samples.len() * 2) as u32).to_le_bytes());
        for s in samples {
            d.extend_from_slice(&s.to_le_bytes());
        }
        d
    }

    #[test]
    fn wav_import() {
        let p = parse_wav("x", &wav16(&[0, 16384, -16384, 32767]), 60).unwrap();
        let z = &p.zones[0];
        assert_eq!(z.end, 4);
        assert_eq!(z.rate, 44100);
        assert!((z.data[1] - 0.5).abs() < 1e-4);
    }

    #[test]
    fn names_to_roots() {
        assert_eq!(root_from_name("Piano C4.wav"), Some(60));
        assert_eq!(root_from_name("Str_Db3.wav"), Some(49));
        assert_eq!(root_from_name("bass-A#1.wav"), Some(34));
        assert_eq!(root_from_name("Organ_060.wav"), Some(60));
        assert_eq!(root_from_name("kick.wav"), None);
        let files = vec![("x C3.wav".to_string(), wav16(&[0, 1, 2])), ("x C5.wav".to_string(), wav16(&[0, 1, 2]))];
        let p = preset_from_wavs("x", &files).unwrap();
        assert_eq!(p.zones[0].key, (0, 60));
        assert_eq!(p.zones[1].key, (61, 127));
    }

    #[test]
    fn gm_map() {
        assert_eq!(gm_for_bank(33).1[0], 32);
        assert_eq!(gm_for_bank(0).0, 128);
    }
}
