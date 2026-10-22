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
APP="target/XGWorks NextGen.app"
rm -rf "$APP"
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
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST
# ad-hoc signature so Gatekeeper lets a locally built app start
codesign --force --deep -s - "$APP"
# XGworks data: put your XGworks V4.0 folder as ~/XG4 (or next to the app as XG4,
# or set XGWORKS_DIR).
echo "built $APP ($VERSION)"
