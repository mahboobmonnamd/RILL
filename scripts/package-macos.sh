#!/bin/sh
# Package one NSWindow app. GUI does not spawn the user shell.
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
sh scripts/fetch-libghostty-vt.sh
cargo build --release -p rilld -p rill-host

TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
APP="$ROOT/dist/Rill.app"
MACOS="$APP/Contents/MacOS"
RES="$APP/Contents/Resources"
rm -rf "$APP"
mkdir -p "$MACOS" "$RES"
cp host/macos/Info.plist "$APP/Contents/Info.plist"
cp host-surface.toml "$RES/host-surface.toml"
cp "$TARGET_DIR/release/rilld" "$MACOS/rilld"

HOST_LIB="$TARGET_DIR/release/librill_host.a"
CHIP0_VT=$(ls -1 "$TARGET_DIR"/release/build/rill-chip0-*/out/librill_chip0_vt.a | head -1)
GHOSTTY_VT="${RILL_GHOSTTY_DIR:-$ROOT/third_party/ghostty}/zig-out/lib/libghostty-vt.a"

clang -fobjc-arc -O2 -fmodules \
  -Werror=implicit-function-declaration \
  -o "$MACOS/Rill" \
  host/macos/main.m host/macos/TerminalView.m \
  "$HOST_LIB" \
  "$CHIP0_VT" \
  "$GHOSTTY_VT" \
  -I host/macos \
  -framework Cocoa -framework Metal -framework QuartzCore -framework CoreText \
  -framework CoreGraphics -framework ApplicationServices \
  -lc++ -lSystem

# T-SPAWN inspects imports, not exports. Surface the same information here so a
# packaging change that pulls in a PTY primitive is visible immediately rather
# than at gate time (PRD FR-SPAWN, docs/TEST-CASES.md).
echo "-- PTY-creation imports in the packaged GUI (expect none) --"
nm -u "$MACOS/Rill" \
  | grep -E '_forkpty|_openpty|_posix_openpt|_grantpt|_unlockpt|_ptsname|_login_tty' \
  || echo "  none"

echo "packaged $APP"
