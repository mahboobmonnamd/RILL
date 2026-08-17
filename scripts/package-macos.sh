#!/bin/sh
# Package one NSWindow app. GUI does not spawn the user shell.
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
sh scripts/fetch-libghostty-vt.sh
cargo build --release -p rill-host
if [ "${RILL_MUTATE:-}" = "drop_POSIX_SPAWN_SETSID" ]; then
  cargo build --release -p rilld --features mutate
else
  cargo build --release -p rilld
fi

TARGET_DIR="${CARGO_TARGET_DIR:-$ROOT/target}"
APP="${RILL_APP:-$ROOT/dist/Rill.app}"
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

EXTRA_SRC=""
EXTRA_LIBS=""
if [ "${RILL_MUTATE:-}" = "openpty_in_main_m" ]; then
  # Constructor object, not main.m: lint-planes forbids PTY primitives in host/.
  EXTRA_SRC="$ROOT/crates/rill-host/tests/fixtures/mutate_openpty.c"
  EXTRA_LIBS="-lutil"
fi

clang -fobjc-arc -O2 -fmodules \
  -Werror=implicit-function-declaration \
  -o "$MACOS/Rill" \
  host/macos/main.m host/macos/TerminalView.m \
  $EXTRA_SRC \
  "$HOST_LIB" \
  "$CHIP0_VT" \
  "$GHOSTTY_VT" \
  -I host/macos \
  -framework Cocoa -framework Metal -framework MetalKit -framework QuartzCore -framework CoreText \
  -framework CoreGraphics -framework ApplicationServices \
  -lc++ -lSystem $EXTRA_LIBS

# T-SPAWN inspects imports, not exports. Surface the same information here so a
# packaging change that pulls in a PTY primitive is visible immediately rather
# than at gate time (PRD FR-SPAWN, docs/TEST-CASES.md).
# clang leaves a linker-signed Mach-O with Info.plist=not bound. TCC then
# records a different identity than LaunchServices (`Rill` vs
# `dev.rill.spike0`), and AXIsProcessTrusted stays false after the user
# enables Rill in Accessibility. Seal inside-out; no hardened runtime —
# that flag blocks the HID post T-NFR needs.
BUNDLE_ID="dev.rill.spike0"
codesign --force --sign - --identifier "${BUNDLE_ID}.rilld" "$MACOS/rilld"
codesign --force --sign - --identifier "$BUNDLE_ID" "$MACOS/Rill"
codesign --force --sign - --identifier "$BUNDLE_ID" "$APP"

echo "-- PTY-creation imports in the packaged GUI (expect none) --"
nm -u "$MACOS/Rill" \
  | grep -E '_forkpty|_openpty|_posix_openpt|_grantpt|_unlockpt|_ptsname|_login_tty' \
  || echo "  none"

echo "packaged $APP"
