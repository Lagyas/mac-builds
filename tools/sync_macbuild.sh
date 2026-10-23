#!/bin/bash
# Copies the committed state (HEAD) of the local repos into F:\rea\_macbuild,
# the private GitHub repo that builds the macOS apps. Music fixtures and
# scratch files stay local.
set -e
R=/f/rea; M=$R/_macbuild
msg="Sync:"
for d in hs2-core hs2-ffi hs2-apple hs2-native hs2-web xgworks-core xgworks-native xgworks-web; do
  rm -rf "$M/$d"; mkdir -p "$M/$d"
  git -C "$R/$d" -c core.autocrlf=false archive HEAD | tar -x -C "$M/$d"
  msg="$msg $d@$(git -C "$R/$d" rev-parse --short HEAD)"
done
rm -rf "$M/xgworks-core/tests/fixtures" "$M/xgworks-core/tests/roundtrip.rs" "$M/xgworks-core/out.xws"
# helper scripts (no binaries / backups) and Claude's project notes
rm -rf "$M/tools" "$M/claude-memory"; mkdir -p "$M/tools" "$M/claude-memory"
cp "$R"/tools/*.ps1 "$R"/tools/*.py "$R"/tools/*.sh "$M/tools/"
cp "$HOME/.claude/projects/F--/memory/"*.md "$M/claude-memory/"
echo "$msg"
