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

#[test]
fn song_parts_from_real_mid() {
    // format-0 .MID: every part's program / volume comes from channel messages
    let data = std::fs::read("tests/fixtures/dream.mid").unwrap();
    let (tracks, _) = xgworks_core::edit_model(&data);
    let m = xgworks_core::xgtbl::XgMemory::from_song(&tracks);
    let parts: Vec<u8> = (0..16u8).filter(|&p| m.get([8, p, 3]).is_some()).collect();
    eprintln!("parts with a program: {parts:?}");
    assert!(parts.len() >= 2);
}

#[test]
fn system_on_plays_first_within_its_tick() {
    use xgworks_core::{edit_playback_plan, EditTrack, Event};
    let vol = Event::Sysex { t: 0, status: 0xF0, data: vec![0x43, 0x10, 0x4C, 8, 0, 0x0B, 33, 0xF7] };
    let on = Event::Sysex { t: 0, status: 0xF0, data: vec![0x43, 0x10, 0x4C, 0, 0, 0x7E, 0, 0xF7] };
    let a = EditTrack { others: vec![vol], ..Default::default() };
    let b = EditTrack { others: vec![on], ..Default::default() };
    let plan = edit_playback_plan(&[a, b], 480);
    assert_eq!(plan[0].2[5..7], [0, 0x7E]);
    assert_eq!(plan[1].2[5..7], [0, 0x0B]);
}

#[test]
fn editor_change_lands_in_the_songs_own_message() {
    // dream.mid: change every part's volume; the song keeps its message count
    // where it already sets the volume, and reads back the new values
    let data = std::fs::read("tests/fixtures/dream.mid").unwrap();
    let (mut tracks, _) = xgworks_core::edit_model(&data);
    let before = XgMemory::from_song(&tracks);
    let set: Vec<u8> = (0..16u8).filter(|&p| before.get([8, p, 0x0B]).is_some()).collect();
    assert!(!set.is_empty());
    let count = |ts: &[xgworks_core::EditTrack]| ts.iter().map(|t| t.others.len()).sum::<usize>();
    let n = count(&tracks);
    let changes: Vec<([u8; 3], Vec<u8>)> = set.iter().map(|&p| ([8, p, 0x0B], vec![11 + p])).collect();
    apply_xg_changes_song(&mut tracks, 0, &changes);
    assert_eq!(count(&tracks), n, "no new messages for values the song sets");
    let after = XgMemory::from_song(&tracks);
    for &p in &set {
        assert_eq!(after.get([8, p, 0x0B]), Some(11 + p), "part {}", p + 1);
    }
}

#[test]
fn sound_sets_from_the_installations() {
    use xgworks_core::soundset;
    let rd = |p: &str| std::fs::read(p).ok();
    if let (Some(m), Some(n), Some(k)) = (rd("F:/rea/XG4/Melody.ini"), rd("F:/rea/XG4/GM2 Normal.ini"), rd("F:/rea/XG4/GM2 SFX Kit.ini")) {
        let gm = soundset::gm(&m);
        assert_eq!(gm.tones.iter().filter(|t| !t.is_empty()).count(), 128);
        let gm2 = soundset::gm2(&n, &k);
        assert_eq!(gm2.tones.iter().filter(|t| !t.is_empty()).count(), 128);
        assert!(gm2.tones[0].len() >= 3, "{:?}", gm2.tones[0]);
        assert_eq!(gm2.drums.len(), 9);
    }
    let Some(exe) = rd("C:/Program Files (x86)/Midifile Optimizer 6 Pro DEMO/MidifileOptimizer.exe") else { return };
    for map in 1..=3 {
        let gs = soundset::gs_from_optimizer(&exe, map).expect("GS map");
        let vars: usize = gs.tones.iter().map(|t| t.len()).sum();
        eprintln!("GS map {map}: {vars} sounds, {} kits; 0 = {:?}", gs.drums.len(), gs.tones[0]);
        assert_eq!(gs.tones[0][0].name, "Piano 1");
        assert!(gs.tones[0].iter().any(|s| s.msb == 8));
        assert!(gs.drums.iter().any(|d| d.program == 0));
    }
}

#[test]
fn song_standard_detected() {
    use xgworks_core::soundset::{detect, Standard};
    let data = std::fs::read("tests/fixtures/dream.mid").unwrap();
    let (tracks, _) = xgworks_core::edit_model(&data);
    assert_eq!(detect(&tracks), Some(Standard::Xg));
    assert_eq!(detect(&[]), None);
}

#[test]
fn model_tree_starts_with_the_mu_series() {
    let Ok(ini) = std::fs::read(format!("{DIR}/xgeditor.ini")) else { return };
    let tree = parse_model_tree(&ini);
    assert_eq!(tree[0].0, "MU Series");
    assert_eq!(&tree[0].1[..3], &["MU2000".to_string(), "MU1000".to_string(), "MU128Ver2".to_string()]);
    assert!(tree.iter().any(|g| g.0 == "Others" && g.1.contains(&"QY100".to_string())));
}

#[test]
fn sound_sets_from_midi_file_editor_and_sc8850() {
    use xgworks_core::soundset;
    let Ok(html) = std::fs::read_to_string("C:/Users/lagya/AppData/Local/Temp/me/midi-file-editor.html") else { return };
    let sx = soundset::sx900_from_mfe(&html).expect("SX900");
    let n: usize = sx.tones.iter().map(|t| t.len()).sum();
    eprintln!("PSR-SX900: {n} voices, {} kits; prog 0 = {:?}", sx.drums.len(), sx.tones[0].first());
    assert!(n > 1000 && !sx.drums.is_empty());
    let sc = soundset::sc8850_from_mfe(&html).expect("SC-8850");
    assert!(sc.tones[0].len() > 3, "{:?}", sc.tones[0]);
    if let Ok(exe) = std::fs::read("C:/Program Files (x86)/Midifile Optimizer 6 Pro DEMO/MidifileOptimizer.exe") {
        let g = soundset::gs_from_optimizer(&exe, 4).expect("SC-8850 map from Optimizer");
        let v: usize = g.tones.iter().map(|t| t.len()).sum();
        eprintln!("SC-8850 (Optimizer): {v} voices, {} kits", g.drums.len());
        assert!(v > 900);
    }
}

#[test]
fn vh_types_from_midi_file_editor() {
    let Ok(html) = std::fs::read_to_string("F:/rea/XG4/midi-file-editor.html") else { return };
    let rows = xgworks_core::soundset::js_rows(&html, "VH_TYPES").expect("VH_TYPES");
    assert!(rows.len() > 50, "{}", rows.len());
    assert_eq!(rows[0][0].str(), Some("StandardDuet"));
}

#[test]
fn vh_real_psr_sx900_file() {
    use xgworks_core::vh;
    let Ok(b) = std::fs::read("F:/rea/0764SX900.MID") else { return };
    let (mut tracks, ppqn) = xgworks_core::edit_model(&b);
    let tm = xgworks_core::listev::TimeMap::new(&tracks, ppqn);
    // what the file has
    let f = vh::analyze(&tracks);
    assert!(f.has_any);
    assert_eq!(f.channel, Some(15));
    assert_eq!((f.settings.type_msb, f.settings.type_lsb, f.settings.lead_dry, f.settings.mic_reverb), (12, 1, 125, 18));
    let c = vh::check_channel(&tracks, 15);
    assert_eq!(c.notes, 78);
    assert!(c.vel_ok() && c.vol_ok());
    // the block written again: in place, the same bytes
    let before: Vec<Vec<u8>> = tracks[0].others.iter().filter_map(|e| match e { xgworks_core::Event::Sysex { data, .. } if data.len() > 3 && (data[..3] == [0x43, 0x73, 0x01] || data[3] == 0x04 || data[3] == 0x10) => Some(data.clone()), _ => None }).collect();
    let n_before = tracks[0].others.len();
    let (n, _base, replaced) = vh::insert_sysex(&mut tracks, &tm, &f.settings, 15);
    assert_eq!((n, replaced), (12, 12));
    assert_eq!(tracks[0].others.len(), n_before);
    let after: Vec<Vec<u8>> = tracks[0].others.iter().filter_map(|e| match e { xgworks_core::Event::Sysex { data, .. } if data.len() > 3 && (data[..3] == [0x43, 0x73, 0x01] || data[3] == 0x04 || data[3] == 0x10) => Some(data.clone()), _ => None }).collect();
    let (mut b1, mut a1) = (before.clone(), after.clone());
    b1.sort();
    a1.sort();
    assert_eq!(b1, a1, "the same messages");
    // fill: the song's harmony notes stay, nothing doubled (format 0: notes as events)
    let ntracks = tracks.len();
    let xf = vh::xf_chords(&tracks);
    assert!(xf.len() > 30, "the song's XF chords");
    // chords found in the notes mostly agree with the song's XF chords
    let found = vh::detect_chords(&tracks, &tm, 15);
    let at = |t: u32| xf.iter().rev().find(|c| c.0 <= t).and_then(|c| c.1.clone());
    let agree = found
        .iter()
        .filter(|&&(t, r, k)| at(t).is_some_and(|pcs| vh::DETECT_KINDS[k].1.iter().take(3).all(|i| pcs.contains(&((r + i) % 12)))))
        .count();
    assert!(agree * 10 >= found.len() * 6, "{agree} of {} detected chords agree", found.len());
    let (_, skipped) = vh::generate(&mut tracks, &tm, &xf, 15, &[], vh::GenMode::Fill, 10, 4, 64);
    assert_eq!(tracks.len(), ntracks, "no new track for a format-0 song");
    assert!(skipped > 0);
    let c2 = vh::check_channel(&tracks, 15);
    assert!(c2.notes >= 78);
    // replace in bars 1-60: the old notes there go, new ones come from the chords
    let (made, _) = vh::generate(&mut tracks, &tm, &found.iter().map(|&(t, r, k)| (t, Some(vh::DETECT_KINDS[k].1.iter().map(|i| (r + i) % 12).collect()))).collect::<Vec<_>>(), 15, &[(1, 60)], vh::GenMode::Replace, 10, 4, 64);
    assert!(made > 0);
    assert!(vh::check_channel(&tracks, 15).vel_ok());
}
