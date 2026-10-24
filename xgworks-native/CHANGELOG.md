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
| 0.8.1 | 2026-10-24 | **App icon** (macOS style, AppIcon.icns in the .app, window icon). **Tool bars** like the original + Cubase: menu bar (File Edit Setup Job Plug-in Window Help), icon bar (New / Open / Save, Undo / Redo, edit tools, MIDI Out, Transport, position), editors bar. **Note palette** in Piano Roll / Staff / Drum (F2–F12 note length, Ctrl = triplets, Shift = velocity ppp–fff, also for the selected notes). **Karaoke preview**: lines centred and wrapped; line / page marks also from .kar files (CR / LF, trailing "/", "\\"). |
| 0.8.0 | 2026-10-24 | **Chord track (C lane)** editing like the original (Track.hlp): pencil click = Chord Insert dialog (root + #/b, 34 XF chord types, on-bass note, position; Insert keeps it open and moves one bar on), arrow = select (Ctrl adds) and drag on the beat grid, double-click = edit, eraser / Del = delete, right-click "C" = Insert / Select All / Clear; Edit ▸ Insert Chord at the song position. Chords are XF chord events on the master track (undo-able). |
| 0.7.1 | 2026-10-24 | **Staff ▸ Print / Export…**: the whole track as A4 pages (systems of 4 bars, clef + key on every system, bar numbers, title, page numbers) in an SVG with the XGmusic font embedded; opens in the browser / Preview to print or save as PDF. |
| 0.7.0 | 2026-10-24 | **Transport window** (the original's Control Panel, Alt+F7, Cubase-like): own window, always on top, big location display (bar:beat:clock, time, tempo, signature), big buttons with bold labels (Top, −1 / +1 bar, Stop, Play, Rec, Loop, Click), L / R locators with Set / Go. **Editors bar**: icon + bold name for Event List, Piano Roll, Staff, Drum, Master Track, Mixer, XG Editor, Lyrics, Markers. Drawn icons for the tools when XGworks.exe is missing. **Preferences** in its own window. **Lyrics**: spaces shown as grey dots; Export / Import .lyrics (Midimusic Chord and Lyrics Protocol: Lyr / Chd lines with bar:beat:tick, Windows-1250, like midi-file-editor). **macOS data lookup**: case-insensitive file names, more places for the XG4 folder, explanation when macOS runs the app from a translocated copy (xattr -cr). |
| 0.6.1 | 2026-10-24 | Setup menu: send XG System On, GM System On, Panic. |
| 0.6.0 | 2026-10-24 | **Staff** window (F4, Window ▸ Staff): the track as notation on the Piano Roll's time axis — treble / bass / grand staff chosen by the notes, guitar / bass written an octave up, key and time signature from the master track, accidentals, ledger lines, stems, flags, beams per beat, dots, ties across bar lines, rests; glyphs from XGworks' XGmusic.ttf (XG4). Editing with the roll's tools (pencil enters the key's note of the line / space). |
| 0.5.0 | 2026-10-24 | **Recording**: REC button (numpad +) + Play records the MIDI input into the selected track (merged, on its channel), Stop writes it in (undo-able); loop restarts keep each pass. **Setup** menu: MIDI input, MIDI Thru to the selected track, **metronome** (while playing / recording, 0–2 bars count-in, notes / velocity on channel 10); ♩ button = click while playing. |
| 0.4.0 | 2026-10-24 | **File** menu: New (Ctrl+N), Open (Ctrl+O), Close, Save (Ctrl+S, back to the opened file), Save As .XWS / .MID, Exit; "save the changes?" before New / Open / Close / quitting. **Edit** menu: Undo / Redo, Add Marker / Marker List (rename, move, delete, go to). **Master Track** window (F8, like the original): the master track's meta events as a list, insert Tempo / Time Signature / Key Signature / Marker / texts; key signature editable ("D Maj"). Lyrics moved to F9. |
| 0.3.1 | 2026-10-24 | Own app folder, nothing to set up: `XGWorks NextGen/` holds the app and `XG4/` (XGworks data, found automatically), `Songs/` (Open / Save start there), `Settings/` (portable settings), `Fonts/` (optional msgothic.ttc). Preferences show the folders; Choose… / Default for another data folder. |
| 0.3.0 | 2026-10-24 | Transport: song position (play / stop keep it), ruler click = position, right-click = L / R locators, loop (🔁); keys Space = play/stop, numpad 0 start, 1 L, 3 R, 2 back 2–4 bars (Preferences ⚙), − loop; chase of program/CC/pitch bend when starting mid-song. **Drum window** (F5, Window ▸ Drum): rows / names / colours from inst.ini + kit, drum marks by velocity, click = monitor. **Lyrics / Karaoke** editor (F8): lyric list, insert / edit / move / delete, karaoke preview, assign syllables to notes, Windows-1250/1252/UTF-8. Window menu (Event List F2, Piano Roll F3, Drum F5, Mixer F6, XG Editor F7, Lyrics F8). Mixer meters always black, green only while events play. Preferences: choose the XGworks data folder (macOS: XG Editor tables were not found). |
| 0.2.1 | 2026-10-22 | macOS: XGworks data folder found via XGWORKS_DIR, ~/XG4, ~/Documents/XG4 or XG4 next to the app; Mac font folders; build_macos.sh (universal .app) |
| 0.2.0 | 2026-10-22 | Renamed to **XGWorks NextGen**; version + commit + build date in the title bar, About and web page |
| 0.1.0 | 2026-10-20 | First snapshot |
