//! The C interface as Swift uses it: header in sync, rendering with events,
//! patches, state round trip, content import.

use hs2_ffi::*;
use std::ffi::{CStr, CString};

fn name(f: impl Fn(*mut std::ffi::c_char, u32) -> u32) -> String {
    let mut b = vec![0 as std::ffi::c_char; 256];
    f(b.as_mut_ptr(), b.len() as u32);
    unsafe { CStr::from_ptr(b.as_ptr()) }.to_string_lossy().into_owned()
}

fn midi(offset: u32, b: [u8; 3]) -> HS2Event {
    HS2Event { offset, kind: EVENT_MIDI, len: 3, data: b, ..Default::default() }
}

fn rms(x: &[f32]) -> f32 {
    (x.iter().map(|v| v * v).sum::<f32>() / x.len() as f32).sqrt()
}

#[test]
fn header_declares_every_exported_function() {
    let h = include_str!("../include/hs2.h");
    let src = include_str!("../src/lib.rs");
    let mut n = 0;
    for line in src.lines() {
        if let Some(rest) = line.split("extern \"C\" fn ").nth(1) {
            let f = rest.split('(').next().unwrap();
            assert!(h.contains(&format!("{f}(")), "{f} missing in hs2.h");
            n += 1;
        }
    }
    assert!(n >= 25, "{n}");
    assert_eq!(std::mem::size_of::<HS2Event>(), 20);
    assert_eq!(std::mem::size_of::<HS2Transport>(), 24);
}

#[test]
fn renders_notes_with_sample_accurate_events() {
    unsafe {
        let e = hs2_engine_new(48000.0);
        assert!(hs2_bank_count(e) > 0);
        assert!(!name(|b, c| hs2_part_patch_name(e, 0, b, c)).is_empty());
        let (mut l, mut r) = (vec![1.0f32; 1024], vec![1.0f32; 1024]);
        // silence before the note at frame 512
        let evs = [midi(512, [0x90, 60, 100])];
        let t = HS2Transport { tempo: 128.0, beat: 0.0, playing: 1, valid: 1 };
        hs2_process(e, l.as_mut_ptr(), r.as_mut_ptr(), 1024, evs.as_ptr(), 1, &t);
        assert!(rms(&l[..512]) < 1e-6, "pre-note {}", rms(&l[..512]));
        let (mut l2, mut r2) = (vec![0.0f32; 8192], vec![0.0f32; 8192]);
        hs2_process(e, l2.as_mut_ptr(), r2.as_mut_ptr(), 8192, std::ptr::null(), 0, &t);
        assert!(rms(&l[512..]) + rms(&l2) > 1e-4, "note {} {}", rms(&l[512..]), rms(&l2));
        // a long buffer (> internal block) and note off
        let (mut l, mut r) = (vec![0.0f32; 4096], vec![0.0f32; 4096]);
        let evs = [midi(0, [0x80, 60, 0])];
        hs2_process(e, l.as_mut_ptr(), r.as_mut_ptr(), 4096, evs.as_ptr(), 1, std::ptr::null());
        let (mut pl, mut pr, mut v) = (0.0, 0.0, 0);
        hs2_meter(e, &mut pl, &mut pr, &mut v);
        assert!(pl >= 0.0 && pr >= 0.0);
        // parameter event
        let p = HS2Event { offset: 0, kind: EVENT_PARAM, param: PARAM_MASTER, value: 0.25, ..Default::default() };
        hs2_process(e, l.as_mut_ptr(), r.as_mut_ptr(), 64, &p, 1, std::ptr::null());
        assert_eq!(hs2_get_param(e, PARAM_MASTER), 0.25);
        hs2_engine_free(e);
    }
}

#[test]
fn patches_state_and_program_change() {
    unsafe {
        let e = hs2_engine_new(44100.0);
        let banks = hs2_bank_count(e);
        let last = banks - 1;
        assert!(hs2_patch_count(e, last) > 0);
        assert_eq!(hs2_load_patch(e, 3, last, 0), 1);
        let want = name(|b, c| hs2_patch_name(e, last, 0, b, c));
        assert_eq!(name(|b, c| hs2_part_patch_name(e, 3, b, c)), want);
        let (mut b, mut p) = (9, 9);
        assert_eq!(hs2_part_source(e, 3, &mut b, &mut p), 1);
        assert_eq!((b, p), (last, 0));
        assert_eq!(hs2_load_patch(e, 99, 0, 0), 0);
        assert_eq!(hs2_load_patch(e, 0, 999, 0), 0);
        hs2_set_param(e, PARAM_HYPE, 0.4);

        let mut len = 0;
        let d = hs2_state_save(e, &mut len);
        assert!(len > 100);
        let state = std::slice::from_raw_parts(d, len as usize).to_vec();
        hs2_bytes_free(d, len);

        let f = hs2_engine_new(48000.0);
        assert_eq!(hs2_state_load(f, state.as_ptr(), state.len() as u32), 1);
        assert_eq!(name(|b, c| hs2_part_patch_name(f, 3, b, c)), want);
        assert!((hs2_get_param(f, PARAM_HYPE) - 0.4).abs() < 1e-6);
        assert_eq!(hs2_state_load(f, b"junk".as_ptr(), 4), 0);

        // sample-rate change keeps the setup
        hs2_set_sample_rate(f, 96000.0);
        assert_eq!(name(|b, c| hs2_part_patch_name(f, 3, b, c)), want);

        // MIDI program change on channel 2 → loader thread loads factory bank 90 prog 1
        let (mut l, mut r) = (vec![0.0f32; 64], vec![0.0f32; 64]);
        let evs = [midi(0, [0xB1, 0, 90]), HS2Event { offset: 0, kind: EVENT_MIDI, len: 2, data: [0xC1, 1, 0], ..Default::default() }];
        hs2_process(f, l.as_mut_ptr(), r.as_mut_ptr(), 64, evs.as_ptr(), 2, std::ptr::null());
        let bank90 = (0..hs2_bank_count(f)).find(|&i| name(|b, c| hs2_bank_name(f, i, b, c)).starts_with("090")).unwrap();
        let want = name(|b, c| hs2_patch_name(f, bank90, 1, b, c));
        let mut ok = false;
        for _ in 0..200 {
            if name(|b, c| hs2_part_patch_name(f, 1, b, c)) == want {
                ok = true;
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(ok, "program change not applied");
        hs2_engine_free(f);
        hs2_engine_free(e);
    }
}

#[test]
fn import_copies_banks_and_only_name_headers_of_audio() {
    let tmp = std::env::temp_dir().join(format!("hs2ffi-{}", std::process::id()));
    let src = tmp.join("src");
    std::fs::create_dir_all(src.join("Samples")).unwrap();
    std::fs::write(src.join("Patches.bin"), vec![7u8; 1000]).unwrap();
    std::fs::write(src.join("Samples/big.bin"), vec![1u8; 600 * 1024]).unwrap();
    let dst = tmp.join("dst");
    let (s, d) = (CString::new(src.to_str().unwrap()).unwrap(), CString::new(dst.to_str().unwrap()).unwrap());
    assert_eq!(unsafe { hs2_import_content(s.as_ptr(), d.as_ptr()) }, 2);
    assert_eq!(std::fs::metadata(dst.join("Patches.bin")).unwrap().len(), 1000);
    assert_eq!(std::fs::metadata(dst.join("Samples/big.bin")).unwrap().len(), 256 * 1024);
    unsafe {
        let e = hs2_engine_new(48000.0);
        assert_eq!(hs2_load_content_dir(e, d.as_ptr()), 0); // no real banks there
        hs2_engine_free(e);
    }
    std::fs::remove_dir_all(&tmp).ok();
    assert!(!unsafe { CStr::from_ptr(hs2_version()) }.to_string_lossy().is_empty());
}
