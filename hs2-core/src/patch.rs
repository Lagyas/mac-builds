//! Hypersonic 2 patch (`StSS` record) and the element / patch parameter model.
//!
//! Layout (see `_hypersonic2/notes/FORMAT.md`): 0x220-byte header (name, search
//! attributes, comment, controller info, author, six Hyperknob names), 256
//! patch parameters, then elements of a 32-byte name + 256 parameters, then a
//! 3-word end mark. Every parameter is a normalized f32 (bipolar centre 0.5).
//! The value → unit curves below are this synth's own (the original curves are
//! not available); the parameter *indices* are the original ones.

pub const PATCH_PARAMS: usize = 256;
pub const ELEM_PARAMS: usize = 256;
const HEADER: usize = 0x220;
/// Header word 0x08 (always 0 in Hypersonic 2 content) marking patches written
/// by this synth: their FX element parameters use this synth's own layout.
pub const OWN_MAGIC: u32 = 0x5832_5348; // "HS2X"
const MAGIC: u32 = 0x0099_0edf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ElemType {
    Fx,
    Drawbar,
    Sample,
    Wavetable,
    Fm,
    Analog,
}

impl ElemType {
    pub fn letter(self) -> &'static str {
        match self {
            Self::Fx => "FX",
            Self::Drawbar => "O",
            Self::Sample => "S",
            Self::Wavetable => "W",
            Self::Fm => "F",
            Self::Analog => "A",
        }
    }
    pub fn index(self) -> usize {
        self as usize
    }
    /// Normalized value stored in element parameter 1.
    pub fn value(self) -> f32 {
        (self as usize as f32 + 0.5) / 16.0
    }
    pub fn from_value(v: f32) -> Self {
        match ((v * 16.0).ceil() as i32 - 1).clamp(0, 5) {
            0 => Self::Fx,
            1 => Self::Drawbar,
            2 => Self::Sample,
            3 => Self::Wavetable,
            4 => Self::Fm,
            _ => Self::Analog,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Element {
    pub name: String,
    pub p: [f32; ELEM_PARAMS],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Patch {
    pub name: String,
    pub attributes: String,
    pub comment: String,
    pub controllers: String,
    pub author: String,
    pub knob_names: [String; 6],
    /// Header words 0x14 / 0x18 (kept for round trips).
    pub hdr: [u32; 2],
    /// Written by this synth (own FX parameter layout).
    pub own: bool,
    pub p: [f32; PATCH_PARAMS],
    pub elements: Vec<Element>,
}

fn cstr(d: &[u8]) -> String {
    let end = d.iter().position(|&c| c == 0).unwrap_or(d.len());
    d[..end].iter().map(|&c| c as char).collect()
}

fn put_str(out: &mut [u8], s: &str) {
    for (i, c) in s.chars().take(out.len().saturating_sub(1)).enumerate() {
        out[i] = if (c as u32) < 256 { c as u8 } else { b'?' };
    }
}

fn f32s<const N: usize>(d: &[u8]) -> [f32; N] {
    let mut a = [0.0; N];
    for (i, v) in a.iter_mut().enumerate() {
        let x = f32::from_le_bytes(d[i * 4..i * 4 + 4].try_into().unwrap());
        *v = if x.is_finite() { x } else { 0.0 };
    }
    a
}

impl Patch {
    pub fn parse(d: &[u8]) -> Option<Patch> {
        if d.len() < HEADER + 4 * (PATCH_PARAMS + 3) || &d[..4] != b"StSS" {
            return None;
        }
        // elements follow until the end mark; trailing bank data may follow the last patch
        let first = HEADER + 4 * PATCH_PARAMS;
        let mut n = 0;
        while n < 64 {
            let o = first + 4 * (8 + ELEM_PARAMS) * n;
            if o + 4 * (8 + ELEM_PARAMS) > d.len() || !(0x20..0x7f).contains(&d[o]) {
                break;
            }
            n += 1;
        }
        let u = |o: usize| u32::from_le_bytes(d[o..o + 4].try_into().unwrap());
        let mut knob_names: [String; 6] = Default::default();
        for (i, k) in knob_names.iter_mut().enumerate() {
            *k = cstr(&d[0x1c0 + 16 * i..0x1d0 + 16 * i]);
        }
        let mut elements = Vec::with_capacity(n);
        for i in 0..n {
            let o = HEADER + 4 * (PATCH_PARAMS + (8 + ELEM_PARAMS) * i);
            elements.push(Element { name: cstr(&d[o..o + 32]), p: f32s(&d[o + 32..]) });
        }
        Some(Patch {
            name: cstr(&d[0x20..0x40]),
            attributes: cstr(&d[0x40..0x80]),
            comment: cstr(&d[0x80..0x100]),
            controllers: cstr(&d[0x100..0x180]),
            author: cstr(&d[0x180..0x1c0]),
            knob_names,
            hdr: [u(0x14), u(0x18)],
            own: u(0x08) == OWN_MAGIC,
            p: f32s(&d[HEADER..]),
            elements,
        })
    }

    pub fn write(&self) -> Vec<u8> {
        let mut d = vec![0u8; HEADER];
        d[..4].copy_from_slice(b"StSS");
        d[4..8].copy_from_slice(&MAGIC.to_le_bytes());
        if self.own {
            d[8..12].copy_from_slice(&OWN_MAGIC.to_le_bytes());
        }
        d[0x0c..0x10].copy_from_slice(&0x150u32.to_le_bytes());
        d[0x10..0x14].copy_from_slice(&u32::MAX.to_le_bytes());
        d[0x14..0x18].copy_from_slice(&self.hdr[0].to_le_bytes());
        d[0x18..0x1c].copy_from_slice(&self.hdr[1].to_le_bytes());
        put_str(&mut d[0x20..0x40], &self.name);
        put_str(&mut d[0x40..0x80], &self.attributes);
        put_str(&mut d[0x80..0x100], &self.comment);
        put_str(&mut d[0x100..0x180], &self.controllers);
        put_str(&mut d[0x180..0x1c0], &self.author);
        for (i, k) in self.knob_names.iter().enumerate() {
            put_str(&mut d[0x1c0 + 16 * i..0x1d0 + 16 * i], k);
        }
        for v in self.p {
            d.extend_from_slice(&v.to_le_bytes());
        }
        for e in &self.elements {
            let mut name = [0u8; 32];
            put_str(&mut name, &e.name);
            d.extend_from_slice(&name);
            for v in e.p {
                d.extend_from_slice(&v.to_le_bytes());
            }
        }
        d.extend_from_slice(&[0u8; 12]);
        d
    }

    /// A patch matches a search when every space-separated word is found in
    /// its name, attributes or comment (Load page: AND logic).
    pub fn matches(&self, query: &str) -> bool {
        let hay = format!("{} {} {}", self.name, self.attributes, self.comment).to_lowercase();
        query.split_whitespace().all(|w| hay.contains(&w.to_lowercase()))
    }

    /// Patch name without the "[" marker HS2 uses for HS2-only patches.
    pub fn own_fx_params(&self) -> bool {
        self.own
    }

    /// Before editing an original patch: give its FX elements this synth's
    /// FX parameters (what they sound like now) and mark it as own.
    pub fn make_own(&mut self) {
        if self.own {
            return;
        }
        for e in &mut self.elements {
            if e.kind() == ElemType::Fx {
                let d = crate::fx::defaults(e.fx_type());
                e.p[ep::FX_P..ep::FX_P + crate::fx::NPARAM].copy_from_slice(&d);
            }
        }
        self.own = true;
    }

    pub fn display_name(&self) -> &str {
        self.name.trim_start_matches('[')
    }

    /// "≈" layer patches (manual p. 38: layers of different sounds such as
    /// piano and strings) — recognised from names like "Piano + Strings".
    pub fn is_layer(&self) -> bool {
        let n = self.display_name();
        n.contains(" + ") || n.contains(" & ") || n.contains(" / ")
    }
}

// ---------------------------------------------------------------------------
// value curves (own)
// ---------------------------------------------------------------------------

/// Enumeration index from a normalized value with `n` entries.
pub fn enum_idx(v: f32, n: usize) -> usize {
    ((v * n as f32) as usize).min(n.saturating_sub(1))
}
pub fn enum_val(i: usize, n: usize) -> f32 {
    (i as f32 + 0.5) / n as f32
}
pub fn bipolar(v: f32) -> f32 {
    (v - 0.5) * 2.0
}
/// Envelope / glide time in seconds (0 → 0.5 ms, 1 → 20 s).
pub fn time_s(v: f32) -> f32 {
    0.0005 + 20.0 * v.clamp(0.0, 1.0).powi(3)
}
pub fn time_v(s: f32) -> f32 {
    ((s - 0.0005).max(0.0) / 20.0).cbrt()
}
/// Filter cutoff as a MIDI-like pitch at key 60 (0 → 46 Hz, 0.25 → C3, 1 → beyond Nyquist);
/// the voice adds key tracking on top.
pub fn cutoff_pitch(v: f32) -> f32 {
    v * 120.0 + 30.0
}
pub fn pitch_hz(p: f32) -> f32 {
    440.0 * 2f32.powf((p - 69.0) / 12.0)
}
/// LFO rate in Hz (0.02 … 50 Hz).
pub fn lfo_hz(v: f32) -> f32 {
    0.02 * 2500f32.powf(v.clamp(0.0, 1.0))
}
/// Output level as a gain (0.5 → unity, 1.0 → +12 dB).
pub fn level_gain(v: f32) -> f32 {
    4.0 * v * v
}
pub fn note_of(v: f32) -> i32 {
    (v * 127.0).round() as i32
}

/// Element parameter indices (original numbering, see `params_gen`).
pub mod ep {
    pub const MUTE: usize = 0;
    pub const TYPE: usize = 1;
    pub const KNOB_MODE: usize = 4;
    pub const KNOB_DEST: usize = 6;
    pub const KNOB_DEPTH: usize = 7;
    pub const PAGE: usize = 17;
    pub const KEY_LO: usize = 49;
    pub const KEY_HI: usize = 50;
    pub const KEY_XF: usize = 51;
    pub const VEL_LO: usize = 52;
    pub const VEL_HI: usize = 53;
    pub const VEL_XF: usize = 54;
    pub const TRIGGER: usize = 55;
    pub const MUTE_COND: usize = 56;
    pub const MONO: usize = 57;
    pub const MAX_VOICES: usize = 58;
    pub const ENV_MODE: usize = 59;
    pub const UNISON: usize = 60;
    pub const UNI_DETUNE: usize = 61;
    pub const UNI_PAN: usize = 62;
    pub const NOTE_DELAY: usize = 63;
    pub const DELAY_TIME: usize = 64;
    pub const START: usize = 67;
    pub const VEL_START: usize = 68;
    pub const RND_START: usize = 69;
    pub const OCTAVE: usize = 70;
    pub const SEMI: usize = 71;
    pub const CENT: usize = 72;
    pub const PITCH_KEY: usize = 73;
    pub const RND_PITCH: usize = 74;
    pub const PATCH_PB: usize = 75;
    pub const GLIDE_MODE: usize = 76;
    pub const GLIDE_TYPE: usize = 77;
    pub const GLIDE_TIME: usize = 78;
    pub const ATTACK_BEND: usize = 80;
    pub const VEL_BEND: usize = 81;
    pub const RELEASE_BEND: usize = 82;
    pub const LFO3_WAVE: usize = 85;
    pub const LFO3_SYNC: usize = 86;
    pub const LFO3_RATE: usize = 87;
    pub const LFO3_PHASE: usize = 88;
    pub const MENV: usize = 89; // L1 T1 L2 T2 L3 T3 S R L5
    pub const WAVE: usize = 98;
    pub const KEY_WAVE: usize = 99;
    pub const VEL_WAVE: usize = 100;
    pub const RND_WAVE: usize = 101;
    pub const OSC2_WAVE_MOD: usize = 102;
    pub const FILTER: usize = 103;
    pub const RESO: usize = 104;
    pub const VEL_RESO: usize = 105;
    pub const CUTOFF: usize = 106;
    pub const KEY_CUTOFF: usize = 107;
    pub const VEL_CUTOFF: usize = 108;
    pub const RND_CUTOFF: usize = 109;
    pub const FENV_DEPTH: usize = 110;
    pub const FENV: usize = 111; // A D S F R
    pub const FENV_VEL: usize = 117;
    pub const FENV_VEL_ATT: usize = 118;
    pub const FENV_VEL_DEC: usize = 119;
    pub const FENV_KEY: usize = 120;
    pub const TEMPO_ENV: usize = 123;
    pub const MUTE_FADE: usize = 124;
    pub const CLICK: usize = 125;
    pub const AENV: usize = 126; // A D S F R
    pub const AENV_VEL: usize = 132;
    pub const AENV_VEL_ATT: usize = 133;
    pub const AENV_VEL_DEC: usize = 134;
    pub const DECAY_KEY: usize = 135;
    pub const RELEASE_KEY: usize = 137;
    pub const RX_VEL: usize = 138;
    pub const LEVEL: usize = 139;
    pub const PAN: usize = 140;
    pub const PAN_KEY: usize = 141;
    pub const RND_PAN: usize = 142;
    pub const ALT_PAN: usize = 143;
    pub const MATRIX: usize = 172; // + 5·row: Src Mod Param Dest Depth
    pub const ROWS: usize = 16;
    // FX element page (type 0)
    pub const FX_TYPE: usize = 19;
    pub const FX_AUX: usize = 20;
    pub const FX_SEND: usize = 21;
    pub const FX_P: usize = 25; // 12 params
    // Sample page
    pub const S_BANK: usize = 18;
    pub const S_WAVE: usize = 19;
    pub const S_MONO: usize = 20;
    pub const S_FORMANT: usize = 22;
    // Wavetable page
    pub const W_BANK: usize = 18;
    pub const W_INDEX: usize = 19;
    pub const W_SYNC: usize = 21;
    pub const W_SEMI: usize = 22;
    pub const W_CENT: usize = 23;
    pub const W_ALIAS: usize = 24;
    pub const W_OSC2: usize = 28;
    pub const W_OSC2_MIX: usize = 29;
    // FM page
    pub const F_M1_COARSE: usize = 17;
    pub const F_M1_FINE: usize = 18;
    pub const F_THRU: usize = 19;
    pub const F_M1_DEPTH: usize = 20;
    pub const F_M1_VEL: usize = 21;
    pub const F_M2_COARSE: usize = 25;
    pub const F_M2_FINE: usize = 26;
    pub const F_M2_KEY: usize = 27;
    pub const F_M2_DEPTH: usize = 28;
    pub const F_M2_VEL: usize = 29;
    pub const F_ENV_RATE: usize = 33;
    pub const F_ENV_LEVEL: usize = 34;
    pub const F_LF: usize = 35;
    // Drawbar page
    pub const D_PERC: usize = 21;
    pub const D_PERC_LEVEL: usize = 22;
    pub const D_PERC_DECAY: usize = 23;
    pub const D_PERC_HARM: usize = 24;
    pub const D_BARS: usize = 25; // 9 drawbars 16' .. 1'
    pub const D_CLICK: usize = 34;
    // Analog page
    pub const A_OSC1_WAVE: usize = 19;
    pub const A_HPF: usize = 20;
    pub const A_OSC1_MIX: usize = 21;
    pub const A_SEMI: usize = 22;
    pub const A_CENT: usize = 23;
    pub const A_PW_MIN: usize = 24;
    pub const A_OSC2_SEMI: usize = 25;
    pub const A_OSC2_CENT: usize = 26;
    pub const A_OSC2_WAVE: usize = 27;
    pub const A_OSC3_WAVE: usize = 28;
    pub const A_OSC3_MIX: usize = 29;
    pub const A_SUB: usize = 30;
    pub const A_NOISE: usize = 31;
    pub const A_OSC3_FM: usize = 32;
    pub const A_OSC2_MIX: usize = 33;
    pub const A_WAVETABLE: usize = 34;
    pub const A_DIST: usize = 35;
    pub const A_ALIAS: usize = 36;
}

/// Patch parameter indices (original numbering).
pub mod pp {
    pub const MUTE: usize = 0;
    pub const GLIDE: usize = 3;
    pub const ARP_ON: usize = 17;
    pub const ARP_LATCH: usize = 18;
    pub const ARP_MODE: usize = 19;
    pub const ARP_OCT: usize = 20;
    pub const ARP_STEPS: usize = 21;
    pub const ARP_SYNC: usize = 22;
    pub const ARP_RATE: usize = 23;
    pub const ARP_GATE: usize = 24;
    pub const ARP_SWING: usize = 25;
    pub const PHRASE_BANK: usize = 26;
    pub const PHRASE_PROG: usize = 27;
    pub const ARP_TRANSPOSE: usize = 28;
    pub const ARP_SCALE: usize = 33;
    pub const KEY_LO: usize = 49;
    pub const KEY_HI: usize = 50;
    pub const KEY_XF: usize = 51;
    pub const VEL_LO: usize = 52;
    pub const VEL_HI: usize = 53;
    pub const VEL_XF: usize = 54;
    pub const TRANSPOSE: usize = 55;
    pub const FINE: usize = 56;
    pub const MONO: usize = 57;
    pub const MAX_VOICES: usize = 58;
    pub const LEVEL_TRIM: usize = 59;
    pub const PAN: usize = 60;
    pub const OUTPUT: usize = 61;
    pub const FX_SEND: usize = 63; // 4
    pub const LEVEL: usize = 67;
    pub const PB_UP: usize = 73;
    pub const PB_DOWN: usize = 74;
    pub const LFO1: usize = 85; // wave sync rate phase
    pub const LFO2: usize = 89;
    pub const KNOB: [usize; 6] = [112, 115, 118, 138, 141, 123];
    pub const GLIDE_TIME: usize = 148;
    pub const STEP: usize = 172; // + 5·s: Note Vel Gate Seq1 Seq2
}

impl Element {
    pub fn new(name: &str, kind: ElemType) -> Element {
        let mut p = [0.0f32; ELEM_PARAMS];
        for (i, v) in DEFAULT_ELEM.iter().enumerate() {
            p[i] = *v;
        }
        p[ep::TYPE] = kind.value();
        for r in 0..ep::ROWS {
            p[ep::MATRIX + 5 * r + 2] = 0.5;
            p[ep::MATRIX + 5 * r + 4] = 0.5;
        }
        if kind == ElemType::Fx {
            p[ep::FX_P..ep::FX_P + crate::fx::NPARAM].copy_from_slice(&crate::fx::defaults(0));
        }
        Element { name: name.into(), p }
    }
    pub fn kind(&self) -> ElemType {
        ElemType::from_value(self.p[ep::TYPE])
    }
    pub fn muted(&self) -> bool {
        self.p[ep::MUTE] >= 0.5
    }
    /// Type-specific page parameter (17..48).
    pub fn page(&self, i: usize) -> f32 {
        self.p[i]
    }
    pub fn key_range(&self) -> (i32, i32, i32) {
        (note_of(self.p[ep::KEY_LO]), note_of(self.p[ep::KEY_HI]), (self.p[ep::KEY_XF] * 48.0) as i32)
    }
    pub fn vel_range(&self) -> (i32, i32, i32) {
        (note_of(self.p[ep::VEL_LO]), note_of(self.p[ep::VEL_HI]), (self.p[ep::VEL_XF] * 127.0) as i32)
    }
    /// Element transpose in semitones (octave, semi, cent).
    pub fn transpose(&self) -> f32 {
        let oct = enum_idx(self.p[ep::OCTAVE], 9) as f32 - 4.0;
        let semi = (bipolar(self.p[ep::SEMI]) * 24.0).round();
        let cent = bipolar(self.p[ep::CENT]);
        oct * 12.0 + semi + cent
    }
    pub fn fx_type(&self) -> u32 {
        (self.p[ep::FX_TYPE] * 128.0).round() as u32
    }
}

/// Defaults of a fresh element (centred bipolar params, open ranges, typical envelopes).
const DEFAULT_ELEM: [f32; 172] = {
    let mut a = [0.0f32; 172];
    let mut i = 0;
    while i < 172 {
        a[i] = 0.0;
        i += 1;
    }
    a[ep::KEY_HI] = 1.0;
    a[ep::VEL_HI] = 1.0;
    a[ep::MAX_VOICES] = 1.0;
    a[ep::OCTAVE] = 0.5;
    a[ep::SEMI] = 0.5;
    a[ep::CENT] = 0.5;
    a[ep::PITCH_KEY] = 0.75;
    a[ep::PATCH_PB] = 1.0;
    a[ep::LFO3_RATE] = 0.5;
    a[ep::FILTER] = 0.1;
    a[ep::CUTOFF] = 1.0;
    a[ep::KEY_CUTOFF] = 0.5;
    a[ep::VEL_CUTOFF] = 0.5;
    a[ep::FENV_DEPTH] = 0.5;
    a[ep::FENV] = 0.0;
    a[ep::FENV + 1] = 0.4;
    a[ep::FENV + 2] = 1.0;
    a[ep::FENV + 3] = 0.5;
    a[ep::FENV + 4] = 0.3;
    a[ep::AENV] = 0.0;
    a[ep::AENV + 1] = 0.4;
    a[ep::AENV + 2] = 1.0;
    a[ep::AENV + 3] = 0.5;
    a[ep::AENV + 4] = 0.25;
    a[ep::AENV_VEL] = 0.75;
    a[ep::LEVEL] = 0.45;
    a[ep::PAN] = 0.5;
    a
};

impl Patch {
    pub fn new(name: &str) -> Patch {
        let mut p = [0.0f32; PATCH_PARAMS];
        p[pp::KEY_HI] = 1.0;
        p[pp::VEL_HI] = 1.0;
        p[pp::TRANSPOSE] = 0.5;
        p[pp::FINE] = 0.5;
        p[pp::MAX_VOICES] = 1.0;
        p[pp::LEVEL_TRIM] = 0.5;
        p[pp::PAN] = 0.5;
        p[pp::LEVEL] = 0.5;
        p[pp::PB_UP] = enum_val(2, 16);
        p[pp::PB_DOWN] = enum_val(2, 16);
        p[pp::MONO] = 0.5;
        p[pp::GLIDE] = 0.5;
        p[pp::ARP_RATE] = enum_val(9, 12);
        p[pp::ARP_GATE] = 0.5;
        for s in 0..16 {
            p[pp::STEP + 5 * s] = 0.5;
            p[pp::STEP + 5 * s + 1] = 0.75;
            p[pp::STEP + 5 * s + 2] = 0.5;
            p[pp::STEP + 5 * s + 3] = 0.5;
            p[pp::STEP + 5 * s + 4] = 0.5;
        }
        Patch {
            name: name.into(),
            attributes: String::new(),
            comment: String::new(),
            controllers: String::new(),
            author: String::new(),
            knob_names: Default::default(),
            hdr: [1, 0],
            own: true,
            p,
            elements: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_types() {
        for t in [ElemType::Fx, ElemType::Drawbar, ElemType::Sample, ElemType::Wavetable, ElemType::Fm, ElemType::Analog] {
            assert_eq!(ElemType::from_value(t.value()), t);
        }
        // values seen in factory patches
        assert_eq!(ElemType::from_value(0.0156), ElemType::Fx);
        assert_eq!(ElemType::from_value(0.16), ElemType::Sample);
        assert_eq!(ElemType::from_value(0.3125), ElemType::Fm);
        assert_eq!(ElemType::from_value(0.3438), ElemType::Analog);
        assert_eq!(ElemType::from_value(0.375), ElemType::Analog);
    }

    #[test]
    fn round_trip() {
        let mut p = Patch::new("Test Pad");
        p.attributes = "Warm Soft Pad".into();
        p.knob_names[0] = "Tone".into();
        p.elements.push(Element::new("Saw Osc", ElemType::Analog));
        p.elements.push(Element::new("Chorus", ElemType::Fx));
        let d = p.write();
        assert_eq!(d.len(), 0x220 + 4 * (256 + 264 * 2 + 3));
        let q = Patch::parse(&d).unwrap();
        assert_eq!(q, p);
        assert!(q.matches("warm pad"));
        assert!(!q.matches("warm organ"));
        assert_eq!(q.elements[0].kind(), ElemType::Analog);
    }
}
