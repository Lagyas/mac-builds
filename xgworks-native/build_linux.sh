#!/bin/sh
# Build "XGWorks NextGen" for Linux (x86-64): a folder with the program, its
# XG4 / Songs / Settings / Fonts folders, an icon and a .desktop file, packed
# as target/XGWorks-NextGen-linux-x86_64.tar.gz.
# Needs Rust (https://rustup.rs) and the development packages, on Debian / Ubuntu:
#   sudo apt install build-essential pkg-config libasound2-dev libgtk-3-dev \
#     libxkbcommon-dev libgl1-mesa-dev libx11-dev libxcursor-dev libxrandr-dev libxi-dev libwayland-dev
# Run from this folder; xgworks-core must sit next to it (../xgworks-core).
set -e
cd "$(dirname "$0")"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
cargo build --release
DIST="target/XGWorks NextGen"
rm -rf "$DIST"
mkdir -p "$DIST/XG4" "$DIST/Songs" "$DIST/Settings" "$DIST/Fonts"
cp target/release/xgworks-native "$DIST/xgworks-nextgen"
strip "$DIST/xgworks-nextgen" || true
cp assets/icon_256.png "$DIST/xgworks-nextgen.png"
cat > "$DIST/xgworks-nextgen.desktop" <<'DESK'
[Desktop Entry]
Type=Application
Name=XGWorks NextGen
Comment=XG song editor and player (.XWS / .MID)
Exec=sh -c 'cd "$(dirname "$(readlink -f "%k")")" && ./xgworks-nextgen %f'
Icon=xgworks-nextgen
Terminal=false
Categories=AudioVideo;Audio;Midi;
MimeType=audio/midi;audio/x-midi;
DESK
cat > "$DIST/XG4/README.txt" <<'TXT'
Copy the files of your XGworks V4.0 installation here (ws.ini, inst.ini, XGmusic.ttf,
Drum Kit.ini, Melody.ini and the other *.ini files, XGworks.exe and the
XGEditor folder). XGWorks NextGen finds them here automatically (upper / lower
case in the file names does not matter).
Optional: midi-file-editor.html (your own) adds the PSR-SX900 / SC-8850 voices
and the Vocal Harmony type names.
TXT
cat > "$DIST/Fonts/README.txt" <<'TXT'
Optional: put msgothic.ttc here for the List window font like the original.
TXT
cat > "$DIST/Songs/README.txt" <<'TXT'
Your songs (.XWS / .MID). Open and Save start in this folder.
TXT
cat > "$DIST/README.txt" <<TXT
XGWorks NextGen $VERSION for Linux (x86-64)

Start: ./xgworks-nextgen  (or a song: ./xgworks-nextgen song.mid)
MIDI out goes through ALSA: a hardware port, or a soft synth such as
FluidSynth / TiMidity (e.g. "fluidsynth -a pulseaudio -m alsa_seq sf2file").
The folder can sit anywhere; XG4, Songs, Settings and Fonts next to the
program are found automatically.
TXT
tar -C target -czf "target/XGWorks-NextGen-linux-x86_64.tar.gz" "XGWorks NextGen"
echo "built $DIST ($VERSION)"
