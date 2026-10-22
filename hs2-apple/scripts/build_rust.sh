#!/bin/bash
# Builds the Rust engine (../hs2-ffi + ../hs2-core) as a static library for
# the architectures Xcode is building (ARCHS), into build/rust/libhs2_ffi.a.
# Run by the HS2Rust target; can also be run by hand.
set -euo pipefail
HERE="$(cd "$(dirname "$0")/.." && pwd)"
FFI="$HERE/../hs2-ffi"
OUT="$HERE/build/rust"
ARCHS="${ARCHS:-$(uname -m)}"
DEPLOY="${MACOSX_DEPLOYMENT_TARGET:-13.0}"
CARGO="$(command -v cargo || echo "$HOME/.cargo/bin/cargo")"
[ -x "$CARGO" ] || { echo "error: cargo not found - install Rust from https://rustup.rs"; exit 1; }
RUSTUP="$(dirname "$CARGO")/rustup"

libs=()
for a in $ARCHS; do
  case "$a" in
    arm64) t=aarch64-apple-darwin ;;
    x86_64) t=x86_64-apple-darwin ;;
    *) echo "warning: skipping arch $a"; continue ;;
  esac
  [ -x "$RUSTUP" ] && "$RUSTUP" target add "$t" >/dev/null 2>&1 || true
  # clean environment: Xcode's variables (SDKROOT, LD, ...) confuse cargo build scripts
  env -i HOME="$HOME" PATH="$(dirname "$CARGO"):/usr/bin:/bin:/usr/sbin:/sbin" \
      MACOSX_DEPLOYMENT_TARGET="$DEPLOY" \
      "$CARGO" build --release --manifest-path "$FFI/Cargo.toml" --target "$t"
  libs+=("$FFI/target/$t/release/libhs2_ffi.a")
done
[ ${#libs[@]} -gt 0 ] || { echo "error: no supported architecture in '$ARCHS'"; exit 1; }
mkdir -p "$OUT"
lipo -create "${libs[@]}" -output "$OUT/libhs2_ffi.a.tmp"
mv -f "$OUT/libhs2_ffi.a.tmp" "$OUT/libhs2_ffi.a"
echo "built $OUT/libhs2_ffi.a (${ARCHS})"
