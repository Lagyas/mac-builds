//! Hypersonic 2 content banks ("Milan Bank file Xnnn.0 Copyright (c)200y Wizoo").
//!
//! Patches are plain data (see `patch`). Sample, wavetable and drawbar banks
//! only expose their names here: their audio is protected and is never read.

use crate::patch::Patch;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BankKind {
    Patch,
    Samples,
    Wavetables,
    Tunings,
    Drawbars,
    Phrases,
    Unknown(u16),
}

impl BankKind {
    fn from_u16(k: u16) -> Self {
        match k {
            1 => Self::Patch,
            2 => Self::Samples,
            3 => Self::Wavetables,
            4 => Self::Tunings,
            5 => Self::Drawbars,
            6 => Self::Phrases,
            k => Self::Unknown(k),
        }
    }
}

/// Bank header: kind and the id in the signature ("P016" → ('P', 16)).
pub fn header(d: &[u8]) -> Option<(BankKind, char, u32)> {
    let sig = d.get(..0x30)?;
    let s = std::str::from_utf8(sig).ok().or_else(|| std::str::from_utf8(&sig[..0x2e]).ok())?;
    let rest = s.strip_prefix("Milan Bank file ").or_else(|| s.strip_prefix("Hypersonic bank "))?;
    let letter = rest.chars().next()?;
    let num: u32 = rest.get(1..4)?.parse().ok()?;
    let kind = u16::from_le_bytes([*d.get(0x30)?, *d.get(0x31)?]);
    Some((BankKind::from_u16(kind), letter, num))
}

/// All patches of a preset patch bank.
pub fn patches(d: &[u8]) -> Vec<Patch> {
    let mut pos: Vec<usize> = d.windows(4).enumerate().filter(|(_, w)| *w == b"StSS").map(|(i, _)| i).collect();
    pos.push(d.len());
    pos.windows(2).filter_map(|w| Patch::parse(&d[w[0]..w[1]])).collect()
}

/// Names stored as pascal strings (u8 length, text, NUL) in a bank header —
/// the longest run of consecutive ones near the start of the file.
pub fn names(d: &[u8]) -> Vec<String> {
    let lim = d.len().min(0x40000);
    let mut best: Vec<String> = Vec::new();
    let mut i = 0x30;
    while i < lim {
        let mut run = Vec::new();
        let mut j = i;
        while j < lim {
            let n = d[j] as usize;
            if n == 0 || j + 1 + n >= d.len() || d[j + 1 + n] != 0 {
                break;
            }
            let t = &d[j + 1..j + 1 + n];
            if !t.iter().all(|&c| (0x20..0x7f).contains(&c)) {
                break;
            }
            run.push(String::from_utf8_lossy(t).into_owned());
            j += n + 2;
        }
        if run.len() > best.len() {
            best = run;
            i = j;
        } else {
            i += 1;
        }
        if best.len() >= 2 && i > 0x30 + 64 * 1024 {
            break;
        }
    }
    best
}

/// "016 Acoustic Basses" → (16, "Acoustic Basses").
pub fn split_file_name(name: &str) -> Option<(u32, &str)> {
    let (n, rest) = name.split_once(' ')?;
    Some((n.parse().ok()?, rest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names() {
        assert_eq!(split_file_name("016 Acoustic Basses"), Some((16, "Acoustic Basses")));
        assert_eq!(split_file_name("Basses"), None);
    }

    #[test]
    fn pascal_names() {
        let mut d = vec![0u8; 0x40];
        for n in ["Noise Band 1", "Red Noise", "White Noise"] {
            d.push(n.len() as u8);
            d.extend_from_slice(n.as_bytes());
            d.push(0);
        }
        d.extend_from_slice(&[0xff; 8]);
        assert_eq!(names(&d), ["Noise Band 1", "Red Noise", "White Noise"]);
    }
}
