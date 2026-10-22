//! hs2 command line: list the patches of a Hypersonic 2 bank, or render a
//! patch to a WAV file.
//!
//!   hs2 list  <bank file>
//!   hs2 render <bank file|factory> <patch #> <out.wav> [note] [soundfont.sf2]

use hs2_core::{bank, factory, patch::Patch, synth::Synth};
use std::sync::Arc;

fn wav(path: &str, l: &[f32], r: &[f32], sr: u32) -> std::io::Result<()> {
    let mut d = Vec::new();
    let n = l.len() as u32;
    d.extend_from_slice(b"RIFF");
    d.extend_from_slice(&(36 + n * 4).to_le_bytes());
    d.extend_from_slice(b"WAVEfmt ");
    d.extend_from_slice(&16u32.to_le_bytes());
    d.extend_from_slice(&1u16.to_le_bytes());
    d.extend_from_slice(&2u16.to_le_bytes());
    d.extend_from_slice(&sr.to_le_bytes());
    d.extend_from_slice(&(sr * 4).to_le_bytes());
    d.extend_from_slice(&4u16.to_le_bytes());
    d.extend_from_slice(&16u16.to_le_bytes());
    d.extend_from_slice(b"data");
    d.extend_from_slice(&(n * 4).to_le_bytes());
    for i in 0..l.len() {
        for v in [l[i], r[i]] {
            d.extend_from_slice(&((v.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
        }
    }
    std::fs::write(path, d)
}

fn load_patches(src: &str) -> Vec<Patch> {
    if src == "factory" {
        return factory::banks().into_iter().flat_map(|b| b.1).collect();
    }
    let d = std::fs::read(src).expect("read bank");
    bank::patches(&d)
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(|s| s.as_str()) {
        Some("list") => {
            for (i, p) in load_patches(&a[2]).iter().enumerate() {
                let els: Vec<String> = p.elements.iter().map(|e| format!("{}:{}", e.kind().letter(), e.name)).collect();
                println!("{i:3} {:32} [{}] knobs={:?}\n    {}", p.name, p.attributes, p.knob_names, els.join(", "));
            }
        }
        Some("render") => {
            let patches = load_patches(&a[2]);
            let idx: usize = a[3].parse().unwrap_or(0);
            let note: u8 = a.get(5).and_then(|s| s.parse().ok()).unwrap_or(60);
            let sr = 44100u32;
            let mut s = Synth::new(sr as f32);
            if let Some(sf) = a.get(6) {
                let d = std::fs::read(sf).expect("read sf2");
                s.sf = hs2_core::sf2::parse(&d).map(Arc::new);
            }
            let p = patches[idx].clone();
            for t in Synth::tables_for(&p) {
                s.wt.get(t);
            }
            println!("{}", p.name);
            s.set_patch(0, Some(Arc::new(p)));
            let total = sr as usize * 4;
            let (mut l, mut r) = (vec![0.0f32; total], vec![0.0f32; total]);
            let chord = [note, note + 4, note + 7];
            for n in chord {
                s.midi(&[0x90, n, 100]);
            }
            let half = sr as usize * 2;
            s.render(&mut l[..half], &mut r[..half]);
            for n in chord {
                s.midi(&[0x80, n, 0]);
            }
            s.render(&mut l[half..], &mut r[half..]);
            wav(&a[4], &l, &r, sr).expect("write wav");
        }
        _ => eprintln!("usage: hs2 list <bank> | hs2 render <bank|factory> <patch#> <out.wav> [note] [sf2]"),
    }
}
