use hs2_core::{bank, synth::Synth};
use std::sync::Arc;
fn main() {
    let dir = std::env::args().nth(1).unwrap();
    let mut s = Synth::new(44100.0);
    for t in 0..46 { s.wt.get(t); }
    let (mut total, mut silent, mut bad) = (0, 0, 0);
    let mut quiet = Vec::new();
    for f in std::fs::read_dir(&dir).unwrap() {
        let f = f.unwrap().path();
        if f.is_dir() { continue; }
        let d = std::fs::read(&f).unwrap();
        for (i, p) in bank::patches(&d).into_iter().enumerate() {
            total += 1;
            let name = p.name.clone();
            let wt = s.wt.clone(); s = Synth::new(44100.0); s.wt = wt;
            s.set_patch(0, Some(Arc::new(p)));
            for n in [36u8, 48, 60, 64, 67, 72] { s.midi(&[0x90, n, 100]); }
            let mut l = vec![0.0; 22050]; let mut r = vec![0.0; 22050];
            s.render(&mut l, &mut r);
            s.panic();
            let mut tail = vec![0.0; 4410]; let mut tr = vec![0.0; 4410];
            s.render(&mut tail, &mut tr);
            let e = (l.iter().map(|x| x * x).sum::<f32>() / 22050.0).sqrt();
            if !e.is_finite() { bad += 1; println!("NaN {} #{i} {}", f.display(), name); }
            else if e < 0.002 { silent += 1; quiet.push(format!("{:.4} {} #{i} {}", e, f.file_name().unwrap().to_string_lossy(), name)); }
        }
    }
    for q in quiet.iter().take(40) { println!("{q}"); }
    println!("total {total} silent {silent} nan {bad}");
}
