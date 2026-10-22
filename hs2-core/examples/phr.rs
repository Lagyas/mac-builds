use hs2_core::{bank, library::Library, synth::Synth};
use std::sync::Arc;
fn main() {
    let root = "F:/rea/_hypersonic2/iso/Hypersonic Content";
    let mut lib = Library::default();
    for f in std::fs::read_dir(format!("{root}/Phrases")).unwrap() {
        let f = f.unwrap().path();
        lib.add_file(&f.file_name().unwrap().to_string_lossy(), &std::fs::read(&f).unwrap());
    }
    let n: usize = lib.phrases.iter().map(|b| b.1.len()).sum();
    println!("phrases {n}");
    let d = std::fs::read(format!("{root}/Patches/086 Phrases")).unwrap();
    let p = bank::patches(&d)[0].clone();
    println!("{}", p.name);
    let mut s = Synth::new(44100.0);
    s.phrases = lib.phrases.clone();
    s.set_patch(0, Some(Arc::new(p)));
    s.midi(&[0x90, 48, 100]);
    let mut l = vec![0.0; 4410];
    let mut r = vec![0.0; 4410];
    let mut onsets = 0;
    let mut prev = 0;
    for _ in 0..40 {
        s.render(&mut l, &mut r);
        let pc = s.polyphony(0);
        if pc > prev { onsets += 1; }
        prev = pc;
    }
    println!("polyphony rises (≈ phrase notes) in 4 s: {onsets}");
}
