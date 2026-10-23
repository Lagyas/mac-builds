---
name: xgw-next
description: "XGW-Next (F:\\XGW-Next, GitHub Lagyas/XGW-Next private) — user's Python/Tkinter XGworks-like sequencer; beta state, how to build/test"
metadata:
  node_type: memory
  type: project
  originSessionId: 661e31a2-05f4-43b1-861e-651ee1cfdde8
  modified: 2026-10-19T20:08:34.284Z
---

Separate from the Rust port ([[xgworks-port]]). Python/Tkinter app `editor/app.py`, tests in `tests/` (pytest; need `PYTHONPATH=editor`). Windows build: `build-local-windows.bat` (py -3.12 venv `.venv`, PyInstaller `XGW-Next.spec` → `dist\XGW-Next.exe`), `verify-windows-beta.bat`. Run .bat from PowerShell as `cmd.exe /c "F:\XGW-Next\x.bat"` (relative fails). Notes per stage in STAGE*.md; commits "Beta 1 Fix N".

2026-10-19: fast-forwarded local `stage111-main` to origin/main a472376; ran Windows checklist (all OK except Exit didn't ask to save → Beta 1 Fix 7 added, 1186 tests pass); fixed CI/build-apps/build-local-macos.sh (pytest missing — CI red since 13 Oct; mac packaged --onedir). Documented in STAGE246-WINDOWS-CHECKLIST-AND-FIX7.md. Committed 2503cf5 and pushed to main (push needs gh creds: git -c credential.helper= -c "credential.helper=!gh auth git-credential" push; GCM holds account Bravissimo26 without access; gh token now has workflow scope). CI green; Build test applications run 39317723529 green on macos-15 + windows (1186 passed each), prerelease build-39317723529 with XGW-Next-macOS.zip / XGW-Next-Windows.zip. User built locally on M3 Mac (build-local-macos.sh, ~/XGW-Next/dist) and the app runs via Terminal (2026-10-19); release ZIP blocked by Gatekeeper (unsigned, Sequoia "malware" warning) — fix via Privacy & Security "Open Anyway" or xattr; notarization would need Apple Developer ID.
GUI testing tools: F:\rea\tools\winshot.ps1 / screen.ps1 (capture incl. menus), mouse.ps1, shot.ps1.
2026-10-20: pushed Fix 8 (mido rtmidi hidden import in mac/CI builds), Fix 9 (View > Text encoding, default cp1250, midi.decode_text), Fix 10 (Piano Roll hit test/drag/Cmd shortcuts/copy-paste), Fix 12 (Lyrics/Karaoke editor: lyrics_ops.py + lyrics_editor.py, Window "Lyrics"). Someone else also commits in this repo concurrently ("XGW Next Local", e.g. Fix 11 undo selection) — always `git fetch` and check history before committing/numbering fixes. CI green at d65cbee.
Versioning (Fix 13, 645c6d5, tag v0.4.0-beta1.13; also tag v0.4.0-beta1.12 = d65cbee): editor/version.py single source (bump FIX each fix, add CHANGELOG.md row, tag v0.4.0-beta1.N); tools/write_build_info.py writes ignored editor/build_info.py in all builds; Help > About / Copy version info; title shows "0.4.0 Beta 1 Fix N (commit)".
