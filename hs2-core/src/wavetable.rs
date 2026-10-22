//! Wavetables: 64 single-cycle waves each, generated here (the original
//! wavetable audio is protected; only the table *names* follow Hypersonic 2).
//! Every wave is band-limited per octave (mip levels) from its spectrum.

use crate::dsp::Rng;
use std::f32::consts::{PI, TAU};
use std::sync::{Arc, Mutex};

pub const WAVES: usize = 64;
pub const LEN: usize = 1024;
const HARM: usize = LEN / 2 - 1;
const MIPS: usize = 10;

pub const NAMES: [&str; 46] = [
    "PWM Pulse", "PWM Pulse S", "Saw to Tri", "Sync Saw 1", "Sync Saw 2", "Sync Saw 2S", "Sync Saw 3", "Sync Saw 3S",
    "Sync Saw Centre", "Sync Square 1", "Sync Square 2", "Sync Square 2S", "Sync Square 3", "Sync Square 3S", "Sync Sine",
    "Sine clipping", "All harmonics", "Odd harmonics", "Even harmonics", "Add octaves", "Add drawbars", "Add 1 - 8 sweep",
    "Add 1 - 16 sweep", "Add 1 - 32 sweep", "Classic sweep 1", "Classic sweep 2", "Mini filter sweep", "Mini filter drive",
    "VoSim sweep", "VoSim vowels", "Metal spring", "Classic Bell Echo", "Robot bells", "Arpeggiator", "Ethnic pluck",
    "Tinkle Pad", "Classic Bell", "Electronic aah", "Clean Bell", "Thin Bell", "Four Bells", "Magic Seq", "White Noise",
    "Single Cycles", "303 waves", "More Drawbars",
];

/// One table: `mips[level][wave]`, LEN samples each.
pub struct Table {
    pub mips: Vec<Vec<Vec<f32>>>,
}

impl Table {
    /// Mip level for a fundamental of `dt` cycles per sample.
    #[inline]
    pub fn level(dt: f32) -> usize {
        // harmonics allowed below Nyquist: 0.5/dt; level k keeps HARM >> k
        let allowed = (0.5 / dt.max(1e-7)) as usize;
        let mut k = 0;
        while k + 1 < MIPS && (HARM >> k) > allowed {
            k += 1;
        }
        k
    }
    /// Read wave position `pos` (0..WAVES-1, fractional = crossfade) at phase.
    #[inline]
    pub fn read(&self, level: usize, pos: f32, phase: f32) -> f32 {
        let pos = pos.clamp(0.0, (WAVES - 1) as f32);
        let w0 = pos as usize;
        let w1 = (w0 + 1).min(WAVES - 1);
        let fw = pos - w0 as f32;
        let x = phase * LEN as f32;
        let i0 = x as usize % LEN;
        let i1 = (i0 + 1) % LEN;
        let fx = x - x.floor();
        let m = &self.mips[level];
        let a = m[w0][i0] + (m[w0][i1] - m[w0][i0]) * fx;
        if fw < 1e-4 {
            return a;
        }
        let b = m[w1][i0] + (m[w1][i1] - m[w1][i0]) * fx;
        a + (b - a) * fw
    }
}

/// Lazily generated set of all tables, shared between UI and audio threads.
#[derive(Default)]
pub struct Bank {
    tables: Mutex<Vec<Option<Arc<Table>>>>,
}

impl Bank {
    pub fn new() -> Bank {
        Bank { tables: Mutex::new(vec![None; NAMES.len()]) }
    }
    /// Get (generating on first use) table `i`.
    pub fn get(&self, i: usize) -> Arc<Table> {
        let i = i.min(NAMES.len() - 1);
        if let Some(t) = self.tables.lock().unwrap()[i].clone() {
            return t;
        }
        let t = Arc::new(generate(i));
        self.tables.lock().unwrap()[i] = Some(t.clone());
        t
    }
    /// Already generated (audio thread: never blocks on generation).
    pub fn try_get(&self, i: usize) -> Option<Arc<Table>> {
        self.tables.try_lock().ok()?.get(i.min(NAMES.len() - 1))?.clone()
    }
}

// ---------------------------------------------------------------------------
// spectra
// ---------------------------------------------------------------------------

fn fft(re: &mut [f32], im: &mut [f32], inverse: bool) {
    let n = re.len();
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = TAU / len as f32 * if inverse { 1.0 } else { -1.0 };
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let tr = re[b] * cr - im[b] * ci;
                let ti = re[b] * ci + im[b] * cr;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
                let nr = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = nr;
            }
        }
        len <<= 1;
    }
}

/// Time-domain wave → mip levels (band-limited copies), normalised to peak 1.
fn mips_of(wave: &[f32]) -> Vec<Vec<f32>> {
    let mut re = wave.to_vec();
    let mut im = vec![0.0; LEN];
    fft(&mut re, &mut im, false);
    re[0] = 0.0;
    im[0] = 0.0;
    let mut out = Vec::with_capacity(MIPS);
    let mut norm = 0.0f32;
    for k in 0..MIPS {
        let keep = (HARM >> k).max(1);
        let (mut r, mut i) = (re.clone(), im.clone());
        for h in keep + 1..LEN - keep {
            r[h] = 0.0;
            i[h] = 0.0;
        }
        fft(&mut r, &mut i, true);
        if k == 0 {
            norm = r.iter().fold(0.0f32, |m, v| m.max(v.abs())).max(1e-9);
        }
        out.push(r.iter().map(|v| v / norm).collect());
    }
    out
}

/// Additive wave from harmonic amplitudes (index 1 = fundamental).
fn additive(amps: &[f32]) -> Vec<f32> {
    let mut re = vec![0.0f32; LEN];
    let mut im = vec![0.0f32; LEN];
    for (h, &a) in amps.iter().enumerate().take(HARM + 1).skip(1) {
        im[h] = -a * 0.5;
        im[LEN - h] = a * 0.5;
    }
    fft(&mut re, &mut im, true);
    re
}

fn saw(t: f32) -> f32 {
    2.0 * t.fract() - 1.0
}
fn square(t: f32) -> f32 {
    if t.fract() < 0.5 {
        1.0
    } else {
        -1.0
    }
}
fn tri(t: f32) -> f32 {
    1.0 - 4.0 * (t.fract() - 0.5).abs()
}

fn from_fn(mut f: impl FnMut(f32) -> f32) -> Vec<f32> {
    (0..LEN).map(|i| f(i as f32 / LEN as f32)).collect()
}

/// Resonant low-pass response at harmonic h for cutoff c (in harmonics).
fn lp_resp(h: f32, c: f32, q: f32, poles: i32) -> f32 {
    let x = h / c.max(0.5);
    let base = 1.0 / (1.0 + x.powi(2 * poles)).sqrt();
    let peak = q * (-((x - 1.0) * 4.0).powi(2)).exp();
    base * (1.0 + peak)
}

const FORMANTS: [[f32; 3]; 5] = [
    [800.0, 1150.0, 2900.0], // a
    [400.0, 1600.0, 2700.0], // e
    [300.0, 2300.0, 3000.0], // i
    [450.0, 800.0, 2830.0],  // o
    [325.0, 700.0, 2530.0],  // u
];

fn formant_wave(f: [f32; 3], f0: f32, bright: f32) -> Vec<f32> {
    let amps: Vec<f32> = (0..=HARM)
        .map(|h| {
            if h == 0 {
                return 0.0;
            }
            let hz = h as f32 * f0;
            let mut a = 0.0;
            for (k, &fc) in f.iter().enumerate() {
                let bw = 80.0 + 40.0 * k as f32;
                a += (1.0 / (k as f32 + 1.0)) * (-((hz - fc) / bw).powi(2)).exp();
            }
            a * (1.0 / (h as f32).powf(1.0 - bright))
        })
        .collect();
    additive(&amps)
}

fn fm_wave(ratio: f32, index: f32, feedback: f32) -> Vec<f32> {
    from_fn(|t| {
        let m = (TAU * ratio * t).sin() + feedback * (TAU * 2.0 * ratio * t).sin();
        (TAU * t + index * m).sin()
    })
}

fn drawbar(reg: &[f32; 9]) -> Vec<f32> {
    // footages 16' 5 1/3' 8' 4' 2 2/3' 2' 1 3/5' 1 1/3' 1' as harmonics of 16'
    const H: [usize; 9] = [1, 3, 2, 4, 6, 8, 10, 12, 16];
    let mut amps = vec![0.0f32; HARM + 1];
    for (k, &h) in H.iter().enumerate() {
        amps[h] += reg[k];
    }
    additive(&amps)
}

const REGISTRATIONS: [[f32; 9]; 8] = [
    [0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0],
    [1.0, 0.75, 1.0, 0.75, 0.5, 0.5, 0.25, 0.25, 0.5],
    [0.0, 0.0, 1.0, 1.0, 1.0, 1.0, 0.0, 0.0, 0.0],
    [1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
    [0.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0],
    [1.0, 0.0, 1.0, 0.5, 0.0, 0.5, 0.0, 0.0, 1.0],
];

fn wave(table: usize, w: usize) -> Vec<f32> {
    let x = w as f32 / (WAVES - 1) as f32;
    let name = NAMES[table];
    match name {
        "PWM Pulse" | "PWM Pulse S" => {
            let pw = 0.5 - 0.48 * x;
            let soft = name.ends_with('S');
            from_fn(|t| {
                let v = if t < pw { 1.0 } else { -1.0 };
                if soft {
                    v * (PI * t).sin().powf(0.3)
                } else {
                    v
                }
            })
        }
        "Saw to Tri" => from_fn(|t| saw(t) * (1.0 - x) + tri(t) * x),
        n if n.starts_with("Sync Saw") || n.starts_with("Sync Square") || n == "Sync Sine" => {
            let max = if n.contains('3') {
                12.0
            } else if n.contains('2') {
                6.0
            } else {
                3.0
            };
            let ratio = if n.contains("Centre") { 1.0 + 4.0 * (1.0 - (2.0 * x - 1.0).abs()) } else { 1.0 + (max - 1.0) * x };
            let smooth = n.ends_with('S');
            from_fn(|t| {
                let s = ratio * t;
                let v = if n.contains("Square") {
                    square(s)
                } else if n == "Sync Sine" {
                    (TAU * s).sin()
                } else {
                    saw(s)
                };
                if smooth {
                    v * (1.0 - t).powf(0.7)
                } else {
                    v
                }
            })
        }
        "Sine clipping" => {
            let g = 1.0 + 24.0 * x * x;
            from_fn(|t| ((TAU * t).sin() * g).clamp(-1.0, 1.0))
        }
        "All harmonics" | "Odd harmonics" | "Even harmonics" => {
            let n = 1 + (x * 63.0) as usize;
            let amps: Vec<f32> = (0..=HARM)
                .map(|h| {
                    let ok = match name {
                        "Odd harmonics" => h % 2 == 1,
                        "Even harmonics" => h == 1 || h % 2 == 0,
                        _ => true,
                    };
                    if h >= 1 && h <= n * 2 && ok {
                        1.0 / h as f32
                    } else {
                        0.0
                    }
                })
                .collect();
            additive(&amps)
        }
        "Add octaves" => {
            let n = x * 6.0;
            let mut amps = vec![0.0f32; HARM + 1];
            for k in 0..7 {
                let a = (n - k as f32 + 1.0).clamp(0.0, 1.0);
                amps[1 << k] = a / (k as f32 + 1.0);
            }
            additive(&amps)
        }
        "Add drawbars" | "More Drawbars" => {
            let pos = x * (REGISTRATIONS.len() - 1) as f32;
            let (a, b) = (pos as usize, (pos as usize + 1).min(REGISTRATIONS.len() - 1));
            let f = pos.fract();
            let off = if name == "More Drawbars" { 3 } else { 0 };
            let ra = REGISTRATIONS[(a + off) % 8];
            let rb = REGISTRATIONS[(b + off) % 8];
            let mut reg = [0.0f32; 9];
            for k in 0..9 {
                reg[k] = ra[k] * (1.0 - f) + rb[k] * f;
            }
            drawbar(&reg)
        }
        n if n.starts_with("Add 1 - ") => {
            let top: f32 = n[8..].split(' ').next().unwrap().parse().unwrap_or(8.0);
            let c = 1.0 + (top - 1.0) * x;
            let amps: Vec<f32> = (0..=HARM)
                .map(|h| if h == 0 { 0.0 } else { (-((h as f32 - c) * 1.2).powi(2)).exp() + 0.3 * (h == 1) as i32 as f32 })
                .collect();
            additive(&amps)
        }
        "Classic sweep 1" | "Classic sweep 2" | "Mini filter sweep" | "Mini filter drive" | "303 waves" => {
            let c = 1.0 + 80.0 * x * x;
            let (q, poles, sq) = match name {
                "Classic sweep 2" => (2.5, 2, false),
                "Mini filter sweep" | "Mini filter drive" => (1.5, 4, false),
                "303 waves" => (3.0, 3, w % 2 == 1),
                _ => (0.8, 2, false),
            };
            let amps: Vec<f32> = (0..=HARM)
                .map(|h| {
                    if h == 0 || (sq && h % 2 == 0) {
                        return 0.0;
                    }
                    lp_resp(h as f32, c, q, poles) / h as f32
                })
                .collect();
            let mut v = additive(&amps);
            if name == "Mini filter drive" {
                let peak = v.iter().fold(0.0f32, |m, s| m.max(s.abs())).max(1e-9);
                for s in &mut v {
                    *s = (*s / peak * 3.0).tanh();
                }
            }
            v
        }
        "VoSim sweep" => {
            let f0 = 110.0;
            let fc = [300.0 + 2500.0 * x, 900.0 + 2000.0 * x, 2600.0];
            formant_wave(fc, f0, 0.3)
        }
        "VoSim vowels" | "Electronic aah" => {
            let pos = if name == "Electronic aah" { 0.0 } else { x * 4.0 };
            let (a, b) = (pos as usize, (pos as usize + 1).min(4));
            let f = pos.fract();
            let mut fc = [0.0; 3];
            for k in 0..3 {
                fc[k] = FORMANTS[a][k] * (1.0 - f) + FORMANTS[b][k] * f;
            }
            let bright = if name == "Electronic aah" { 0.1 + 0.8 * x } else { 0.4 };
            formant_wave(fc, 130.0, bright)
        }
        "Metal spring" => {
            const P: [usize; 10] = [1, 2, 3, 5, 7, 11, 13, 17, 19, 23];
            let mut amps = vec![0.0f32; HARM + 1];
            for (k, &h) in P.iter().enumerate() {
                amps[h * (1 + (x * 3.0) as usize)] += 1.0 / (1.0 + k as f32 * (1.0 - x));
            }
            amps[1] += 0.5;
            additive(&amps)
        }
        "Classic Bell Echo" | "Classic Bell" | "Clean Bell" | "Thin Bell" | "Robot bells" | "Four Bells" | "Tinkle Pad" => {
            let (ratio, imax, fb) = match name {
                "Clean Bell" => (2.0, 3.0, 0.0),
                "Thin Bell" => (5.0, 2.0, 0.0),
                "Robot bells" => (3.0, 6.0, 0.4),
                "Four Bells" => (1.0 + (w / 16) as f32, 4.0, 0.0),
                "Tinkle Pad" => (7.0, 1.5, 0.1),
                "Classic Bell Echo" => (3.0, 4.0, 0.2),
                _ => (4.0, 5.0, 0.0),
            };
            let idx = if name == "Classic Bell Echo" { imax * (0.5 + 0.5 * (x * TAU * 3.0).cos()) } else { imax * x };
            fm_wave(ratio, idx, fb)
        }
        "Arpeggiator" | "Magic Seq" => {
            const SEQ: [usize; 8] = [1, 3, 5, 8, 4, 6, 2, 7];
            let k = SEQ[(w / 8) % 8] * if name == "Magic Seq" { 2 } else { 1 };
            let mut amps = vec![0.0f32; HARM + 1];
            amps[1] = 0.5;
            amps[k.min(HARM)] += 1.0;
            amps[(k * 2).min(HARM)] += 0.3;
            additive(&amps)
        }
        "Ethnic pluck" => {
            let decay = 0.05 + 1.5 * x;
            let amps: Vec<f32> = (0..=HARM)
                .map(|h| if h == 0 { 0.0 } else { (-(h as f32) * decay * 0.2).exp() * (1.0 + 0.5 * ((h as f32) * 0.9).sin()) / (h as f32).sqrt() })
                .collect();
            additive(&amps)
        }
        "White Noise" => {
            let mut rng = Rng::new(0x9e37_79b9 ^ (w as u32 * 7919));
            from_fn(|_| rng.bi())
        }
        _ => {
            // "Single Cycles": a mixed bag drawn from the other tables
            let src = (w * 7 + 3) % (NAMES.len() - 3);
            let src = if NAMES[src] == "Single Cycles" { 0 } else { src };
            wave(src, (w * 13) % WAVES)
        }
    }
}

pub fn generate(table: usize) -> Table {
    let mut mips = vec![Vec::with_capacity(WAVES); MIPS];
    for w in 0..WAVES {
        for (k, m) in mips_of(&wave(table, w)).into_iter().enumerate() {
            mips[k].push(m);
        }
    }
    Table { mips }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tables_generate_and_are_normalised() {
        for t in [0, 2, 16, 29, 36, 42] {
            let tb = generate(t);
            assert_eq!(tb.mips.len(), MIPS);
            let peak = tb.mips[0][WAVES / 2].iter().fold(0.0f32, |m, v| m.max(v.abs()));
            assert!(peak > 0.3 && peak < 1.5, "{} {}", NAMES[t], peak);
        }
        assert_eq!(Table::level(440.0 / 48000.0), 4);
        assert_eq!(Table::level(20.0 / 48000.0), 0);
    }
}
