# xgworks-core

Portable Rust core for the Yamaha **XGworks `.XWS`** project format and its
conversion to/from Standard MIDI Files. This is **milestone M0** of the
ARM / macOS / web port: one dependency-free `std` crate that compiles unchanged
for native targets *and* for `wasm32` (the web). The platform MIDI I/O and UI
layers sit on top of it.

It is a direct port of the reverse-engineered Python reference (`../xws_dump.py`),
which is validated to round-trip against real files. The same two files are used
as test fixtures here (`tests/fixtures/`).

## What it does

- Parse the `.XWS` chunk container (`S4WH` / `S4WB` / `S4WT` / `S4ST`).
- Decode `S4WB` event streams (SMF-like; notes are NoteOn + 4-byte BE gate).
- Decode `S4WT` track records and their block **placements** (block + start tick).
- `export_named` — `.XWS` → `.MID`, reconstructing the original named tracks.
- `export_blocks` — `.XWS` → `.MID`, one track per `S4WB` block (raw).
- `import_mid` — `.MID` → `.XWS`.

## Build & test

```sh
cargo test                 # runs the round-trip tests against the real fixtures
cargo run --bin xgworks -- dump   "../A Dream of a Dream.XWS"
cargo run --bin xgworks -- mid    "../A Dream of a Dream.XWS" out.mid
cargo run --bin xgworks -- import "../A dream of a dream.MID" out.xws
```

The tests assert, against the real "A Dream of a Dream" project:

- the chunk walk consumes the whole file,
- the `S4WH` header reads as v4 / 101 tracks / 24 blocks / 480 PPQN,
- `export_named` reproduces the original `.MID` track-for-track (names + notes),
- `.MID` → `.XWS` → `.MID` is note-identical with the original.

## Web (WASM) build

The crate has no platform dependencies, so the core builds for the browser as-is:

```sh
rustup target add wasm32-unknown-unknown
cargo build --target wasm32-unknown-unknown --release --lib
# then wrap with wasm-bindgen / wasm-pack for a JS binding
```

## Next milestones

- **M1** headless player: feed `export`/live events to CoreMIDI (native, via the
  `midir` crate) or the Web MIDI API (wasm). Add a tick-clock + scheduler that
  sends NoteOn and schedules NoteOff after each note's gate.
- **M2** web UI (track list + piano roll + transport) over the wasm core.
- **M3** native macOS/ARM build (egui or Tauri) on the same core.
- **M4** editing + plugin reimplementation (XG editor, mixer).
