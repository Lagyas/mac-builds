//! Demix on the real fixtures.
use xgworks_core::{demix, edit_model};

#[test]
fn xws_tracks_are_single_channel() {
    let data = std::fs::read("tests/fixtures/dream.xws").unwrap();
    let (tracks, _) = edit_model(&data);
    let multi: Vec<(String, Vec<u8>)> = tracks
        .iter()
        .filter(|t| !t.conductor)
        .map(|t| (t.name.clone(), demix::channels_used(t)))
        .filter(|(_, c)| c.len() > 1)
        .collect();
    assert!(multi.is_empty(), "{multi:?}");
}

#[test]
fn format0_midi_keeps_note_channels() {
    let mid = std::fs::read("tests/fixtures/dream.mid").unwrap();
    // merge every MTrk into one format-0 track
    let (tracks, ppqn) = edit_model(&mid);
    let notes: usize = tracks.iter().map(|t| t.notes.len()).sum();
    let mut one = xgworks_core::EditTrack { name: "All".into(), ..Default::default() };
    for t in &tracks {
        for n in &t.notes {
            one.others.push(xgworks_core::Event::Note { t: n.tick, ch: t.channel, note: n.pitch, vel: n.vel, gate: n.dur });
        }
        one.others.extend(t.others.iter().cloned());
    }
    let f0 = xgworks_core::write_mid(&[one], ppqn);
    let (loaded, _) = edit_model(&f0);
    assert_eq!(loaded.len(), 1);
    let chans = demix::channels_used(&loaded[0]);
    assert!(chans.len() > 1, "{chans:?}");
    let d = demix::by_channel(&loaded, &[0]).unwrap();
    let (split, n) = (d.tracks, d.count);
    assert_eq!(n, 1);
    let after: usize = split.iter().map(|t| t.notes.len()).sum();
    assert_eq!(after, notes, "every note lands on a channel track");
    assert!(split.iter().skip(1).all(|t| demix::channels_used(t).len() == 1));
}
