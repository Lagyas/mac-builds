//! C ABI of HS2 – Hypersynth 2 for the Swift / AUv3 front end.
//! The header is `include/hs2.h`; keep both in sync (tests/abi.rs checks the
//! function list).
//!
//! Threads: `hs2_process` runs on the audio thread and takes the synth lock
//! once per buffer. Everything heavy (reading banks, generating wavetables,
//! parsing state) happens outside the lock on the caller's thread; MIDI
//! program changes are fulfilled by a loader thread of the engine.

use hs2_core::library::{Bank, Library};
use hs2_core::synth::PARTS;
use hs2_core::{factory, sf2, state, Patch, Synth};
use std::ffi::{c_char, CStr};
use std::path::Path;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

pub mod version;

/// Parameter addresses (also the AU parameter addresses).
pub const PARAM_MASTER: u32 = 0;
pub const PARAM_HYPE: u32 = 1;
pub const PARAM_TUNE: u32 = 2;

pub const EVENT_MIDI: u8 = 0;
pub const EVENT_PARAM: u8 = 1;

/// One event inside a `hs2_process` buffer, sorted by `offset`.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct HS2Event {
    /// Frame offset inside the buffer.
    pub offset: u32,
    /// EVENT_PARAM: parameter address.
    pub param: u32,
    /// EVENT_PARAM: new value.
    pub value: f32,
    /// EVENT_MIDI or EVENT_PARAM.
    pub kind: u8,
    /// EVENT_MIDI: bytes used in `data` (1..3).
    pub len: u8,
    pub data: [u8; 3],
}

/// Host transport for the arpeggiator / phrase sync.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct HS2Transport {
    pub tempo: f64,
    /// Song position in beats (quarter notes).
    pub beat: f64,
    pub playing: i32,
    /// 0 = unknown (keep the synth's own tempo / state).
    pub valid: i32,
}

enum Job {
    Program { part: usize, bank: u32, prog: u32 },
}

pub struct Engine {
    synth: Arc<Mutex<Synth>>,
    lib: Arc<RwLock<Library>>,
    /// (bank index, patch index) each part was loaded from, for the UI.
    part_src: Arc<Mutex<[Option<(usize, usize)>; PARTS]>>,
    loader: Sender<Job>,
    status: Mutex<String>,
}

fn base_library() -> Library {
    let mut lib = Library::default();
    for (name, patches) in factory::banks() {
        let n = lib.banks.len() as u32 + 90;
        lib.banks.push(Bank { number: n, name: name.into(), user: false, patches: patches.into_iter().map(Arc::new).collect() });
    }
    lib
}

/// Generate the wavetables `p` needs (outside the synth lock).
fn prepare(wt: &hs2_core::wavetable::Bank, p: &Patch) {
    for t in Synth::tables_for(p) {
        wt.get(t);
    }
}

impl Engine {
    pub fn new(sr: f32) -> Engine {
        let synth = Arc::new(Mutex::new(Synth::new(sr)));
        let lib = Arc::new(RwLock::new(base_library()));
        let part_src = Arc::new(Mutex::new([None; PARTS]));
        let (tx, rx) = channel::<Job>();
        {
            let (synth, lib, part_src) = (synth.clone(), lib.clone(), part_src.clone());
            std::thread::Builder::new()
                .name("hs2-loader".into())
                .spawn(move || {
                    // ends when the engine (the only sender) is dropped
                    while let Ok(job) = rx.recv() {
                        match job {
                            Job::Program { part, bank, prog } => {
                                let lib = lib.read().unwrap();
                                let Some(bi) = lib.banks.iter().position(|b| b.number == bank) else { continue };
                                let Some(p) = lib.banks[bi].patches.get(prog as usize).cloned() else { continue };
                                drop(lib);
                                let wt = synth.lock().unwrap().wt.clone();
                                prepare(&wt, &p);
                                synth.lock().unwrap().set_patch(part, Some(p));
                                part_src.lock().unwrap()[part] = Some((bi, prog as usize));
                            }
                        }
                    }
                })
                .expect("loader thread");
        }
        let e = Engine { synth, lib, part_src, loader: tx, status: Mutex::new(String::new()) };
        e.load_patch(0, 0, 0);
        e
    }

    fn wt(&self) -> Arc<hs2_core::wavetable::Bank> {
        self.synth.lock().unwrap().wt.clone()
    }

    pub fn set_sample_rate(&self, sr: f32) {
        let mut s = self.synth.lock().unwrap();
        if (s.sr - sr).abs() < 0.5 {
            return;
        }
        let saved = state::save(&s, "rate");
        let mut n = Synth::new(sr);
        n.wt = s.wt.clone();
        n.sf = s.sf.clone();
        n.sample_map = s.sample_map.clone();
        n.phrases = s.phrases.clone();
        state::load(&mut n, &saved);
        *s = n;
    }

    /// Render `l`/`r` applying `events` (sorted by offset) at their frames.
    pub fn process(&self, l: &mut [f32], r: &mut [f32], events: &[HS2Event], transport: Option<&HS2Transport>) {
        let n = l.len().min(r.len());
        let mut s = self.synth.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(t) = transport.filter(|t| t.valid != 0) {
            if t.tempo > 0.0 {
                s.tempo = t.tempo as f32;
            }
            let playing = t.playing != 0;
            if playing && (!s.playing || (s.song_beats - t.beat).abs() > 0.25) {
                s.song_beats = t.beat; // follow host locates
            }
            s.playing = playing;
        }
        let mut done = 0usize;
        for ev in events {
            let at = (ev.offset as usize).clamp(done, n);
            if at > done {
                s.render(&mut l[done..at], &mut r[done..at]);
                done = at;
            }
            match ev.kind {
                EVENT_MIDI => {
                    let len = (ev.len as usize).clamp(1, 3);
                    s.midi(&ev.data[..len]);
                }
                EVENT_PARAM => set_param(&mut s, ev.param, ev.value),
                _ => {}
            }
        }
        if done < n {
            s.render(&mut l[done..n], &mut r[done..n]);
        }
        for (ch, bank, prog) in std::mem::take(&mut s.program_requests) {
            let _ = self.loader.send(Job::Program { part: ch, bank: bank as u32, prog: prog as u32 });
        }
    }

    pub fn midi(&self, msg: &[u8]) {
        let reqs = {
            let mut s = self.synth.lock().unwrap();
            s.midi(msg);
            std::mem::take(&mut s.program_requests)
        };
        for (ch, bank, prog) in reqs {
            let _ = self.loader.send(Job::Program { part: ch, bank: bank as u32, prog: prog as u32 });
        }
    }

    pub fn load_patch(&self, part: usize, bank_index: usize, idx: usize) -> bool {
        if part >= PARTS {
            return false;
        }
        let Some(p) = self.lib.read().unwrap().banks.get(bank_index).and_then(|b| b.patches.get(idx)).cloned() else { return false };
        prepare(&self.wt(), &p);
        self.synth.lock().unwrap().set_patch(part, Some(p));
        self.part_src.lock().unwrap()[part] = Some((bank_index, idx));
        true
    }

    pub fn load_combi(&self, idx: usize) -> bool {
        let Some(c) = self.lib.read().unwrap().combis.get(idx).cloned() else { return false };
        let wt = self.wt();
        for cp in &c.parts {
            prepare(&wt, &cp.patch);
        }
        self.synth.lock().unwrap().load_combi(&c);
        *self.part_src.lock().unwrap() = [None; PARTS];
        true
    }

    /// Read a Hypersonic 2 content folder (patch / phrase / tuning / sample
    /// name banks, combis .fxb, user patches .hsp). Returns patch banks added.
    pub fn load_content_dir(&self, dir: &str) -> usize {
        let files = read_tree(dir);
        let mut lib = self.lib.write().unwrap();
        let mut banks = 0;
        for (name, data) in &files {
            let base = name.rsplit('/').next().unwrap_or(name);
            if let Some(hs2_core::bank::BankKind::Patch) = lib.add_file(base, data) {
                banks += 1;
            }
        }
        for (name, data) in &files {
            if name.to_lowercase().ends_with(".fxb") {
                lib.add_combi(data);
            }
        }
        lib.combis.sort_by(|a, b| a.name.cmp(&b.name));
        let mut user: std::collections::BTreeMap<String, Vec<Patch>> = Default::default();
        for (name, data) in &files {
            if name.to_lowercase().ends_with(".hsp") {
                let folder = name.rsplit('/').nth(1).unwrap_or("User").to_string();
                if let Some(p) = Patch::parse(data) {
                    user.entry(folder).or_default().push(p);
                }
            }
        }
        for (f, ps) in user {
            lib.add_user_bank(&f, ps);
        }
        let phrases = lib.phrases.clone();
        let nph: usize = phrases.iter().map(|b| b.1.len()).sum();
        *self.status.lock().unwrap() = format!(
            "Content: {} patches in {} banks, {} phrases, {} combis",
            lib.total_patches(),
            lib.banks.len(),
            nph,
            lib.combis.len()
        );
        drop(lib);
        self.synth.lock().unwrap().phrases = phrases;
        *self.part_src.lock().unwrap() = [None; PARTS];
        banks
    }

    /// SoundFont (.sf2) or WAV used by Sample elements. Returns presets, -1 on error.
    pub fn load_soundfont(&self, path: &str) -> i32 {
        let Ok(data) = std::fs::read(path) else { return -1 };
        let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
        let sf = if name.to_lowercase().ends_with(".wav") {
            sf2::parse_wav(name, &data, 60).map(|p| sf2::SoundFont { name: name.into(), presets: vec![p] })
        } else {
            sf2::parse(&data)
        };
        match sf {
            Some(sf) => {
                let n = sf.presets.len() as i32;
                self.synth.lock().unwrap().sf = Some(Arc::new(sf));
                *self.status.lock().unwrap() = format!("SoundFont {name}: {n} presets");
                n
            }
            None => -1,
        }
    }

    pub fn save_state(&self) -> Vec<u8> {
        state::save(&self.synth.lock().unwrap(), "AU")
    }

    pub fn load_state(&self, d: &[u8]) -> bool {
        // parse once off the lock to generate the wavetables, then for real
        let sr = self.synth.lock().unwrap().sr;
        let mut tmp = Synth::new(sr);
        tmp.wt = self.wt();
        if state::load(&mut tmp, d).is_none() {
            return false;
        }
        for p in tmp.parts.iter().filter_map(|p| p.patch.clone()) {
            prepare(&tmp.wt, &p);
        }
        let ok = state::load(&mut self.synth.lock().unwrap(), d).is_some();
        *self.part_src.lock().unwrap() = [None; PARTS];
        ok
    }
}

fn set_param(s: &mut Synth, id: u32, v: f32) {
    match id {
        PARAM_MASTER => s.master = v.clamp(0.0, 1.0),
        PARAM_HYPE => s.hype = v.clamp(0.0, 1.0),
        PARAM_TUNE => s.master_tune = v.clamp(-1.0, 1.0),
        _ => {}
    }
}

fn get_param(s: &Synth, id: u32) -> f32 {
    match id {
        PARAM_MASTER => s.master,
        PARAM_HYPE => s.hype,
        PARAM_TUNE => s.master_tune,
        _ => 0.0,
    }
}

const AUDIO_DIRS: [&str; 3] = ["Samples/", "Wavetables/", "Drawbars/"];
const NAMES_ONLY: usize = 256 * 1024;
const MAX_FILE: u64 = 64 * 1024 * 1024;

/// All files under `dir` (relative path with '/', bytes). The protected
/// audio banks are read for their names only (first 256 KB), never decoded.
fn read_tree(dir: &str) -> Vec<(String, Vec<u8>)> {
    let mut out = Vec::new();
    walk(Path::new(dir), Path::new(dir), &mut |rel, p| {
        let data = if AUDIO_DIRS.iter().any(|k| rel.contains(k)) {
            head(p)
        } else {
            match std::fs::metadata(p) {
                Ok(m) if m.len() < MAX_FILE => std::fs::read(p).unwrap_or_default(),
                _ => return,
            }
        };
        out.push((rel.to_string(), data));
    });
    out
}

fn head(p: &Path) -> Vec<u8> {
    use std::io::Read;
    let mut b = vec![0u8; NAMES_ONLY];
    let mut n = 0;
    if let Ok(mut f) = std::fs::File::open(p) {
        while n < b.len() {
            match f.read(&mut b[n..]) {
                Ok(0) | Err(_) => break,
                Ok(k) => n += k,
            }
        }
    }
    b.truncate(n);
    b
}

fn walk(base: &Path, d: &Path, f: &mut dyn FnMut(&str, &Path)) {
    let Ok(rd) = std::fs::read_dir(d) else { return };
    let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            walk(base, &p, f);
        } else {
            let rel = p.strip_prefix(base).unwrap_or(&p).to_string_lossy().replace('\\', "/");
            f(&rel, &p);
        }
    }
}

/// Copy what the synth reads from a content folder into `dst` (the shared
/// app-group folder of the app and the Audio Unit): bank files whole, the
/// protected audio banks only as their name headers. Returns files copied.
pub fn import_content(src: &str, dst: &str) -> std::io::Result<usize> {
    let dst = Path::new(dst);
    let mut n = 0;
    let mut err = None;
    walk(Path::new(src), Path::new(src), &mut |rel, p| {
        let data = if AUDIO_DIRS.iter().any(|k| rel.contains(k)) {
            head(p)
        } else {
            match std::fs::metadata(p) {
                Ok(m) if m.len() < MAX_FILE => match std::fs::read(p) {
                    Ok(d) => d,
                    Err(_) => return,
                },
                _ => return,
            }
        };
        let to = dst.join(rel);
        let r = to.parent().map(std::fs::create_dir_all).unwrap_or(Ok(())).and_then(|_| std::fs::write(&to, &data));
        match r {
            Ok(_) => n += 1,
            Err(e) => err = Some(e),
        }
    });
    match err {
        Some(e) if n == 0 => Err(e),
        _ => Ok(n),
    }
}

// ---------------------------------------------------------------------------
// C ABI
// ---------------------------------------------------------------------------

/// Copy `s` NUL-terminated into `buf` (capacity `cap`); returns the full
/// UTF-8 length so the caller can retry with a bigger buffer.
unsafe fn put_str(s: &str, buf: *mut c_char, cap: u32) -> u32 {
    if !buf.is_null() && cap > 0 {
        let mut k = s.len().min(cap as usize - 1);
        while !s.is_char_boundary(k) {
            k -= 1;
        }
        std::ptr::copy_nonoverlapping(s.as_ptr(), buf as *mut u8, k);
        *buf.add(k) = 0;
    }
    s.len() as u32
}

unsafe fn cstr<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        None
    } else {
        CStr::from_ptr(p).to_str().ok()
    }
}

unsafe fn eng<'a>(e: *const Engine) -> Option<&'a Engine> {
    e.as_ref()
}

/// Version text of the engine (static, NUL-terminated).
#[no_mangle]
pub extern "C" fn hs2_version() -> *const c_char {
    static V: OnceLock<std::ffi::CString> = OnceLock::new();
    V.get_or_init(|| std::ffi::CString::new(version::about()).unwrap()).as_ptr()
}

#[no_mangle]
pub extern "C" fn hs2_engine_new(sample_rate: f32) -> *mut Engine {
    Box::into_raw(Box::new(Engine::new(if sample_rate > 0.0 { sample_rate } else { 48000.0 })))
}

/// # Safety
/// `e` from `hs2_engine_new`, not used afterwards.
#[no_mangle]
pub unsafe extern "C" fn hs2_engine_free(e: *mut Engine) {
    if !e.is_null() {
        drop(Box::from_raw(e));
    }
}

/// # Safety
/// `e` from `hs2_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn hs2_set_sample_rate(e: *const Engine, sample_rate: f32) {
    if let Some(e) = eng(e) {
        if sample_rate > 0.0 {
            e.set_sample_rate(sample_rate);
        }
    }
}

/// Render `frames` frames into `left`/`right`, applying `events` (sorted by
/// offset). `transport` may be NULL.
///
/// # Safety
/// `left`/`right` hold `frames` floats; `events` holds `n_events` events.
#[no_mangle]
pub unsafe extern "C" fn hs2_process(
    e: *const Engine,
    left: *mut f32,
    right: *mut f32,
    frames: u32,
    events: *const HS2Event,
    n_events: u32,
    transport: *const HS2Transport,
) {
    if left.is_null() || right.is_null() {
        return;
    }
    let l = std::slice::from_raw_parts_mut(left, frames as usize);
    let r = std::slice::from_raw_parts_mut(right, frames as usize);
    let Some(e) = eng(e) else {
        l.fill(0.0);
        r.fill(0.0);
        return;
    };
    let ev = if events.is_null() { &[][..] } else { std::slice::from_raw_parts(events, n_events as usize) };
    e.process(l, r, ev, transport.as_ref());
}

/// MIDI from the UI (on-screen keyboard); not for the audio thread.
///
/// # Safety
/// `data` holds `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn hs2_midi(e: *const Engine, data: *const u8, len: u32) {
    if let (Some(e), false) = (eng(e), data.is_null()) {
        e.midi(std::slice::from_raw_parts(data, len as usize));
    }
}

/// # Safety
/// `e` from `hs2_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn hs2_panic(e: *const Engine) {
    if let Some(e) = eng(e) {
        e.synth.lock().unwrap().panic();
    }
}

/// # Safety
/// `path` NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn hs2_load_content_dir(e: *const Engine, path: *const c_char) -> i32 {
    match (eng(e), cstr(path)) {
        (Some(e), Some(p)) => e.load_content_dir(p) as i32,
        _ => -1,
    }
}

/// Copy a content folder into the shared folder (see `import_content`).
/// Returns files copied, -1 on error.
///
/// # Safety
/// Paths NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn hs2_import_content(src: *const c_char, dst: *const c_char) -> i32 {
    match (cstr(src), cstr(dst)) {
        (Some(s), Some(d)) => import_content(s, d).map(|n| n as i32).unwrap_or(-1),
        _ => -1,
    }
}

/// # Safety
/// `path` NUL-terminated UTF-8.
#[no_mangle]
pub unsafe extern "C" fn hs2_load_soundfont(e: *const Engine, path: *const c_char) -> i32 {
    match (eng(e), cstr(path)) {
        (Some(e), Some(p)) => e.load_soundfont(p),
        _ => -1,
    }
}

/// # Safety
/// `e` from `hs2_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn hs2_bank_count(e: *const Engine) -> u32 {
    eng(e).map(|e| e.lib.read().unwrap().banks.len() as u32).unwrap_or(0)
}

/// Bank name ("016 Acoustic Basses" style: number and name). Returns the full length.
///
/// # Safety
/// `buf` holds `cap` bytes.
#[no_mangle]
pub unsafe extern "C" fn hs2_bank_name(e: *const Engine, bank: u32, buf: *mut c_char, cap: u32) -> u32 {
    let Some(e) = eng(e) else { return 0 };
    let lib = e.lib.read().unwrap();
    let s = lib.banks.get(bank as usize).map(|b| format!("{:03} {}", b.number, b.name)).unwrap_or_default();
    put_str(&s, buf, cap)
}

/// # Safety
/// `e` from `hs2_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn hs2_patch_count(e: *const Engine, bank: u32) -> u32 {
    eng(e).and_then(|e| e.lib.read().unwrap().banks.get(bank as usize).map(|b| b.patches.len() as u32)).unwrap_or(0)
}

/// # Safety
/// `buf` holds `cap` bytes.
#[no_mangle]
pub unsafe extern "C" fn hs2_patch_name(e: *const Engine, bank: u32, patch: u32, buf: *mut c_char, cap: u32) -> u32 {
    let Some(e) = eng(e) else { return 0 };
    let lib = e.lib.read().unwrap();
    let s = lib.banks.get(bank as usize).and_then(|b| b.patches.get(patch as usize)).map(|p| p.display_name().to_string()).unwrap_or_default();
    put_str(&s, buf, cap)
}

/// Load patch `patch` of bank index `bank` into `part` (0..15). Returns 1 on success.
///
/// # Safety
/// `e` from `hs2_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn hs2_load_patch(e: *const Engine, part: u32, bank: u32, patch: u32) -> i32 {
    eng(e).map(|e| e.load_patch(part as usize, bank as usize, patch as usize) as i32).unwrap_or(0)
}

/// Name of the patch on `part` (empty when none).
///
/// # Safety
/// `buf` holds `cap` bytes.
#[no_mangle]
pub unsafe extern "C" fn hs2_part_patch_name(e: *const Engine, part: u32, buf: *mut c_char, cap: u32) -> u32 {
    let Some(e) = eng(e) else { return 0 };
    let s = e.synth.lock().unwrap().parts.get(part as usize).and_then(|p| p.patch.clone()).map(|p| p.display_name().to_string()).unwrap_or_default();
    put_str(&s, buf, cap)
}

/// Bank / patch index `part` was loaded from; 0 when unknown (combi, state, MIDI bank).
///
/// # Safety
/// `bank` / `patch` writable.
#[no_mangle]
pub unsafe extern "C" fn hs2_part_source(e: *const Engine, part: u32, bank: *mut u32, patch: *mut u32) -> i32 {
    let Some(e) = eng(e) else { return 0 };
    match e.part_src.lock().unwrap().get(part as usize).copied().flatten() {
        Some((b, p)) if !bank.is_null() && !patch.is_null() => {
            *bank = b as u32;
            *patch = p as u32;
            1
        }
        _ => 0,
    }
}

/// # Safety
/// `e` from `hs2_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn hs2_combi_count(e: *const Engine) -> u32 {
    eng(e).map(|e| e.lib.read().unwrap().combis.len() as u32).unwrap_or(0)
}

/// # Safety
/// `buf` holds `cap` bytes.
#[no_mangle]
pub unsafe extern "C" fn hs2_combi_name(e: *const Engine, idx: u32, buf: *mut c_char, cap: u32) -> u32 {
    let Some(e) = eng(e) else { return 0 };
    let s = e.lib.read().unwrap().combis.get(idx as usize).map(|c| c.name.clone()).unwrap_or_default();
    put_str(&s, buf, cap)
}

/// # Safety
/// `e` from `hs2_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn hs2_load_combi(e: *const Engine, idx: u32) -> i32 {
    eng(e).map(|e| e.load_combi(idx as usize) as i32).unwrap_or(0)
}

/// Set a parameter from the UI thread (the audio thread sends EVENT_PARAM).
///
/// # Safety
/// `e` from `hs2_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn hs2_set_param(e: *const Engine, id: u32, value: f32) {
    if let Some(e) = eng(e) {
        set_param(&mut e.synth.lock().unwrap(), id, value);
    }
}

/// # Safety
/// `e` from `hs2_engine_new`.
#[no_mangle]
pub unsafe extern "C" fn hs2_get_param(e: *const Engine, id: u32) -> f32 {
    eng(e).map(|e| get_param(&e.synth.lock().unwrap(), id)).unwrap_or(0.0)
}

/// Output peak levels (0..1+) and the number of sounding voices.
///
/// # Safety
/// Pointers writable (may be NULL).
#[no_mangle]
pub unsafe extern "C" fn hs2_meter(e: *const Engine, left: *mut f32, right: *mut f32, voices: *mut u32) {
    let Some(e) = eng(e) else { return };
    let s = e.synth.lock().unwrap();
    if !left.is_null() {
        *left = s.peak[0];
    }
    if !right.is_null() {
        *right = s.peak[1];
    }
    if !voices.is_null() {
        *voices = s.total_polyphony() as u32;
    }
}

/// Status line of the last content / SoundFont load.
///
/// # Safety
/// `buf` holds `cap` bytes.
#[no_mangle]
pub unsafe extern "C" fn hs2_status(e: *const Engine, buf: *mut c_char, cap: u32) -> u32 {
    let Some(e) = eng(e) else { return 0 };
    let s = e.status.lock().unwrap().clone();
    put_str(&s, buf, cap)
}

/// Whole setup (parts, patches, FX, master) for the host's preset / song.
/// Free the result with `hs2_bytes_free`.
///
/// # Safety
/// `len` writable.
#[no_mangle]
pub unsafe extern "C" fn hs2_state_save(e: *const Engine, len: *mut u32) -> *mut u8 {
    let Some(e) = eng(e) else { return std::ptr::null_mut() };
    let d = e.save_state().into_boxed_slice();
    if !len.is_null() {
        *len = d.len() as u32;
    }
    Box::into_raw(d) as *mut u8
}

/// # Safety
/// `p`/`len` exactly as returned by `hs2_state_save`.
#[no_mangle]
pub unsafe extern "C" fn hs2_bytes_free(p: *mut u8, len: u32) {
    if !p.is_null() {
        drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(p, len as usize)));
    }
}

/// # Safety
/// `data` holds `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn hs2_state_load(e: *const Engine, data: *const u8, len: u32) -> i32 {
    match (eng(e), data.is_null()) {
        (Some(e), false) => e.load_state(std::slice::from_raw_parts(data, len as usize)) as i32,
        _ => 0,
    }
}
