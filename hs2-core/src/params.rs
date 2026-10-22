//! Parameter names for the editor (original indices).

include!("params_gen.rs");

use crate::patch::ElemType;

/// Name of element parameter `p` for an element of `kind`.
pub fn element(p: usize, kind: ElemType) -> String {
    if p >= 172 {
        let r = (p - 172) / 5;
        return format!("{} {}", r + 1, ["Src", "Mod", "Param", "Dest", "Depth"][(p - 172) % 5]);
    }
    if (17..49).contains(&p) {
        return TYPE_PAGE[kind.index()][p - 17].to_string();
    }
    ELEMENT[p].to_string()
}

/// Mod matrix source list (Source column; Modify column = OFF + these + modifiers).
pub const MOD_SOURCES: [&str; 28] = [
    "OFF", "Constant", "Random", "Velocity", "Key", "Pitch", "Mod Wheel", "Aftertouch", "Pitch Bend", "Breath Ctl", "Wheel 3",
    "Foot Ctl", "Sus Ped", "Soft Ped", "Knob 1", "Knob 2", "Knob 3", "Knob 4", "Knob 5", "Knob 6", "Seq 1", "Seq 2", "LFO 1",
    "LFO 2", "LFO 3", "Mod Env", "Filt Env", "Amp Env",
];

pub const MODIFIERS: [&str; 29] = [
    "Add", "Multiply", "Drive", "Clip +", "Clip -", "Rect +", "Rect -", "Curve", "Sine", "Exp", "Log", "Switch", "Quant 1",
    "Quant 2", "S+H", "Drift", "Sine Amp", "Sine Freq", "Lin Ramp", "Delay Ramp", "Exp Ramp", "Log Ramp", "Tri Env", "Gate Env",
    "Log Lag", "Lin Lag", "Peak Hold", "Diff Hold", "X-Fade",
];

/// Destination groups (value·128 bands, see `synth::eval_matrix`).
pub const DESTS: [(&str, f32); 9] = [
    ("OFF", 0.0),
    ("Pitch", 12.5),
    ("LFO3 Rate", 20.5),
    ("Wave", 30.5),
    ("Cutoff", 44.5),
    ("Filt Env", 52.5),
    ("Amp", 64.5),
    ("Osc2 Wave", 80.5),
    ("Pan", 7.5),
];

pub fn dest_name(v: f32) -> &'static str {
    let d = (v * 128.0) as usize;
    match d {
        0 => "OFF",
        1 | 9..=16 => "Pitch",
        2 | 27..=40 => "Wave",
        3 | 73..=127 => "Osc2 Wave",
        4 | 41..=48 => "Cutoff",
        49..=56 => "Filt Env",
        5 => "Reso",
        6 | 57..=72 => "Amp",
        7 => "Pan",
        8 | 17..=26 => "LFO3 Rate",
        _ => "Mod Env Rate",
    }
}

pub fn source_name(i: usize) -> String {
    match i {
        0..=27 => MOD_SOURCES[i].to_string(),
        28..=43 => format!("Row {}", i - 27),
        _ => "?".into(),
    }
}

pub fn modify_name(i: usize) -> String {
    match i {
        0..=27 => MOD_SOURCES[i].to_string(),
        28..=56 => MODIFIERS[i - 28].to_string(),
        _ => "?".into(),
    }
}
