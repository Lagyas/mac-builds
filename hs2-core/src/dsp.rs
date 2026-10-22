//! DSP building blocks: band-limited oscillators, the multimode filter,
//! ADSFR / modulation envelopes, LFOs and a small random generator.

use std::f32::consts::{PI, TAU};

#[derive(Clone, Copy, Debug)]
pub struct Rng(u32);

impl Rng {
    pub fn new(seed: u32) -> Rng {
        Rng(seed.max(1))
    }
    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }
    /// Uniform in [0, 1).
    pub fn unit(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / (1u32 << 24) as f32
    }
    /// Uniform in [-1, 1).
    pub fn bi(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }
}

/// PolyBLEP residual for a discontinuity at phase 0 (t, dt in cycles).
#[inline]
pub fn blep(t: f32, dt: f32) -> f32 {
    if t < dt {
        let x = t / dt;
        x + x - x * x - 1.0
    } else if t > 1.0 - dt {
        let x = (t - 1.0) / dt;
        x * x + x + x + 1.0
    } else {
        0.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wave {
    Saw,
    Square,
    Pulse,
    Triangle,
    Sine,
    SawTri,
    Noise,
}

/// Band-limited classic oscillator with optional hard sync to a master phase.
#[derive(Clone, Copy, Debug, Default)]
pub struct Osc {
    pub phase: f32,
    tri: f32,
}

impl Osc {
    /// One sample. `dt` = freq / sr, `pw` pulse width 0.02..0.98.
    #[inline]
    pub fn tick(&mut self, wave: Wave, dt: f32, pw: f32, rng: &mut Rng) -> f32 {
        let dt = dt.clamp(1e-6, 0.45);
        let t = self.phase;
        let out = match wave {
            Wave::Saw => 2.0 * t - 1.0 - blep(t, dt),
            Wave::Square | Wave::Pulse => {
                let pw = if wave == Wave::Square { 0.5 } else { pw.clamp(0.02, 0.98) };
                let mut v = if t < pw { 1.0 } else { -1.0 };
                v += blep(t, dt);
                v -= blep((t - pw + 1.0).fract(), dt);
                v
            }
            Wave::Triangle => {
                // integrated square (leaky), amplitude normalised
                let mut sq = if t < 0.5 { 1.0 } else { -1.0 };
                sq += blep(t, dt);
                sq -= blep((t + 0.5).fract(), dt);
                self.tri = self.tri * 0.9995 + sq * dt * 4.0;
                self.tri
            }
            Wave::Sine => (t * TAU).sin(),
            Wave::SawTri => 0.5 * (2.0 * t - 1.0 - blep(t, dt)) + 0.5 * (1.0 - 4.0 * (t - 0.5).abs()),
            Wave::Noise => rng.bi(),
        };
        self.phase += dt;
        if self.phase >= 1.0 {
            self.phase -= 1.0;
        }
        out
    }
}

// ---------------------------------------------------------------------------
// filter
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FilterMode {
    Off,
    Lp24,
    Lp12,
    Bp18,
    Bp12,
    Hp12,
    Peak,
    Presence,
    Damper,
}

impl FilterMode {
    pub const ALL: [FilterMode; 9] = [
        Self::Off,
        Self::Lp24,
        Self::Lp12,
        Self::Bp18,
        Self::Bp12,
        Self::Hp12,
        Self::Peak,
        Self::Presence,
        Self::Damper,
    ];
    pub fn name(self) -> &'static str {
        ["Filter Off", "Lowpass 24dB", "Lowpass 12dB", "Bandpass 18dB", "Bandpass 12dB", "Highpass 12dB", "Peaking EQ", "Presence", "Damper"][self as usize]
    }
}

/// Zero-delay-feedback state variable filter (one 12 dB stage).
#[derive(Clone, Copy, Debug, Default)]
pub struct Svf {
    ic1: f32,
    ic2: f32,
}

pub struct SvfOut {
    pub lp: f32,
    pub bp: f32,
    pub hp: f32,
}

impl Svf {
    #[inline]
    pub fn tick(&mut self, x: f32, g: f32, k: f32) -> SvfOut {
        let a1 = 1.0 / (1.0 + g * (g + k));
        let a2 = g * a1;
        let a3 = g * a2;
        let v3 = x - self.ic2;
        let v1 = a1 * self.ic1 + a2 * v3;
        let v2 = self.ic2 + a2 * self.ic1 + a3 * v3;
        self.ic1 = 2.0 * v1 - self.ic1;
        self.ic2 = 2.0 * v2 - self.ic2;
        SvfOut { lp: v2, bp: v1, hp: x - k * v1 - v2 }
    }
    pub fn reset(&mut self) {
        *self = Svf::default();
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Filter {
    a: Svf,
    b: Svf,
    lp1: f32,
}

impl Filter {
    /// `cut` in Hz, `reso` 0..1, `drive` ≥ 1 pre-gain with soft clipping.
    #[inline]
    pub fn tick(&mut self, mode: FilterMode, x: f32, cut: f32, reso: f32, sr: f32, drive: f32) -> f32 {
        if mode == FilterMode::Off {
            return x;
        }
        let x = if drive > 1.001 { (x * drive).tanh() / drive.sqrt() } else { x };
        let fc = cut.clamp(10.0, sr * 0.49);
        let g = (PI * fc / sr).tan();
        let reso = reso.clamp(0.0, 1.0);
        let k = 2.0 - 1.98 * reso;
        match mode {
            FilterMode::Off => x,
            FilterMode::Lp12 => self.a.tick(x, g, k).lp,
            FilterMode::Lp24 => {
                let y = self.a.tick(x, g, k).lp;
                self.b.tick(y, g, 1.414).lp
            }
            FilterMode::Bp12 => self.a.tick(x, g, k).bp * k,
            FilterMode::Bp18 => {
                let y = self.a.tick(x, g, k).bp * k;
                // extra 6 dB/oct low-pass above the band
                let c = 1.0 - (-TAU * fc / sr).exp();
                self.lp1 += c * (y - self.lp1);
                self.lp1 * 1.4
            }
            FilterMode::Hp12 => self.a.tick(x, g, k).hp,
            FilterMode::Peak => {
                // bell: boost around the cutoff, depth from resonance
                let o = self.a.tick(x, g, 1.0);
                x + o.bp * (reso * 6.0)
            }
            FilterMode::Presence => {
                let o = self.a.tick(x, g, 1.414);
                x + o.hp * (0.5 + reso * 3.0)
            }
            FilterMode::Damper => {
                let o = self.a.tick(x, g, 1.414);
                o.lp + o.bp * 1.414 * (1.0 - reso) * 0.5
            }
        }
    }
    pub fn reset(&mut self) {
        *self = Filter::default();
    }
}

/// One-pole high-pass (DC / HPF).
#[derive(Clone, Copy, Debug, Default)]
pub struct OnePole {
    z: f32,
}

impl OnePole {
    #[inline]
    pub fn lp(&mut self, x: f32, c: f32) -> f32 {
        self.z += c * (x - self.z);
        self.z
    }
    #[inline]
    pub fn hp(&mut self, x: f32, c: f32) -> f32 {
        x - self.lp(x, c)
    }
}

pub fn onepole_coef(hz: f32, sr: f32) -> f32 {
    1.0 - (-TAU * hz / sr).exp()
}

// ---------------------------------------------------------------------------
// envelopes
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    Attack,
    Decay,
    Sustain,
    Release,
    Off,
}

/// Attack / Decay / Sustain / Fade / Release envelope (manual p.71).
#[derive(Clone, Copy, Debug)]
pub struct Adsfr {
    pub stage: Stage,
    pub level: f32,
    pub a: f32,
    pub d: f32,
    pub s: f32,
    /// <0 fade out, >0 fade up (seconds; 0 = hold)
    pub f: f32,
    pub r: f32,
}

impl Default for Adsfr {
    fn default() -> Self {
        Adsfr { stage: Stage::Off, level: 0.0, a: 0.001, d: 0.3, s: 1.0, f: 0.0, r: 0.2 }
    }
}

impl Adsfr {
    pub fn trigger(&mut self, from_zero: bool) {
        if from_zero {
            self.level = 0.0;
        }
        self.stage = Stage::Attack;
    }
    pub fn release(&mut self) {
        if self.stage != Stage::Off {
            self.stage = Stage::Release;
        }
    }
    /// Advance by `dt` seconds (control rate) and return the level.
    pub fn step(&mut self, dt: f32) -> f32 {
        match self.stage {
            Stage::Attack => {
                self.level += dt / self.a.max(1e-4);
                if self.level >= 1.0 {
                    self.level = 1.0;
                    self.stage = Stage::Decay;
                }
            }
            Stage::Decay => {
                let c = (-dt * 4.6 / self.d.max(1e-4)).exp();
                self.level = self.s + (self.level - self.s) * c;
                if (self.level - self.s).abs() < 1e-4 {
                    self.level = self.s;
                    self.stage = Stage::Sustain;
                }
            }
            Stage::Sustain => {
                if self.f < 0.0 {
                    self.level *= (-dt * 4.6 / (-self.f).max(1e-3)).exp();
                } else if self.f > 0.0 {
                    self.level = (self.level + dt / self.f.max(1e-3)).min(1.0);
                }
            }
            Stage::Release => {
                self.level *= (-dt * 6.9 / self.r.max(1e-4)).exp();
                if self.level < 1e-4 {
                    self.level = 0.0;
                    self.stage = Stage::Off;
                }
            }
            Stage::Off => self.level = 0.0,
        }
        self.level
    }
    pub fn active(&self) -> bool {
        self.stage != Stage::Off
    }
}

/// Five-level modulation envelope (Mod Page: L1 T1 L2 T2 L3 T3 S R L5).
#[derive(Clone, Copy, Debug, Default)]
pub struct ModEnv {
    pub levels: [f32; 5], // L1 L2 L3 S L5
    pub times: [f32; 4],  // T1 T2 T3 R
    seg: usize,
    pos: f32,
    from: f32,
    pub value: f32,
    released: bool,
}

impl ModEnv {
    pub fn trigger(&mut self) {
        self.seg = 0;
        self.pos = 0.0;
        self.from = self.levels[0];
        self.value = self.levels[0];
        self.released = false;
    }
    pub fn release(&mut self) {
        self.released = true;
        self.from = self.value;
        self.seg = 3;
        self.pos = 0.0;
    }
    pub fn step(&mut self, dt: f32, rate: f32) -> f32 {
        let (target, t) = match self.seg {
            0 => (self.levels[1], self.times[0]),
            1 => (self.levels[2], self.times[1]),
            2 => (self.levels[3], self.times[2]),
            _ if self.released => (self.levels[4], self.times[3]),
            _ => return self.value,
        };
        self.pos += dt * rate / t.max(1e-4);
        if self.pos >= 1.0 {
            self.value = target;
            self.from = target;
            self.pos = 0.0;
            if self.seg < 3 {
                self.seg += 1;
            } else {
                self.seg = 4;
            }
        } else {
            self.value = self.from + (target - self.from) * self.pos;
        }
        if self.seg == 4 {
            self.value = self.levels[4];
        }
        self.value
    }
}

// ---------------------------------------------------------------------------
// LFO
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LfoWave {
    Constant,
    Sine,
    Triangle,
    Sawtooth,
    Square,
    SampleHold,
    SampleHoldAlt,
    RandomDrift,
    SlowDrift,
}

impl LfoWave {
    pub const ALL: [LfoWave; 9] = [
        Self::Constant,
        Self::Sine,
        Self::Triangle,
        Self::Sawtooth,
        Self::Square,
        Self::SampleHold,
        Self::SampleHoldAlt,
        Self::RandomDrift,
        Self::SlowDrift,
    ];
    pub fn name(self) -> &'static str {
        ["Constant", "Sine", "Triangle", "Sawtooth", "Square", "S+H Random", "S+H Alternate", "Random Drift", "Slow Drift"][self as usize]
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LfoSync {
    Off,
    FirstNote,
    EachNote,
    TempoNote,
    TempoBeat,
}

impl LfoSync {
    pub const ALL: [LfoSync; 5] = [Self::Off, Self::FirstNote, Self::EachNote, Self::TempoNote, Self::TempoBeat];
    pub fn name(self) -> &'static str {
        ["Off", "First Note", "Each Note", "Tempo + Note", "Tempo + Beat"][self as usize]
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Lfo {
    pub phase: f32,
    held: f32,
    target: f32,
    drift: f32,
    alt: bool,
    pub value: f32,
}

impl Default for Lfo {
    fn default() -> Self {
        Lfo { phase: 0.0, held: 0.0, target: 0.0, drift: 0.0, alt: false, value: 0.0 }
    }
}

impl Lfo {
    pub fn restart(&mut self, phase: f32) {
        self.phase = phase.fract();
    }
    /// Advance by `dt` seconds at `hz`, return value in -1..1.
    pub fn step(&mut self, wave: LfoWave, hz: f32, dt: f32, rng: &mut Rng) -> f32 {
        let prev = self.phase;
        self.phase += hz * dt;
        let wrapped = self.phase >= 1.0;
        self.phase = self.phase.fract();
        if wrapped || prev == 0.0 && self.held == 0.0 {
            self.alt = !self.alt;
            self.held = match wave {
                LfoWave::SampleHoldAlt if self.alt => -0.8,
                _ => rng.bi(),
            };
            self.target = rng.bi();
        }
        let t = self.phase;
        self.value = match wave {
            LfoWave::Constant => 1.0,
            LfoWave::Sine => (t * TAU).sin(),
            LfoWave::Triangle => 1.0 - 4.0 * (t - 0.5).abs(),
            LfoWave::Sawtooth => 1.0 - 2.0 * t,
            LfoWave::Square => {
                if t < 0.5 {
                    1.0
                } else {
                    -1.0
                }
            }
            LfoWave::SampleHold | LfoWave::SampleHoldAlt => self.held,
            LfoWave::RandomDrift => {
                self.drift += (self.target - self.drift) * (hz * dt * 4.0).min(1.0);
                self.drift
            }
            LfoWave::SlowDrift => {
                self.drift += (self.target * 0.5 - self.drift) * (hz * dt).min(1.0);
                self.drift
            }
        };
        self.value
    }
}

/// Soft clipper used on outputs and drives.
#[inline]
pub fn soft_clip(x: f32) -> f32 {
    if x.abs() < 1.0 {
        x - x * x * x / 3.0
    } else {
        x.signum() * 2.0 / 3.0
    }
}

/// Equal-power pan gains for pan in -1..1.
#[inline]
pub fn pan_gains(pan: f32) -> (f32, f32) {
    let a = (pan.clamp(-1.0, 1.0) + 1.0) * PI / 4.0;
    (a.cos() * std::f32::consts::SQRT_2, a.sin() * std::f32::consts::SQRT_2)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saw_is_bounded_and_zero_mean() {
        let mut o = Osc::default();
        let mut rng = Rng::new(1);
        let (mut sum, mut peak) = (0.0f32, 0.0f32);
        for _ in 0..48000 {
            let v = o.tick(Wave::Saw, 440.0 / 48000.0, 0.5, &mut rng);
            sum += v;
            peak = peak.max(v.abs());
        }
        assert!(peak < 1.2);
        assert!((sum / 48000.0).abs() < 0.02);
    }

    #[test]
    fn lowpass_attenuates_highs() {
        let mut f = Filter::default();
        let mut e = 0.0;
        for i in 0..4800 {
            let x = (i as f32 * TAU * 8000.0 / 48000.0).sin();
            let y = f.tick(FilterMode::Lp24, x, 500.0, 0.0, 48000.0, 1.0);
            if i > 2400 {
                e += y * y;
            }
        }
        assert!(e / 2400.0 < 1e-4);
    }

    #[test]
    fn envelope_runs_through() {
        let mut e = Adsfr { a: 0.01, d: 0.05, s: 0.5, f: 0.0, r: 0.05, ..Default::default() };
        e.trigger(true);
        for _ in 0..100 {
            e.step(0.001);
        }
        assert!((e.level - 0.5).abs() < 0.05);
        e.release();
        for _ in 0..500 {
            e.step(0.001);
        }
        assert!(!e.active());
    }
}
