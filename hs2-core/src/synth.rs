//! The instrument: 16 parts (MIDI channels), each holding a patch of up to
//! 16 elements, voices, insert FX elements, patch FX, four global FX and a
//! master section ("Hype", volume). Rendering is block based: control-rate
//! modulation every `CTL` samples.

use crate::arp::{self, Arp};
use crate::dsp::*;
use crate::fx::{self, Fx};
use crate::patch::*;
use crate::sf2::{self, SoundFont, Zone};
use crate::wavetable::{self, Table};
use std::sync::Arc;

pub const PARTS: usize = 16;
pub const MAX_VOICES: usize = 128;
const CTL: usize = 32;
const MAX_BLOCK: usize = 256;

#[derive(Clone, Copy, Debug, Default)]
pub struct Controllers {
    pub mw: f32,
    pub breath: f32,
    pub wheel3: f32,
    pub foot: f32,
    pub sustain: bool,
    pub soft: bool,
    pub at: f32,
    /// -1..1
    pub bend: f32,
    pub volume: f32,
    pub expression: f32,
}

/// Hyperknob macros derived from the knob names (see `Part::macros`).
#[derive(Clone, Copy, Debug, Default)]
pub struct Macros {
    pub cutoff: f32,  // semitones
    pub reso: f32,    // -1..1
    pub attack: f32,  // time factor exponent (-1..1 → /8 .. ×8)
    pub release: f32, // idem
    pub decay: f32,
    pub drive: f32, // 0..1
    pub vibrato: f32,
    pub width: f32,
    pub level: f32, // dB
    pub fx_send: [f32; 4],
    pub fx_mix: f32, // insert fx mix offset
    pub rotary_fast: f32,
    pub octave: f32,
    pub vel_sens: f32,
    pub balance: f32,
    pub wave: f32,
    pub detune: f32,
    pub lfo_rate: f32,
    pub start: f32,
}

#[derive(Clone)]
pub struct Part {
    pub patch: Option<Arc<Patch>>,
    pub mute: bool,
    /// Layered with the part above (plays from its MIDI channel).
    pub link: bool,
    pub level: f32,
    pub pan: f32,
    pub output: usize,
    pub sends: [f32; 4],
    pub patch_fx_on: bool,
    pub key_range: (i32, i32, i32),
    pub vel_range: (i32, i32, i32),
    pub semi: i32,
    pub cent: f32,
    pub max_voices: usize,
    pub lock: bool,
    pub knobs: [f32; 6],
    pub ctl: Controllers,
    pub patch_fx: [Option<FxSlot>; 4],
    // runtime
    seg_of: Vec<usize>,
    elem_fx: Vec<Option<Fx>>,
    parallel: Vec<bool>,
    segs: usize,
    lfo: [Lfo; 2],
    arp: Arp,
    arp_set: arp::Settings,
    held: Vec<(u8, u8)>,
    sustained: Vec<u8>,
    pub macros: Macros,
    phrase: Option<Arc<crate::phrase::Phrase>>,
    note_count: u32,
    pub meter: f32,
    pub meter_vel: f32,
    pub activity: f32,
}

/// Samples assigned to one Hypersonic 2 sample bank.
#[derive(Clone)]
pub enum SampleSource {
    /// A SoundFont; `preset` = Some((bank, program)) forces one preset,
    /// None picks the General MIDI program matching the HS2 bank.
    Font { sf: Arc<SoundFont>, preset: Option<(u16, u16)> },
    /// A WAV multisample (drum banks: one file per Wave index).
    Wavs(Arc<sf2::Preset>),
}

#[derive(Clone, Copy, Debug)]
pub struct FxSlot {
    pub id: u32,
    pub p: [f32; fx::NPARAM],
    pub on: bool,
}

impl FxSlot {
    pub fn new(id: u32) -> FxSlot {
        FxSlot { id, p: fx::defaults(id), on: true }
    }
}

impl Default for Part {
    fn default() -> Part {
        Part {
            patch: None,
            mute: false,
            link: false,
            level: 0.75,
            pan: 0.0,
            output: 0,
            sends: [0.15, 0.0, 0.0, 0.0],
            patch_fx_on: false,
            key_range: (0, 127, 0),
            vel_range: (1, 127, 0),
            semi: 0,
            cent: 0.0,
            max_voices: 64,
            lock: false,
            knobs: [0.5; 6],
            ctl: Controllers { volume: 1.0, expression: 1.0, ..Default::default() },
            patch_fx: [None; 4],
            seg_of: Vec::new(),
            elem_fx: Vec::new(),
            parallel: Vec::new(),
            segs: 1,
            lfo: [Lfo::default(); 2],
            arp: Arp::default(),
            arp_set: arp::Settings::from_patch(&Patch::new("")),
            held: Vec::new(),
            sustained: Vec::new(),
            macros: Macros::default(),
            phrase: None,
            note_count: 0,
            meter: 0.0,
            meter_vel: 0.0,
            activity: 0.0,
        }
    }
}

impl Part {
    pub fn name(&self) -> &str {
        self.patch.as_ref().map(|p| p.display_name()).unwrap_or("")
    }
    /// Recompute the Hyperknob macros from the knob names and positions.
    /// Each knob's offset (patch param) is its neutral position.
    pub fn update_macros(&mut self) {
        let mut m = Macros::default();
        let Some(p) = &self.patch else {
            self.macros = m;
            return;
        };
        for k in 0..6 {
            let ofs = p.p[pp::KNOB[k] + 1];
            let ofs = if ofs <= 0.0 || ofs >= 1.0 { 0.5 } else { ofs };
            let d = self.knobs[k] - ofs; // -1..1
            let name = p.knob_names[k].to_lowercase();
            let has = |w: &[&str]| w.iter().any(|x| name.contains(x));
            if has(&["reverb", "room", "ambien", "space", "hall"]) {
                m.fx_send[0] += d;
            } else if has(&["delay", "echo"]) {
                m.fx_send[2] += d;
            } else if has(&["rotary", "speed", "leslie"]) {
                m.rotary_fast += d;
            } else if has(&["chorus", "phaser", "flang", "warmth", "ensemble", "vibe", "tremolo"]) {
                m.fx_mix += d;
            } else if has(&["vibrato"]) {
                m.vibrato += d;
            } else if has(&["reso", "q "]) {
                m.reso += d;
            } else if has(&["attack shape", "thump", "click", "punch"]) {
                m.start += d;
            } else if has(&["attack", "swell"]) {
                m.attack += d;
            } else if has(&["release", "sustain", "length", "fade"]) {
                m.release += d;
            } else if has(&["decay"]) {
                m.decay += d;
            } else if has(&["drive", "dirt", "distort", "overdrive", "grit", "crunch", "fuzz"]) {
                m.drive += d;
            } else if has(&["width", "spread", "stereo"]) {
                m.width += d;
            } else if has(&["octave"]) {
                m.octave += d;
            } else if has(&["veloc", "dynamic"]) {
                m.vel_sens += d;
            } else if has(&["balance", "mix", "layer", "x-fade", "xfade"]) {
                m.balance += d;
            } else if has(&["volume", "level", "gain"]) {
                m.level += d * 24.0;
            } else if has(&["color", "colour", "select", "wave", "shape", "formant", "timbre", "harmonic"]) {
                m.wave += d;
            } else if has(&["detune", "fat", "unison"]) {
                m.detune += d;
            } else if has(&["rate", "lfo", "motion", "movement"]) {
                m.lfo_rate += d;
            } else {
                // "Tone", "Brightness", "EQ", "Cutoff", "Filter" and unknown labels
                m.cutoff += d * 48.0;
            }
        }
        self.macros = m;
    }
}

#[derive(Clone, Copy)]
struct GlobalFxRt {
    level: f32,
    pan: f32,
}

#[derive(Clone)]
struct Voice {
    active: bool,
    part: usize,
    elem: usize,
    kind: ElemType,
    note: u8,
    key: f32,
    vel: f32,
    rnd: f32,
    released: bool,
    sustained: bool,
    age: u64,
    xf: f32,
    pitch: f32,
    glide_from: f32,
    glide_left: f32,
    glide_time: f32,
    delay: f32,
    kill: f32,
    osc: [Osc; 4],
    ph: [f32; 12],
    zone: Option<Zone>,
    spos: f64,
    sstep: f64,
    drum: u8,
    fb: Option<Fallback>,
    filt: [Filter; 2],
    hp: [OnePole; 2],
    aenv: Adsfr,
    fenv: Adsfr,
    menv: ModEnv,
    lfo3: Lfo,
    perc: f32,
    click: f32,
    ks: Box<[f32; 2048]>,
    ks_len: usize,
    ks_pos: usize,
    rng: Rng,
    // per control block
    c_amp: f32,
    c_amp_prev: f32,
    c_pan: (f32, f32),
    c_dt: [f32; 4],
    c_cut: f32,
    c_reso: f32,
    c_wave: f32,
    c_wave2: f32,
    c_mode: FilterMode,
    c_drive: f32,
    c_fm: [f32; 2],
    c_table: Option<Arc<Table>>,
    c_table2: Option<Arc<Table>>,
    row: [f32; 16],
    row_state: [f32; 16],
}

/// Built-in replacement for a Sample element when no SoundFont is loaded:
/// a small synthesis model per Hypersonic sample category.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fallback {
    Piano,
    EPiano,
    Organ,
    Mallet,
    Pluck,
    Bass,
    Strings,
    Brass,
    Wind,
    Voice,
    Pad,
    Noise,
    Kick,
    Snare,
    Tom,
    HiHat,
    Cymbal,
    Perc,
}

fn fallback_for(bank: u32, wave: u32) -> Fallback {
    match bank {
        0 => Fallback::Kick,
        1 => Fallback::Snare,
        2 | 23 => Fallback::Tom,
        3 => Fallback::HiHat,
        4 => Fallback::Cymbal,
        5..=8 | 24 => match wave % 4 {
            0 => Fallback::Perc,
            1 => Fallback::Tom,
            2 => Fallback::HiHat,
            _ => Fallback::Snare,
        },
        9 | 10 => Fallback::Piano,
        11..=15 | 19 => Fallback::EPiano,
        16..=18 => Fallback::Organ,
        20..=22 | 102 => Fallback::Mallet,
        30..=39 => Fallback::Bass,
        40..=49 | 57 | 58 | 90 | 91 => Fallback::Pluck,
        50..=56 => Fallback::Strings,
        59..=59 | 70..=79 => Fallback::Brass,
        60..=69 => Fallback::Voice,
        80..=89 => Fallback::Wind,
        105 => Fallback::Noise,
        _ => Fallback::Pad,
    }
}

impl Voice {
    fn new() -> Voice {
        Voice {
            active: false,
            part: 0,
            elem: 0,
            kind: ElemType::Analog,
            note: 0,
            key: 0.0,
            vel: 0.0,
            rnd: 0.0,
            released: false,
            sustained: false,
            age: 0,
            xf: 1.0,
            pitch: 60.0,
            glide_from: 60.0,
            glide_left: 0.0,
            glide_time: 0.0,
            delay: 0.0,
            kill: 0.0,
            osc: [Osc::default(); 4],
            ph: [0.0; 12],
            zone: None,
            spos: 0.0,
            sstep: 0.0,
            drum: 0,
            fb: None,
            filt: [Filter::default(); 2],
            hp: [OnePole::default(); 2],
            aenv: Adsfr::default(),
            fenv: Adsfr::default(),
            menv: ModEnv::default(),
            lfo3: Lfo::default(),
            perc: 0.0,
            click: 0.0,
            ks: Box::new([0.0; 2048]),
            ks_len: 0,
            ks_pos: 0,
            rng: Rng::new(1),
            c_amp: 0.0,
            c_amp_prev: 0.0,
            c_pan: (1.0, 1.0),
            c_dt: [0.0; 4],
            c_cut: 20000.0,
            c_reso: 0.0,
            c_wave: 0.0,
            c_wave2: 0.0,
            c_mode: FilterMode::Off,
            c_drive: 1.0,
            c_fm: [0.0; 2],
            c_table: None,
            c_table2: None,
            row: [0.0; 16],
            row_state: [0.0; 16],
        }
    }
}

pub struct Synth {
    pub sr: f32,
    pub parts: Vec<Part>,
    voices: Vec<Voice>,
    pub global_fx: [FxSlot; 4],
    gfx: Vec<Fx>,
    gfx_rt: [GlobalFxRt; 4],
    pub master: f32,
    /// "Output Hype" 0..1 (mastering-style brightness / punch)
    pub hype: f32,
    pub master_tune: f32,
    pub tempo: f32,
    pub playing: bool,
    pub song_beats: f64,
    pub wt: Arc<wavetable::Bank>,
    pub sf: Option<Arc<SoundFont>>,
    /// Per HS2 sample bank assignments (override `sf`).
    pub sample_map: Vec<(u32, SampleSource)>,
    /// Program change requests (part, bank, program) for the host to fulfil.
    pub program_requests: Vec<(usize, u8, u8)>,
    /// Phrase banks for the arpeggiator: (bank number, phrases).
    pub phrases: Vec<(u32, Vec<Arc<crate::phrase::Phrase>>)>,
    bank_msb: [u8; 16],
    rng: Rng,
    clock: u64,
    buf: Vec<[Vec<f32>; 2]>, // per part per segment (flattened: part * 16 + seg)
    part_out: [Vec<f32>; 2],
    send_bus: [[Vec<f32>; 2]; 4],
    fxtmp: [Vec<f32>; 2],
    hype_lp: [OnePole; 2],
    hype_env: f32,
    pub peak: [f32; 2],
    arp_ev: Vec<arp::Event>,
}

fn mk_bufs(n: usize) -> [Vec<f32>; 2] {
    [vec![0.0; n], vec![0.0; n]]
}

impl Synth {
    pub fn new(sr: f32) -> Synth {
        let global_fx = [FxSlot::new(1), FxSlot::new(3), FxSlot::new(11), FxSlot::new(20)];
        let gfx = global_fx.iter().map(|s| Fx::new(s.id, s.p, sr)).collect();
        let mut s = Synth {
            sr,
            parts: vec![Part::default(); PARTS],
            voices: (0..MAX_VOICES).map(|_| Voice::new()).collect(),
            global_fx,
            gfx,
            gfx_rt: [GlobalFxRt { level: 1.0, pan: 0.0 }; 4],
            master: 0.7,
            hype: 0.0,
            master_tune: 0.0,
            tempo: 120.0,
            playing: false,
            song_beats: 0.0,
            wt: Arc::new(wavetable::Bank::new()),
            sf: None,
            sample_map: Vec::new(),
            program_requests: Vec::new(),
            phrases: Vec::new(),
            bank_msb: [0; 16],
            rng: Rng::new(0x1234_5678),
            clock: 0,
            buf: (0..PARTS * 16).map(|_| mk_bufs(MAX_BLOCK)).collect(),
            part_out: mk_bufs(MAX_BLOCK),
            send_bus: [mk_bufs(MAX_BLOCK), mk_bufs(MAX_BLOCK), mk_bufs(MAX_BLOCK), mk_bufs(MAX_BLOCK)],
            fxtmp: mk_bufs(MAX_BLOCK),
            hype_lp: [OnePole::default(); 2],
            hype_env: 0.0,
            peak: [0.0; 2],
            arp_ev: Vec::with_capacity(64),
        };
        for (i, p) in s.parts.iter_mut().enumerate() {
            p.lfo[0].phase = i as f32 * 0.07;
        }
        s
    }

    /// Wavetables a patch needs (generate them before `set_patch`, off the audio thread).
    pub fn tables_for(p: &Patch) -> Vec<usize> {
        let mut v = Vec::new();
        for e in &p.elements {
            match e.kind() {
                ElemType::Wavetable => v.push(enum_idx(e.p[ep::W_INDEX], 64).min(wavetable::NAMES.len() - 1)),
                ElemType::Analog => v.push(enum_idx(e.p[ep::A_WAVETABLE], 64).min(wavetable::NAMES.len() - 1)),
                _ => {}
            }
        }
        v
    }

    /// Load a patch into a part (resets MIDI/Mix settings unless locked).
    pub fn set_patch(&mut self, part: usize, patch: Option<Arc<Patch>>) {
        self.all_notes_off_part(part, true);
        let sr = self.sr;
        let pt = &mut self.parts[part];
        if let Some(p) = &patch {
            if !pt.lock {
                pt.key_range = (note_of(p.p[pp::KEY_LO]), note_of(p.p[pp::KEY_HI]).max(if p.p[pp::KEY_HI] == 0.0 { 127 } else { 0 }), (p.p[pp::KEY_XF] * 48.0) as i32);
                pt.vel_range = (note_of(p.p[pp::VEL_LO]).max(1), note_of(p.p[pp::VEL_HI]).max(if p.p[pp::VEL_HI] == 0.0 { 127 } else { 1 }), (p.p[pp::VEL_XF] * 127.0) as i32);
                pt.semi = if p.p[pp::TRANSPOSE] > 0.0 { (bipolar(p.p[pp::TRANSPOSE]) * 36.0).round() as i32 } else { 0 };
                pt.cent = if p.p[pp::FINE] > 0.0 { bipolar(p.p[pp::FINE]) * 100.0 } else { 0.0 };
                let sends = [p.p[pp::FX_SEND], p.p[pp::FX_SEND + 1], p.p[pp::FX_SEND + 2], p.p[pp::FX_SEND + 3]];
                if sends.iter().any(|&s| s > 0.0) {
                    pt.sends = sends;
                }
            }
            for k in 0..6 {
                let ofs = p.p[pp::KNOB[k] + 1];
                pt.knobs[k] = if ofs <= 0.0 || ofs >= 1.0 { 0.5 } else { ofs };
            }
            // chain segments: synth elements before the n-th FX element feed segment n
            let mut seg = 0;
            pt.seg_of.clear();
            pt.elem_fx.clear();
            pt.parallel.clear();
            for e in &p.elements {
                if e.kind() == ElemType::Fx {
                    let id = e.fx_type();
                    // original content stores the original FX layout: use own defaults
                    let mut prm = fx::defaults(id);
                    if p.own_fx_params() && e.p[ep::FX_P..ep::FX_P + fx::NPARAM].iter().any(|&v| v != 0.0) {
                        prm.copy_from_slice(&e.p[ep::FX_P..ep::FX_P + fx::NPARAM]);
                    }
                    let mut f = Fx::new(id, prm, sr);
                    f.on = !e.muted();
                    pt.elem_fx.push(Some(f));
                    pt.parallel.push(e.p[ep::TYPE] == 0.0);
                    pt.seg_of.push(seg);
                    seg = (seg + 1).min(15);
                } else {
                    pt.elem_fx.push(None);
                    pt.parallel.push(false);
                    pt.seg_of.push(seg);
                }
            }
            pt.segs = seg + 1;
            pt.arp_set = arp::Settings::from_patch(p);
            pt.phrase = find_phrase(&self.phrases, pt.arp_set.phrase);
        } else {
            pt.seg_of.clear();
            pt.elem_fx.clear();
            pt.segs = 1;
        }
        pt.patch = patch;
        pt.update_macros();
    }

    /// Apply an edited copy of the part's patch without stopping its voices
    /// (same element structure); otherwise like `edit_patch`.
    pub fn update_patch(&mut self, part: usize, patch: Patch) {
        let same = self.parts[part]
            .patch
            .as_ref()
            .map(|old| old.elements.len() == patch.elements.len() && old.elements.iter().zip(&patch.elements).all(|(a, b)| (a.kind() == ElemType::Fx) == (b.kind() == ElemType::Fx)))
            .unwrap_or(false);
        if !same {
            self.edit_patch(part, patch);
            return;
        }
        let sr = self.sr;
        let pt = &mut self.parts[part];
        for (j, e) in patch.elements.iter().enumerate() {
            if let Some(Some(f)) = pt.elem_fx.get_mut(j) {
                let id = e.fx_type();
                let mut prm = fx::defaults(id);
                if patch.own_fx_params() && e.p[ep::FX_P..ep::FX_P + fx::NPARAM].iter().any(|&v| v != 0.0) {
                    prm.copy_from_slice(&e.p[ep::FX_P..ep::FX_P + fx::NPARAM]);
                }
                if f.id != id {
                    *f = Fx::new(id, prm, sr);
                }
                f.p = prm;
                f.on = !e.muted();
            }
        }
        pt.arp_set = arp::Settings::from_patch(&patch);
        pt.phrase = find_phrase(&self.phrases, pt.arp_set.phrase);
        pt.patch = Some(Arc::new(patch));
        pt.update_macros();
    }

    /// Replace the patch data of a part after an edit (keeps part settings).
    pub fn edit_patch(&mut self, part: usize, patch: Patch) {
        let lock = self.parts[part].lock;
        let knobs = self.parts[part].knobs;
        let (kr, vr, sends) = (self.parts[part].key_range, self.parts[part].vel_range, self.parts[part].sends);
        self.parts[part].lock = true;
        self.set_patch(part, Some(Arc::new(patch)));
        let pt = &mut self.parts[part];
        pt.lock = lock;
        pt.knobs = knobs;
        pt.key_range = kr;
        pt.vel_range = vr;
        pt.sends = sends;
        pt.update_macros();
    }

    /// Load a Hypersonic 2 combi: all parts are cleared, then filled.
    pub fn load_combi(&mut self, c: &crate::combi::Combi) {
        for i in 0..PARTS {
            self.parts[i].lock = false;
            self.set_patch(i, None);
            self.parts[i].link = false;
            self.parts[i].mute = false;
            self.parts[i].level = 0.75;
            self.parts[i].pan = 0.0;
        }
        for cp in &c.parts {
            if cp.part >= PARTS {
                continue;
            }
            for t in Synth::tables_for(&cp.patch) {
                self.wt.get(t);
            }
            self.set_patch(cp.part, Some(Arc::new(cp.patch.clone())));
            self.parts[cp.part].link = cp.link;
        }
    }

    pub fn set_global_fx(&mut self, slot: usize, fxs: FxSlot) {
        self.global_fx[slot] = fxs;
        if self.gfx[slot].id != fxs.id {
            self.gfx[slot] = Fx::new(fxs.id, fxs.p, self.sr);
        }
        self.gfx[slot].p = fxs.p;
        self.gfx[slot].on = fxs.on;
    }

    pub fn set_global_fx_out(&mut self, slot: usize, level: f32, pan: f32) {
        self.gfx_rt[slot] = GlobalFxRt { level, pan };
    }

    // -----------------------------------------------------------------------
    // MIDI
    // -----------------------------------------------------------------------

    /// Parts playing from MIDI channel `ch` (the part itself and linked parts below).
    pub fn parts_for_channel(&self, ch: usize) -> Vec<usize> {
        let mut out = Vec::new();
        for i in 0..PARTS {
            // the channel a part listens to: walk up while linked
            let mut j = i;
            while j > 0 && self.parts[j].link {
                j -= 1;
            }
            if j == ch {
                out.push(i);
            }
        }
        out
    }

    pub fn midi(&mut self, msg: &[u8]) {
        if msg.is_empty() {
            return;
        }
        let st = msg[0];
        let ch = (st & 0x0f) as usize;
        let d1 = *msg.get(1).unwrap_or(&0);
        let d2 = *msg.get(2).unwrap_or(&0);
        match st & 0xf0 {
            0x90 if d2 > 0 => {
                for p in self.parts_for_channel(ch) {
                    self.note_on(p, d1, d2);
                }
            }
            0x80 | 0x90 => {
                for p in self.parts_for_channel(ch) {
                    self.note_off(p, d1);
                }
            }
            0xb0 => {
                for p in self.parts_for_channel(ch) {
                    self.cc(p, d1, d2);
                }
                if d1 == 0 {
                    self.bank_msb[ch] = d2;
                }
            }
            0xc0 => self.program_requests.push((ch, self.bank_msb[ch], d1)),
            0xd0 => {
                for p in self.parts_for_channel(ch) {
                    self.parts[p].ctl.at = d1 as f32 / 127.0;
                }
            }
            0xe0 => {
                let v = ((d2 as i32) << 7 | d1 as i32) - 8192;
                for p in self.parts_for_channel(ch) {
                    self.parts[p].ctl.bend = v as f32 / 8192.0;
                }
            }
            _ => {}
        }
    }

    pub fn cc(&mut self, part: usize, cc: u8, v: u8) {
        let f = v as f32 / 127.0;
        let pt = &mut self.parts[part];
        match cc {
            1 => pt.ctl.mw = f,
            2 => pt.ctl.breath = f,
            3 => pt.ctl.wheel3 = f,
            4 => pt.ctl.foot = f,
            7 => pt.ctl.volume = f,
            10 => pt.pan = (f - 0.5) * 2.0,
            11 => pt.ctl.expression = f,
            64 => {
                let on = v >= 64;
                pt.ctl.sustain = on;
                pt.arp.sustain(on);
                if !on {
                    let notes = std::mem::take(&mut self.parts[part].sustained);
                    for n in notes {
                        self.release_note(part, n);
                    }
                }
            }
            67 => pt.ctl.soft = v >= 64,
            120 | 123 => self.all_notes_off_part(part, cc == 120),
            121 => {
                pt.ctl = Controllers { volume: 1.0, expression: 1.0, ..Default::default() };
            }
            _ => {}
        }
    }

    pub fn panic(&mut self) {
        for p in 0..PARTS {
            self.all_notes_off_part(p, true);
        }
    }

    pub fn all_notes_off_part(&mut self, part: usize, hard: bool) {
        let mut ev = Vec::new();
        self.parts[part].arp.all_off(&mut ev);
        self.parts[part].held.clear();
        self.parts[part].sustained.clear();
        for v in self.voices.iter_mut().filter(|v| v.active && v.part == part) {
            if hard {
                v.active = false;
            } else {
                v.released = true;
                v.aenv.release();
                v.fenv.release();
                v.menv.release();
            }
        }
    }

    pub fn note_on(&mut self, part: usize, note: u8, vel: u8) {
        let pt = &mut self.parts[part];
        if pt.mute || pt.patch.is_none() {
            return;
        }
        pt.activity = 1.0;
        pt.meter_vel = vel as f32 / 127.0;
        if pt.arp_set.on {
            let s = pt.arp_set;
            pt.arp.note_on(&s, note, vel);
            return;
        }
        self.trigger(part, note, vel);
    }

    pub fn note_off(&mut self, part: usize, note: u8) {
        let pt = &mut self.parts[part];
        if pt.arp_set.on {
            pt.arp.note_off(note);
            return;
        }
        if pt.ctl.sustain {
            if !pt.sustained.contains(&note) {
                pt.sustained.push(note);
            }
            for v in self.voices.iter_mut().filter(|v| v.active && v.part == part && v.note == note) {
                v.sustained = true;
            }
            pt.held.retain(|h| h.0 != note);
            return;
        }
        self.release_note(part, note);
    }

    fn release_note(&mut self, part: usize, note: u8) {
        let pt = &mut self.parts[part];
        pt.held.retain(|h| h.0 != note);
        let mono = pt.patch.as_ref().map(|p| patch_mono(p.p[pp::MONO])).unwrap_or(false);
        let back = if mono { pt.held.last().copied() } else { None };
        let mut moved = false;
        for v in self.voices.iter_mut().filter(|v| v.active && v.part == part && v.note == note && !v.released) {
            if let Some((n, _)) = back {
                // mono: return to the last held note (legato)
                v.note = n;
                v.glide_from = v.pitch;
                v.glide_left = v.glide_time;
                v.key = n as f32;
                moved = true;
            } else {
                v.released = true;
                v.aenv.release();
                v.fenv.release();
                v.menv.release();
            }
        }
        let _ = moved;
    }

    /// Start voices for every element of the part's patch that responds.
    fn trigger(&mut self, part: usize, note: u8, vel: u8) {
        let Some(patch) = self.parts[part].patch.clone() else { return };
        let pt = &self.parts[part];
        let n = note as i32 + pt.semi;
        let Some(pxf) = range_gain(n, pt.key_range) else { return };
        let Some(pvf) = range_gain(vel as i32, pt.vel_range) else { return };
        let mono = patch_mono(patch.p[pp::MONO]);
        let glide_on = patch.p[pp::GLIDE] > 0.75;
        let legato = mono && !pt.held.is_empty();
        let prev_pitch = pt.held.last().map(|h| h.0 as f32);
        let ctl = pt.ctl;
        let macros = pt.macros;
        self.parts[part].held.retain(|h| h.0 != note);
        self.parts[part].held.push((note, vel));
        self.parts[part].note_count += 1;
        let nc = self.parts[part].note_count;
        let max_voices = self.parts[part].max_voices.min((patch.p[pp::MAX_VOICES] * 64.0).max(1.0) as usize).max(1);
        let transpose = self.parts[part].semi as f32 + self.parts[part].cent / 100.0 + bipolar(patch.p[pp::TRANSPOSE]) * 0.0 + self.master_tune;
        let tables: Vec<Option<Arc<Table>>> = patch
            .elements
            .iter()
            .map(|e| match e.kind() {
                ElemType::Wavetable => self.wt.try_get(enum_idx(e.p[ep::W_INDEX], 64)),
                ElemType::Analog => self.wt.try_get(enum_idx(e.p[ep::A_WAVETABLE], 64)),
                _ => None,
            })
            .collect();
        for (ei, e) in patch.elements.iter().enumerate() {
            let kind = e.kind();
            if kind == ElemType::Fx || e.muted() {
                continue;
            }
            let (klo, khi, kxf) = e.key_range();
            let khi = if e.p[ep::KEY_HI] == 0.0 && e.p[ep::KEY_LO] == 0.0 { 127 } else { khi };
            let Some(kg) = range_gain(n, (klo, khi, kxf)) else { continue };
            let (vlo, vhi, vxf) = e.vel_range();
            let vhi = if e.p[ep::VEL_HI] == 0.0 { 127 } else { vhi };
            let Some(vg) = range_gain(vel as i32, (vlo, vhi, vxf)) else { continue };
            // trigger conditions (Mod Page: Trigger view)
            let tc = enum_idx(e.p[ep::TRIGGER], 9);
            let ok = match tc {
                1 => ctl.mw >= 0.5,
                2 => ctl.mw < 0.5,
                3 => !ctl.sustain,
                4 => ctl.sustain,
                5 => nc % 2 == 1,
                6 => nc % 2 == 0,
                _ => true,
            };
            if !ok {
                continue;
            }
            let emono = mono || e.p[ep::MONO] >= 0.2;
            // mute conditions: same note / groups
            let mc = enum_idx(e.p[ep::MUTE_COND], 12);
            for v in self.voices.iter_mut().filter(|v| v.active && v.part == part) {
                let hit = match mc {
                    1 => v.note == note && v.elem == ei,
                    2 => v.elem != ei,
                    3 => true,
                    g @ 4..=11 => {
                        let other = patch.elements.get(v.elem).map(|o| enum_idx(o.p[ep::MUTE_COND], 12) == g).unwrap_or(false);
                        other
                    }
                    _ => false,
                };
                if hit {
                    v.kill = 0.003;
                }
            }
            // mono legato: retune the running voice
            if emono {
                if let Some(v) = self.voices.iter_mut().find(|v| v.active && v.part == part && v.elem == ei && !v.released) {
                    if legato {
                        v.glide_from = v.pitch;
                        v.note = note;
                        v.key = n as f32;
                        v.glide_left = v.glide_time.max(if glide_on { 0.05 } else { 0.0 });
                        continue;
                    } else {
                        v.kill = 0.002;
                    }
                }
            }
            // voice limit per part
            let used = self.voices.iter().filter(|v| v.active && v.part == part && v.kill == 0.0).count();
            if used >= max_voices {
                if let Some(v) = self.voices.iter_mut().filter(|v| v.active && v.part == part && v.kill == 0.0).min_by_key(|v| (!v.released, v.age)) {
                    v.kill = 0.004;
                }
            }
            let slot = match self.voices.iter().position(|v| !v.active) {
                Some(s) => s,
                None => {
                    let s = self.voices.iter().enumerate().min_by_key(|(_, v)| (!v.released, v.age)).map(|(i, _)| i).unwrap_or(0);
                    s
                }
            };
            let seed = self.rng.next_u32();
            let sr = self.sr;
            let resolved = if kind == ElemType::Sample {
                let bank = (e.p[ep::S_BANK] * 128.0) as u32;
                let wave = (e.p[ep::S_WAVE] * 128.0) as u32;
                self.resolve_sample(bank, wave, n.clamp(0, 127) as u8, vel)
            } else {
                None
            };
            let clock = self.clock;
            let v = &mut self.voices[slot];
            v.active = true;
            v.part = part;
            v.elem = ei;
            v.kind = kind;
            v.note = note;
            v.key = n as f32;
            let vs = (1.0 + macros.vel_sens).clamp(0.0, 2.0);
            v.vel = (vel as f32 / 127.0).powf(vs.max(0.05));
            v.rng = Rng::new(seed);
            v.rnd = v.rng.bi();
            v.released = false;
            v.sustained = false;
            v.age = clock;
            v.xf = kg * vg * pxf * pvf;
            v.kill = 0.0;
            v.delay = if e.p[ep::NOTE_DELAY] >= 0.5 { time_s(e.p[ep::DELAY_TIME]) * 0.25 } else { 0.0 };
            let base = n as f32 + transpose + e.transpose() + if macros.octave > 0.25 && ei > 0 { 12.0 } else if macros.octave < -0.25 && ei > 0 { -12.0 } else { 0.0 };
            v.pitch = base;
            v.glide_time = if glide_on || enum_idx(e.p[ep::GLIDE_MODE], 4) == 3 { time_s(e.p[ep::GLIDE_TIME].max(patch.p[pp::GLIDE_TIME])) * 0.5 } else { 0.0 };
            if let (Some(pp_), true) = (prev_pitch, v.glide_time > 0.0) {
                v.glide_from = pp_ + transpose + e.transpose();
                v.glide_left = v.glide_time;
            } else {
                v.glide_from = base;
                v.glide_left = 0.0;
            }
            let rs = e.p[ep::RND_START];
            v.osc = [Osc::default(); 4];
            for k in 0..4 {
                v.osc[k].phase = if rs > 0.0 { v.rng.unit() } else { e.p[ep::START] * 0.0 };
            }
            v.ph = [0.0; 12];
            if rs > 0.01 {
                for k in 0..12 {
                    v.ph[k] = v.rng.unit();
                }
            }
            v.filt = [Filter::default(); 2];
            v.hp = [OnePole::default(); 2];
            // envelopes
            let tscale = |base: f32, m: f32| base * 8f32.powf(m);
            let ap = &e.p;
            let velatt = bipolar(ap[ep::AENV_VEL_ATT]) * v.vel;
            v.aenv = Adsfr {
                a: tscale(time_s(ap[ep::AENV]), macros.attack) * 4f32.powf(-velatt),
                d: tscale(time_s(ap[ep::AENV + 1]), macros.decay),
                s: ap[ep::AENV + 2],
                f: fade(ap[ep::AENV + 3]),
                r: tscale(time_s(ap[ep::AENV + 4]), macros.release) * (1.0 - bipolar(ap[ep::RELEASE_KEY]) * (n as f32 - 60.0) / 60.0).clamp(0.2, 4.0),
                ..Default::default()
            };
            v.fenv = Adsfr {
                a: tscale(time_s(ap[ep::FENV]), macros.attack),
                d: tscale(time_s(ap[ep::FENV + 1]), macros.decay),
                s: ap[ep::FENV + 2],
                f: fade(ap[ep::FENV + 3]),
                r: tscale(time_s(ap[ep::FENV + 4]), macros.release),
                ..Default::default()
            };
            let analog_env = enum_idx(ap[ep::ENV_MODE], 2) == 1;
            v.aenv.trigger(!analog_env);
            v.fenv.trigger(!analog_env);
            v.menv.levels = [bipolar(ap[ep::MENV]), bipolar(ap[ep::MENV + 2]), bipolar(ap[ep::MENV + 4]), bipolar(ap[ep::MENV + 6]), bipolar(ap[ep::MENV + 8])];
            v.menv.times = [time_s(ap[ep::MENV + 1]), time_s(ap[ep::MENV + 3]), time_s(ap[ep::MENV + 5]), time_s(ap[ep::MENV + 7])];
            v.menv.trigger();
            let lsync = enum_idx(ap[ep::LFO3_SYNC], 5);
            if lsync == 2 || lsync == 3 || v.lfo3.phase == 0.0 {
                v.lfo3.restart(if ap[ep::LFO3_PHASE] > 0.98 { v.rng.unit() } else { ap[ep::LFO3_PHASE] });
            }
            v.row_state = [0.0; 16];
            v.c_table = tables[ei].clone();
            v.c_table2 = None;
            v.c_amp_prev = 0.0;
            v.c_amp = 0.0;
            v.perc = 1.0;
            v.click = if kind == ElemType::Drawbar { ap[ep::D_CLICK] } else { ap[ep::CLICK] * 0.5 };
            v.zone = None;
            v.fb = None;
            v.drum = 0;
            if kind == ElemType::Sample {
                let bank = (ap[ep::S_BANK] * 128.0) as u32;
                let wave = (ap[ep::S_WAVE] * 128.0) as u32;
                let mut zone = None;
                if let Some((z, drum)) = resolved {
                    v.drum = drum;
                    zone = Some(z);
                }
                if let Some(z) = zone {
                    let start_ofs = (ap[ep::START] + macros.start.max(0.0) * 0.3).clamp(0.0, 0.9);
                    v.spos = z.start as f64 + start_ofs as f64 * (z.end - z.start) as f64 * 0.25;
                    v.sstep = 0.0;
                    v.zone = Some(z);
                } else {
                    let fbk = fallback_for(bank, wave);
                    v.fb = Some(fbk);
                    if matches!(fbk, Fallback::Pluck | Fallback::Bass) {
                        let hz = pitch_hz(base);
                        v.ks_len = ((sr / hz.max(20.0)) as usize).clamp(4, 2047);
                        v.ks_pos = 0;
                        for i in 0..v.ks_len {
                            v.ks[i] = v.rng.bi() * 0.8;
                        }
                    }
                }
            }
            v.c_mode = FilterMode::ALL[enum_idx(ap[ep::FILTER], 9)];
        }
    }

    /// Zone for a Sample element: the bank's assignment, else the global
    /// SoundFont (GM program for the HS2 bank). Returns (zone, drum root):
    /// drum root > 0 means the sample is not pitched by the played key.
    pub fn resolve_sample(&self, bank: u32, wave: u32, key: u8, vel: u8) -> Option<(Zone, u8)> {
        let (gm_bank, progs) = sf2::gm_for_bank(bank);
        let prog = progs[wave as usize % progs.len()];
        let from_font = |sf: &SoundFont, preset: Option<(u16, u16)>| -> Option<(Zone, u8)> {
            match preset {
                Some((b, p)) => {
                    let pr = sf.preset(b, p)?;
                    if b >= 128 {
                        let dn = if sf2::is_drum_bank(bank) { prog as u8 } else { key };
                        pr.find(dn, vel).next().cloned().map(|z| (z, dn.max(1)))
                    } else {
                        pr.find(key, vel).next().cloned().map(|z| (z, 0))
                    }
                }
                None if gm_bank == 128 => sf.preset(128, 0).and_then(|pr| pr.find(prog as u8, vel).next().cloned()).map(|z| (z, prog as u8)),
                None => sf.preset(0, prog).and_then(|pr| pr.find(key, vel).next().cloned()).map(|z| (z, 0)),
            }
        };
        if let Some((_, src)) = self.sample_map.iter().find(|(b, _)| *b == bank) {
            let r = match src {
                SampleSource::Font { sf, preset } => from_font(sf, *preset),
                SampleSource::Wavs(p) if sf2::is_drum_bank(bank) && !p.zones.is_empty() => {
                    let z = p.zones[wave as usize % p.zones.len()].clone();
                    let root = z.root.max(1);
                    Some((z, root))
                }
                SampleSource::Wavs(p) => p.find(key, vel).next().cloned().map(|z| (z, 0)),
            };
            if r.is_some() {
                return r;
            }
        }
        self.sf.as_ref().and_then(|sf| from_font(sf, None))
    }

    // -----------------------------------------------------------------------
    // rendering
    // -----------------------------------------------------------------------

    /// Render `l.len()` samples (any length) into the output buffers.
    pub fn render(&mut self, l: &mut [f32], r: &mut [f32]) {
        let n = l.len().min(r.len());
        let mut o = 0;
        while o < n {
            let m = (n - o).min(MAX_BLOCK);
            self.render_block(&mut l[o..o + m], &mut r[o..o + m]);
            o += m;
        }
    }

    fn render_block(&mut self, out_l: &mut [f32], out_r: &mut [f32]) {
        let n = out_l.len();
        out_l.fill(0.0);
        out_r.fill(0.0);
        for b in self.send_bus.iter_mut() {
            b[0][..n].fill(0.0);
            b[1][..n].fill(0.0);
        }
        let beats = n as f64 * self.tempo as f64 / 60.0 / self.sr as f64;
        // arpeggiators
        for p in 0..PARTS {
            if self.parts[p].arp_set.on && (self.parts[p].arp.active()) {
                let s = self.parts[p].arp_set;
                let mut ev = std::mem::take(&mut self.arp_ev);
                ev.clear();
                match (&s.mode, self.parts[p].phrase.clone()) {
                    (arp::Mode::Phrase, Some(ph)) => self.parts[p].arp.advance_phrase(&s, &ph, beats, &mut ev),
                    _ => self.parts[p].arp.advance(&s, beats, &mut ev),
                }
                for e in &ev {
                    match *e {
                        arp::Event::On(nn, vv) => self.trigger(p, nn, vv),
                        arp::Event::Off(nn) => self.release_note(p, nn),
                        arp::Event::Cc(c, v) => self.cc(p, c, v),
                    }
                }
                self.arp_ev = ev;
            }
        }
        if self.playing {
            self.song_beats += beats;
        }
        // part LFOs (control rate, once per block)
        let dt_block = n as f32 / self.sr;
        let tempo = self.tempo;
        for p in self.parts.iter_mut() {
            if let Some(pa) = &p.patch {
                for k in 0..2 {
                    let base = if k == 0 { pp::LFO1 } else { pp::LFO2 };
                    let wave = LfoWave::ALL[enum_idx(pa.p[base], 9)];
                    let sync = enum_idx(pa.p[base + 1], 5);
                    let hz = if sync >= 3 { tempo / 60.0 / lfo_beats(pa.p[base + 2]) } else { lfo_hz(pa.p[base + 2]) } * 2f32.powf(p.macros.lfo_rate * 3.0);
                    let mut rng = Rng::new(0x777 + k as u32);
                    p.lfo[k].step(wave, hz, dt_block, &mut rng);
                }
            }
            p.activity *= 0.9;
        }
        // voices into part segment buffers
        for p in 0..PARTS {
            let segs = self.parts[p].segs.max(1);
            for s in 0..segs {
                let b = &mut self.buf[p * 16 + s];
                b[0][..n].fill(0.0);
                b[1][..n].fill(0.0);
            }
        }
        self.clock += n as u64;
        let sr = self.sr;
        for vi in 0..MAX_VOICES {
            if !self.voices[vi].active {
                continue;
            }
            let part = self.voices[vi].part;
            let Some(patch) = self.parts[part].patch.clone() else {
                self.voices[vi].active = false;
                continue;
            };
            let ei = self.voices[vi].elem;
            let Some(e) = patch.elements.get(ei) else {
                self.voices[vi].active = false;
                continue;
            };
            let seg = *self.parts[part].seg_of.get(ei).unwrap_or(&0);
            let ctx = PartCtx {
                ctl: self.parts[part].ctl,
                lfo: [self.parts[part].lfo[0].value, self.parts[part].lfo[1].value],
                knobs: self.parts[part].knobs,
                seq: [self.parts[part].arp.seq1, self.parts[part].arp.seq2],
                macros: self.parts[part].macros,
                pb_up: pb_range(patch.p[pp::PB_UP]),
                pb_down: pb_range(patch.p[pp::PB_DOWN]),
                tempo,
                elems: patch.elements.len(),
            };
            let buf = &mut self.buf[part * 16 + seg];
            let (bl, br) = buf.split_at_mut(1);
            render_voice(&mut self.voices[vi], e, &ctx, &mut bl[0][..n], &mut br[0][..n], sr, &self.wt);
        }
        // parts: segments → insert FX chain → mix / sends
        for p in 0..PARTS {
            if self.parts[p].patch.is_none() {
                self.parts[p].meter *= 0.9;
                continue;
            }
            let segs = self.parts[p].segs.max(1);
            self.part_out[0][..n].fill(0.0);
            self.part_out[1][..n].fill(0.0);
            let mut fx_index = 0;
            let n_fx = self.parts[p].elem_fx.iter().filter(|f| f.is_some()).count();
            let mix_ofs = self.parts[p].macros.fx_mix;
            let rot = self.parts[p].macros.rotary_fast;
            let drive_m = self.parts[p].macros.drive;
            for s in 0..segs {
                for i in 0..n {
                    self.part_out[0][i] += self.buf[p * 16 + s][0][i];
                    self.part_out[1][i] += self.buf[p * 16 + s][1][i];
                }
                if s < n_fx {
                    // the s-th FX element
                    let mut k = 0;
                    let mut idx = None;
                    for (j, f) in self.parts[p].elem_fx.iter().enumerate() {
                        if f.is_some() {
                            if k == s {
                                idx = Some(j);
                                break;
                            }
                            k += 1;
                        }
                    }
                    if let Some(j) = idx {
                        let parallel = self.parts[p].parallel[j];
                        let f = self.parts[p].elem_fx[j].as_mut().unwrap();
                        let saved = (f.p[3], f.p[5], f.p[0]);
                        if matches!(f.family(), fx::Family::Chorus | fx::Family::Flanger | fx::Family::Phaser | fx::Family::Modulation) {
                            f.p[3] = (f.p[3] + mix_ofs * 0.5).clamp(0.0, 1.0);
                            if f.id == 55 {
                                f.p[5] = if rot > 0.0 { 1.0 } else if rot < 0.0 { 0.0 } else { f.p[5] };
                            }
                        }
                        if f.family() == fx::Family::Distortion {
                            f.p[0] = (f.p[0] + drive_m * 0.5).clamp(0.0, 1.0);
                        }
                        if parallel {
                            self.fxtmp[0][..n].copy_from_slice(&self.part_out[0][..n]);
                            self.fxtmp[1][..n].copy_from_slice(&self.part_out[1][..n]);
                            let (a, b) = self.fxtmp.split_at_mut(1);
                            f.process(&mut a[0][..n], &mut b[0][..n], tempo, true);
                            for i in 0..n {
                                self.part_out[0][i] += self.fxtmp[0][i] * 0.35;
                                self.part_out[1][i] += self.fxtmp[1][i] * 0.35;
                            }
                        } else {
                            let (a, b) = self.part_out.split_at_mut(1);
                            f.process(&mut a[0][..n], &mut b[0][..n], tempo, false);
                        }
                        f.p[3] = saved.0;
                        f.p[5] = saved.1;
                        f.p[0] = saved.2;
                    }
                    fx_index += 1;
                }
            }
            let _ = fx_index;
            let pt = &self.parts[p];
            let patch = pt.patch.as_ref().unwrap();
            let lvl = pt.level * pt.level * 2.0 * pt.ctl.volume * pt.ctl.expression * level_gain(patch.p[pp::LEVEL].max(0.05)) * level_gain(patch.p[pp::LEVEL_TRIM].max(0.05)) * 10f32.powf(pt.macros.level / 20.0) * 0.25;
            let pan = (pt.pan + bipolar(patch.p[pp::PAN].max(0.0001))).clamp(-1.0, 1.0);
            let (gl, gr) = pan_gains(pan);
            let mut sends = pt.sends;
            for k in 0..4 {
                sends[k] = (sends[k] + pt.macros.fx_send[k] * 0.5).clamp(0.0, 1.0);
            }
            let mut pk = 0.0f32;
            let use_pfx = pt.patch_fx_on;
            for i in 0..n {
                let a = self.part_out[0][i] * lvl * gl;
                let b = self.part_out[1][i] * lvl * gr;
                pk = pk.max(a.abs()).max(b.abs());
                out_l[i] += a;
                out_r[i] += b;
                for k in 0..4 {
                    if sends[k] > 0.0 && !(use_pfx && pt.patch_fx[k].is_some()) {
                        self.send_bus[k][0][i] += a * sends[k];
                        self.send_bus[k][1][i] += b * sends[k];
                    }
                }
            }
            self.parts[p].meter = pk.max(self.parts[p].meter * 0.85);
        }
        // global FX
        for k in 0..4 {
            let (a, b) = self.send_bus[k].split_at_mut(1);
            self.gfx[k].process(&mut a[0][..n], &mut b[0][..n], tempo, true);
            let rt = self.gfx_rt[k];
            let (gl, gr) = pan_gains(rt.pan);
            for i in 0..n {
                out_l[i] += self.send_bus[k][0][i] * rt.level * gl;
                out_r[i] += self.send_bus[k][1][i] * rt.level * gr;
            }
        }
        // master: Hype (presence + soft compression) and volume
        let hype = self.hype;
        let c = onepole_coef(3000.0, sr);
        let mv = self.master * self.master * 1.5;
        let mut pk = [0.0f32; 2];
        for i in 0..n {
            let mut a = out_l[i];
            let mut b = out_r[i];
            if hype > 0.0 {
                let ha = self.hype_lp[0].hp(a, c);
                let hb = self.hype_lp[1].hp(b, c);
                a += ha * hype * 0.8;
                b += hb * hype * 0.8;
                let e = a.abs().max(b.abs());
                self.hype_env = e.max(self.hype_env * 0.9995);
                let g = 1.0 / (1.0 + self.hype_env * hype * 0.8);
                a *= g * (1.0 + hype * 0.6);
                b *= g * (1.0 + hype * 0.6);
            }
            a = soft_clip((a * mv).clamp(-1.5, 1.5));
            b = soft_clip((b * mv).clamp(-1.5, 1.5));
            pk[0] = pk[0].max(a.abs());
            pk[1] = pk[1].max(b.abs());
            out_l[i] = a;
            out_r[i] = b;
        }
        self.peak = [pk[0].max(self.peak[0] * 0.9), pk[1].max(self.peak[1] * 0.9)];
    }

    /// Voices currently sounding in a part (polyphony meter).
    pub fn polyphony(&self, part: usize) -> usize {
        self.voices.iter().filter(|v| v.active && v.part == part).count()
    }
    pub fn total_polyphony(&self) -> usize {
        self.voices.iter().filter(|v| v.active).count()
    }
}

fn find_phrase(banks: &[(u32, Vec<Arc<crate::phrase::Phrase>>)], (bank, idx): (u32, u32)) -> Option<Arc<crate::phrase::Phrase>> {
    let b = banks.iter().find(|b| b.0 == bank).or(banks.first())?;
    b.1.get(idx as usize).or(b.1.first()).cloned()
}

fn fade(v: f32) -> f32 {
    if (v - 0.5).abs() < 0.02 {
        0.0
    } else if v < 0.5 {
        -time_s((0.5 - v) * 2.0)
    } else {
        time_s((v - 0.5) * 2.0)
    }
}

fn pb_range(v: f32) -> f32 {
    if v <= 0.0 {
        2.0
    } else {
        (v * 16.0).floor().clamp(0.0, 24.0)
    }
}

/// Patch Mono Mode: 0 = forced poly, 0.5 = element default, 1 = forced mono.
fn patch_mono(v: f32) -> bool {
    v > 0.75
}

fn lfo_beats(v: f32) -> f32 {
    const B: [f32; 10] = [16.0, 8.0, 4.0, 2.0, 1.0, 0.5, 0.333, 0.25, 0.1667, 0.125];
    B[enum_idx(v, B.len())]
}

/// Range membership with crossfade: Some(gain) when inside (gaps when lo > hi).
fn range_gain(x: i32, (lo, hi, xf): (i32, i32, i32)) -> Option<f32> {
    if lo <= hi {
        if x < lo - xf || x > hi + xf {
            return None;
        }
        let mut g = 1.0;
        if xf > 0 {
            if x < lo {
                g *= 1.0 - (lo - x) as f32 / (xf + 1) as f32;
            }
            if x > hi {
                g *= 1.0 - (x - hi) as f32 / (xf + 1) as f32;
            }
        }
        Some(g)
    } else if x > hi && x < lo {
        None
    } else {
        Some(1.0)
    }
}

struct PartCtx {
    ctl: Controllers,
    lfo: [f32; 2],
    knobs: [f32; 6],
    seq: [f32; 2],
    macros: Macros,
    pb_up: f32,
    pb_down: f32,
    tempo: f32,
    elems: usize,
}

#[derive(Clone, Copy, Default)]
struct ModOut {
    pitch: f32,
    wave: f32,
    wave2: f32,
    cutoff: f32,
    reso: f32,
    amp: f32,
    pan: f32,
    lfo3_rate: f32,
    menv_rate: f32,
}

/// Mod matrix row source (Src list: OFF Constant Random Velocity Key Pitch ModWheel …).
fn source(i: usize, v: &Voice, c: &PartCtx) -> f32 {
    match i {
        1 => 1.0,
        2 => v.rnd,
        3 => v.vel,
        4 => (v.key - 60.0) / 64.0,
        5 => (v.pitch - 60.0) / 64.0,
        6 => c.ctl.mw,
        7 => c.ctl.at,
        8 => c.ctl.bend,
        9 => c.ctl.breath,
        10 => c.ctl.wheel3,
        11 => c.ctl.foot,
        12 => c.ctl.sustain as i32 as f32,
        13 => c.ctl.soft as i32 as f32,
        14..=19 => c.knobs[i - 14],
        20 => c.seq[0],
        21 => c.seq[1],
        22 => c.lfo[0],
        23 => c.lfo[1],
        24 => v.lfo3.value,
        25 => v.menv.value,
        26 => v.fenv.level,
        27 => v.aenv.level,
        28..=43 => v.row[(i - 28).min(15)],
        _ => 0.0,
    }
}

fn eval_matrix(v: &mut Voice, e: &Element, c: &PartCtx, dt: f32) -> ModOut {
    let mut m = ModOut::default();
    for r in 0..ep::ROWS {
        let o = ep::MATRIX + 5 * r;
        let si = enum_idx(e.p[o], 72);
        let di = (e.p[o + 3] * 128.0) as usize;
        if si == 0 && v.row[r] == 0.0 {
            continue;
        }
        let mut x = source(si, v, c);
        let mi = enum_idx(e.p[o + 1], 72);
        let prm = bipolar(e.p[o + 2]);
        if mi > 0 && mi < 28 {
            x *= source(mi, v, c);
        } else if mi >= 28 {
            let st = &mut v.row_state[r];
            x = match mi - 28 {
                0 => x + prm,
                1 => x * prm * 4.0,
                2 => (x * (1.0 + prm.abs() * 8.0)).clamp(-1.0, 1.0),
                3 => x.min(prm),
                4 => x.max(prm),
                5 => {
                    if x > prm {
                        2.0 * prm - x
                    } else {
                        x
                    }
                }
                6 => {
                    if x < prm {
                        2.0 * prm - x
                    } else {
                        x
                    }
                }
                7 => x.signum() * x.abs().powf(2f32.powf(prm * 2.0)),
                8 => (x * std::f32::consts::FRAC_PI_2 * (1.0 + prm.abs() * 3.0)).sin(),
                9 => x.signum() * (x.abs() * 4.0).exp_m1() / 4f32.exp_m1(),
                10 => x.signum() * (1.0 + x.abs() * 9.0).log10(),
                11 => (x > prm) as i32 as f32,
                12 | 13 => {
                    let steps = 2.0 + prm.abs() * 14.0;
                    (x * steps).round() / steps
                }
                18 | 19 | 20 | 21 | 22 | 23 => {
                    // one-shot ramps from note on: state = elapsed time
                    *st += dt;
                    let t = (0.01 + prm.abs() * 3.0).max(0.01);
                    let u = (*st / t).clamp(0.0, 1.0);
                    let shape = match mi - 28 {
                        19 => ((*st - t) / t).clamp(0.0, 1.0),
                        20 => u * u,
                        21 => u.sqrt(),
                        22 => 1.0 - (2.0 * u - 1.0).abs(),
                        23 => (*st < t) as i32 as f32,
                        _ => u,
                    };
                    x * if prm < 0.0 { 1.0 - shape } else { shape }
                }
                24 | 25 => {
                    let k = (dt * (1.0 + (1.0 - prm.abs()) * 30.0)).min(1.0);
                    *st += (x - *st) * k;
                    *st
                }
                26 | 27 => {
                    *st = x.abs().max(*st * (1.0 - dt * 2.0));
                    *st
                }
                _ => x,
            };
        }
        let x = if x.is_finite() { x.clamp(-2.0, 2.0) } else { 0.0 };
        v.row[r] = x;
        let depth = bipolar(e.p[o + 4]);
        let a = x * depth;
        match di {
            0 => {}
            1 | 9..=16 => m.pitch += a * 12.0,
            2 | 27..=40 => m.wave += a,
            3 | 73..=127 => m.wave2 += a,
            4 | 41..=56 => m.cutoff += a * 60.0,
            5 => m.reso += a,
            6 | 57..=72 => m.amp += a,
            7 => m.pan += a,
            8 | 17..=26 => m.lfo3_rate += a,
            _ => m.menv_rate += a,
        }
    }
    m
}

/// Control-rate update of one voice.
fn control(v: &mut Voice, e: &Element, c: &PartCtx, sr: f32, wt: &wavetable::Bank) {
    let dt = CTL as f32 / sr;
    let p = &e.p;
    let lw = LfoWave::ALL[enum_idx(p[ep::LFO3_WAVE], 9)];
    let lsync = enum_idx(p[ep::LFO3_SYNC], 5);
    let m0 = eval_matrix(v, e, c, dt);
    let lhz = if lsync >= 3 { c.tempo / 60.0 / lfo_beats(p[ep::LFO3_RATE]) } else { lfo_hz(p[ep::LFO3_RATE]) } * 2f32.powf(m0.lfo3_rate * 3.0 + c.macros.lfo_rate * 3.0);
    let mut rng = v.rng;
    v.lfo3.step(lw, lhz, dt, &mut rng);
    v.rng = rng;
    v.menv.step(dt, 2f32.powf(m0.menv_rate * 3.0));
    let a = v.aenv.step(dt);
    let f = v.fenv.step(dt);
    // glide
    if v.glide_left > 0.0 {
        v.glide_left = (v.glide_left - dt).max(0.0);
    }
    let target = v.key - v.note as f32 + v.note as f32; // key already includes part transpose
    let _ = target;
    let bend = if c.ctl.bend >= 0.0 { c.ctl.bend * c.pb_up } else { c.ctl.bend * c.pb_down };
    let bend = if p[ep::PATCH_PB] >= 0.5 || p[ep::PATCH_PB] == 0.0 { bend } else { 0.0 };
    let keytrack = p[ep::PITCH_KEY];
    let kt = if keytrack == 0.0 { 1.0 } else { keytrack / 0.75 };
    let mut base = v.pitch_base(e);
    base = 60.0 + (base - 60.0) * kt;
    if v.glide_left > 0.0 && v.glide_time > 0.0 {
        let u = v.glide_left / v.glide_time;
        base = base + (v.glide_from - base) * u;
    }
    let vib = c.macros.vibrato.max(0.0) * c.lfo[0] * 0.5;
    let rndp = bipolar(p[ep::RND_PITCH].max(0.5)) * v.rnd * 0.5;
    let pitch = (base + bend + m0.pitch.clamp(-48.0, 48.0) + vib + rndp).clamp(-36.0, 150.0);
    v.pitch = pitch;
    let hz = pitch_hz(pitch);
    // filter
    let fmode = FilterMode::ALL[enum_idx(p[ep::FILTER], 9)];
    v.c_mode = fmode;
    let kc = (p[ep::KEY_CUTOFF] * 2.0) * (v.key - 60.0) + (v.pitch - v.key).min(24.0);
    let vc = bipolar(p[ep::VEL_CUTOFF]) * (v.vel - 0.5) * 60.0;
    let env = bipolar(p[ep::FENV_DEPTH]) * 96.0 * f * (1.0 + bipolar(p[ep::FENV_VEL]) * (v.vel - 0.5) * 2.0);
    let rnd = p[ep::RND_CUTOFF] * v.rnd * 24.0;
    let cut_p = cutoff_pitch(p[ep::CUTOFF]) + kc + vc + env + rnd + m0.cutoff + c.macros.cutoff;
    v.c_cut = pitch_hz(cut_p).clamp(12.0, sr * 0.48);
    v.c_reso = (p[ep::RESO] + bipolar(p[ep::VEL_RESO]) * v.vel * 0.5 + m0.reso + c.macros.reso * 0.5).clamp(0.0, 1.0);
    v.c_drive = 1.0 + (c.macros.drive.max(0.0) * 8.0) + if v.kind == ElemType::Analog { p[ep::A_DIST] * 8.0 } else { 0.0 };
    // amplitude
    let rxv = enum_idx(p[ep::RX_VEL], 2) == 0;
    let vs = if rxv { 1.0 - p[ep::AENV_VEL] + p[ep::AENV_VEL] * v.vel } else { 1.0 };
    let bal = if c.elems > 1 { 1.0 + c.macros.balance * if v.elem == 0 { -1.0 } else { 1.0 } } else { 1.0 };
    let amp = a * level_gain(p[ep::LEVEL]) * vs * v.xf * (1.0 + m0.amp).max(0.0) * bal.clamp(0.0, 2.0) * 0.35;
    let mut amp = amp;
    if v.kill > 0.0 {
        v.kill = (v.kill - dt).max(0.0);
        amp *= 0.0;
        if v.kill == 0.0 {
            v.active = false;
        }
    }
    if v.delay > 0.0 {
        v.delay -= dt;
        amp = 0.0;
    }
    v.c_amp_prev = v.c_amp;
    v.c_amp = amp;
    // pan
    let mut pan = bipolar(p[ep::PAN].max(0.0001)) + bipolar(p[ep::PAN_KEY].max(0.5)) * (v.key - 60.0) / 48.0 + p[ep::RND_PAN] * v.rnd + m0.pan;
    if p[ep::ALT_PAN] > 0.0 && v.note % 2 == 1 {
        pan = -pan - p[ep::ALT_PAN];
    }
    pan = (pan * (1.0 + c.macros.width)).clamp(-1.0, 1.0);
    v.c_pan = pan_gains(pan);
    // wave / oscillator params
    let kw = bipolar(p[ep::KEY_WAVE].max(0.5)) * (v.key - 60.0) / 64.0;
    let vw = bipolar(p[ep::VEL_WAVE].max(0.5)) * v.vel;
    let rw = p[ep::RND_WAVE] * v.rnd;
    v.c_wave = (p[ep::WAVE] + kw + vw + rw + m0.wave + c.macros.wave * 0.5).clamp(0.0, 1.0);
    v.c_wave2 = (p[ep::OSC2_WAVE_MOD] + m0.wave2 + c.macros.wave * 0.3).clamp(0.0, 1.0);
    let sdt = hz / sr;
    match v.kind {
        ElemType::Analog => {
            let s1 = (bipolar(p[ep::A_SEMI]) * 24.0).round() + bipolar(p[ep::A_CENT]);
            let s2 = (bipolar(p[ep::A_OSC2_SEMI]) * 24.0).round() + bipolar(p[ep::A_OSC2_CENT]) + c.macros.detune * 0.3;
            v.c_dt = [sdt * 2f32.powf(s1 / 12.0), sdt * 2f32.powf(s2 / 12.0), sdt, sdt * 0.5];
            if v.c_table.is_none() {
                v.c_table = wt.try_get(enum_idx(p[ep::A_WAVETABLE], 64));
            }
        }
        ElemType::Wavetable => {
            let s = (bipolar(p[ep::W_SEMI]) * 24.0).round() + bipolar(p[ep::W_CENT]);
            v.c_dt = [sdt, sdt * 2f32.powf(s / 12.0), 0.0, 0.0];
            if v.c_table.is_none() {
                v.c_table = wt.try_get(enum_idx(p[ep::W_INDEX], 64));
            }
        }
        ElemType::Fm => {
            const R: [f32; 18] = [0.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 1.414];
            let r1 = R[enum_idx(p[ep::F_M1_COARSE], R.len())] * (1.0 + bipolar(p[ep::F_M1_FINE].max(0.5)) * 0.06);
            let r2 = R[enum_idx(p[ep::F_M2_COARSE], R.len())] * (1.0 + bipolar(p[ep::F_M2_FINE].max(0.5)) * 0.06);
            let kt2 = p[ep::F_M2_KEY];
            let car = if p[ep::F_LF] > 0.5 { 0.5 / sr } else { sdt };
            let m2dt = if kt2 > 0.0 { sdt * r2 * kt2 + (1.0 - kt2) * 440.0 / sr * r2 } else { sdt * r2 };
            v.c_dt = [car, sdt * r1, m2dt, 0.0];
            // modulator 2 envelope (Level: +attack / −decay, Rate)
            let lvl = bipolar(p[ep::F_ENV_LEVEL].max(0.5));
            let rate = 0.2 + p[ep::F_ENV_RATE] * 20.0;
            v.perc = if lvl >= 0.0 { (v.perc - dt * rate).max(0.0) } else { (v.perc - dt * rate).max(0.0) };
            let env2 = if lvl >= 0.0 { 1.0 - v.perc * lvl } else { 1.0 + lvl * (1.0 - v.perc) };
            let master = 0.5 + v.c_wave * 2.0;
            let vs1 = 1.0 + bipolar(p[ep::F_M1_VEL].max(0.5)) * (v.vel - 0.5) * 2.0;
            let vs2 = 1.0 + bipolar(p[ep::F_M2_VEL].max(0.5)) * (v.vel - 0.5) * 2.0;
            v.c_fm = [p[ep::F_M1_DEPTH] * 6.0 * master * vs1.max(0.0), p[ep::F_M2_DEPTH] * 6.0 * master * vs2.max(0.0) * env2.max(0.0)];
        }
        ElemType::Drawbar => {
            v.c_dt = [sdt, 0.0, 0.0, 0.0];
            let decay = 2.0 + (1.0 - p[ep::D_PERC_DECAY]) * 12.0;
            v.perc *= (-dt * decay).exp();
        }
        ElemType::Sample => {
            if let Some(z) = &v.zone {
                let root = if v.drum > 0 { v.drum as f32 } else { z.root as f32 };
                let pitch_rel = if v.drum > 0 { pitch - v.key } else { pitch - root } + z.tune / 100.0;
                v.sstep = (z.rate as f64 / sr as f64) * 2f64.powf(pitch_rel as f64 / 12.0);
            }
            v.c_dt = [sdt, 0.0, 0.0, 0.0];
            if matches!(v.fb, Some(Fallback::Kick | Fallback::Tom | Fallback::Snare)) {
                v.perc *= (-dt * if v.fb == Some(Fallback::Kick) { 18.0 } else { 10.0 }).exp();
            } else {
                v.perc *= (-dt * 3.0).exp();
            }
        }
        ElemType::Fx => {}
    }
    if !v.aenv.active() && v.released {
        v.active = false;
    }
    if !v.aenv.active() && !v.released && v.aenv.s == 0.0 {
        v.active = false;
    }
}

impl Voice {
    /// Pitch from key and element transpose (no modulation).
    fn pitch_base(&self, e: &Element) -> f32 {
        self.key + e.transpose()
    }
}

fn render_voice(v: &mut Voice, e: &Element, c: &PartCtx, l: &mut [f32], r: &mut [f32], sr: f32, wt: &wavetable::Bank) {
    let n = l.len();
    let p = &e.p;
    let mut i = 0;
    while i < n && v.active {
        control(v, e, c, sr, wt);
        let m = CTL.min(n - i);
        let (a0, a1) = (v.c_amp_prev, v.c_amp);
        for j in 0..m {
            let g = a0 + (a1 - a0) * (j as f32 / m as f32);
            let (sl, sr_) = osc_sample(v, p, sr);
            let yl = v.filt[0].tick(v.c_mode, sl, v.c_cut, v.c_reso, sr, v.c_drive);
            let yr = if v.zone.is_some() { v.filt[1].tick(v.c_mode, sr_, v.c_cut, v.c_reso, sr, v.c_drive) } else { yl };
            l[i + j] += yl * g * v.c_pan.0;
            r[i + j] += yr * g * v.c_pan.1;
        }
        i += m;
    }
}

const DRAW_RATIO: [f32; 9] = [0.5, 1.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0];

/// One raw oscillator sample (stereo for samples, mono duplicated otherwise).
#[inline]
fn osc_sample(v: &mut Voice, p: &[f32; ELEM_PARAMS], sr: f32) -> (f32, f32) {
    use std::f32::consts::TAU;
    match v.kind {
        ElemType::Analog => {
            let modes = |x: f32| enum_idx(x, 6);
            let mut rng = v.rng;
            let osc = |o: &mut Osc, mode: usize, knob: f32, dt: f32, table: &Option<Arc<Table>>, rng: &mut Rng| -> f32 {
                match mode {
                    0 | 1 => {
                        // hard sync: slave at ratio 1..8 set by the wave knob
                        let ratio = 1.0 + knob * 7.0;
                        let prev = o.phase;
                        let master = (prev + dt).fract();
                        let slave = (master * ratio).fract();
                        o.phase = master;
                        let sdt = dt * ratio;
                        if mode == 0 {
                            2.0 * slave - 1.0 - blep(slave, sdt.min(0.45))
                        } else {
                            let mut s = if slave < 0.5 { 1.0 } else { -1.0 };
                            s += blep(slave, sdt.min(0.45)) - blep((slave + 0.5).fract(), sdt.min(0.45));
                            s
                        }
                    }
                    2 => o.tick(Wave::Pulse, dt, 0.5 + (knob - 0.5) * 0.96, rng),
                    3 => o.tick(Wave::Saw, dt * 2f32.powf((knob - 0.5) * 2.0), 0.5, rng),
                    4 => o.tick(Wave::Square, dt * 2f32.powf((knob - 0.5) * 2.0), 0.5, rng),
                    _ => {
                        let ph = o.phase;
                        o.phase = (o.phase + dt).fract();
                        match table {
                            Some(t) => t.read(Table::level(dt), knob * 63.0, ph),
                            None => (ph * TAU).sin(),
                        }
                    }
                }
            };
            let t = v.c_table.clone();
            let o1 = osc(&mut v.osc[0], modes(p[ep::A_OSC1_WAVE]), v.c_wave, v.c_dt[0], &t, &mut rng);
            let o2 = osc(&mut v.osc[1], modes(p[ep::A_OSC2_WAVE]), v.c_wave2, v.c_dt[1], &t, &mut rng);
            let w3 = match enum_idx(p[ep::A_OSC3_WAVE], 3) {
                0 => Wave::Saw,
                1 => Wave::Square,
                _ => Wave::Triangle,
            };
            let o3 = v.osc[2].tick(w3, v.c_dt[2], 0.5, &mut rng);
            let sub = v.osc[3].tick(if w3 == Wave::Triangle { Wave::Triangle } else { Wave::Square }, v.c_dt[3], 0.5, &mut rng);
            let noise = rng.bi();
            v.rng = rng;
            let mix1 = if p[ep::A_OSC1_MIX] == 0.0 && p[ep::A_OSC2_MIX] == 0.0 && p[ep::A_OSC3_MIX] == 0.0 { 1.0 } else { p[ep::A_OSC1_MIX] };
            let mut s = o1 * mix1 + o2 * p[ep::A_OSC2_MIX] + o3 * p[ep::A_OSC3_MIX] + sub * p[ep::A_SUB] + noise * p[ep::A_NOISE] * 0.7;
            if p[ep::A_HPF] > 0.0 {
                s = v.hp[0].hp(s, onepole_coef(20.0 * 2f32.powf(p[ep::A_HPF] * 9.0), sr));
            }
            (s, s)
        }
        ElemType::Wavetable => {
            let ph = v.ph[0];
            v.ph[0] = (ph + v.c_dt[0]).fract();
            let s1 = match &v.c_table {
                Some(t) => t.read(Table::level(v.c_dt[0]), v.c_wave * 63.0, ph),
                None => (ph * TAU).sin(),
            };
            let mix2 = p[ep::W_OSC2_MIX];
            let mut s = s1 * (1.0 - mix2 * 0.5);
            if mix2 > 0.0 {
                let w = match enum_idx(p[ep::W_OSC2], 4) {
                    0 => Wave::Saw,
                    1 => Wave::Square,
                    2 => Wave::Triangle,
                    _ => Wave::Sine,
                };
                let mut rng = v.rng;
                s += v.osc[1].tick(w, v.c_dt[1], 0.5, &mut rng) * mix2;
                v.rng = rng;
            }
            (s, s)
        }
        ElemType::Fm => {
            let m1 = (v.ph[1] * TAU).sin();
            let m2 = (v.ph[2] * TAU).sin();
            let s = (v.ph[0] * TAU + m1 * v.c_fm[0] + m2 * v.c_fm[1]).sin() + m1 * p[ep::F_THRU] * 0.5;
            v.ph[0] = (v.ph[0] + v.c_dt[0]).fract();
            v.ph[1] = (v.ph[1] + v.c_dt[1]).fract();
            v.ph[2] = (v.ph[2] + v.c_dt[2]).fract();
            (s, s)
        }
        ElemType::Drawbar => {
            let mut s = 0.0;
            for k in 0..9 {
                let lvl = p[ep::D_BARS + k];
                if lvl > 0.0 {
                    s += (v.ph[k] * TAU).sin() * lvl;
                }
                v.ph[k] = (v.ph[k] + v.c_dt[0] * DRAW_RATIO[k]).fract();
            }
            if p[ep::D_PERC] >= 0.5 && v.perc > 0.001 {
                let h = if p[ep::D_PERC_HARM] >= 0.5 { 3.0 } else { 2.0 };
                s += (v.ph[9] * TAU).sin() * v.perc * (0.4 + p[ep::D_PERC_LEVEL]);
                v.ph[9] = (v.ph[9] + v.c_dt[0] * h).fract();
            }
            if v.click > 0.001 {
                s += v.rng.bi() * v.click * 0.3;
                v.click *= 0.995;
            }
            (s * 0.3, s * 0.3)
        }
        ElemType::Sample => {
            if let Some(z) = &v.zone {
                let i = v.spos as usize;
                if i + 1 >= z.end || i + 1 >= z.data.len() {
                    v.aenv.stage = Stage::Off;
                    v.released = true;
                    return (0.0, 0.0);
                }
                let f = (v.spos - i as f64) as f32;
                let a = z.data[i];
                let b = z.data[i + 1];
                let s = (a + (b - a) * f) * z.gain;
                v.spos += v.sstep;
                if z.looped && v.spos >= z.loop_end as f64 {
                    v.spos -= (z.loop_end - z.loop_start).max(1) as f64;
                }
                let (gl, gr) = pan_gains(z.pan);
                (s * gl, s * gr)
            } else {
                let s = fallback_sample(v, sr);
                (s, s)
            }
        }
        ElemType::Fx => (0.0, 0.0),
    }
}

fn fallback_sample(v: &mut Voice, sr: f32) -> f32 {
    use std::f32::consts::TAU;
    let dt = v.c_dt[0];
    let fb = v.fb.unwrap_or(Fallback::Pad);
    let mut rng = v.rng;
    let s = match fb {
        Fallback::Pluck | Fallback::Bass => {
            if v.ks_len > 1 {
                let i = v.ks_pos;
                let j = (i + 1) % v.ks_len;
                let damp = if fb == Fallback::Bass { 0.497 } else { 0.4985 };
                let y = (v.ks[i] + v.ks[j]) * damp;
                v.ks[i] = y;
                v.ks_pos = j;
                y * 1.5 + if fb == Fallback::Bass { v.osc[1].tick(Wave::Sine, dt, 0.5, &mut rng) * 0.4 } else { 0.0 }
            } else {
                0.0
            }
        }
        Fallback::Piano | Fallback::EPiano | Fallback::Mallet => {
            let (ratio, idx) = match fb {
                Fallback::Piano => (1.0, 1.2),
                Fallback::EPiano => (1.0, 0.8),
                _ => (3.5, 2.0),
            };
            let m = (v.ph[1] * TAU).sin() * idx * v.perc;
            let s = (v.ph[0] * TAU + m).sin() + if fb == Fallback::Piano { (v.ph[2] * TAU).sin() * 0.3 * v.perc } else { 0.0 };
            v.ph[0] = (v.ph[0] + dt).fract();
            v.ph[1] = (v.ph[1] + dt * ratio).fract();
            v.ph[2] = (v.ph[2] + dt * 2.003).fract();
            s * 0.7
        }
        Fallback::Organ => {
            let mut s = 0.0;
            for (k, r) in [1.0f32, 2.0, 3.0, 4.0].iter().enumerate() {
                s += (v.ph[k] * TAU).sin() / (k as f32 + 1.0);
                v.ph[k] = (v.ph[k] + dt * r).fract();
            }
            s * 0.5
        }
        Fallback::Strings | Fallback::Brass | Fallback::Pad => {
            let det = if fb == Fallback::Brass { 0.002 } else { 0.006 };
            let a = v.osc[0].tick(Wave::Saw, dt * (1.0 + det), 0.5, &mut rng);
            let b = v.osc[1].tick(Wave::Saw, dt * (1.0 - det), 0.5, &mut rng);
            let c = if fb == Fallback::Pad { v.osc[2].tick(Wave::Saw, dt * 2.0, 0.5, &mut rng) * 0.3 } else { 0.0 };
            (a + b + c) * 0.4
        }
        Fallback::Wind | Fallback::Voice => {
            let t = v.osc[0].tick(Wave::Triangle, dt, 0.5, &mut rng);
            let n = rng.bi() * 0.05;
            let s = v.osc[1].tick(Wave::Sine, dt * 2.0, 0.5, &mut rng) * 0.2;
            (t + n + s) * 0.7
        }
        Fallback::Noise => rng.bi() * 0.5,
        Fallback::Kick => {
            let f = 45.0 + 160.0 * v.perc * v.perc;
            v.ph[0] = (v.ph[0] + f / sr).fract();
            (v.ph[0] * TAU).sin() * (0.3 + v.perc)
        }
        Fallback::Tom => {
            let f = pitch_hz(v.key - 12.0) * (1.0 + v.perc);
            v.ph[0] = (v.ph[0] + f / sr).fract();
            (v.ph[0] * TAU).sin() * v.perc.sqrt()
        }
        Fallback::Snare => {
            v.ph[0] = (v.ph[0] + 185.0 / sr).fract();
            ((v.ph[0] * TAU).sin() * 0.5 * v.perc + rng.bi() * 0.6) * v.perc.sqrt()
        }
        Fallback::HiHat | Fallback::Cymbal => {
            let x = rng.bi();
            let y = v.hp[1].hp(x, onepole_coef(if fb == Fallback::HiHat { 7000.0 } else { 4000.0 }, sr));
            let mut metal = 0.0;
            for (k, r) in [1.0f32, 1.342, 1.2312, 1.6532, 1.9523, 2.1523].iter().enumerate() {
                v.ph[k] = (v.ph[k] + 400.0 * r / sr).fract();
                metal += if v.ph[k] < 0.5 { 1.0 } else { -1.0 };
            }
            y * 0.6 + v.hp[0].hp(metal * 0.05, onepole_coef(6000.0, sr))
        }
        Fallback::Perc => {
            let f = pitch_hz(v.key + 12.0);
            v.ph[0] = (v.ph[0] + f / sr).fract();
            ((v.ph[0] * TAU).sin() * 0.7 + rng.bi() * 0.2) * v.perc
        }
    };
    v.rng = rng;
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn analog_patch() -> Patch {
        let mut p = Patch::new("Saw");
        let mut e = Element::new("Saw Osc", ElemType::Analog);
        e.p[ep::A_OSC1_WAVE] = enum_val(3, 6); // Saw Pitch
        e.p[ep::WAVE] = 0.5;
        e.p[ep::A_OSC1_MIX] = 1.0;
        e.p[ep::FILTER] = enum_val(1, 9);
        e.p[ep::CUTOFF] = 0.7;
        p.elements.push(e);
        p.elements.push(Element::new("Chorus", ElemType::Fx));
        let ci = 1;
        p.elements[ci].p[ep::FX_TYPE] = 20.0 / 128.0;
        p
    }

    #[test]
    fn note_makes_sound_and_stops() {
        let mut s = Synth::new(48000.0);
        s.set_patch(0, Some(Arc::new(analog_patch())));
        s.midi(&[0x90, 60, 100]);
        let mut l = vec![0.0; 4800];
        let mut r = vec![0.0; 4800];
        s.render(&mut l, &mut r);
        let e: f32 = l.iter().map(|x| x * x).sum::<f32>() / l.len() as f32;
        assert!(e > 1e-5, "energy {e}");
        assert!(l.iter().all(|x| x.is_finite()));
        s.midi(&[0x80, 60, 0]);
        for _ in 0..40 {
            s.render(&mut l, &mut r);
        }
        assert_eq!(s.polyphony(0), 0);
    }

    #[test]
    fn sample_map_overrides() {
        let mut s = Synth::new(44100.0);
        let sine: Vec<f32> = (0..44100).map(|i| (i as f32 * 0.0627).sin() * 0.5).collect();
        let zone = |root: u8| Zone {
            key: (0, 127),
            vel: (0, 127),
            data: Arc::new(sine.clone()),
            start: 0,
            end: sine.len(),
            loop_start: 0,
            loop_end: 0,
            looped: false,
            root,
            tune: 0.0,
            rate: 44100,
            gain: 1.0,
            pan: 0.0,
        };
        let pr = sf2::Preset { name: "bass".into(), bank: 0, program: 0, zones: vec![zone(40), zone(52)] };
        s.sample_map.push((33, SampleSource::Wavs(Arc::new(pr.clone()))));
        assert_eq!(s.resolve_sample(33, 0, 45, 100).map(|z| z.0.root), Some(40));
        assert!(s.resolve_sample(34, 0, 45, 100).is_none());
        s.sample_map.push((0, SampleSource::Wavs(Arc::new(pr))));
        // drum bank: Wave index picks the file, played unpitched
        let (z, drum) = s.resolve_sample(0, 1, 36, 100).unwrap();
        assert_eq!((z.root, drum), (52, 52));
        let mut p = Patch::new("b");
        let mut e = Element::new("Bass", ElemType::Sample);
        e.p[ep::S_BANK] = 33.5 / 128.0;
        p.elements.push(e);
        s.set_patch(0, Some(Arc::new(p)));
        s.midi(&[0x90, 40, 100]);
        let mut l = vec![0.0; 2048];
        let mut r = vec![0.0; 2048];
        s.render(&mut l, &mut r);
        assert!(l.iter().map(|x| x * x).sum::<f32>() > 1e-3);
    }

    #[test]
    fn link_and_ranges() {
        let mut s = Synth::new(48000.0);
        s.parts[1].link = true;
        assert_eq!(s.parts_for_channel(0), vec![0, 1]);
        assert_eq!(range_gain(50, (60, 72, 0)), None);
        assert_eq!(range_gain(65, (72, 60, 0)), None);
        assert!(range_gain(58, (60, 72, 6)).unwrap() < 1.0);
    }

    #[test]
    fn every_element_type_renders() {
        for kind in [ElemType::Drawbar, ElemType::Sample, ElemType::Wavetable, ElemType::Fm, ElemType::Analog] {
            let mut s = Synth::new(44100.0);
            let mut p = Patch::new("t");
            let mut e = Element::new("e", kind);
            e.p[ep::D_BARS + 2] = 1.0;
            e.p[ep::F_M1_DEPTH] = 0.3;
            e.p[ep::S_BANK] = 33.5 / 128.0;
            p.elements.push(e);
            for t in Synth::tables_for(&p) {
                s.wt.get(t);
            }
            s.set_patch(0, Some(Arc::new(p)));
            s.midi(&[0x90, 48, 110]);
            let mut l = vec![0.0; 2048];
            let mut r = vec![0.0; 2048];
            s.render(&mut l, &mut r);
            let e: f32 = l.iter().map(|x| x * x).sum();
            assert!(e > 1e-6 && e.is_finite(), "{kind:?} {e}");
        }
    }
}
