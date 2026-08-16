#!/bin/sh
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
GHOSTTY="${RILL_GHOSTTY_DIR:-$ROOT/third_party/ghostty}"
if [ ! -f "$GHOSTTY/zig-out/lib/libghostty-vt.a" ]; then
  if [ ! -d "$GHOSTTY/.git" ]; then
    git clone --depth 1 https://github.com/ghostty-org/ghostty.git "$GHOSTTY"
  fi
  (cd "$GHOSTTY" && zig build -Demit-lib-vt -Doptimize=ReleaseFast)
fi
