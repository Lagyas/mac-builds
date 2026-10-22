//! Parameter value → display text, as XGworks shows it.
//!
//! RE: `XGworks.exe` formats XG values in `FUN_00601e93` (via `CPrmStr`): one of
//! 18 string tables indexed by the raw value, plus a unit suffix. Effect
//! parameters are described per effect type by 16 descriptors
//! `{name, min, max, kind, offset, divisor, table, unit}` (see `xgunits_data`).
//! `kind`: 1 = number, 2/6 = table lookup, 3 = enum (list from `ws.ini [unit]`),
//! 4 = raw − offset, 5 = raw ÷ divisor, 7 = raw × divisor, 0 = unused.

use std::collections::HashMap;

pub use crate::xgunits_data::Desc;
use crate::xgunits_data::{FX_DESC, NOTE_NAMES, PAN_RANDOM, TABLES};
use crate::xgtbl::{FxParam, Ini, TblParam};

/// Enum lists from `ws.ini` (`[Er Type] 0=S-H ...`).
#[derive(Clone, Debug, Default)]
pub struct Enums(pub HashMap<String, Vec<String>>);

impl Enums {
    pub fn parse(bytes: &[u8]) -> Enums {
        let ini = Ini::parse(bytes);
        let mut m = HashMap::new();
        for (name, kv) in &ini.sections {
            let mut v: Vec<(usize, String)> =
                kv.iter().filter_map(|(k, val)| k.parse::<usize>().ok().map(|i| (i, val.clone()))).collect();
            v.sort_by_key(|x| x.0);
            m.insert(name.clone(), v.into_iter().map(|x| x.1).collect());
        }
        Enums(m)
    }
    fn get(&self, key: &str, i: u32) -> Option<&str> {
        self.0.get(key)?.get(i as usize).map(|s| s.as_str())
    }
}

/// Lowercase alphanumerics of a name without a parenthesised unit, with the
/// usual XGworks abbreviations expanded ("Lvl" → "level", …).
fn norm(name: &str) -> String {
    let base = name.split('(').next().unwrap_or(name).to_ascii_lowercase();
    let base = base.replace("lvl", "level").replace("fb ", "feedback ").replace("freq", "frequency");
    base.chars().filter(|c| c.is_ascii_alphanumeric()).collect::<String>().replace("frequencyuency", "frequency")
}

fn names_match(a: &str, b: &str) -> bool {
    let (a, b) = (norm(a), norm(b));
    !a.is_empty() && !b.is_empty() && (a == b || (a.len() >= 5 && b.len() >= 5 && (a.starts_with(&b) || b.starts_with(&a))))
}

/// The descriptor block that best fits an effect's `.tbl` param list (names
/// first, then ranges and used slots). The EXE's descriptors come from the
/// newest models, so ranges may differ for older ones; `None` if nothing fits.
pub fn match_fx_desc(params: &[Option<FxParam>]) -> Option<&'static [Desc; 16]> {
    let used = params.iter().filter(|p| p.is_some()).count();
    if used == 0 {
        return None;
    }
    let score = |blk: &[Desc; 16]| -> i32 {
        let mut s = 0;
        for (k, p) in params.iter().enumerate().take(16) {
            let d = &blk[k];
            match p {
                Some(p) if d.3 != 0 => {
                    if names_match(&p.name, d.0) {
                        s += 4;
                    }
                    if d.1 as u32 == p.min && d.2 as u32 == p.max {
                        s += 2;
                    }
                }
                Some(_) => s -= 3,
                None if d.3 != 0 && !d.0.is_empty() => s -= 1,
                None => {}
            }
        }
        s
    };
    let (best, sc) = FX_DESC.iter().map(|b| (b, score(b))).max_by_key(|x| x.1)?;
    (sc >= 3 * used as i32).then_some(best)
}

/// Descriptor for one effect param: from the matched block, but only if the
/// slot's name agrees (so a partial match never mislabels a value).
pub fn fx_slot_desc(params: &[Option<FxParam>], k: usize) -> Option<&'static Desc> {
    let blk = match_fx_desc(params)?;
    let p = params.get(k)?.as_ref()?;
    let d = &blk[k];
    (d.3 != 0 && (names_match(&p.name, d.0) || (d.1 as u32 == p.min && d.2 as u32 == p.max))).then_some(d)
}

fn table(n: u8, raw: u32) -> Option<&'static str> {
    TABLES.get(n as usize)?.get(raw as usize).copied().filter(|s| !s.is_empty())
}

/// Display text of an effect parameter value.
pub fn fx_display(d: &Desc, raw: u32, enums: &Enums) -> String {
    let (_, _, _, kind, off, div, tab, unit) = *d;
    let num_unit = |s: String| if unit.starts_with(' ') { format!("{s}{unit}") } else { s };
    match kind {
        2 | 6 if tab != 0 => table(tab, raw).map(|s| num_unit(s.to_string())).unwrap_or_else(|| raw.to_string()),
        3 => enums.get(unit, raw).map(str::to_string).unwrap_or_else(|| raw.to_string()),
        4 => num_unit(format!("{}", raw as i32 - off as i32)),
        5 if div > 1 => {
            let dec = (div as f64).log10().round() as usize;
            num_unit(format!("{:.*}", dec, raw as f64 / div as f64))
        }
        7 => num_unit(format!("{}", raw * div as u32)),
        _ => num_unit(raw.to_string()),
    }
}

fn pan(raw: u32, random_at_zero: bool) -> String {
    if random_at_zero {
        PAN_RANDOM.get(raw as usize).map_or_else(|| raw.to_string(), |s| s.to_string())
    } else {
        TABLES[15].get(raw as usize).filter(|s| !s.is_empty()).map_or_else(|| raw.to_string(), |s| s.to_string())
    }
}

/// Display text for a block parameter from the `.tbl` (system / part / EQ /
/// drum / effect return…), using its name, form and display offset.
pub fn param_display(p: &TblParam, raw: u32) -> String {
    let n = p.name.to_ascii_lowercase();
    let signed = raw as i64 + p.disp_offset as i64;
    if n.contains("pan") {
        // part / drum pan: 0 = random; effect returns use L63..R63 from 1
        return pan(raw, p.min() == 0);
    }
    if n.contains("note limit") || n.contains("note asgn") || n == "low note" || n == "high note" {
        return NOTE_NAMES.get(raw as usize).map_or_else(|| raw.to_string(), |s| s.to_string());
    }
    if n.contains("(khz)") || (n.contains("freq") && n.contains("eq")) {
        if let Some(s) = table(3, raw) {
            return format!("{s} kHz");
        }
    }
    if n.starts_with("master tune") {
        return format!("{:+.1} cent", signed as f64 / 10.0);
    }
    if n == "detune" {
        return format!("{:+.1} Hz", signed as f64 / 10.0);
    }
    if n.starts_with("q") || n.contains(" q") {
        return format!("{:.1}", raw as f64 / 10.0);
    }
    if n.contains("(db)") || n.contains("gain") {
        return format!("{signed:+} dB");
    }
    if p.size() == 1 && p.max() == 1 && p.min() == 0 && p.form == 9 {
        return if raw == 0 { "Off".into() } else { "On".into() };
    }
    if p.disp_offset < 0 { format!("{signed:+}") } else { signed.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fp(min: u32, max: u32, name: &str) -> Option<FxParam> {
        Some(FxParam { min, max, form: 0, name: name.into() })
    }

    #[test]
    fn hall_matches_and_formats() {
        // *Hall_rev_var signature from MU128.tbl
        let mut list = vec![None; 16];
        list[0] = fp(0, 0x45, "Reverb Time (s)");
        list[1] = fp(0, 0x0a, "Diffusion");
        list[2] = fp(0, 0x3f, "Initial Delay");
        list[3] = fp(0, 0x34, "HPF Cutoff (kHz)");
        list[4] = fp(0x22, 0x3c, "LPF Cutoff (kHz)");
        list[9] = fp(1, 0x7f, "Dry/Wet");
        list[10] = fp(0, 0x3f, "Rev Delay");
        list[11] = fp(0, 4, "Density");
        list[12] = fp(1, 0x7f, "Er/Rev Balance");
        list[13] = fp(1, 0x0a, "High Damp");
        list[14] = fp(1, 0x7f, "Feedback Level");
        let d = match_fx_desc(&list).expect("hall block");
        let e = Enums::default();
        assert_eq!(d[0].0, "Rev Time");
        assert_eq!(fx_display(&d[0], 0, &e), "0.3 s");
        assert_eq!(fx_display(&d[3], 0, &e), "0.020(Thru) kHz");
        assert_eq!(fx_display(&d[9], 64, &e), "D=W");
        assert_eq!(fx_display(&d[13], 5, &e), "0.5"); // high damp ÷10
        assert_eq!(fx_display(&d[14], 64, &e), "0"); // feedback −64
    }

    #[test]
    fn enums_from_ini() {
        let e = Enums::parse(b"[Pan Dir]\n0=L<->R\n1=L->R\n");
        let d: Desc = ("Pan Direction", 0, 5, 3, 0, 1, 0, "Pan Dir");
        assert_eq!(fx_display(&d, 1, &e), "L->R");
        assert_eq!(fx_display(&d, 4, &e), "4");
    }
}
