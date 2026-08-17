#!/bin/sh
# T-NFR required mutation (ADR 0002 D3, ADR 0009 D5).
# Restores the 60 Hz NSTimer. Must miss p95 on this presenter.
# Run from Terminal.app (not Cursor). Expect RED. Green here is a broken instrument.
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

SOCK="/tmp/rill-nfr-timer-pump.sock"
OUT="/tmp/rill-nfr-timer-pump.out"
ERR="/tmp/rill-nfr-timer-pump.err"
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

echo "==== T-NFR hid RILL_MUTATE=timer_pump (must miss) ===="
echo "Rill will enter a fullscreen Space. Leave it in front until it quits."
open -n -W --stdout "$OUT" --stderr "$ERR" \
  --env "RILL_SOCKET=$SOCK" --env "RILL_MUTATE=timer_pump" \
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

if grep -q 'missed the' "$ERR"; then
  echo
  echo "ok: timer_pump went red (ADR 0009 D5)."
  exit 0
fi
if grep -q '^T-NFR mode=hid' "$OUT" && ! grep -q '^T-NFR: ' "$ERR"; then
  echo
  echo "BROKEN INSTRUMENT: timer_pump stayed green." >&2
  exit 1
fi
echo
echo "timer_pump did not produce a p95 miss. See $OUT $ERR" >&2
exit 1
