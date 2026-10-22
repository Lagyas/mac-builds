//! Parses the real XGworks data files when they are present (F:\rea\XG4\XGEditor).
use xgworks_core::xgtbl::*;

const DIR: &str = "F:/rea/XG4/XGEditor";

#[test]
fn real_mu128_table_and_voices() {
    let Ok(tbl) = std::fs::read(format!("{DIR}/MU128.tbl")) else { return };
    let t = parse_tbl(&tbl);
    assert!(t.blocks.len() >= 20, "blocks: {}", t.blocks.len());
    let part = t.block("Part_1").unwrap();
    assert_eq!(part.instances(), 64);
    let vol = part.params.iter().find(|p| p.name == "Vol").unwrap();
    assert_eq!((vol.lo, vol.default(0)), (0x0B, 100));
    assert_eq!(t.rev_types.len(), 13);
    assert!(t.var_types.len() > 40);
    for slot in FxSlot::ALL {
        for (_, _, list) in t.types(slot) {
            assert!(t.fx_params.contains_key(list), "missing param list {list}");
        }
    }
    assert_eq!(t.fx_name(0x41, 0x00), "CHORUS1");

    let ini = std::fs::read(format!("{DIR}/xgeditor.ini")).unwrap();
    let models = parse_models(&ini);
    assert!(models.iter().any(|m| m.0 == "MU128" && m.1 == "MU128.tbl"));

    let v = std::fs::read(format!("{DIR}/XGVoiceListExtension5.ini")).unwrap();
    let voices = parse_voice_list(&v);
    assert!(voices.iter().any(|v| v.msb == 0 && v.program == 0 && v.lsb == 0 && v.name == "Grand Piano"));
    assert!(voices.iter().any(|v| v.msb == 127 && v.program == 0 && v.name == "Standard Kit"));
    assert!(voices.len() > 500, "voices: {}", voices.len());
}

#[test]
fn real_effect_lists_match_exe_descriptors() {
    let Ok(tbl) = std::fs::read(format!("{DIR}/MU128.tbl")) else { return };
    let t = parse_tbl(&tbl);
    let mut miss = Vec::new();
    let mut n = 0;
    for (name, list) in &t.fx_params {
        if list.iter().all(|p| p.is_none()) {
            continue;
        }
        n += 1;
        if xgworks_core::xgunits::match_fx_desc(list).is_none() {
            miss.push(name.clone());
        }
    }
    miss.sort();
    println!("matched {}/{n}; unmatched: {miss:?}", n - miss.len());
    assert!(miss.len() * 5 <= n, "too many unmatched: {miss:?}");
}
