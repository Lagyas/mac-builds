//! Built-in patches (own sounds) so the instrument works without the
//! Hypersonic 2 content folder.

use crate::patch::*;

fn analog(name: &str, osc1: usize, wave: f32, cutoff: f32, reso: f32, env: f32, a: [f32; 5], f: [f32; 5]) -> Element {
    let mut e = Element::new(name, ElemType::Analog);
    e.p[ep::A_OSC1_WAVE] = enum_val(osc1, 6);
    e.p[ep::WAVE] = wave;
    e.p[ep::A_OSC1_MIX] = 1.0;
    e.p[ep::FILTER] = enum_val(1, 9);
    e.p[ep::CUTOFF] = cutoff;
    e.p[ep::RESO] = reso;
    e.p[ep::FENV_DEPTH] = env;
    e.p[ep::KEY_CUTOFF] = 0.25;
    for k in 0..5 {
        e.p[ep::AENV + k] = a[k];
        e.p[ep::FENV + k] = f[k];
    }
    e
}

fn fx(name: &str, id: u32, p: &[(usize, f32)]) -> Element {
    let mut e = Element::new(name, ElemType::Fx);
    e.p[ep::FX_TYPE] = id as f32 / 128.0;
    let d = crate::fx::defaults(id);
    e.p[ep::FX_P..ep::FX_P + 12].copy_from_slice(&d);
    for &(i, v) in p {
        e.p[ep::FX_P + i] = v;
    }
    e
}

fn row(e: &mut Element, r: usize, src: usize, m: usize, dest: f32, depth: f32) {
    let o = ep::MATRIX + 5 * r;
    e.p[o] = enum_val(src, 72);
    e.p[o + 1] = if m == 0 { 0.0 } else { enum_val(m, 72) };
    e.p[o + 2] = 0.5;
    e.p[o + 3] = dest;
    e.p[o + 4] = depth;
}

/// Vibrato on the mod wheel (MW × LFO2 → pitch).
fn mw_vibrato(e: &mut Element) {
    row(e, 0, 6, 23, 12.5 / 128.0, 0.53);
}

fn patch(name: &str, attrs: &str, comment: &str, knobs: [&str; 6], els: Vec<Element>) -> Patch {
    let mut p = Patch::new(name);
    p.attributes = attrs.into();
    p.comment = comment.into();
    p.author = "hs2 factory".into();
    for (k, n) in knobs.iter().enumerate() {
        p.knob_names[k] = (*n).into();
        p.p[pp::KNOB[k] + 1] = 0.5;
    }
    p.p[pp::LFO2] = enum_val(1, 9);
    p.p[pp::LFO2 + 2] = 0.62;
    p.elements = els;
    p
}

pub fn banks() -> Vec<(&'static str, Vec<Patch>)> {
    let sweep_pad = {
        let mut a = analog("Saw Layer", 3, 0.5, 0.45, 0.2, 0.62, [0.45, 0.6, 0.85, 0.5, 0.55], [0.5, 0.7, 0.6, 0.5, 0.55]);
        a.p[ep::A_OSC2_WAVE] = enum_val(3, 6);
        a.p[ep::A_OSC2_MIX] = 0.8;
        a.p[ep::A_OSC2_CENT] = 0.56;
        mw_vibrato(&mut a);
        vec![a, fx("Chorus", 20, &[(3, 0.4)]), fx("Long Reverb", 1, &[(3, 0.3)])]
    };
    let bass = {
        let mut a = analog("Bass", 3, 0.5, 0.18, 0.35, 0.78, [0.0, 0.5, 0.75, 0.5, 0.1], [0.0, 0.32, 0.0, 0.5, 0.2]);
        a.p[ep::A_SUB] = 0.6;
        a.p[ep::OCTAVE] = enum_val(3, 9);
        vec![a]
    };
    let lead = {
        let mut a = analog("Lead", 2, 0.35, 0.5, 0.3, 0.6, [0.02, 0.5, 0.9, 0.5, 0.2], [0.05, 0.45, 0.4, 0.5, 0.3]);
        a.p[ep::A_OSC2_WAVE] = enum_val(0, 6);
        a.p[ep::OSC2_WAVE_MOD] = 0.3;
        a.p[ep::A_OSC2_MIX] = 0.6;
        mw_vibrato(&mut a);
        vec![a, fx("Delay", 11, &[(3, 0.25)])]
    };
    let ep = {
        let mut f = Element::new("FM Tine", ElemType::Fm);
        f.p[ep::F_M1_COARSE] = enum_val(1, 18);
        f.p[ep::F_M1_DEPTH] = 0.25;
        f.p[ep::F_M2_COARSE] = enum_val(13, 18);
        f.p[ep::F_M2_DEPTH] = 0.12;
        f.p[ep::F_ENV_LEVEL] = 0.2;
        f.p[ep::F_ENV_RATE] = 0.4;
        f.p[ep::F_M1_VEL] = 0.8;
        f.p[ep::AENV + 1] = 0.75;
        f.p[ep::AENV + 2] = 0.0;
        f.p[ep::AENV + 4] = 0.3;
        f.p[ep::WAVE] = 0.3;
        vec![f, fx("Phaser", 40, &[(3, 0.4), (0, 0.3)]), fx("Short Reverb", 3, &[(3, 0.2)])]
    };
    let organ = {
        let mut d = Element::new("Drawbars", ElemType::Drawbar);
        for (k, v) in [1.0, 0.75, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0].iter().enumerate() {
            d.p[ep::D_BARS + k] = *v;
        }
        d.p[ep::D_PERC] = 1.0;
        d.p[ep::D_PERC_LEVEL] = 0.5;
        d.p[ep::D_PERC_HARM] = 1.0;
        d.p[ep::D_CLICK] = 0.3;
        d.p[ep::AENV + 4] = 0.08;
        vec![d, fx("Rotary", 55, &[(3, 1.0), (6, 0.3)])]
    };
    let bells = {
        let mut w = Element::new("WT Bell", ElemType::Wavetable);
        w.p[ep::W_INDEX] = enum_val(36, 64);
        w.p[ep::WAVE] = 0.2;
        row(&mut w, 0, 26, 0, 30.5 / 128.0, 0.8); // filt env → wave
        w.p[ep::FENV + 1] = 0.6;
        w.p[ep::FENV + 2] = 0.0;
        w.p[ep::AENV + 1] = 0.8;
        w.p[ep::AENV + 2] = 0.0;
        w.p[ep::AENV + 4] = 0.55;
        vec![w, fx("Long Reverb", 1, &[(3, 0.4)])]
    };
    let vowels = {
        let mut w = Element::new("Vowel Pad", ElemType::Wavetable);
        w.p[ep::W_INDEX] = enum_val(29, 64);
        w.p[ep::WAVE] = 0.0;
        row(&mut w, 0, 23, 0, 30.5 / 128.0, 0.7); // LFO2 → wave
        w.p[ep::AENV] = 0.5;
        w.p[ep::AENV + 4] = 0.6;
        vec![w, fx("Ensemble", 23, &[(3, 0.5)]), fx("Hall", 1, &[(3, 0.35)])]
    };
    let strings = {
        let mut s = Element::new("Strings", ElemType::Sample);
        s.p[ep::S_BANK] = 50.5 / 128.0;
        s.p[ep::AENV] = 0.42;
        s.p[ep::AENV + 4] = 0.5;
        s.p[ep::FILTER] = enum_val(2, 9);
        s.p[ep::CUTOFF] = 0.8;
        vec![s, fx("Hall", 1, &[(3, 0.3)])]
    };
    let piano = {
        let mut s = Element::new("Piano", ElemType::Sample);
        s.p[ep::S_BANK] = 9.5 / 128.0;
        s.p[ep::AENV + 1] = 0.8;
        s.p[ep::AENV + 2] = 0.0;
        s.p[ep::AENV + 3] = 0.5;
        s.p[ep::AENV + 4] = 0.3;
        vec![s, fx("Room", 3, &[(3, 0.2)])]
    };
    let kit = {
        let mut v = Vec::new();
        for (name, bank, key) in [("Kick", 0.5, 36), ("Snare", 1.5, 38), ("Hat Closed", 3.5, 42), ("Hat Open", 3.5, 46), ("Tom Lo", 2.5, 41), ("Tom Hi", 2.5, 45), ("Crash", 4.5, 49), ("Ride", 4.5, 51)] {
            let mut s = Element::new(name, ElemType::Sample);
            s.p[ep::S_BANK] = bank / 128.0;
            s.p[ep::S_WAVE] = match key {
                42 => 0.0,
                46 => 2.5 / 256.0,
                49 => 0.0,
                51 => 2.5 / 256.0,
                45 => 2.5 / 256.0,
                _ => 0.0,
            };
            s.p[ep::KEY_LO] = key as f32 / 127.0;
            s.p[ep::KEY_HI] = key as f32 / 127.0;
            s.p[ep::AENV + 1] = if key == 49 || key == 51 { 0.7 } else { 0.45 };
            s.p[ep::AENV + 2] = 0.0;
            s.p[ep::AENV + 4] = 0.3;
            s.p[ep::MUTE_COND] = if key == 42 || key == 46 { enum_val(4, 12) } else { 0.0 };
            v.push(s);
        }
        v.push(fx("Room", 3, &[(3, 0.15)]));
        v
    };
    vec![
        (
            "Factory Synths",
            vec![
                patch("Sweep Pad", "Warm Soft Pad Analog Chorus", "Detuned saw pad. MW: Vibrato", ["Tone", "Reso", "Attack", "Release", "Chorus", "Reverb"], sweep_pad),
                patch("Mono Bass", "Fat Analog Bass", "Punchy analog bass with sub oscillator.", ["Tone", "Reso", "Decay", "Drive", "Octave", "Release"], bass),
                patch("PWM Lead", "Bright Lead Analog Delay", "MW: Vibrato", ["Tone", "Reso", "Color", "Vibrato", "Delay", "Release"], lead),
                patch("Vowel Pad", "Breathy Vocal Pad Wavetable", "LFO sweeps the VoSim vowels.", ["Tone", "Color", "Attack", "Release", "Width", "Reverb"], vowels),
                patch("Glass Bells", "Bright Metallic Bell Wavetable", "", ["Tone", "Color", "Decay", "Release", "Width", "Reverb"], bells),
            ],
        ),
        (
            "Factory Keys",
            vec![
                patch("FM Tines", "Electric E-Piano FM Phaser", "Velocity controls the tine bark.", ["Tone", "Velocity", "Decay", "Release", "Phaser", "Reverb"], ep),
                patch("Rock Organ", "Organ Drawbar Rotary", "MW: rotary speed.", ["Tone", "Drive", "Rotary Speed", "Percussion", "Width", "Reverb"], organ),
                patch("Piano", "Acoustic Piano Natural", "Uses the loaded SoundFont (GM piano).", ["Tone", "Velocity", "Decay", "Release", "Width", "Reverb"], piano),
                patch("Strings", "String Sect Orchestra Natural", "Uses the loaded SoundFont (GM strings).", ["Tone", "Attack", "Release", "Width", "Velocity", "Reverb"], strings),
            ],
        ),
        ("Factory Drums", vec![patch("Basic Kit", "Drums Natural Percussion", "GM key map.", ["Tone", "Decay", "Drive", "Width", "Level", "Reverb"], kit)]),
    ]
}
