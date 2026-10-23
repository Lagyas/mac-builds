# xgworks-web

**Milestone M1** — a browser player for XGworks `.XWS` projects (and `.MID`),
built on `xgworks-core` compiled to WebAssembly and playing over the **Web MIDI
API**. No wasm-bindgen: a tiny C-ABI shim over linear memory, so the only tool
needed is the `wasm32` target.

## Build

```sh
rustup target add wasm32-unknown-unknown        # once
cargo build --release --target wasm32-unknown-unknown
# copy the artifact next to the page:
cp target/wasm32-unknown-unknown/release/xgworks_web.wasm web/xgworks_web.wasm
```

## Run

Serve the `web/` folder over HTTP (the wasm cannot be fetched from `file://`):

```sh
python -m http.server 8787 --bind 127.0.0.1 --directory web
# open http://127.0.0.1:8787 in Chrome or Edge (Web MIDI; Safari has none)
```

Pick a MIDI output (on Windows the built-in **Microsoft GS Wavetable Synth**
works with no hardware), load an `.XWS` or `.MID`, press Play. Allow SysEx when
prompted so the XG setup messages are sent.

## How it works

- `xgworks-core::playback_plan(bytes)` merges all tracks, expands notes to
  NoteOn/NoteOff via the gate, applies the tempo map, and returns
  `(time_ms, raw MIDI bytes)` sorted by time — accepting either `.XWS` or SMF.
- The wasm shim packs that into a blob; `index.html` schedules every message with
  `output.send(bytes, startTime + time_ms)` so the browser's MIDI subsystem keeps
  the timing.

Verified in Node against the real files: `.XWS` and its `.MID` export yield an
identical plan (10,796 messages, 3,632 notes, 27 SysEx, ~223 s).

## Next (M1 polish / M2)

- Transport: pause/seek, a playhead and tempo display.
- M2: track list + piano-roll view driven by the same core.
- Native desktop (M3) reuses `xgworks-core` with `midir`/CoreMIDI instead of Web MIDI.
