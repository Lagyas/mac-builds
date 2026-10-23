---
name: verify-against-original
description: "For the XGworks port, always check UI/behavior against the original XGworks before building"
metadata:
  node_type: memory
  type: feedback
  originSessionId: 1404da8e-0249-4ab1-8ce4-f25f85fcdd84
  modified: 2026-10-15T06:34:45.033Z
---

Every reimplemented window/feature of the XGworks port must be checked against the original XGworks (help files `XG4\*.hlp`, EXE/DLL resources — dialogs, toolbars, bitmaps, menus — and RE of the code) BEFORE designing it, and the result compared again afterwards.

**Why:** user (2026-10-15) said "Kontroluj vždy s originálem" after I built a piano roll without the original's left panel and bottom controller pane.
**How to apply:** before building a view, extract its layout from Piano.hlp/Track.hlp/etc. and the EXE resources; list the original's panes/controls; then implement matching them. Related: [[xgworks-port]].
