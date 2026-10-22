//! Effects: FX elements, patch FX and global FX share these algorithms.
//! The FX *type* numbering follows Hypersonic 2 (`Element::fx_type`, round(v·128));
//! the algorithms and the meaning of the 12 FX parameters are this synth's own.

use crate::dsp::{onepole_coef, OnePole, Rng};
use std::f32::consts::{PI, TAU};

pub const NPARAM: usize = 12;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Reverb,
    Delay,
    Chorus,
    Flanger,
    Phaser,
    Modulation,
    Pitch,
    Filter,
    Eq,
    Dynamics,
    Distortion,
    Misc,
}

/// FX type list for menus: (id, name).
pub const TYPES: [(u32, &str); 46] = [
    (0, "Reverb"), (1, "Hall"), (3, "Room"), (4, "Reverse Reverb"), (5, "Non-Linear Reverb"), (6, "Early Reflections"),
    (7, "Resonators"), (8, "Overheads"), (10, "Mono Delay"), (11, "Stereo Delay"), (12, "Ping-Pong"), (13, "Tape Echo"),
    (14, "Ducking Delay"), (15, "Grain Delay"), (16, "Diffuser Delay"), (19, "Chorus + Reverb"), (20, "Chorus"),
    (21, "Quad Chorus"), (22, "Space Chorus"), (23, "Ensemble"), (30, "Flanger"), (31, "Step Flanger"), (40, "Phaser"),
    (41, "Deep Phaser"), (42, "Bi-Phaser"), (50, "Tremolo"), (51, "Stereo Width"), (52, "Stereo Enhancer"),
    (54, "Scanner Vibrato"), (55, "Rotary Speaker"), (60, "Pitch Shift"), (61, "Detune"), (70, "Vintage Filter"),
    (71, "Wah"), (72, "Talkbox"), (80, "Shelving EQ"), (81, "Parametric EQ"), (85, "Enhancer"), (90, "Limiter"),
    (91, "Compressor"), (92, "Multiband Comp"), (95, "Gate"), (100, "Distortion"), (101, "Overdrive"),
    (102, "Bit Crusher"), (103, "Amp Simulator"),
];

pub fn family(id: u32) -> Family {
    match id {
        0..=9 => Family::Reverb,
        10..=18 => Family::Delay,
        19..=29 => Family::Chorus,
        30..=39 => Family::Flanger,
        40..=49 => Family::Phaser,
        50..=59 => Family::Modulation,
        60..=69 => Family::Pitch,
        70..=79 => Family::Filter,
        80..=89 => Family::Eq,
        90..=99 => Family::Dynamics,
        100..=109 => Family::Distortion,
        _ => Family::Misc,
    }
}

pub fn type_name(id: u32) -> &'static str {
    TYPES.iter().rev().find(|t| t.0 <= id).map(|t| t.1).unwrap_or("Effect")
}

/// Names of the 12 FX parameters for the editor.
pub fn param_names(id: u32) -> [&'static str; NPARAM] {
    match family(id) {
        Family::Reverb => ["Time", "Damping", "Predelay", "Mix", "Width", "Low Cut", "Size", "Diffusion", "Chorus", "Level", "", ""],
        Family::Delay => ["Time L", "Time R", "Feedback", "Mix", "Damping", "Sync", "Mod Rate", "Mod Depth", "Duck", "Level", "Spread", ""],
        Family::Chorus => ["Rate", "Depth", "Delay", "Mix", "Feedback", "Voices", "Width", "Reverb", "", "Level", "", ""],
        Family::Flanger => ["Rate", "Depth", "Delay", "Mix", "Feedback", "Sync", "Width", "Steps", "", "Level", "", ""],
        Family::Phaser => ["Rate", "Depth", "Centre", "Mix", "Feedback", "Stages", "Width", "Sync", "", "Level", "", ""],
        Family::Modulation => ["Rate", "Depth", "Shape", "Mix", "Width", "Speed", "Drive", "Horn/Drum", "", "Level", "", ""],
        Family::Pitch => ["Pitch", "Detune", "Delay", "Mix", "Feedback", "Width", "", "", "", "Level", "", ""],
        Family::Filter => ["Cutoff", "Resonance", "Env Depth", "Mix", "Rate", "LFO Depth", "Vowel", "Sens", "", "Level", "", ""],
        Family::Eq => ["Low Gain", "Low Freq", "Mid Gain", "Mid Freq", "Mid Q", "High Gain", "High Freq", "", "", "Level", "", ""],
        Family::Dynamics => ["Threshold", "Ratio", "Attack", "Release", "Gain", "Knee", "", "", "", "Level", "", ""],
        Family::Distortion => ["Drive", "Tone", "Bits", "Mix", "Rate", "Cabinet", "Bias", "", "", "Level", "", ""],
        Family::Misc => ["Frequency", "Depth", "Shape", "Mix", "", "", "", "", "", "Level", "", ""],
    }
}

/// Factory defaults of the 12 params for a type.
pub fn defaults(id: u32) -> [f32; NPARAM] {
    let mut p = [0.5; NPARAM];
    p[9] = 0.5; // level (unity)
    match family(id) {
        Family::Reverb => {
            p[0] = if id == 1 { 0.75 } else if id == 3 { 0.35 } else { 0.55 };
            p[3] = 0.35;
            p[2] = 0.1;
        }
        Family::Delay => {
            p[0] = 0.35;
            p[1] = 0.45;
            p[2] = 0.35;
            p[3] = 0.3;
        }
        Family::Eq => {
            p[4] = 0.4;
        }
        Family::Dynamics => {
            p[0] = 0.6;
            p[1] = 0.4;
            p[2] = 0.2;
            p[3] = 0.4;
        }
        Family::Distortion => {
            p[0] = 0.4;
            p[3] = 1.0;
            p[2] = 1.0;
        }
        _ => p[3] = 0.5,
    }
    p
}

// ---------------------------------------------------------------------------
// building blocks
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct DelayLine {
    buf: Vec<f32>,
    pos: usize,
}

impl DelayLine {
    fn new(n: usize) -> DelayLine {
        DelayLine { buf: vec![0.0; n.max(4)], pos: 0 }
    }
    #[inline]
    fn push(&mut self, x: f32) {
        self.pos = (self.pos + 1) % self.buf.len();
        self.buf[self.pos] = x;
    }
    /// Read `d` samples back (fractional, linear interpolation).
    #[inline]
    fn read(&self, d: f32) -> f32 {
        let n = self.buf.len();
        let d = d.clamp(1.0, (n - 2) as f32);
        let i = d as usize;
        let f = d - i as f32;
        let a = self.buf[(self.pos + n - i) % n];
        let b = self.buf[(self.pos + n - i - 1) % n];
        a + (b - a) * f
    }
}

#[derive(Clone)]
struct Comb {
    buf: Vec<f32>,
    pos: usize,
    store: f32,
}

impl Comb {
    fn new(n: usize) -> Comb {
        Comb { buf: vec![0.0; n], pos: 0, store: 0.0 }
    }
    #[inline]
    fn tick(&mut self, x: f32, fb: f32, damp: f32) -> f32 {
        let y = self.buf[self.pos];
        self.store = y * (1.0 - damp) + self.store * damp;
        self.buf[self.pos] = x + self.store * fb;
        self.pos = (self.pos + 1) % self.buf.len();
        y
    }
}

#[derive(Clone)]
struct Allpass {
    buf: Vec<f32>,
    pos: usize,
}

impl Allpass {
    fn new(n: usize) -> Allpass {
        Allpass { buf: vec![0.0; n], pos: 0 }
    }
    #[inline]
    fn tick(&mut self, x: f32, g: f32) -> f32 {
        let b = self.buf[self.pos];
        let y = -x + b;
        self.buf[self.pos] = x + b * g;
        self.pos = (self.pos + 1) % self.buf.len();
        y
    }
}

/// RBJ biquad.
#[derive(Clone, Copy, Default)]
struct Biquad {
    b: [f32; 3],
    a: [f32; 2],
    z: [f32; 2],
}

impl Biquad {
    fn set(&mut self, kind: u8, f: f32, q: f32, gain_db: f32, sr: f32) {
        let w = TAU * f.clamp(10.0, sr * 0.45) / sr;
        let (s, c) = w.sin_cos();
        let al = s / (2.0 * q.max(0.1));
        let a = 10f32.powf(gain_db / 40.0);
        let (b0, b1, b2, a0, a1, a2) = match kind {
            0 => {
                // low shelf
                let sq = 2.0 * a.sqrt() * al;
                (a * ((a + 1.0) - (a - 1.0) * c + sq), 2.0 * a * ((a - 1.0) - (a + 1.0) * c), a * ((a + 1.0) - (a - 1.0) * c - sq), (a + 1.0) + (a - 1.0) * c + sq, -2.0 * ((a - 1.0) + (a + 1.0) * c), (a + 1.0) + (a - 1.0) * c - sq)
            }
            1 => {
                // high shelf
                let sq = 2.0 * a.sqrt() * al;
                (a * ((a + 1.0) + (a - 1.0) * c + sq), -2.0 * a * ((a - 1.0) + (a + 1.0) * c), a * ((a + 1.0) + (a - 1.0) * c - sq), (a + 1.0) - (a - 1.0) * c + sq, 2.0 * ((a - 1.0) - (a + 1.0) * c), (a + 1.0) - (a - 1.0) * c - sq)
            }
            2 => (1.0 + al * a, -2.0 * c, 1.0 - al * a, 1.0 + al / a, -2.0 * c, 1.0 - al / a), // peak
            3 => (al, 0.0, -al, 1.0 + al, -2.0 * c, 1.0 - al),                                 // bandpass
            4 => ((1.0 - c) / 2.0, 1.0 - c, (1.0 - c) / 2.0, 1.0 + al, -2.0 * c, 1.0 - al),     // lowpass
            _ => ((1.0 + c) / 2.0, -(1.0 + c), (1.0 + c) / 2.0, 1.0 + al, -2.0 * c, 1.0 - al),  // highpass
        };
        self.b = [b0 / a0, b1 / a0, b2 / a0];
        self.a = [a1 / a0, a2 / a0];
    }
    #[inline]
    fn tick(&mut self, x: f32) -> f32 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}

// ---------------------------------------------------------------------------
// the effect
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct Fx {
    pub id: u32,
    pub p: [f32; NPARAM],
    pub on: bool,
    sr: f32,
    rng: Rng,
    // reverb
    combs: Vec<Comb>,
    aps: Vec<Allpass>,
    pre: [DelayLine; 2],
    // delay / chorus / flanger / pitch
    dl: [DelayLine; 2],
    lp: [OnePole; 4],
    phase: f32,
    phase2: f32,
    fb: [f32; 2],
    // phaser
    ap1: [[f32; 8]; 2],
    // eq / filter
    bq: [[Biquad; 3]; 2],
    env: f32,
    gain: f32,
    hold: [f32; 2],
    hold_n: f32,
    coef_tick: u32,
}

const COMB_T: [usize; 8] = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617];
const AP_T: [usize; 4] = [556, 441, 341, 225];

impl Fx {
    pub fn new(id: u32, p: [f32; NPARAM], sr: f32) -> Fx {
        let scale = sr / 44100.0;
        let fam = family(id);
        let (combs, aps) = if matches!(fam, Family::Reverb) || id == 19 {
            let mut c = Vec::new();
            let mut a = Vec::new();
            for ch in 0..2 {
                for t in COMB_T {
                    c.push(Comb::new(((t + 23 * ch) as f32 * scale * 1.2) as usize));
                }
                for t in AP_T {
                    a.push(Allpass::new(((t + 23 * ch) as f32 * scale) as usize));
                }
            }
            (c, a)
        } else {
            (Vec::new(), Vec::new())
        };
        let dl_len = match fam {
            Family::Delay => (sr * 2.1) as usize,
            Family::Reverb => 8,
            _ => (sr * 0.1) as usize,
        };
        Fx {
            id,
            p,
            on: true,
            sr,
            rng: Rng::new(0x51ed ^ id),
            combs,
            aps,
            pre: [DelayLine::new((sr * 0.25) as usize), DelayLine::new((sr * 0.25) as usize)],
            dl: [DelayLine::new(dl_len), DelayLine::new(dl_len)],
            lp: [OnePole::default(); 4],
            phase: 0.0,
            phase2: 0.25,
            fb: [0.0; 2],
            ap1: [[0.0; 8]; 2],
            bq: [[Biquad::default(); 3]; 2],
            env: 0.0,
            gain: 1.0,
            hold: [0.0; 2],
            hold_n: 0.0,
            coef_tick: 0,
        }
    }

    pub fn family(&self) -> Family {
        family(self.id)
    }

    /// Process a stereo block in place. `send` = true: 100 % wet (send FX).
    pub fn process(&mut self, l: &mut [f32], r: &mut [f32], tempo: f32, send: bool) {
        if !self.on {
            if send {
                l.fill(0.0);
                r.fill(0.0);
            }
            return;
        }
        let sr = self.sr;
        let p = self.p;
        let mix = if send { 1.0 } else { p[3] };
        let level = 4.0 * p[9] * p[9];
        let dry = if send { 0.0 } else { 1.0 - mix };
        match self.family() {
            Family::Reverb => {
                let fb = 0.7 + 0.28 * p[0];
                let damp = 0.05 + 0.85 * p[1];
                let pre = 1.0 + p[2] * p[2] * sr * 0.2;
                let width = p[4];
                let lc = onepole_coef(20.0 + p[5] * p[5] * 800.0, sr);
                let rev = self.id == 4;
                for i in 0..l.len() {
                    let x = (l[i] + r[i]) * 0.5;
                    let x = self.lp[0].hp(x, lc);
                    self.pre[0].push(x);
                    let xin = self.pre[0].read(pre) * 0.03;
                    let (mut ol, mut or) = (0.0, 0.0);
                    for k in 0..8 {
                        ol += self.combs[k].tick(xin, fb, damp);
                        or += self.combs[8 + k].tick(xin, fb, damp);
                    }
                    for k in 0..4 {
                        ol = self.aps[k].tick(ol, 0.5);
                        or = self.aps[4 + k].tick(or, 0.5);
                    }
                    if rev {
                        // swelling envelope instead of decay
                        self.env = (self.env + 1.0 / (sr * (0.1 + p[0]))).fract();
                        ol *= self.env;
                        or *= self.env;
                    }
                    let wl = ol * (0.5 + 0.5 * width) + or * (0.5 - 0.5 * width);
                    let wr = or * (0.5 + 0.5 * width) + ol * (0.5 - 0.5 * width);
                    l[i] = (l[i] * dry + wl * mix) * level;
                    r[i] = (r[i] * dry + wr * mix) * level;
                }
            }
            Family::Delay => {
                let beat = 60.0 / tempo.max(20.0);
                let sync = p[5] > 0.5;
                let t = |v: f32| -> f32 {
                    if sync {
                        const DIV: [f32; 8] = [0.125, 0.1667, 0.25, 0.333, 0.5, 0.75, 1.0, 1.5];
                        DIV[((v * 8.0) as usize).min(7)] * beat * sr
                    } else {
                        (0.005 + v * v * 1.995) * sr
                    }
                };
                let (tl, tr) = (t(p[0]), t(if self.id == 10 { p[0] } else { p[1] }));
                let fb = p[2] * 0.95;
                let damp = onepole_coef(1000.0 + (1.0 - p[4]) * 15000.0, sr);
                let ping = self.id == 12;
                let duck = self.id == 14;
                let mrate = p[6] * 3.0;
                let mdepth = p[7] * p[7] * sr * 0.004;
                for i in 0..l.len() {
                    self.phase = (self.phase + mrate / sr).fract();
                    let m = (self.phase * TAU).sin() * mdepth;
                    let yl = self.dl[0].read(tl + m);
                    let yr = self.dl[1].read(tr - m);
                    let yl = self.lp[0].lp(yl, damp);
                    let yr = self.lp[1].lp(yr, damp);
                    let (il, ir) = if ping { ((l[i] + r[i]) * 0.5 + yr * fb, yl * fb) } else { (l[i] + yl * fb, r[i] + yr * fb) };
                    self.dl[0].push(il);
                    self.dl[1].push(if self.id == 10 { il } else { ir });
                    let mut g = 1.0;
                    if duck {
                        self.env = self.env.max((l[i].abs() + r[i].abs()) * 0.5) * 0.9995;
                        g = (1.0 - self.env * 4.0 * p[8].max(0.3)).clamp(0.0, 1.0);
                    }
                    l[i] = (l[i] * dry + yl * mix * g) * level;
                    r[i] = (r[i] * dry + yr * mix * g) * level;
                }
            }
            Family::Chorus | Family::Flanger | Family::Pitch => {
                let fam = self.family();
                let (base, depth, rate, fb) = match fam {
                    Family::Flanger => (0.0005 + p[2] * 0.006, p[1] * 0.004, 0.02 + p[0] * p[0] * 5.0, (p[4] - 0.5) * 1.9),
                    Family::Pitch => (0.012 + p[2] * 0.03, 0.0, 0.0, p[4] * 0.6),
                    _ => (0.006 + p[2] * 0.02, p[1] * 0.006, 0.05 + p[0] * p[0] * 6.0, p[4] * 0.5),
                };
                let voices = match self.id {
                    21 => 4,
                    22 | 23 => 3,
                    _ => 1,
                };
                let step = self.id == 31;
                // pitch shift: ±12 semis (Pitch), detune: ±50 cents
                let shift = if self.id == 61 { (p[1] - 0.5) * 1.0 } else { (p[0] - 0.5) * 24.0 };
                let ratio = 2f32.powf(shift / 12.0);
                let win = base * sr;
                for i in 0..l.len() {
                    let (xl, xr) = (l[i], r[i]);
                    self.dl[0].push(xl + self.fb[0] * fb);
                    self.dl[1].push(xr + self.fb[1] * fb);
                    let (mut yl, mut yr) = (0.0, 0.0);
                    if fam == Family::Pitch {
                        // two crossfaded taps sweeping through a window
                        self.phase = (self.phase + (1.0 - ratio) / win.max(1.0)).rem_euclid(1.0);
                        for (k, ph) in [self.phase, (self.phase + 0.5).fract()].into_iter().enumerate() {
                            let g = (PI * ph).sin();
                            let d = 2.0 + ph * win;
                            if self.id == 61 {
                                let ph2 = if k == 0 { ph } else { 1.0 - ph };
                                yl += self.dl[0].read(2.0 + ph * win) * g;
                                yr += self.dl[1].read(2.0 + ph2 * win) * g;
                            } else {
                                yl += self.dl[0].read(d) * g;
                                yr += self.dl[1].read(d) * g;
                            }
                        }
                    } else {
                        self.phase = (self.phase + rate / sr).fract();
                        for v in 0..voices {
                            let ph = (self.phase + v as f32 / voices as f32).fract();
                            let lfo = if step { ((ph * 8.0).floor() / 8.0 * TAU).sin() } else { (ph * TAU).sin() };
                            let lfo_r = if step { (((ph + 0.25).fract() * 8.0).floor() / 8.0 * TAU).sin() } else { ((ph + 0.25) * TAU).sin() };
                            yl += self.dl[0].read((base + depth * (1.0 + lfo) * 0.5) * sr);
                            yr += self.dl[1].read((base + depth * (1.0 + lfo_r) * 0.5) * sr);
                        }
                        yl /= voices as f32;
                        yr /= voices as f32;
                    }
                    self.fb = [yl, yr];
                    l[i] = (xl * dry + yl * mix) * level;
                    r[i] = (xr * dry + yr * mix) * level;
                }
            }
            Family::Phaser => {
                let stages = match self.id {
                    41 => 8,
                    42 => 6,
                    _ => 4,
                };
                let rate = 0.02 + p[0] * p[0] * 4.0;
                let fb = (p[4] - 0.5) * 1.6;
                for i in 0..l.len() {
                    self.phase = (self.phase + rate / sr).fract();
                    for ch in 0..2 {
                        let ph = if ch == 0 { self.phase } else { (self.phase + 0.25 * p[6] * 2.0).fract() };
                        let lfo = 0.5 + 0.5 * (ph * TAU).sin();
                        let f = 200.0 * 2f32.powf((p[2] * 4.0) + lfo * p[1] * 5.0);
                        let t = (PI * f.min(sr * 0.45) / sr).tan();
                        let a = (t - 1.0) / (t + 1.0);
                        let x = if ch == 0 { l[i] } else { r[i] };
                        let mut y = x + self.fb[ch] * fb;
                        for s in 0..stages {
                            let z = self.ap1[ch][s];
                            let o = a * y + z;
                            self.ap1[ch][s] = y - a * o;
                            y = o;
                        }
                        self.fb[ch] = y;
                        let out = (x * dry + (x + y) * 0.5 * mix * 2.0) * level;
                        if ch == 0 {
                            l[i] = out
                        } else {
                            r[i] = out
                        }
                    }
                }
            }
            Family::Modulation => {
                match self.id {
                    51 | 52 => {
                        // stereo width / enhancer
                        let w = p[4] * 2.0;
                        for i in 0..l.len() {
                            let m = (l[i] + r[i]) * 0.5;
                            let s = (l[i] - r[i]) * 0.5 * w;
                            l[i] = (m + s) * level;
                            r[i] = (m - s) * level;
                        }
                    }
                    55 => {
                        // rotary: horn + drum with doppler and amplitude modulation
                        let fast = p[5] > 0.5;
                        let target = if fast { 6.5 } else { 0.8 };
                        self.gain += (target - self.gain) * 0.0005;
                        let drive = 1.0 + p[6] * 6.0;
                        let xc = onepole_coef(800.0, sr);
                        for i in 0..l.len() {
                            let x = ((l[i] + r[i]) * 0.5 * drive).tanh() / drive.sqrt();
                            let lo = self.lp[0].lp(x, xc);
                            let hi = x - lo;
                            self.phase = (self.phase + self.gain / sr).fract();
                            self.phase2 = (self.phase2 + self.gain * 0.85 / sr).fract();
                            let (hs, hc) = (self.phase * TAU).sin_cos();
                            let (ds, _) = (self.phase2 * TAU).sin_cos();
                            self.dl[0].push(hi);
                            let hl = self.dl[0].read(sr * (0.002 + 0.0012 * hs)) * (1.0 + 0.4 * hc);
                            let hr = self.dl[0].read(sr * (0.002 - 0.0012 * hs)) * (1.0 - 0.4 * hc);
                            let dl = lo * (1.0 + 0.25 * ds);
                            let dr = lo * (1.0 - 0.25 * ds);
                            let (yl, yr) = (hl + dl, hr + dr);
                            l[i] = (l[i] * dry + yl * mix) * level;
                            r[i] = (r[i] * dry + yr * mix) * level;
                        }
                    }
                    54 => {
                        // scanner vibrato: modulated delay, no dry
                        let rate = 5.0 + p[0] * 3.0;
                        for i in 0..l.len() {
                            self.phase = (self.phase + rate / sr).fract();
                            let x = (l[i] + r[i]) * 0.5;
                            self.dl[0].push(x);
                            let y = self.dl[0].read(sr * (0.001 + 0.002 * p[1] * (1.0 + (self.phase * TAU).sin())));
                            l[i] = (x * (1.0 - mix) + y * mix) * level;
                            r[i] = l[i];
                        }
                    }
                    _ => {
                        // tremolo / auto-pan
                        let rate = 0.1 + p[0] * p[0] * 15.0;
                        let depth = p[1];
                        let stereo = p[4];
                        for i in 0..l.len() {
                            self.phase = (self.phase + rate / sr).fract();
                            let shape = |ph: f32| {
                                let s = (ph * TAU).sin();
                                let sq = s.signum();
                                s + (sq - s) * p[2]
                            };
                            let a = shape(self.phase);
                            let b = shape((self.phase + 0.5 * stereo).fract());
                            l[i] *= (1.0 - depth * 0.5 * (1.0 + a)) * level;
                            r[i] *= (1.0 - depth * 0.5 * (1.0 + b)) * level;
                        }
                    }
                }
            }
            Family::Filter => {
                let talk = self.id == 72;
                let rate = 0.05 + p[4] * p[4] * 8.0;
                self.coef_tick = 0;
                for i in 0..l.len() {
                    let x = (l[i] + r[i]) * 0.5;
                    self.env = self.env.max(x.abs()) * (1.0 - 20.0 / sr);
                    self.phase = (self.phase + rate / sr).fract();
                    if self.coef_tick % 16 == 0 {
                        let lfo = (self.phase * TAU).sin() * p[5];
                        let e = (self.env * (1.0 + p[7] * 8.0)).min(1.0) * p[2];
                        let f = 100.0 * 2f32.powf(p[0] * 7.0 + e * 4.0 + lfo * 2.0);
                        let q = 0.7 + p[1] * 10.0;
                        for ch in 0..2 {
                            if talk {
                                const V: [[f32; 3]; 5] = [[800.0, 1150.0, 2900.0], [400.0, 1600.0, 2700.0], [300.0, 2300.0, 3000.0], [450.0, 800.0, 2830.0], [325.0, 700.0, 2530.0]];
                                let pos = (p[6] * 4.0 + e * 2.0).min(3.99);
                                let (a, b) = (pos as usize, pos as usize + 1);
                                for k in 0..3 {
                                    let fr = V[a][k] + (V[b][k] - V[a][k]) * pos.fract();
                                    self.bq[ch][k].set(3, fr, 6.0 + p[1] * 10.0, 0.0, sr);
                                }
                            } else {
                                self.bq[ch][0].set(if self.id == 70 { 4 } else { 3 }, f, q, 0.0, sr);
                            }
                        }
                    }
                    self.coef_tick += 1;
                    for ch in 0..2 {
                        let xin = if ch == 0 { l[i] } else { r[i] };
                        let y = if talk {
                            (self.bq[ch][0].tick(xin) + self.bq[ch][1].tick(xin) * 0.7 + self.bq[ch][2].tick(xin) * 0.4) * 2.0
                        } else {
                            self.bq[ch][0].tick(xin)
                        };
                        let out = (xin * dry + y * mix) * level;
                        if ch == 0 {
                            l[i] = out
                        } else {
                            r[i] = out
                        }
                    }
                }
            }
            Family::Eq => {
                if self.coef_tick == 0 || self.coef_tick % 64 == 0 {
                    let db = |v: f32| (v - 0.5) * 36.0;
                    for ch in 0..2 {
                        self.bq[ch][0].set(0, 40.0 * 2f32.powf(p[1] * 4.0), 0.7, db(p[0]), sr);
                        self.bq[ch][1].set(2, 200.0 * 2f32.powf(p[3] * 6.0), 0.3 + p[4] * 4.0, db(p[2]), sr);
                        let hg = if self.id == 85 { 6.0 + (p[5] - 0.5) * 12.0 } else { db(p[5]) };
                        self.bq[ch][2].set(1, 2000.0 * 2f32.powf(p[6] * 3.0), 0.7, hg, sr);
                    }
                }
                self.coef_tick = self.coef_tick.wrapping_add(1);
                for i in 0..l.len() {
                    let mut a = l[i];
                    let mut b = r[i];
                    for k in 0..3 {
                        a = self.bq[0][k].tick(a);
                        b = self.bq[1][k].tick(b);
                    }
                    l[i] = a * level;
                    r[i] = b * level;
                }
            }
            Family::Dynamics => {
                let thr = 10f32.powf((p[0] - 1.0) * 2.4); // -48 dB .. 0 dB
                let ratio = if self.id == 90 { 50.0 } else { 1.0 + p[1] * 19.0 };
                let att = (-1.0 / (sr * (0.0002 + p[2] * 0.05))).exp();
                let rel = (-1.0 / (sr * (0.01 + p[3] * 1.0))).exp();
                let makeup = 10f32.powf((p[4] - 0.5) * 1.2);
                let gate = self.id == 95;
                for i in 0..l.len() {
                    let x = l[i].abs().max(r[i].abs());
                    let c = if x > self.env { att } else { rel };
                    self.env = x + c * (self.env - x);
                    let g = if gate {
                        if self.env > thr * 0.25 {
                            1.0
                        } else {
                            0.0
                        }
                    } else if self.env > thr {
                        (thr / self.env).powf(1.0 - 1.0 / ratio)
                    } else {
                        1.0
                    };
                    self.gain += (g - self.gain) * 0.01;
                    l[i] *= self.gain * makeup * level;
                    r[i] *= self.gain * makeup * level;
                }
            }
            Family::Distortion => {
                let drive = 1.0 + p[0] * p[0] * 60.0;
                let tone = onepole_coef(500.0 + p[1] * 12000.0, sr);
                let bits = 2f32.powf(2.0 + p[2] * 14.0);
                let hold = 1.0 + (1.0 - p[4]) * (1.0 - p[4]) * 32.0;
                let cab = self.id == 103;
                let cabc = onepole_coef(1500.0 + p[5] * 4000.0, sr);
                for i in 0..l.len() {
                    for ch in 0..2 {
                        let x = if ch == 0 { l[i] } else { r[i] };
                        let mut y = match self.id {
                            102 => {
                                if ch == 0 {
                                    self.hold_n += 1.0;
                                    if self.hold_n >= hold {
                                        self.hold_n -= hold;
                                        self.hold = [l[i], r[i]];
                                    }
                                }
                                (self.hold[ch] * bits).round() / bits
                            }
                            101 => {
                                let v = x * drive + (p[6] - 0.5) * 0.3;
                                v / (1.0 + v.abs())
                            }
                            _ => (x * drive).tanh(),
                        };
                        y = self.lp[ch].lp(y, tone);
                        if cab {
                            y = self.lp[2 + ch].lp(y, cabc);
                        }
                        let y = y / drive.sqrt().max(1.0) * if self.id == 102 { 1.0 } else { 1.5 };
                        let out = (x * dry + y * mix) * level;
                        if ch == 0 {
                            l[i] = out
                        } else {
                            r[i] = out
                        }
                    }
                }
            }
            Family::Misc => {
                let f = 20.0 * 2f32.powf(p[0] * 10.0);
                for i in 0..l.len() {
                    self.phase = (self.phase + f / sr).fract();
                    let m = (self.phase * TAU).sin() * p[1] + (1.0 - p[1]);
                    l[i] = (l[i] * dry + l[i] * m * mix) * level;
                    r[i] = (r[i] * dry + r[i] * m * mix) * level;
                }
                let _ = self.rng.unit();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_type_is_stable() {
        for (id, _) in TYPES {
            let mut fx = Fx::new(id, defaults(id), 48000.0);
            let mut l = vec![0.0f32; 4096];
            let mut r = vec![0.0f32; 4096];
            l[0] = 1.0;
            r[0] = 1.0;
            for _ in 0..20 {
                fx.process(&mut l, &mut r, 120.0, false);
                for v in l.iter().chain(r.iter()) {
                    assert!(v.is_finite() && v.abs() < 50.0, "{}", type_name(id));
                }
                l.fill(0.0);
                r.fill(0.0);
            }
        }
    }

    #[test]
    fn names() {
        assert_eq!(type_name(1), "Hall");
        assert_eq!(type_name(2), "Hall");
        assert_eq!(family(55), Family::Modulation);
    }
}
