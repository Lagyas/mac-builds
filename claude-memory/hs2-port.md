---
name: hs2-port
description: "HyperSonic 2 \"in the style of\" synth project — DRM limits, decoded formats, project layout, status"
metadata:
  node_type: memory
  type: project
  originSessionId: 661e31a2-05f4-43b1-861e-651ee1cfdde8
  modified: 2026-10-16T18:50:46.398Z
---

Started 2026-10-16. User asked for HyperSonic 2 (Steinberg/Wizoo VSTi, 2005) "same approach as XGworks" ([[xgworks-port]]); agreed on option 1: a new synth **in the style of HS2** (same UI/controls, reads original patches), own engine, user's own samples (SF2/WAV).

**Why not a true port:** Hypersonic.dll is Syncrosoft-protected (engine encrypted in .rdata; sample/wavetable/drawbar audio not plain). Refused to break DRM, run the H2O warez setup in F:\rea\_hypersonic2, or use a virtual dongle. **How to apply:** never decrypt engine/samples; never embed Steinberg content/bitmaps in builds — user loads own content folder; UI is own vector drawing after the original layout (res\2_128.png, sheet2.png). Verify against original resources/manual ([[verify-against-original]]).

Projects (all build): F:\rea\hs2-core (pure std; bank/patch/params/dsp/wavetable/fx/sf2/arp/synth/library/factory/state; 17 tests; `examples/scan.rs` renders all 2747 factory patches: 0 NaN), F:\rea\hs2-native (egui 0.28 + cpal 0.15 + midir; app.rs panel, pages.rs 7 display pages, panel.rs widgets; settings %APPDATA%\hs2-native.ini `content=`/`soundfont=`), F:\rea\hs2-web (build.ps1, wasm-bindgen 0.2.129 pinned; web/index.html folder picker → add_file, Web MIDI non-blocking). Name shown: "Hypersynth 2".

Format facts: notes in F:\rea\_hypersonic2\notes\FORMAT.md (patch layout, param index maps from plain UI code FUN_1001cb60/FUN_1000cee0 + table 0x1001eaf0, elem type = ceil(v·16)−1, FX type = round(v·128), sample bank = floor(p18·128), wave = floor(p19·128), mod src/mod = floor(v·72), dest bands ×128 heuristic). Original FX params use unknown layout → own defaults unless patch.own (header word 0x08 = "HS2X"). Elements are read until a non-printable name (last patch in bank has trailing data).

Done 2026-10-16: phrases (phrase.rs; 304 phrases; arp Mode order with Phrase at index 6, p26/p27 = bank/prog ·128; 086 patches carry extra unknown trailing data so all point to phrase 0), combis (combi.rs; 100/100 .fxb parse; chunk StSS 0x990f3e/46, 16 part offsets, part flags 0x100 = link; Load page bank "120 Combis"), build_macos.sh (untested, no Mac), sample mapping (Synth.sample_map per HS2 sample bank: SF2 auto-GM/fixed preset or WAV set; drum banks pick WAV by Wave index; hs2-native/src/mapwin.rs window, settings map=bank|sf2|path|b|p / map=bank|wav|p1*p2; native picks return full paths). Original samples stay unavailable (DRM; user asked to bypass, declined) — do not offer workarounds. Open/next: sample bank names need full Samples extraction (only 2 extracted in iso\), user hasn't listened yet.

2026-10-20: F:\rea\hs2-core|hs2-native|hs2-web are local git repos (branch main, no remote; .gitignore target/ web/pkg/ *.wav); snapshot commits made. Commit new work there too.

**macOS native (2026-10-22, v0.2.0):** F:\rea\hs2-ffi (Rust C ABI, include/hs2.h, tests/abi.rs) + F:\rea\hs2-apple (XcodeGen project.yml: HS2.app SwiftUI standalone + HS2AU AUv3 aumu/hs2s/Lgys, shared Swift in Shared/, app group $(TeamIdentifierPrefix)cz.lagyas.hs2 for imported content). Swift was NOT compiled here (no Mac) — first Mac build may need fixes. Transfer zip: F:\rea\HS2-mac-src.zip.
