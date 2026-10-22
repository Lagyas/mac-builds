//! Saving / loading a whole setup ("combi" of this synth): parts with their
//! patches and MIDI/Mix settings, global FX and master settings.
//! Own binary format: "HS2S", version, then length-prefixed records.

use crate::patch::Patch;
use crate::synth::{FxSlot, Synth, PARTS};
use std::sync::Arc;

const MAGIC: &[u8; 4] = b"HS2S";
const VERSION: u32 = 1;

struct W(Vec<u8>);
impl W {
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f32(&mut self, v: f32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn i32(&mut self, v: i32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn bytes(&mut self, b: &[u8]) {
        self.u32(b.len() as u32);
        self.0.extend_from_slice(b);
    }
}

struct R<'a>(&'a [u8], usize);
impl R<'_> {
    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let s = self.0.get(self.1..self.1 + n)?;
        self.1 += n;
        Some(s)
    }
    fn u32(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }
    fn bytes(&mut self) -> Option<&[u8]> {
        let n = self.u32()? as usize;
        self.take(n)
    }
}

fn put_fx(w: &mut W, f: &FxSlot) {
    w.u32(f.id);
    w.u32(f.on as u32);
    for v in f.p {
        w.f32(v);
    }
}

fn get_fx(r: &mut R) -> Option<FxSlot> {
    let id = r.u32()?;
    let on = r.u32()? != 0;
    let mut p = [0.0; crate::fx::NPARAM];
    for v in p.iter_mut() {
        *v = r.f32()?;
    }
    Some(FxSlot { id, p, on })
}

pub fn save(s: &Synth, name: &str) -> Vec<u8> {
    let mut w = W(Vec::new());
    w.0.extend_from_slice(MAGIC);
    w.u32(VERSION);
    w.bytes(name.as_bytes());
    w.f32(s.master);
    w.f32(s.hype);
    w.f32(s.master_tune);
    w.f32(s.tempo);
    for f in &s.global_fx {
        put_fx(&mut w, f);
    }
    for p in &s.parts {
        w.bytes(&p.patch.as_ref().map(|x| x.write()).unwrap_or_default());
        w.u32(p.mute as u32 | (p.link as u32) << 1 | (p.patch_fx_on as u32) << 2 | (p.lock as u32) << 3);
        w.f32(p.level);
        w.f32(p.pan);
        w.u32(p.output as u32);
        for v in p.sends {
            w.f32(v);
        }
        for v in [p.key_range.0, p.key_range.1, p.key_range.2, p.vel_range.0, p.vel_range.1, p.vel_range.2, p.semi] {
            w.i32(v);
        }
        w.f32(p.cent);
        w.u32(p.max_voices as u32);
        for v in p.knobs {
            w.f32(v);
        }
        for f in &p.patch_fx {
            match f {
                Some(f) => {
                    w.u32(1);
                    put_fx(&mut w, f);
                }
                None => w.u32(0),
            }
        }
    }
    w.0
}

/// Load a saved setup into the synth; returns its name.
pub fn load(s: &mut Synth, d: &[u8]) -> Option<String> {
    if d.get(..4)? != MAGIC {
        return None;
    }
    let mut r = R(d, 4);
    let _ver = r.u32()?;
    let name = String::from_utf8_lossy(r.bytes()?).into_owned();
    s.master = r.f32()?;
    s.hype = r.f32()?;
    s.master_tune = r.f32()?;
    s.tempo = r.f32()?;
    for k in 0..4 {
        let f = get_fx(&mut r)?;
        s.set_global_fx(k, f);
    }
    for i in 0..PARTS {
        let pd = r.bytes()?.to_vec();
        let flags = r.u32()?;
        let patch = Patch::parse(&pd).map(Arc::new);
        for t in patch.as_ref().map(|p| Synth::tables_for(p)).unwrap_or_default() {
            s.wt.get(t);
        }
        s.parts[i].lock = false;
        s.set_patch(i, patch);
        let p = &mut s.parts[i];
        p.mute = flags & 1 != 0;
        p.link = flags & 2 != 0;
        p.patch_fx_on = flags & 4 != 0;
        p.lock = flags & 8 != 0;
        p.level = r.f32()?;
        p.pan = r.f32()?;
        p.output = r.u32()? as usize;
        for v in p.sends.iter_mut() {
            *v = r.f32()?;
        }
        let mut iv = [0i32; 7];
        for v in iv.iter_mut() {
            *v = r.i32()?;
        }
        p.key_range = (iv[0], iv[1], iv[2]);
        p.vel_range = (iv[3], iv[4], iv[5]);
        p.semi = iv[6];
        p.cent = r.f32()?;
        p.max_voices = r.u32()? as usize;
        for v in p.knobs.iter_mut() {
            *v = r.f32()?;
        }
        for k in 0..4 {
            p.patch_fx[k] = if r.u32()? != 0 { Some(get_fx(&mut r)?) } else { None };
        }
        p.update_macros();
    }
    Some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let mut s = Synth::new(48000.0);
        let bank = crate::factory::banks();
        s.set_patch(3, Some(Arc::new(bank[0].1[1].clone())));
        s.parts[3].level = 0.3;
        s.parts[4].link = true;
        s.parts[3].knobs[2] = 0.9;
        s.master = 0.4;
        let d = save(&s, "My Combi");
        let mut t = Synth::new(48000.0);
        assert_eq!(load(&mut t, &d).as_deref(), Some("My Combi"));
        assert_eq!(t.parts[3].name(), "Mono Bass");
        assert_eq!(t.parts[3].level, 0.3);
        assert!(t.parts[4].link);
        assert_eq!(t.parts[3].knobs[2], 0.9);
        assert_eq!(t.master, 0.4);
    }
}
