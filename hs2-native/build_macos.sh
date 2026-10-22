#!/bin/sh
# Build "HS2 Classic.app" (the egui panel, same as on Windows) on a Mac,
# universal binary (Apple Silicon + Intel).
# Needs: Xcode command line tools (xcode-select --install) and Rust (https://rustup.rs).
# Run from this folder; hs2-core must sit next to it (../hs2-core).
set -e
cd "$(dirname "$0")"
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin
APP="target/HS2 Classic.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/hs2-native" \
  target/aarch64-apple-darwin/release/hs2-native \
  target/x86_64-apple-darwin/release/hs2-native
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>HS2 Classic</string>
  <key>CFBundleDisplayName</key><string>HS2 – Hypersynth 2 (classic panel)</string>
  <key>CFBundleIdentifier</key><string>cz.lagyas.hs2.classic</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleExecutable</key><string>hs2-native</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
# ad-hoc signature so Gatekeeper lets a locally built app start
codesign --force --deep -s - "$APP"
echo "built $APP ($VERSION)"
