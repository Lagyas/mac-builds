# HS2 – Hypersynth 2 — changelog

HS2 – Hypersynth 2 is the synth in the style of Hypersonic 2.

Version = `version` in `Cargo.toml` (the app, `hs2-web` uses the same number).
Every build records the app commit, the `hs2-core` commit (suffix `+changes` =
uncommitted edits) and the UTC build date (`build.rs` → `src/version.rs`).
Where to see it: the window title (`HS2 – Hypersynth 2 0.2.0 (commit)`), the **About** button (version block + Copy version info),
and in the browser build the start page and the tab title. Put the version info
into every bug report.

When you change something: bump `version` (PATCH for fixes, MINOR for features),
add a row here and commit as "HS2 – Hypersynth 2 X.Y.Z: …".

| Version | Date | Change |
|---|---|---|
| 0.2.0 | 2026-10-22 | Renamed to **HS2 – Hypersynth 2**; version + commit + build date in the title bar, About and web page |
| 0.1.0 | 2026-10-20 | First snapshot |
