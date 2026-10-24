#!/bin/sh
# Build "XGWorks NextGen.app" on a Mac,
# universal binary (Apple Silicon + Intel).
# Needs: Xcode command line tools (xcode-select --install) and Rust (https://rustup.rs).
# Run from this folder; xgworks-core must sit next to it (../xgworks-core).
set -e
cd "$(dirname "$0")"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin
DIST="target/XGWorks NextGen"
APP="$DIST/XGWorks NextGen.app"
rm -rf "$DIST"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/xgworks-native" \
  target/aarch64-apple-darwin/release/xgworks-native \
  target/x86_64-apple-darwin/release/xgworks-native
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>XGWorks NextGen</string>
  <key>CFBundleDisplayName</key><string>XGWorks NextGen</string>
  <key>CFBundleIdentifier</key><string>cz.lagyas.xgworks-nextgen</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleExecutable</key><string>xgworks-native</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
# app icon (assets/icon_1024.png -> AppIcon.icns)
ICONSET=target/AppIcon.iconset
rm -rf "$ICONSET"; mkdir -p "$ICONSET"
for s in 16 32 128 256 512; do
  sips -z $s $s assets/icon_1024.png --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  d=$((s * 2)); sips -z $d $d assets/icon_1024.png --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/AppIcon.icns"
# ad-hoc signature so Gatekeeper lets a locally built app start
codesign --force --deep -s - "$APP"
# The app's folder: everything it needs sits next to it (found automatically).
mkdir -p "$DIST/XG4" "$DIST/Songs" "$DIST/Settings" "$DIST/Fonts"
cat > "$DIST/XG4/README.txt" <<'TXT'
Copy the files of your XGworks V4.0 installation here (ws.ini, inst.ini, XGmusic.ttf,
Drum Kit.ini, Melody.ini and the other *.ini files, XGworks.exe and the
XGEditor folder). XGWorks NextGen finds them here automatically.
TXT
cat > "$DIST/Fonts/README.txt" <<'TXT'
Optional: put msgothic.ttc here for the List window font like the original.
TXT
cat > "$DIST/Songs/README.txt" <<'TXT'
Your songs (.XWS / .MID). Open and Save start in this folder.
TXT
echo "built $DIST ($VERSION)"
