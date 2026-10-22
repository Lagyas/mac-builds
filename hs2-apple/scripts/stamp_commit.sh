#!/bin/bash
# Writes the git commits (hs2-apple, hs2-ffi, hs2-core) into the built
# Info.plist key HS2GitCommit, shown in About / "Copy version info".
set -u
c() {
  local d="$1" h
  h=$(git -C "$d" rev-parse --short HEAD 2>/dev/null) || { echo "?"; return; }
  [ -n "$(git -C "$d" status --porcelain --untracked-files=no 2>/dev/null)" ] && h="$h+changes"
  echo "$h"
}
ROOT="${SRCROOT:-$(cd "$(dirname "$0")/.." && pwd)}"
VALUE="app $(c "$ROOT")"
PLIST="${TARGET_BUILD_DIR}/${INFOPLIST_PATH}"
/usr/libexec/PlistBuddy -c "Set :HS2GitCommit $VALUE" "$PLIST" 2>/dev/null \
  || /usr/libexec/PlistBuddy -c "Add :HS2GitCommit string $VALUE" "$PLIST"
echo "HS2GitCommit = $VALUE"
