# xgworks-native

**Milestones M3 + M4** — a native desktop **editor/player** for XGworks `.XWS` /
`.MID`, built on the same `xgworks-core` as the web build. UI via **egui/eframe**;
MIDI output via **midir** (CoreMIDI on macOS, WinMM on Windows, ALSA on Linux).
A dedicated player thread schedules each message against a monotonic clock.

Features:

- Open `.XWS` or `.MID`; pick a MIDI output; Play / Stop; track list with
  per-track mute/solo; a piano roll with a moving playhead.
- **Editing (M4):** select a track, then on the roll **drag on empty space to add
  a note**, **drag a note to move it**, **right-click a note to delete** it. New
  notes use the velocity in the status bar; positions snap to a 1/16 grid.
- **Save** the edited song back to `.XWS` or `.MID` (buttons in the toolbar).

Edits go through `xgworks-core`'s editable model (`edit_model` → `EditTrack`/
`EditNote` → `write_xws` / `write_mid`), which is covered by round-trip tests.

## Run

```sh
cargo run --release          # from this folder
# or launch the built binary:
#   Windows:  target\release\xgworks-native.exe
#   macOS:    target/release/xgworks-native
```

On Windows the built-in **Microsoft GS Wavetable Synth** works with no hardware.
On macOS, enable the **IAC Driver** (Audio MIDI Setup) and route it to a soft
synth, or connect a hardware/USB MIDI device.

## macOS / Apple Silicon

No code changes — the crate is pure-portable over `xgworks-core` + egui + midir:

```sh
cargo build --release                       # on an arm64 Mac -> native arm64
cargo build --release --target aarch64-apple-darwin   # explicit, if cross-set up
```

egui gives the same UI on every platform; midir binds CoreMIDI automatically.

## Notes

- Mute/solo is applied when Play is pressed (messages for silent tracks are not
  scheduled). Toggling mid-playback updates the roll dimming but not the already
  scheduled stream — press Stop then Play to re-apply.
- Timing: the player thread sleeps until each event's time (≤8 ms granularity so
  Stop stays responsive). For tighter timing later, switch to a small lookahead
  ring buffer.

## Next (M4)

Editing: note add/move/delete on the roll, write back via `xgworks-core`'s
`import`/placement writer; then reimplement XGworks' plugin editors (mixer, XG
editor) as modules.
