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
| 0.2.1 | 2026-10-22 | macOS: XGworks data folder found via XGWORKS_DIR, ~/XG4, ~/Documents/XG4 or XG4 next to the app; Mac font folders; build_macos.sh (universal .app) |
| 0.2.0 | 2026-10-22 | Renamed to **XGWorks NextGen**; version + commit + build date in the title bar, About and web page |
| 0.1.0 | 2026-10-20 | First snapshot |
