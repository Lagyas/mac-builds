//! WASM shim for the browser player/editor. No wasm-bindgen: plain C-ABI over
//! linear memory, so it builds with only `cargo build --target wasm32-...`.
//!
//! JS flow:
//!   1. ptr = xgw_alloc(fileBytes.length);  copy file bytes into memory[ptr..].
//!   2. outPtr = xgw_plan(ptr, len);
//!   3. read the packed per-track plan at memory[outPtr..] (see xgw_plan docs).

use xgworks_core::track_plan;

/// Allocate `len` bytes in wasm memory and return the pointer (leaked).
#[no_mangle]
pub extern "C" fn xgw_alloc(len: usize) -> *mut u8 {
    let mut v = vec![0u8; len];
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// Build the per-track plan for the file and return a pointer to a packed blob:
///
/// ```text
/// u32 total_len (incl. this header)
/// u16 track_count
/// per track:  u8 channel, u8 name_len, name_len bytes (UTF-8)
/// u32 note_count
/// per note:   u16 track, f32 t_ms, f32 dur_ms, u8 pitch, u8 vel
/// u32 msg_count
/// per msg:    u16 track, f32 t_ms, u8 len, len raw MIDI bytes
/// ```
///
/// # Safety
/// `ptr` must point to `len` readable bytes from `xgw_alloc`.
#[no_mangle]
pub unsafe extern "C" fn xgw_plan(ptr: *const u8, len: usize) -> *mut u8 {
    let input = std::slice::from_raw_parts(ptr, len);
    let tracks = track_plan(input);

    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(&0u32.to_le_bytes()); // reserve total-length header

    out.extend_from_slice(&(tracks.len() as u16).to_le_bytes());
    for t in &tracks {
        out.push(t.channel);
        let nb = t.name.as_bytes();
        let n = nb.len().min(255);
        out.push(n as u8);
        out.extend_from_slice(&nb[..n]);
    }

    let total_notes: usize = tracks.iter().map(|t| t.notes.len()).sum();
    out.extend_from_slice(&(total_notes as u32).to_le_bytes());
    for (ti, t) in tracks.iter().enumerate() {
        for n in &t.notes {
            out.extend_from_slice(&(ti as u16).to_le_bytes());
            out.extend_from_slice(&(n.t_ms as f32).to_le_bytes());
            out.extend_from_slice(&(n.dur_ms as f32).to_le_bytes());
            out.push(n.pitch);
            out.push(n.vel);
        }
    }

    let total_msgs: usize = tracks.iter().map(|t| t.messages.len()).sum();
    out.extend_from_slice(&(total_msgs as u32).to_le_bytes());
    for (ti, t) in tracks.iter().enumerate() {
        for (tm, bytes) in &t.messages {
            out.extend_from_slice(&(ti as u16).to_le_bytes());
            out.extend_from_slice(&(*tm as f32).to_le_bytes());
            let n = bytes.len().min(255);
            out.push(n as u8);
            out.extend_from_slice(&bytes[..n]);
        }
    }

    let total = out.len() as u32;
    out[0..4].copy_from_slice(&total.to_le_bytes());
    let p = out.as_mut_ptr();
    std::mem::forget(out);
    p
}
