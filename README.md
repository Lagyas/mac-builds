# HS2 – Hypersynth 2 + XGWorks NextGen (private)

Contents:

| Folder | What |
|---|---|
| `hs2-core` | HS2 – Hypersynth 2 engine (Rust, no dependencies) |
| `hs2-ffi` | C interface of the engine for Swift (`include/hs2.h`) |
| `hs2-apple` | **macOS: HS2.app (SwiftUI) + AUv3 plug-in** (XcodeGen) |
| `hs2-native` | HS2 Classic: egui panel (Windows / macOS / Linux) |
| `hs2-web` | HS2 in the browser (wasm) |
| `xgworks-core` | XGWorks NextGen core (.XWS / SMF, XG tables, jobs …) |
| `xgworks-native` | XGWorks NextGen editor/player (egui) |
| `xgworks-web` | XGWorks NextGen in the browser (wasm) |
| `tools` | helper scripts (Windows GUI testing, resource dumps) |
| `PREHLED-PROJEKTU.md` | project overview / handoff (Czech) |
| `claude-memory` | Claude Code memory notes for these projects |

No Yamaha / Steinberg content is in this repository. The apps read the user's own installation at run time.

## Build on a Mac

Do this once:

```
xcode-select --install                          # or install Xcode from the App Store (needed for HS2.app)
curl https://sh.rustup.rs -sSf | sh             # Rust, then open a new Terminal
brew install xcodegen                           # Homebrew: https://brew.sh
git clone https://github.com/Lagyas/mac-builds.git
cd mac-builds
```

**HS2.app + AUv3 plug-in** (needs Xcode):

```
cd hs2-apple
xcodegen generate
open HS2.xcodeproj
```

In Xcode, select your Team under Signing for the targets HS2 and HS2AU, then press ⌘R.

To build from the command line without a team instead:

```
xcodebuild -project HS2.xcodeproj -scheme HS2 -configuration Release -derivedDataPath dd \
  CODE_SIGN_STYLE=Manual CODE_SIGN_IDENTITY=- DEVELOPMENT_TEAM= build
open dd/Build/Products/Release/HS2.app
```

To check that the plug-in is registered: `auval -v aumu hs2s Lgys`

**HS2 Classic.app**: `sh hs2-native/build_macos.sh` → `hs2-native/target/HS2 Classic.app`

**XGWorks NextGen.app**: `sh xgworks-native/build_macos.sh` → `xgworks-native/target/XGWorks NextGen.app`. It needs your XGworks V4.0 folder copied as `~/XG4`.

The apps are not notarized. On first start, run `xattr -cr <app>` or use right-click ▸ Open.

## CI

Actions ▸ **macOS apps** ▸ Run workflow builds all three apps into one zip, attached as a release. It needs GitHub Actions macOS minutes on the account.
