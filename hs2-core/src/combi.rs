//! Hypersonic 2 combis (.fxb: VST `CcnK`/`FBCh` with an `StSS` combi chunk).
//!
//! Chunk at 0xa0: "StSS", u32 0x00990f46, 0, u32 0x142, then 16 u32 part
//! offsets (relative to the chunk, 0 = empty part). A part is a patch without
//! its "StSS" header: u32 flags (0x100 = linked to the part above, part 1
//! holds 0x7800 + combi number), u32 / u32 patch header words, 0, then the
//! patch from its name on. The combi name is the last string of the file.
//! A part's MIDI/Mix settings live in the patch parameters (key / velocity
//! range, transpose, pan, sends, level trim).

use crate::patch::Patch;

#[derive(Clone, Debug)]
pub struct CombiPart {
    pub part: usize,
    pub link: bool,
    pub patch: Patch,
}

#[derive(Clone, Debug)]
pub struct Combi {
    pub name: String,
    pub parts: Vec<CombiPart>,
}

fn u32le(d: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?))
}

pub fn parse(d: &[u8]) -> Option<Combi> {
    if d.get(..4)? != b"CcnK" || d.get(8..12)? != b"FBCh" {
        return None;
    }
    let base = (0x20..d.len().min(0x400)).find(|&i| &d[i..i + 4] == b"StSS" && matches!(u32le(d, i + 4), Some(v) if v & 0xffff_ff00 == 0x0099_0f00))?;
    let mut slots = [0usize; 16];
    for (i, s) in slots.iter_mut().enumerate() {
        *s = u32le(d, base + 0x10 + 4 * i)? as usize;
    }
    let mut starts: Vec<usize> = slots.iter().filter(|&&o| o > 0).map(|&o| base + o).collect();
    starts.sort_unstable();
    let mut parts = Vec::new();
    for (i, &o) in slots.iter().enumerate() {
        if o == 0 {
            continue;
        }
        let h = base + o;
        let end = starts.iter().copied().find(|&s| s > h).unwrap_or(d.len());
        if h + 16 >= end {
            continue;
        }
        let flags = u32le(d, h)?;
        let mut rec = Vec::with_capacity(end - h + 0x10);
        rec.extend_from_slice(b"StSS");
        rec.extend_from_slice(&0x0099_0edfu32.to_le_bytes());
        rec.extend_from_slice(&0u32.to_le_bytes());
        rec.extend_from_slice(&0x150u32.to_le_bytes());
        rec.extend_from_slice(&u32::MAX.to_le_bytes());
        rec.extend_from_slice(&d[h + 4..end]);
        if let Some(patch) = Patch::parse(&rec) {
            parts.push(CombiPart { part: i, link: i > 0 && flags & 0x100 != 0, patch });
        }
    }
    // the combi name: last printable string of the file
    let tail = &d[d.len().saturating_sub(0x80)..];
    let name = tail
        .split(|&c| c == 0)
        .filter(|s| s.len() >= 2 && s.iter().all(|c| (0x20..0x7f).contains(c)))
        .last()
        .map(|s| s.iter().map(|&c| c as char).collect())
        .unwrap_or_default();
    Some(Combi { name, parts })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_built_combi() {
        let mut p = Patch::new("Pad A");
        p.elements.push(crate::patch::Element::new("Osc", crate::patch::ElemType::Analog));
        let rec = p.write();
        let body = &rec[0x14..rec.len() - 12]; // from header word 0x14 on, no end mark
        let mut d = b"CcnK\0\0\0\0FBCh\0\0\0\x01".to_vec();
        d.resize(0xa0, 0);
        let base = d.len();
        d.extend_from_slice(b"StSS");
        d.extend_from_slice(&0x0099_0f46u32.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes());
        d.extend_from_slice(&0x142u32.to_le_bytes());
        let slots_at = d.len();
        d.resize(slots_at + 64, 0);
        for (k, flags) in [(0usize, 0x7800u32), (1, 0x100)] {
            let off = d.len() - base;
            d[slots_at + 4 * k..slots_at + 4 * k + 4].copy_from_slice(&(off as u32).to_le_bytes());
            d.extend_from_slice(&flags.to_le_bytes());
            d.extend_from_slice(body);
            d.extend_from_slice(&[0; 4]);
        }
        d.extend_from_slice(b"\0\0\0\0My Combi\0\0\0\0");
        let c = parse(&d).unwrap();
        assert_eq!(c.name, "My Combi");
        assert_eq!(c.parts.len(), 2);
        assert_eq!(c.parts[0].patch.name, "Pad A");
        assert_eq!(c.parts[0].patch.elements.len(), 1);
        assert!(c.parts[1].link && !c.parts[0].link);
    }
}
