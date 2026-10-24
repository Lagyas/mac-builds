# XGWorks NextGen — changelog

XGWorks NextGen is the Rust port of Yamaha XGworks V4.0 (.XWS / .MID editor and player).

Version = `version` in `Cargo.toml` (the app, `xgworks-web` uses the same number).
Every build records the app commit, the `xgworks-core` commit (suffix `+changes` =
uncommitted edits) and the UTC build date (`build.rs` → `src/version.rs`).
Where to see it: the window title (`XGWorks NextGen 0.2.0 (commit)`), the Toolbar **Help** menu,
and in the browser build the start page and the tab title. Put the version info
into every bug report.

When you change something: bump `version` (PATCH for fixes, MINOR for features),
add a row here and commit as "XGWorks NextGen X.Y.Z: …".

| Version | Date | Change |
|---|---|---|
| 0.3.1 | 2026-10-24 | Own app folder, nothing to set up: `XGWorks NextGen/` holds the app and `XG4/` (XGworks data, found automatically), `Songs/` (Open / Save start there), `Settings/` (portable settings), `Fonts/` (optional msgothic.ttc). Preferences show the folders; Choose… / Default for another data folder. |
| 0.3.0 | 2026-10-24 | Transport: song position (play / stop keep it), ruler click = position, right-click = L / R locators, loop (🔁); keys Space = play/stop, numpad 0 start, 1 L, 3 R, 2 back 2–4 bars (Preferences ⚙), − loop; chase of program/CC/pitch bend when starting mid-song. **Drum window** (F5, Window ▸ Drum): rows / names / colours from inst.ini + kit, drum marks by velocity, click = monitor. **Lyrics / Karaoke** editor (F8): lyric list, insert / edit / move / delete, karaoke preview, assign syllables to notes, Windows-1250/1252/UTF-8. Window menu (Event List F2, Piano Roll F3, Drum F5, Mixer F6, XG Editor F7, Lyrics F8). Mixer meters always black, green only while events play. Preferences: choose the XGworks data folder (macOS: XG Editor tables were not found). |
| 0.2.1 | 2026-10-22 | macOS: XGworks data folder found via XGWORKS_DIR, ~/XG4, ~/Documents/XG4 or XG4 next to the app; Mac font folders; build_macos.sh (universal .app) |
| 0.2.0 | 2026-10-22 | Renamed to **XGWorks NextGen**; version + commit + build date in the title bar, About and web page |
| 0.1.0 | 2026-10-20 | First snapshot |
