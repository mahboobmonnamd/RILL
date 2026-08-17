#!/bin/sh
# Run gate-closing T-NFR hid from Terminal.app (not Cursor).
# Accessibility must already be enabled for dist/Rill.app.
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

echo "==== power ===="
pmset -g batt

APP="$ROOT/dist/Rill.app"
RILLD="$APP/Contents/MacOS/rilld"
require() { echo "PRECONDITION FAILED: $1" >&2; exit 1; }
[ -x "$RILLD" ] || require "packaged rilld at $RILLD"
[ -x "$APP/Contents/MacOS/Rill" ] || require "packaged GUI at $APP"

SOCK="/tmp/rill-nfr-user.sock"
OUT="/tmp/rill-nfr-hid.out"
ERR="/tmp/rill-nfr-hid.err"
: > "$OUT"
: > "$ERR"
pkill -x rilld 2>/dev/null || true
pkill -x Rill 2>/dev/null || true
sleep 0.3
rm -f "$SOCK" "${SOCK}.lock"

RILL_SOCKET="$SOCK" "$RILLD" &
RILLD_PID=$!
trap 'kill "$RILLD_PID" 2>/dev/null || true' EXIT
i=0
while [ "$i" -lt 100 ]; do
  [ -S "$SOCK" ] && break
  i=$((i + 1))
  sleep 0.05
done
[ -S "$SOCK" ] || require "rilld did not bind $SOCK"

echo "==== T-NFR hid ===="
echo "Rill will enter a fullscreen Space. Leave it in front until it quits."
echo "Cmd-Q if it does not exit. Re-add Accessibility after every rebuild."
open -n -W --stdout "$OUT" --stderr "$ERR" \
  --env "RILL_SOCKET=$SOCK" \
  "$APP" --args --nfr-key=hid

echo
echo "==== stdout ($OUT) ===="
cat "$OUT"
echo "==== stderr ($ERR) ===="
cat "$ERR"

kill "$RILLD_PID" 2>/dev/null || true
wait "$RILLD_PID" 2>/dev/null || true
rm -f "$SOCK" "${SOCK}.lock"
trap - EXIT
echo
echo "Done. If paste into chat fails, say 'read the nfr files' — they are"
echo "  $OUT"
echo "  $ERR"
