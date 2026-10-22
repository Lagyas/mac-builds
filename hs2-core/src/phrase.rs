//! Arpeggiator phrases (Phrases/ banks, "Milan Bank file Hnnn.0").
//!
//! Record: u32 size, name at +4, u32 event count at +0x1f4, events at +0x1f8
//! (24 bytes each: f64 time in beats, f64 duration in beats, status, data1,
//! data2, 5 pad). Note interpretation (own reading of the data): channel 14
//! notes 36..59 pick the n-th held note (36 = lowest), other channels are
//! relative to the lowest held note (60 = that note); CC events are sent as is.

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Event {
    pub time: f64,
    pub dur: f64,
    pub status: u8,
    pub d1: u8,
    pub d2: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Phrase {
    pub name: String,
    pub events: Vec<Event>,
    /// Loop length in beats (whole bars).
    pub length: f64,
}

fn u32le(d: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(d[o..o + 4].try_into().unwrap())
}
fn f64le(d: &[u8], o: usize) -> f64 {
    f64::from_le_bytes(d[o..o + 8].try_into().unwrap())
}

fn record_at(d: &[u8], r: usize) -> Option<(Phrase, usize)> {
    if r + 0x1f8 > d.len() || !d[r + 4..r + 7].iter().all(|c| c.is_ascii_digit()) || d[r + 7] != b' ' {
        return None;
    }
    let size = u32le(d, r) as usize;
    let n = u32le(d, r + 0x1f4) as usize;
    if size < 0x1f8 || r + size > d.len() || 0x1f8 + 24 * n > size {
        return None;
    }
    let raw = &d[r + 4..r + 0x24];
    let end = raw.iter().position(|&c| c == 0).unwrap_or(raw.len());
    let name: String = raw[..end].iter().map(|&c| c as char).collect();
    let mut events = Vec::with_capacity(n);
    for k in 0..n {
        let o = r + 0x1f8 + 24 * k;
        let (time, dur) = (f64le(d, o), f64le(d, o + 8));
        if !time.is_finite() || !dur.is_finite() || !(0.0..1e4).contains(&time) {
            return None;
        }
        events.push(Event { time, dur: dur.max(0.0), status: d[o + 16], d1: d[o + 17], d2: d[o + 18] });
    }
    let last = events.iter().map(|e| e.time + e.dur.min(0.5)).fold(0.0f64, f64::max);
    let length = ((last / 4.0).ceil() * 4.0).max(4.0);
    // drop the "NNN " number prefix
    let name = name.get(4..).unwrap_or(&name).trim().to_string();
    Some((Phrase { name, events, length }, size))
}

/// All phrases of a phrase bank file.
pub fn parse(d: &[u8]) -> Vec<Phrase> {
    for r in 0x30..d.len().saturating_sub(0x1f8) {
        let mut out = Vec::new();
        let mut p = r;
        while let Some((ph, size)) = record_at(d, p) {
            out.push(ph);
            p += size;
        }
        // the chain must run to (nearly) the end of the file
        if out.len() >= 1 && d.len().saturating_sub(p) < 64 {
            return out;
        }
    }
    Vec::new()
}

impl Phrase {
    /// Map a phrase note to a key for the held notes (sorted, lowest first).
    pub fn map_note(&self, e: &Event, held: &[u8]) -> Option<u8> {
        let low = *held.first()? as i32;
        let n = e.d1 as i32;
        let key = if e.status & 0x0f == 14 && (36..60).contains(&n) {
            let idx = (n - 36) as usize;
            let k = held.len();
            held[idx % k] as i32 + 12 * (idx / k) as i32
        } else if e.status & 0x0f == 9 {
            n
        } else {
            low + n - 60
        };
        (0..128).contains(&key).then_some(key as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(name: &str, ev: &[(f64, f64, u8, u8, u8)]) -> Vec<u8> {
        let size = 0x1f8 + 24 * ev.len() + 24;
        let mut d = vec![0u8; size];
        d[..4].copy_from_slice(&(size as u32).to_le_bytes());
        d[4..4 + name.len()].copy_from_slice(name.as_bytes());
        d[0x1f4..0x1f8].copy_from_slice(&(ev.len() as u32).to_le_bytes());
        for (k, e) in ev.iter().enumerate() {
            let o = 0x1f8 + 24 * k;
            d[o..o + 8].copy_from_slice(&e.0.to_le_bytes());
            d[o + 8..o + 16].copy_from_slice(&e.1.to_le_bytes());
            d[o + 16] = e.2;
            d[o + 17] = e.3;
            d[o + 18] = e.4;
        }
        d
    }

    #[test]
    fn parse_and_map() {
        let mut d = vec![0u8; 0x40];
        d.extend(record("000 Echo 01", &[(0.0, 0.2, 0x90, 60, 64), (0.25, 0.2, 0x90, 67, 50)]));
        d.extend(record("001 Arp", &[(0.0, 0.5, 0x9e, 37, 64)]));
        let ps = parse(&d);
        assert_eq!(ps.len(), 2);
        assert_eq!(ps[0].name, "Echo 01");
        assert_eq!(ps[0].length, 4.0);
        assert_eq!(ps[0].map_note(&ps[0].events[1], &[48, 52]), Some(55));
        assert_eq!(ps[1].map_note(&ps[1].events[0], &[48, 52, 55]), Some(52));
        assert_eq!(ps[1].map_note(&ps[1].events[0], &[48]), Some(60));
    }
}
