# Přehled projektů (předávka pro Claude) — stav 2026-10-23

Tento dokument shrnuje tři projekty, aby se v nich dalo pokračovat v jiném
rozhraní Claude (claude.ai Projects / Claude Code). Vlož ho do znalostí
projektu; zdrojáky přidej z GitHubu (repo `Lagyas/mac-builds`, „Add from GitHub“).

## Pravidla spolupráce
- Odpovídat **česky**; kód, identifikátory a komentáře v kódu anglicky.
- Názvy: **XGW Next** (Python), **XGWorks NextGen** (Rust port XGworks), **HS2 – Hypersynth 2** (syntezátor). Nemíchat.
- XGWorks NextGen: každé okno/funkci **ověřit proti originálu** XGworks V4.0 (nápověda `XG4\*.hlp`, resources z EXE, RE kódu) před návrhem i po něm.
- Do repozitářů **nikdy** nedávat proprietární obsah Yamaha/Steinberg (instalačky, EXE/DLL, zvukové banky, bitmapy, licence, hudební soubory). Aplikace čtou data z uživatelovy instalace za běhu.
- HS2: **neobcházet DRM Syncrosoft**, nedešifrovat engine ani samply, nepoužívat warez (H2O) ani emulátor klíče a nenabízet cesty, jak originální samply získat. Uživatel hraje vlastními samply (SF2/WAV).
- Commit/push jen na požádání. Verzovat: zvýšit verzi + řádek v CHANGELOG při každé změně.

## 1. XGW Next (Python / Tkinter)
- Repo: https://github.com/Lagyas/XGW-Next (lokálně `F:\XGW-Next`). Paralelně do něj commituje i jiný pracovník („XGW Next Local“) → vždy `git fetch` a zkontrolovat historii před číslováním fixu.
- Verze: `editor/version.py` (MAJOR.MINOR.PATCH + Beta + FIX), aktuálně **0.4.0 Beta 1 Fix 13** (tag `v0.4.0-beta1.13`, 645c6d5). `tools/write_build_info.py` zapisuje commit/datum do buildu; Help ▸ About / Copy version info. CHANGELOG.md tabulka fixů.
- Testy: `PYTHONPATH=editor pytest` (1208 passed). Build: `build-local-windows.bat` (PyInstaller `XGW-Next.spec`), `build-local-macos.sh`; GitHub Actions `ci.yml`, `build-apps.yml` (prerelease s .zip).
- Hotovo v Beta 1: Fix 7 dotaz na uložení, 8 MIDI backend v mac buildu, 9 kódování textu (výchozí cp1250, View ▸ Text encoding), 10 Piano Roll (chytání not, posun nahoru, Cmd/Ctrl zkratky, copy/paste), 11 undo výběru, 12 Lyrics/Karaoke editor, 13 verzování.
- Otevřené: na Macu uživatel nevidí „Track ▸ Demix by Note…“ (v kódu je) — nejspíš starý checkout; čeká se na `git log -1` / `git status` z Macu. Mac aplikace nepodepsané → `xattr -cr`.

## 2. XGWorks NextGen (Rust port Yamaha XGworks V4.0)
- Složky: `xgworks-core` (formát .XWS, SMF, mixer, XG tabulky, List, Joby, XF info, importer, gtrtab), `xgworks-native` (egui 0.28 + midir; Track View, Piano Roll, List, Mixer, XG Editor, Job menu, XF editor, Auto Play, Importer), `xgworks-web` (stejná aplikace ve wasm, Web MIDI).
- Verze **0.2.1** (title „XGWorks NextGen 0.2.1 (commit)“, menu Help s verzí). Data XGworks za běhu: `XGWORKS_DIR`, `~/XG4`, `~/Documents/XG4`, `XG4` vedle aplikace (Windows `F:\rea\XG4`).
- Formát .XWS plně dekódován: kontejner `[tag][len BE32][payload]`, `S4WH` hlavička (480 PPQN, 101 stop), `S4WB` proudy událostí (noty = NoteOn + 4B gate), `S4WT` stopy (0x20 hlavička: type, port, ch, transpose, solo, mute, inst type…, bloky placement start/end/idx + jméno), `S4ST`. XF info = tick-0 meta v prvním S4WB.
- Tabulky `listnames_data.rs` a `xgunits_data.rs` jsou generované z XGworks.exe nástroji v `xgworks-core/tools` (názvy parametrů/jednotek).
- Pozastaveno: Guitar Arranger (core `gtrtab.rs` hotov, UI ne), AutoArranger nezačat. Další nápady: XG softsynth, ověření generovaných .XWS v originálu.
- Build: `cargo build --release` v `xgworks-native`; Mac `.app`: `sh build_macos.sh`; web: `xgworks-web/build.ps1` (wasm-bindgen 0.2.129).

## 3. HS2 – Hypersynth 2 (syntezátor ve stylu Steinberg/Wizoo Hypersonic 2)
- Vlastní engine, čte originální patche, fráze, ladění a kombinace z uživatelova obsahu; samply = vlastní SF2/WAV (mapování per HS2 sample bank).
- `hs2-core` (čistý std: bank/patch/params/dsp/wavetable/fx/sf2/arp/phrase/combi/synth/library/factory/state; 21 testů), `hs2-native` (egui panel, „HS2 Classic“, verze 0.2.0), `hs2-web` (wasm, Web Audio + Web MIDI).
- **macOS nativně (0.2.0):** `hs2-ffi` (C ABI `include/hs2.h`, test `tests/abi.rs` 4/4) + `hs2-apple` (XcodeGen `project.yml`: **HS2.app** SwiftUI + **AUv3** „Lagyas: HS2 – Hypersynth 2“ `aumu/hs2s/Lgys`; presety z patchů, automatizace Master/Hype/Tune, stav v projektu, tempo hostitele; obsah přes app group `$(TeamIdentifierPrefix)cz.lagyas.hs2`). **Swift zatím nebyl zkompilován** — první build na Macu může potřebovat opravy.
- Formátová fakta: patch záznam „StSS“, 0x220 hlavička, 256 patch parametrů, elementy (32B jméno + 256 f32) až do netisknutelného jména; typ elementu = ceil(v·16)−1, FX typ = round(v·128), sample bank/wave = floor(·128), mod src/mod = floor(v·72); vlastní patch = slovo 0x08 „HS2X“; fráze 24B události; kombinace .fxb chunk StSS 0x990f3e/46, 16 partů, flag 0x100 = link.

## Mac buildy
- Privátní repo **https://github.com/Lagyas/mac-builds** = kopie všech Rust/Swift projektů (sync skript `tools/sync_macbuild.sh` z lokálních repozitářů), workflow `.github/workflows/macos.yml` (spouští se ručně: Actions ▸ macOS apps ▸ Run workflow) → zip se třemi aplikacemi (HS2.app, HS2 Classic.app, XGWorks NextGen.app).
- Stav 2026-10-23: GitHub úlohu nespustil kvůli **billingu účtu** (platba / spending limit; macOS minuty u privátních repo jsou placené). Po opravě billingu spustit workflow znovu.
- Ruční build na Macu: viz `README.md` v kořeni repa.

## Lokální struktura (Windows, F:\rea)
`hs2-core, hs2-ffi, hs2-apple, hs2-native, hs2-web, xgworks-core, xgworks-native, xgworks-web` = samostatné lokální git repozitáře (větev main, tagy v0.2.x); `tools/` = pomocné skripty (screenshoty, myš, RE resources); `XG4/` a `_hypersonic2/` = uživatelovy originální instalace (nikam nenahrávat).
