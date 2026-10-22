# HS2 – Hypersynth 2 for macOS (SwiftUI + AUv3)

Native Mac version of HS2 – Hypersynth 2:

- **HS2.app**: a standalone synth with a SwiftUI window, an on-screen keyboard and MIDI input from every CoreMIDI device. You also use it to import your Hypersonic 2 content and SoundFont.
- **AUv3 plug-in** "Lagyas: HS2 – Hypersynth 2" (instrument `aumu / hs2s / Lgys`). It runs in Logic Pro, GarageBand, Ableton Live, MainStage and other Audio Unit hosts. It has:
  - presets, made from every patch in the library;
  - automation of Master, Output Hype and Master Tune;
  - song state saved with the project;
  - host tempo and transport for the arpeggiator.

The sound engine is the Rust code shared with the Windows/web version (`../hs2-core`), linked as a static library through `../hs2-ffi` (C interface `include/hs2.h`). Only the user interface and the Audio Unit wrapper are Swift.

## Folder layout (keep the folders side by side)

```
hs2-core/    Rust engine (patches, phrases, DSP)
hs2-ffi/     C interface for Swift (static library libhs2_ffi.a)
hs2-apple/   this Xcode project (XcodeGen spec project.yml)
```

## Build (Apple Silicon or Intel Mac, macOS 13+)

1. Install Xcode 15 or newer from the App Store and open it once.
2. Install Rust: `curl https://sh.rustup.rs -sSf | sh`, then open a new Terminal.
3. Install XcodeGen: `brew install xcodegen` (Homebrew from https://brew.sh).
4. Generate the project:
   ```
   cd hs2-apple
   xcodegen generate
   open HS2.xcodeproj
   ```
5. Set up signing. In Xcode, under target **HS2** and then target **HS2AU**, open Signing & Capabilities, choose your **Team** (a free Apple ID works) and set Signing Certificate to "Apple Development". The team is needed for the app group through which the plug-in sees the content imported by the app. Alternatively, write the team into `project.yml` (`DEVELOPMENT_TEAM`, `CODE_SIGN_IDENTITY: "Apple Development"`) and run `xcodegen generate` again.
6. Select scheme **HS2** and press Run (⌘R). The first build also compiles the Rust engine; the `HS2Rust` target runs `scripts/build_rust.sh`.

After the first launch of HS2.app, macOS registers the plug-in. Check it in Terminal:

```
auval -v aumu hs2s Lgys
```

In Logic or GarageBand, choose the instrument under **AU Instruments ▸ Lagyas ▸ HS2 – Hypersynth 2**. If it is missing, quit the host and run `killall -9 AudioComponentRegistrar`.

## Content

In HS2.app, click **Import HS2 content folder…** and choose your Hypersonic 2 content folder:

- Patch, phrase and tuning banks and combis are copied whole.
- Sample banks are copied only as their name headers. The protected samples are never decoded.

To play real samples, use **SoundFont / WAV…**, which assigns your own SoundFont (.sf2) or WAV files to the Sample elements.

Both go into the shared app-group folder. In the plug-in, click **Reload content** after importing.

## Version

- Version = `MARKETING_VERSION` in `project.yml`, the same number as the Rust crates.
- The build stamps the git commit into Info.plist (`scripts/stamp_commit.sh`).
- The engine reports its own commit and the hs2-core commit.
- You can see all of it in **About…** (button **Copy version info**) and in the window title. Put it into every bug report. See `CHANGELOG.md`.
