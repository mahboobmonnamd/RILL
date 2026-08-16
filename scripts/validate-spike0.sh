#!/bin/sh
# Named Spike 0 gates. Socket-only tests do not close T-KILL, T-SPAWN, or T-NFR.
# Evidence: Proven / Partial / Manual / External. Packaged-app gates are not
# proven by in-process fixtures. T-NFR is Proven only on battery.
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

echo "== T-BYTES / T-DROP / T-RESIZE / T-EXIT / T-ATTACH / T-RESYNC / T-KILL (library + e2e) =="
cargo test --workspace --offline -- --test-threads=1

echo "== package =="
sh scripts/package-macos.sh
GUI="$ROOT/dist/Rill.app/Contents/MacOS/Rill"
RILLD="$ROOT/dist/Rill.app/Contents/MacOS/rilld"
test -x "$GUI" || fail "packaged GUI missing"
test -x "$RILLD" || fail "packaged rilld missing"

echo "== T-SPAWN (packaged GUI) =="
RILL_REQUIRE_PACKAGE=1 cargo test -p rill-host --offline --test t_spawn -- --nocapture

echo "== T-KILL packaged rilld (quit process group) =="
RILL_RILLD_BIN="$RILLD" cargo test -p rilld --offline --test persist_e2e -- --nocapture

echo "== T-NFR packaged Rill --nfr-key =="
SOCK="/tmp/rill-spike0-validate-$$.sock"
export RILL_SOCKET="$SOCK"
"$RILLD" &
RILLD_PID=$!
cleanup() {
  kill "$RILLD_PID" 2>/dev/null || true
  wait "$RILLD_PID" 2>/dev/null || true
  rm -f "$SOCK"
}
trap cleanup EXIT
i=0
while [ "$i" -lt 50 ]; do
  if [ -S "$SOCK" ]; then
    break
  fi
  i=$((i + 1))
  sleep 0.05
done
test -S "$SOCK" || fail "packaged rilld did not bind $SOCK"

set +e
NFR_OUT="$("$GUI" --nfr-key 2>&1)"
NFR_RC=$?
set -e
echo "$NFR_OUT"
echo "$NFR_OUT" | grep -q 'control_rpc=0' || fail "T-NFR control RPC on warm path"
test "$NFR_RC" -eq 0 || fail "T-NFR packaged --nfr-key failed (rc=$NFR_RC)"

BATT=$(echo "$NFR_OUT" | sed -n 's/.*battery=\([01]\).*/\1/p' | tail -1)
echo
echo "==== Spike 0 validation summary ===="
echo "T-BYTES   library: run above"
echo "T-DROP    library: run above"
echo "T-ATTACH  library: run above"
echo "T-RESIZE  library: run above"
echo "T-EXIT    library: run above"
echo "T-SPAWN   packaged nm: pass"
echo "T-KILL    persist_e2e cargo + packaged rilld: pass"
echo "T-RESYNC  library + persist reconnect: pass"
if [ "$BATT" = "1" ]; then
  echo "T-NFR     packaged --nfr-key on battery: p95 gate passed (still Partial until key→GPU frame)"
else
  echo "T-NFR     packaged --nfr-key on AC: Partial (Proven requires battery + key→first GPU frame)"
fi
echo "Stop rule: do not open Milestone 1 until T-NFR is Proven on a packaged build on battery."
