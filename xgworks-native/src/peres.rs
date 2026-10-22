//! Minimal PE resource reader: pulls RT_BITMAP resources out of the original
//! XGworks.exe at run time (so none of Yamaha's artwork ships with the port).

use std::collections::HashMap;

pub struct Bitmap {
    pub w: usize,
    pub h: usize,
    /// RGBA, top-down.
    pub rgba: Vec<u8>,
}

fn u16_at(d: &[u8], o: usize) -> Option<usize> {
    Some(u16::from_le_bytes(d.get(o..o + 2)?.try_into().ok()?) as usize)
}
fn u32_at(d: &[u8], o: usize) -> Option<usize> {
    Some(u32::from_le_bytes(d.get(o..o + 4)?.try_into().ok()?) as usize)
}

/// The raw data of every resource of type `ty` with a numeric id: (id, bytes).
fn resources(exe: &[u8], ty: usize) -> Vec<(u32, &[u8])> {
    let mut out = Vec::new();
    let _ = (|| -> Option<()> {
        let pe = u32_at(exe, 0x3C)?;
        let nsec = u16_at(exe, pe + 6)?;
        let opt = pe + 24;
        let opt_size = u16_at(exe, pe + 20)?;
        let dd = if u16_at(exe, opt)? == 0x20B { opt + 112 } else { opt + 96 };
        let rsrc_rva = u32_at(exe, dd + 2 * 8)?;
        let secs: Vec<(usize, usize, usize)> = (0..nsec)
            .filter_map(|i| {
                let s = opt + opt_size + 40 * i;
                Some((u32_at(exe, s + 12)?, u32_at(exe, s + 8)?.max(u32_at(exe, s + 16)?), u32_at(exe, s + 20)?))
            })
            .collect();
        let off = |rva: usize| secs.iter().find(|s| s.0 <= rva && rva < s.0 + s.1).map(|s| rva - s.0 + s.2);
        let base = off(rsrc_rva)?;
        let entries = |o: usize| -> Vec<(usize, usize)> {
            let n = u16_at(exe, base + o + 12).unwrap_or(0) + u16_at(exe, base + o + 14).unwrap_or(0);
            (0..n)
                .filter_map(|k| Some((u32_at(exe, base + o + 16 + 8 * k)?, u32_at(exe, base + o + 20 + 8 * k)?)))
                .collect()
        };
        for (t, p1) in entries(0) {
            if t != ty {
                continue;
            }
            for (id, p2) in entries(p1 & 0x7FFF_FFFF) {
                if id & 0x8000_0000 != 0 {
                    continue;
                }
                let Some(&(_, p3)) = entries(p2 & 0x7FFF_FFFF).first() else { continue };
                let (Some(rva), Some(size)) = (u32_at(exe, base + p3), u32_at(exe, base + p3 + 4)) else { continue };
                if let Some(d) = off(rva).and_then(|o| exe.get(o..o + size)) {
                    out.push((id as u32, d));
                }
            }
        }
        Some(())
    })();
    out
}

/// All bitmaps whose numeric id passes `want`.
pub fn load_bitmaps(exe: &[u8], want: impl Fn(u32) -> bool) -> HashMap<u32, Bitmap> {
    resources(exe, 2).into_iter().filter(|(id, _)| want(*id)).filter_map(|(id, d)| Some((id, decode_dib(d)?))).collect()
}

/// A mouse cursor: image (transparent where the AND mask is set) + hot spot.
pub struct Cursor {
    pub img: Bitmap,
    pub hot: (usize, usize),
}

/// The cursors of the given RT_GROUP_CURSOR ids (first image of each group).
pub fn load_cursors(exe: &[u8], want: impl Fn(u32) -> bool) -> HashMap<u32, Cursor> {
    let curs: HashMap<u32, &[u8]> = resources(exe, 1).into_iter().collect();
    let mut out = HashMap::new();
    for (gid, g) in resources(exe, 12) {
        if !want(gid) {
            continue;
        }
        // NEWHEADER + RESDIR entries (14 bytes); the cursor id is the last WORD
        let Some(cid) = u16_at(g, 6 + 12) else { continue };
        let Some(d) = curs.get(&(cid as u32)) else { continue };
        if let Some(c) = decode_cursor(d) {
            out.insert(gid, c);
        }
    }
    out
}

/// RT_CURSOR: hot spot (2 WORDs) + DIB with doubled height (XOR then AND mask).
fn decode_cursor(d: &[u8]) -> Option<Cursor> {
    let hot = (u16_at(d, 0)?, u16_at(d, 2)?);
    let h = &d[4..];
    let hsz = u32_at(h, 0)?;
    let w = u32_at(h, 4)?;
    let hh = u32_at(h, 8)? / 2;
    let bpp = u16_at(h, 14)?;
    if bpp != 1 {
        return None;
    }
    let pal = hsz + 8;
    let stride = (w + 31) / 32 * 4;
    let xor = pal;
    let and = xor + stride * hh;
    let bit = |base: usize, x: usize, y: usize| h.get(base + (hh - 1 - y) * stride + x / 8).map_or(0, |b| (b >> (7 - x % 8)) & 1);
    let mut rgba = vec![0u8; w * hh * 4];
    for y in 0..hh {
        for x in 0..w {
            let (a, xb) = (bit(and, x, y), bit(xor, x, y));
            let px = match (a, xb) {
                (0, 0) => [0, 0, 0, 255],
                (0, _) => [255, 255, 255, 255],
                (_, 0) => [0, 0, 0, 0],
                _ => [0, 0, 0, 255], // screen-inverting pixels: drawn black
            };
            rgba[(y * w + x) * 4..][..4].copy_from_slice(&px);
        }
    }
    Some(Cursor { img: Bitmap { w, h: hh, rgba }, hot })
}

/// Decode a packed DIB (BITMAPINFOHEADER + palette + bits), 1/4/8/24 bpp.
fn decode_dib(d: &[u8]) -> Option<Bitmap> {
    let hdr = u32_at(d, 0)?;
    let w = i32::from_le_bytes(d.get(4..8)?.try_into().ok()?);
    let h = i32::from_le_bytes(d.get(8..12)?.try_into().ok()?);
    let bpp = u16_at(d, 14)?;
    if u32_at(d, 16)? != 0 {
        return None; // compressed
    }
    let used = u32_at(d, 32)?;
    let ncol = if bpp <= 8 { if used > 0 { used } else { 1 << bpp } } else { 0 };
    let pal: Vec<[u8; 3]> = (0..ncol).map(|i| {
        let p = hdr + 4 * i;
        [d.get(p + 2).copied().unwrap_or(0), d.get(p + 1).copied().unwrap_or(0), d.get(p).copied().unwrap_or(0)]
    }).collect();
    let (wu, hu) = (w.unsigned_abs() as usize, h.unsigned_abs() as usize);
    let stride = (wu * bpp + 31) / 32 * 4;
    let bits = hdr + 4 * ncol;
    let mut rgba = vec![0u8; wu * hu * 4];
    for y in 0..hu {
        let src_row = if h > 0 { hu - 1 - y } else { y };
        let row = d.get(bits + src_row * stride..bits + (src_row + 1) * stride)?;
        for x in 0..wu {
            let c = match bpp {
                24 => [row[x * 3 + 2], row[x * 3 + 1], row[x * 3]],
                8 => pal.get(row[x] as usize).copied().unwrap_or_default(),
                4 => pal.get(((row[x / 2] >> (4 - 4 * (x % 2))) & 0x0F) as usize).copied().unwrap_or_default(),
                1 => pal.get(((row[x / 8] >> (7 - x % 8)) & 1) as usize).copied().unwrap_or_default(),
                _ => return None,
            };
            rgba[(y * wu + x) * 4..(y * wu + x) * 4 + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
        }
    }
    Some(Bitmap { w: wu, h: hu, rgba })
}

/// Toolbar bitmaps use light grey (C0C0C0) for "button face": make it transparent.
pub fn face_to_alpha(b: &mut Bitmap) {
    for px in b.rgba.chunks_exact_mut(4) {
        if px[..3] == [192, 192, 192] {
            px[3] = 0;
        }
    }
}
