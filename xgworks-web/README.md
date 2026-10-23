# xgworks-web

The browser version of the XGworks editor. It is **the same app as
`../xgworks-native`** (Track View, Piano Roll, List, Mixer, XG Editor, playback,
editing, save), compiled to WebAssembly with eframe's web backend. MIDI goes out
through the **Web MIDI API** (Chrome / Edge; Safari has none).

The previous M1 player page (C-ABI wasm shim) is kept in `legacy-m1/`.

## Build

```powershell
rustup target add wasm32-unknown-unknown                  # once
cargo install wasm-bindgen-cli --version 0.2.129 --locked # once; = wasm-bindgen in Cargo.lock
.\build.ps1                                               # -> web/pkg
```

## Run

Serve over HTTP (wasm cannot load from `file://`). Served from `F:\rea`, the
page finds the XGworks installation at `F:\rea\XG4` by itself:

```sh
python -m http.server 8790 --bind 127.0.0.1 --directory F:\rea
# http://127.0.0.1:8790/xgworks-web/web/
```

Allow MIDI (with SysEx) when the browser asks; the outputs appear in the
toolbar combo as soon as access is granted.

URL options:

- `?xg4=<url>` — where the XGworks folder is on the server (default `../../XG4/`).
- `?song=<url>` — open a song at start, e.g. `?song=../../A%20Dream%20of%20a%20Dream.XWS`.
- `&args=--list 2 --mixer` — the native command-line options (`--roll N`, `--list N`, `--mixer`, `--xg`).

## XGworks data

No Yamaha data is part of the build. The page reads, from an installed XGworks
V4.0: `XGworks.exe` (toolbar strips, mixer panel bitmaps), `ws.ini` + the voice
list `.ini` files it names, `DrvShortName.ini`, `Trk.ini`, and
`XGEditor\xgeditor.ini` + the `.tbl` / voice lists it names. If they are not on
the server, the page offers **Choose the XGworks folder…**; the needed files are
then kept in the browser's IndexedDB for the next visit. Without them the app
still runs, with plain buttons and no voice / parameter names.

The List window uses MS Gothic like the original if `fonts/msgothic.ttc` (next
to the page) or `msgothic.ttc` in the XGworks folder exists; otherwise
monospace at the same character width.

## Differences from the native build

- Sub-windows (Piano Roll, List, Mixer, XG Editor) are windows inside the page;
  the keys go to the window last clicked or opened.
- Playback is scheduled 250 ms ahead with Web MIDI timestamps (no threads in
  the browser); keep the tab visible while playing, background tabs are throttled.
- Save = download. The XG Editor's "Data folder…" button is native only.
