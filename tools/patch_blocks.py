p = r'F:\rea\xgworks-core\src\lib.rs'
s = open(p, encoding='utf-8').read()


def rep(a, b):
    global s
    assert s.count(a) == 1, a[:70]
    s = s.replace(a, b)


rep('''#[derive(Clone, Debug)]
pub struct EditTrack {
    pub name: String,
    pub channel: u8,
    pub notes: Vec<EditNote>,
    /// Non-note events (CC / program / pitch / SysEx / meta) preserved in ticks.
    pub others: Vec<Event>,
}''', '''/// A block (XGworks "part") placed on a track: name and tick span. Events stay
/// merged on the track (absolute ticks); on save each event goes back to the
/// last block starting at or before it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EditBlock {
    pub name: String,
    pub start: u32,
    pub end: u32,
    /// The 32-byte placement as loaded (unknown bytes are kept on save).
    pub raw: Vec<u8>,
}

#[derive(Clone, Debug, Default)]
pub struct EditTrack {
    pub name: String,
    pub channel: u8,
    pub notes: Vec<EditNote>,
    /// Non-note events (CC / program / pitch / SysEx / meta) preserved in ticks.
    pub others: Vec<Event>,
    /// Blocks in time order (empty = one implicit block spanning the track).
    pub blocks: Vec<EditBlock>,
    /// Raw 0x20-byte S4WT header as loaded (port, transpose, ...), kept on save.
    pub header: Vec<u8>,
    /// XGworks' unnamed first track (tempo / markers / setup); not listed in
    /// the Track View.
    pub conductor: bool,
}

/// Decode a used S4WT record with block names: `[0x20 header][track name]`
/// then `[32-byte placement][block name]*`. Placement = `01 00`, start BE32,
/// end BE32, S4WB index BE32, padding. Returns (name, [(block index, block)]).
pub fn parse_s4wt_blocks(pay: &[u8]) -> Option<(String, Vec<(u32, EditBlock)>)> {
    if pay.len() <= 0x20 || pay[0] == 0 {
        return None;
    }
    let mut i = 0x20usize;
    let name = read_cstring(pay, &mut i);
    let mut out = Vec::new();
    while i + 32 <= pay.len() && pay[i] == 1 {
        let raw = pay[i..i + 32].to_vec();
        let (start, end, idx) = (be_u32(pay, i + 2), be_u32(pay, i + 6), be_u32(pay, i + 10));
        i += 32;
        let bname = if i < pay.len() { read_cstring(pay, &mut i) } else { String::new() };
        out.push((idx, EditBlock { name: bname, start, end, raw }));
    }
    Some((name, out))
}''')

rep('''pub fn edit_model(data: &[u8]) -> (Vec<EditTrack>, u16) {
    let (tracks, ppqn) = song_tracks(data);''', '''pub fn edit_model(data: &[u8]) -> (Vec<EditTrack>, u16) {
    if !(data.len() >= 4 && &data[0..4] == b"MThd") {
        return edit_model_xws(data);
    }
    let (tracks, ppqn) = song_tracks(data);''')

rep('''        out.push(EditTrack { name, channel: ch, notes, others });
    }
    (out, ppqn)
}''', '''        out.push(EditTrack { name, channel: ch, notes, others, ..Default::default() });
    }
    (out, ppqn)
}

fn edit_model_xws(data: &[u8]) -> (Vec<EditTrack>, u16) {
    let mut ppqn = 480u16;
    let mut blocks: Vec<&[u8]> = Vec::new();
    let mut s4wt: Vec<&[u8]> = Vec::new();
    for c in iter_chunks(data) {
        match &c.tag {
            b"S4WH" => ppqn = parse_s4wh(c.payload).ppqn.max(1),
            b"S4WB" => blocks.push(c.payload),
            b"S4WT" => s4wt.push(c.payload),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for (k, pay) in s4wt.iter().enumerate() {
        let Some((name, pls)) = parse_s4wt_blocks(pay) else { continue };
        let mut t = EditTrack {
            name: name.clone(),
            channel: pay[2] & 0x0F,
            header: pay[..0x20].to_vec(),
            conductor: k == 0 && name.is_empty(),
            ..Default::default()
        };
        for (idx, b) in pls {
            if let Some(buf) = blocks.get(idx as usize) {
                for e in parse_s4wb(buf) {
                    match e.shifted(b.start) {
                        Event::Note { t: tk, note, vel, gate, .. } => {
                            t.notes.push(EditNote { tick: tk, dur: gate, pitch: note, vel })
                        }
                        other => t.others.push(other),
                    }
                }
            }
            t.blocks.push(b);
        }
        t.blocks.sort_by_key(|b| b.start);
        out.push(t);
    }
    (out, ppqn)
}''')

a = s.index('/// Serialize an edited model back to an `.XWS`')
b = s.index('/// Serialize an edited model to a Standard MIDI File')
s = s[:a] + '''/// Serialize an edited model back to an `.XWS`: one S4WB per block, one S4WT
/// per track (original header and placement bytes kept), pool padded to 101.
pub fn write_xws(tracks: &[EditTrack], ppqn: u16) -> Vec<u8> {
    let mut blocks: Vec<Vec<u8>> = Vec::new();
    let mut s4wt: Vec<Vec<u8>> = Vec::new();
    for (k, t) in tracks.iter().enumerate() {
        let name = if t.name.is_empty() && !t.conductor { format!("Track{}", k) } else { t.name.clone() };
        let spans: Vec<EditBlock> = if t.blocks.is_empty() {
            vec![EditBlock { name: name.clone(), start: 0, end: edit_track_end(t), raw: Vec::new() }]
        } else {
            t.blocks.clone()
        };
        // each event -> the last block starting at or before it (else the first)
        let mut per: Vec<Vec<Event>> = vec![Vec::new(); spans.len()];
        for e in edit_track_events(t) {
            let et = e.t();
            let bi = spans.iter().rposition(|b| b.start <= et).unwrap_or(0);
            let rel = 0u32.wrapping_sub(spans[bi].start.min(et));
            per[bi].push(e.shifted(rel));
        }
        let mut rec = if t.header.len() == 0x20 {
            t.header.clone()
        } else {
            let mut h = vec![0u8; 0x20];
            h[6] = 1;
            h
        };
        rec[0] = 1;
        rec[2] = t.channel & 0x0F;
        rec.extend(mfc_cstring(&name));
        for (b, evs) in spans.iter().zip(per.iter()) {
            let idx = blocks.len() as u32;
            let last = evs
                .iter()
                .map(|e| match e {
                    Event::Note { t, gate, .. } => t.wrapping_add(*gate),
                    other => other.t(),
                })
                .max()
                .unwrap_or(0);
            let end = b.end.max(b.start.wrapping_add(last));
            blocks.push(encode_s4wb(evs));
            let mut pl = if b.raw.len() == 32 { b.raw.clone() } else { vec![0u8; 32] };
            pl[0] = 1;
            pl[1] = 0;
            pl[2..6].copy_from_slice(&b.start.to_be_bytes());
            pl[6..10].copy_from_slice(&end.to_be_bytes());
            pl[10..14].copy_from_slice(&idx.to_be_bytes());
            rec.extend(pl);
            rec.extend(mfc_cstring(&b.name));
        }
        s4wt.push(rec);
    }
    while s4wt.len() < TRACK_POOL {
        s4wt.push(build_s4wt("", 0, 0, 0, false));
    }
    let mut out = Vec::new();
    push_chunk(&mut out, b"S4WH", &build_s4wh(blocks.len() as u32, ppqn));
    for b in &blocks {
        push_chunk(&mut out, b"S4WB", b);
    }
    for s in &s4wt {
        push_chunk(&mut out, b"S4WT", s);
    }
    push_chunk(&mut out, b"S4ST", &vec![0u8; 138]);
    out
}

''' + s[b:]
open(p, 'w', encoding='utf-8').write(s)

for p, a, b in [
    (r'F:\rea\xgworks-core\src\mixer.rs', 'others,\n        }', 'others,\n            ..Default::default()\n        }'),
    (r'F:\rea\xgworks-core\src\xgtbl.rs', 'EditTrack { name: "t".into(), channel: 0, notes: vec![], others: vec![] }',
     'EditTrack { name: "t".into(), ..Default::default() }'),
]:
    t = open(p, encoding='utf-8').read()
    assert t.count(a) == 1, (p, t.count(a))
    open(p, 'w', encoding='utf-8').write(t.replace(a, b))
print('ok')
