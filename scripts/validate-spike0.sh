#!/bin/sh
# Spike 0 gate runner. ADR 0002 D5, D10.
#
# Rules this script obeys, learned the hard way (see docs/SPIKE-0-AUDIT.md S4-3,
# where the previous version printed "pass" for three gates and never ran one of
# them):
#
#   1. No summary line is printed without a recorded result.
#   2. A missing precondition is a FAILURE, never a skip.
#   3. Every result goes into evidence/spike0-<utc>.json; the human summary is
#      rendered from that file.
#   4. --negative-controls asserts each gate goes RED under its own mutation.
#      A gate that stays green under its mutation is a broken instrument and
#      fails the run regardless of what the unmutated gate did.
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

NEGATIVE_CONTROLS=0
[ "${1:-}" = "--negative-controls" ] && NEGATIVE_CONTROLS=1

UTC="$(date -u +%Y%m%dT%H%M%SZ)"
EVIDENCE_DIR="$ROOT/evidence"
EVIDENCE="$EVIDENCE_DIR/spike0-$UTC.json"
mkdir -p "$EVIDENCE_DIR"

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"; kill ${RILLD_PID:-} 2>/dev/null || true' EXIT

RESULTS="$TMP/gates.jsonl"
CONTROLS="$TMP/controls.jsonl"
: > "$RESULTS"
: > "$CONTROLS"

ANY_FAIL=0

json_escape() { python3 -c 'import json,sys; print(json.dumps(sys.stdin.read()))'; }

# run_gate <id> <command...>
run_gate() {
  id="$1"; shift
  echo "== $id =="
  out="$TMP/$id.out"
  set +e
  "$@" > "$out" 2>&1
  rc=$?
  set -e
  cat "$out"
  cls="Red"
  [ "$rc" -eq 0 ] && cls="Green-unproven"   # ADR 0002 D2: never "Proven" from a bare pass
  [ "$rc" -ne 0 ] && ANY_FAIL=1
  printf '{"id":%s,"command":%s,"exit":%d,"class":%s,"stdout":%s}\n' \
    "\"$id\"" \
    "$(printf '%s ' "$@" | json_escape)" \
    "$rc" \
    "\"$cls\"" \
    "$(cat "$out" | json_escape)" >> "$RESULTS"
  return 0
}

# Record a gate that hosted macos-14 cannot close (no panel, no Spaces, no
# Retina). Evidence still stores Red; the GitHub job does not fail for it.
# Same contract as T-NFR under RILL_NFR_OPTIONAL (ADR 0009 D4).
run_gate_hosted_optional() {
  before=$ANY_FAIL
  run_gate "$@"
  if [ "${RILL_NFR_OPTIONAL:-}" = 1 ]; then
    ANY_FAIL=$before
  fi
}

# run_control <gate-id> <mutation> <command...>
# The mutation MUST turn the gate red. Green here means the gate is blind.
run_control() {
  id="$1"; mut="$2"; shift 2
  echo "== negative control: $id / $mut =="
  set +e
  RILL_MUTATE="$mut" "$@" > "$TMP/$id.$mut.out" 2>&1
  rc=$?
  set -e
  if [ "$rc" -eq 0 ]; then
    echo "BROKEN INSTRUMENT: $id stayed green under mutation '$mut'." >&2
    echo "  The gate cannot detect the defect it exists to detect (ADR 0002 D3)." >&2
    ANY_FAIL=1
    went_red=false
  else
    echo "ok: $id went red under '$mut'"
    went_red=true
  fi
  printf '{"gate":"%s","mutation":"%s","went_red":%s}\n' "$id" "$mut" "$went_red" >> "$CONTROLS"
}

require() {
  what="$1"; shift
  if ! "$@"; then
    echo "PRECONDITION FAILED: $what" >&2
    echo "  ADR 0002 D5: a missing precondition is a failure, not a skip." >&2
    exit 1
  fi
}

# ---------------------------------------------------------------- preconditions
echo "== preconditions =="
require "third_party/ghostty.pin exists" test -f third_party/ghostty.pin
GHOSTTY_SHA="$(sed -n 's/^sha *= *//p' third_party/ghostty.pin | tr -d '[:space:]')"
GHOSTTY_DIR="${RILL_GHOSTTY_DIR:-$ROOT/third_party/ghostty}"
require "libghostty-vt built at the pin (run scripts/fetch-libghostty-vt.sh)" \
  test -f "$GHOSTTY_DIR/.rill-built-sha"
require "libghostty-vt provenance matches the pin" \
  test "$(cat "$GHOSTTY_DIR/.rill-built-sha")" = "$GHOSTTY_SHA"
echo "libghostty-vt: $GHOSTTY_SHA"

echo "== plane lints =="
run_gate "LINT-PLANES" sh scripts/lint-planes.sh

# ------------------------------------------------------------------ library tier
run_gate "T-BYTES-chip"   cargo test -p rill-chip0 --offline t_bytes -- --nocapture
run_gate "T-LOOK"         cargo test -p rill-chip0 --offline t_ghostty_look -- --nocapture
# Isolate ASan: instrumented C objects in the default target dir break later
# rilld / persist_e2e links (rustc -nodefaultlibs does not pull clang_rt).
run_gate "T-BYTES-asan"   env CARGO_TARGET_DIR="$TMP/asan-target" RILL_ASAN=1 \
  cargo test -p rill-chip0 --offline t_bytes -- --nocapture
run_gate "T-BYTES-kernel" cargo test -p rill-kernel --offline t_bytes -- --nocapture
run_gate "T-DROP"         cargo test -p rill-kernel --offline t_drop -- --test-threads=1 --nocapture
run_gate "T-RESIZE"       cargo test -p rill-kernel --offline t_resize -- --nocapture
run_gate "T-EXIT"         cargo test -p rill-kernel --offline t_exit -- --nocapture
run_gate "T-EXIT-detach"  cargo test -p rilld       --offline t_exit_across_detach -- --nocapture
run_gate "T-ATTACH"       cargo test -p rilld       --offline t_attach -- --test-threads=1 --nocapture
run_gate "T-RESYNC"       cargo test -p rilld       --offline t_resync -- --test-threads=1 --nocapture
run_gate "T-GRAPH-SPAWN"  cargo test -p rill-kernel --offline t_graph_two_sessions_have_distinct_child_pids -- --nocapture
run_gate "T-GRAPH-ISOLATE" cargo test -p rill-kernel --offline t_graph_histories_do_not_mix -- --nocapture
run_gate "T-GRAPH-ATTACH" cargo test -p rill-kernel --offline t_graph_second_attach_to_same_id_is_refused -- --nocapture
run_gate "T-GRAPH-ATTACH-B" cargo test -p rill-kernel --offline t_graph_attach_to_a_second_id_is_accepted -- --nocapture
run_gate "T-GRAPH-TERMINATE" cargo test -p rill-kernel --offline t_graph_terminate_one_leaf_leaves_the_other_alive -- --nocapture
run_gate "T-ATTACH-NAMED" cargo test -p rilld       --offline t_attach_named_id -- --test-threads=1 --nocapture
run_gate "T-GRAPH-FLOOD"  cargo test -p rilld       --offline t_graph_flood -- --test-threads=1 --nocapture
run_gate "T-ATTACH-PROTO" cargo test -p rilld       --offline t_attach_protocol_mismatch -- --test-threads=1 --nocapture
run_gate "T-GRAPH-NESTED" cargo test -p rilld       --offline t_nested_rilld_bind -- --test-threads=1 --nocapture
run_gate "T-GRAPH-DELIVERY" cargo test -p rill-kernel --offline t_graph_input_write_is_dispatched -- --nocapture
run_gate "T-GRAPH-EVENTS" cargo test -p rill-kernel --offline t_graph_event_ids_are_unique -- --nocapture
run_gate "T-GRAPH-LAYOUT" cargo test -p rill-kernel --offline t_graph_layout_snapshot -- --nocapture
run_gate "T-GRAPH-EPHEMERAL" cargo test -p rill-kernel --offline t_graph_ephemeral_drop -- --test-threads=1 --nocapture
run_gate "T-GRAPH-OBSERVE" cargo test -p rilld       --offline t_observe_attach -- --test-threads=1 --nocapture
run_gate "T-GRAPH-KILL-N" cargo test -p rilld       --offline t_graph_dropping_the_daemon -- --test-threads=1 --nocapture

# ---------------------------------------------------------------------- package
echo "== package =="
run_gate "PACKAGE" sh scripts/package-macos.sh
GUI="$ROOT/dist/Rill.app/Contents/MacOS/Rill"
RILLD="$ROOT/dist/Rill.app/Contents/MacOS/rilld"
require "packaged GUI at $GUI" test -x "$GUI"
require "packaged rilld at $RILLD" test -x "$RILLD"

# T-SPAWN carries its own permanent positive control: the same check is run
# against a fixture that DOES create a PTY and must report a violation.
run_gate "T-SPAWN" cargo test -p rill-host --offline --test t_spawn -- --nocapture

run_gate "T-KILL" env RILL_RILLD_BIN="$RILLD" RILL_GUI_APP="$ROOT/dist/Rill.app" \
  cargo test -p rilld --offline --test persist_e2e -- --nocapture
run_gate_hosted_optional "T-FS-EXIT" env RILL_GUI_APP="$ROOT/dist/Rill.app" \
  cargo test -p rill-host --offline --test t_fullscreen_exit -- --nocapture
run_gate "T-WINDOWED" env RILL_GUI_APP="$ROOT/dist/Rill.app" \
  cargo test -p rill-host --offline --test t_windowed -- --nocapture
run_gate "T-LOOK-GLASS" env RILL_GUI_APP="$ROOT/dist/Rill.app" \
  cargo test -p rill-host --offline --test t_look_glass -- --nocapture
run_gate_hosted_optional "T-GLYPH-SCALE" env RILL_GUI_APP="$ROOT/dist/Rill.app" \
  cargo test -p rill-host --offline --test t_glyph_scale -- --nocapture
run_gate "T-SPLIT" env RILL_GUI_APP="$ROOT/dist/Rill.app" \
  cargo test -p rill-host --offline --test t_split -- --nocapture
run_gate "T-NAV-IDENTITY" env RILL_GUI_APP="$ROOT/dist/Rill.app" \
  cargo test -p rill-host --offline --test t_host_identity -- --nocapture
run_gate "T-SPLIT-LOOK" env RILL_GUI_APP="$ROOT/dist/Rill.app" \
  cargo test -p rill-host --offline --test t_split_look \
  t_chrome_nav_background -- --nocapture
run_gate "T-CHROME-INSET" env RILL_GUI_APP="$ROOT/dist/Rill.app" \
  cargo test -p rill-host --offline --test t_split_look \
  t_chrome_heading_top_inset -- --nocapture
run_gate "T-CHROME-FONT" env RILL_GUI_APP="$ROOT/dist/Rill.app" \
  cargo test -p rill-host --offline --test t_split_look \
  t_chrome_section_font -- --nocapture
run_gate "T-DOCK-REOPEN" env RILL_GUI_APP="$ROOT/dist/Rill.app" \
  cargo test -p rill-host --offline --test t_dock_reopen -- --nocapture

# ------------------------------------------------------------------------ T-NFR
echo "== T-NFR =="
SOCK="$TMP/rill-nfr.sock"
RILL_SOCKET="$SOCK" "$RILLD" &
RILLD_PID=$!
i=0
while [ "$i" -lt 100 ]; do
  [ -S "$SOCK" ] && break
  i=$((i + 1)); sleep 0.05
done
require "packaged rilld bound $SOCK" test -S "$SOCK"

# ADR 0003 D7: hid mode closes the gate; app mode is a CI diagnostic and is
# marked as not gate-closing. Neither is skipped.
# Launch the .app through LaunchServices so TCC Accessibility attaches to
# the bundle. Direct `Contents/MacOS/Rill` is not the identity the user
# enabled; `open` exits 0 even when the app does not, so grade the report.
NFR_MODE="${RILL_NFR_MODE:-hid}"
APP="$ROOT/dist/Rill.app"
run_t_nfr() {
  nfr_out="$TMP/T-NFR.app.out"
  nfr_err="$TMP/T-NFR.app.err"
  : > "$nfr_out"
  : > "$nfr_err"
  # Never `open -W`. On GitHub-hosted macos-14, CAMetalLayer nextDrawable can
  # block forever and -W then never returns, so the job sits until the 90
  # minute timeout — after every gate CI can actually close already passed
  # (ADR 0009 D4). Poll the report files; reap the app at the deadline.
  timeout_sec="${RILL_NFR_TIMEOUT_SEC:-200}"
  if [ "${RILL_NFR_OPTIONAL:-}" = 1 ]; then
    timeout_sec="${RILL_NFR_TIMEOUT_SEC:-45}"
  fi
  echo "T-NFR: launching $APP --nfr-key=$NFR_MODE (bound ${timeout_sec}s)"
  if [ -n "${RILL_MUTATE:-}" ]; then
    open -n --stdout "$nfr_out" --stderr "$nfr_err" \
      --env "RILL_SOCKET=$SOCK" --env "RILL_MUTATE=$RILL_MUTATE" \
      "$APP" --args "--nfr-key=$NFR_MODE"
  else
    open -n --stdout "$nfr_out" --stderr "$nfr_err" \
      --env "RILL_SOCKET=$SOCK" "$APP" --args "--nfr-key=$NFR_MODE"
  fi
  elapsed=0
  while [ "$elapsed" -lt "$timeout_sec" ]; do
    if grep -q '^T-NFR mode=' "$nfr_out" 2>/dev/null; then
      break
    fi
    if grep -q '^T-NFR: ' "$nfr_err" 2>/dev/null; then
      sleep 1
      break
    fi
    sleep 1
    elapsed=$((elapsed + 1))
    if [ $((elapsed % 10)) -eq 0 ]; then
      echo "T-NFR: still waiting (${elapsed}s/${timeout_sec}s)"
    fi
  done
  if [ "$elapsed" -ge "$timeout_sec" ]; then
    echo "T-NFR: timed out after ${timeout_sec}s (no panel or nextDrawable blocked)" >&2
  fi
  killall -TERM Rill 2>/dev/null || true
  sleep 1
  killall -KILL Rill 2>/dev/null || true
  cat "$nfr_out"
  cat "$nfr_err" >&2
  if grep -q '^T-NFR: ' "$nfr_err"; then
    return 1
  fi
  grep -q '^T-NFR mode=' "$nfr_out"
}
# GitHub-hosted macos-14 has no panel. Record T-NFR but do not fail the suite
# for it (ADR 0009 D4). Other library gates still fail the job.
run_gate_hosted_optional "T-NFR" run_t_nfr

# --------------------------------------------------------------- negative controls
if [ "$NEGATIVE_CONTROLS" -eq 1 ]; then
  echo
  echo "== negative controls (ADR 0002 D3) =="
  # Mutations live behind the `mutate` cargo feature, so shipping builds carry
  # no mutation code at all (ADR 0002 D3). Pass `--features` and `mutate` as
  # separate argv words: a single `--features mutate` token is a cargo error
  # (zsh does not split `$MUT`, and that red is not a demonstrated mutation).
  run_control "T-BYTES-chip" drop_high_bytes \
    cargo test -p rill-chip0 --offline --features mutate t_bytes
  run_control "T-LOOK-OVERLAY" skip_ghostty_overlay \
    cargo test -p rill-chip0 --offline --features mutate t_ghostty_look_overlay -- --nocapture
  run_control "T-LOOK-UNKNOWN" unknown_theme_wipes \
    cargo test -p rill-chip0 --offline --features mutate t_ghostty_look_unknown -- --nocapture
  run_control "T-LOOK-CELL" skip_theme_apply \
    cargo test -p rill-chip0 --offline --features mutate t_ghostty_look_themed_empty -- --nocapture
  run_control "T-LOOK-FILE" invent_theme_rgb \
    cargo test -p rill-chip0 --offline --features mutate t_ghostty_look_theme_file -- --nocapture
  run_control "T-LOOK-ANSI" skip_vt_look_colors \
    cargo test -p rill-chip0 --offline --features mutate t_ghostty_look_sgr_green -- --nocapture
  run_control "T-DROP" drop_on_full \
    cargo test -p rill-kernel --offline --features mutate t_drop -- --test-threads=1
  run_control "T-RESIZE" resize_before_data \
    cargo test -p rill-kernel --offline --features mutate t_resize
  run_control "T-EXIT-detach" clear_outbound_on_detach \
    cargo test -p rilld --offline --features mutate t_exit_across_detach
  run_control "T-ATTACH" accept_replaces_client \
    cargo test -p rilld --offline --features mutate t_attach -- --test-threads=1
  run_control "T-RESYNC" no_resync \
    cargo test -p rilld --offline --features mutate t_resync -- --test-threads=1
  run_control "T-GRAPH-SPAWN" single_session \
    cargo test -p rill-kernel --offline --features mutate t_graph_two_sessions_have_distinct_child_pids
  run_control "T-GRAPH-ISOLATE" single_session \
    cargo test -p rill-kernel --offline --features mutate t_graph_histories_do_not_mix
  run_control "T-GRAPH-ATTACH-B" single_session \
    cargo test -p rill-kernel --offline --features mutate t_graph_attach_to_a_second_id_is_accepted
  run_control "T-GRAPH-TERMINATE" terminate_all_leaves \
    cargo test -p rill-kernel --offline --features mutate t_graph_terminate_one_leaf_leaves_the_other_alive
  run_control "T-ATTACH-NAMED" ignore_session_id \
    cargo test -p rilld --offline --features mutate t_attach_named_id -- --test-threads=1
  run_control "T-GRAPH-FLOOD" starve_other_leaves \
    cargo test -p rilld --offline --features mutate t_graph_flood -- --test-threads=1
  run_control "T-ATTACH-PROTO" ignore_protocol_version \
    cargo test -p rilld --offline --features mutate t_attach_protocol_mismatch -- --test-threads=1
  run_control "T-GRAPH-NESTED" skip_nested_guard \
    cargo test -p rilld --offline --features mutate t_nested_rilld_bind -- --test-threads=1
  run_control "T-GRAPH-DELIVERY" always_pending \
    cargo test -p rill-kernel --offline --features mutate t_graph_input_write_is_dispatched
  run_control "T-GRAPH-EVENTS" duplicate_event_ids \
    cargo test -p rill-kernel --offline --features mutate t_graph_event_ids_are_unique
  run_control "T-GRAPH-LAYOUT" omit_second_leaf \
    cargo test -p rill-kernel --offline --features mutate t_graph_layout_snapshot
  run_control "T-GRAPH-EPHEMERAL" ignore_ephemeral \
    cargo test -p rill-kernel --offline --features mutate t_graph_ephemeral_drop -- --test-threads=1
  run_control "T-GRAPH-OBSERVE" allow_observer_write \
    cargo test -p rilld --offline --features mutate t_observe_attach -- --test-threads=1
  run_control "T-PARTIAL-WRITE" replay_full_frame \
    cargo test -p rilld --offline --features mutate t_outbound_partial_write -- --test-threads=1
  run_control "T-ATTACHED-POLL" idle_poll_while_attached \
    cargo test -p rilld --offline --features mutate t_attached_session_poll_does_not_sleep -- --test-threads=1
  if [ "${RILL_NFR_OPTIONAL:-}" = 1 ]; then
    # Hosted 1× / no Spaces: these mutations go red for the missing
    # precondition, not for the defect, so they are not D3 evidence.
    printf '{"gate":"T-NFR","mutation":"timer_pump","went_red":null}\n' >> "$CONTROLS"
    printf '{"gate":"T-FS-EXIT","mutation":"wait_forever_on_inflight","went_red":null}\n' >> "$CONTROLS"
    printf '{"gate":"T-GLYPH-SCALE","mutation":"skip_glyph_backing_scale","went_red":null}\n' >> "$CONTROLS"
  else
    run_control "T-NFR" timer_pump run_t_nfr
  fi
  run_t_kill_setsid() {
    mut_app="$TMP/Rill-nosetsid.app"
    RILL_MUTATE=drop_POSIX_SPAWN_SETSID RILL_APP="$mut_app" sh scripts/package-macos.sh
    env RILL_RILLD_BIN="$mut_app/Contents/MacOS/rilld" RILL_GUI_APP="$mut_app" \
      cargo test -p rilld --offline --test persist_e2e -- --nocapture
  }
  run_control "T-KILL" drop_POSIX_SPAWN_SETSID run_t_kill_setsid
  if [ "${RILL_NFR_OPTIONAL:-}" != 1 ]; then
    run_control "T-FS-EXIT" wait_forever_on_inflight \
      env RILL_GUI_APP="$ROOT/dist/Rill.app" \
      cargo test -p rill-host --offline --test t_fullscreen_exit -- --nocapture
  fi
  run_control "T-WINDOWED" always_toggle_fullscreen \
    env RILL_GUI_APP="$ROOT/dist/Rill.app" \
    cargo test -p rill-host --offline --test t_windowed -- --nocapture
  run_control "T-LOOK-GLASS" window_alpha_from_opacity \
    env RILL_GUI_APP="$ROOT/dist/Rill.app" \
    cargo test -p rill-host --offline --test t_look_glass -- --nocapture
  if [ "${RILL_NFR_OPTIONAL:-}" != 1 ]; then
    run_control "T-GLYPH-SCALE" skip_glyph_backing_scale \
      env RILL_GUI_APP="$ROOT/dist/Rill.app" \
      cargo test -p rill-host --offline --test t_glyph_scale -- --nocapture
  fi
  run_control "T-SPLIT" no_chrome \
    env RILL_GUI_APP="$ROOT/dist/Rill.app" \
    cargo test -p rill-host --offline --test t_split -- --nocapture
  run_control "T-NAV-IDENTITY" host_indicator_from_home \
    env RILL_GUI_APP="$ROOT/dist/Rill.app" \
    cargo test -p rill-host --offline --test t_host_identity -- --nocapture
  run_control "T-SPLIT-LOOK" hardcoded_chrome_gray \
    env RILL_GUI_APP="$ROOT/dist/Rill.app" \
    cargo test -p rill-host --offline --test t_split_look \
    t_chrome_nav_background -- --nocapture
  run_control "T-CHROME-INSET" hardcoded_chrome_y \
    env RILL_GUI_APP="$ROOT/dist/Rill.app" \
    cargo test -p rill-host --offline --test t_split_look \
    t_chrome_heading_top_inset -- --nocapture
  run_control "T-CHROME-FONT" tiny_chrome_font \
    env RILL_GUI_APP="$ROOT/dist/Rill.app" \
    cargo test -p rill-host --offline --test t_split_look \
    t_chrome_section_font -- --nocapture
  run_control "T-DOCK-REOPEN" skip_dock_reopen \
    env RILL_GUI_APP="$ROOT/dist/Rill.app" \
    cargo test -p rill-host --offline --test t_dock_reopen -- --nocapture
  run_t_spawn_openpty() {
    mut_app="$TMP/Rill-openpty.app"
    RILL_MUTATE=openpty_in_main_m RILL_APP="$mut_app" sh scripts/package-macos.sh
    RILL_GUI_BIN="$mut_app/Contents/MacOS/Rill" \
      cargo test -p rill-host --offline --test t_spawn \
      t_spawn_gui_binary_does_not_import_pty_creation_symbols -- --nocapture
  }
  run_control "T-SPAWN" openpty_in_main_m run_t_spawn_openpty
fi

# ------------------------------------------------------------------- evidence
POWER="ac"
if command -v pmset >/dev/null 2>&1 && pmset -g batt 2>/dev/null | grep -q "Battery Power"; then
  POWER="battery"
fi
REFRESH="$(system_profiler SPDisplaysDataType 2>/dev/null \
  | sed -n 's/.*UI Looks like:.*@ \([0-9]*\)Hz.*/\1/p' | head -1)"
[ -n "$REFRESH" ] || REFRESH="null"

{
  printf '{\n'
  printf '  "utc": "%s",\n' "$UTC"
  printf '  "git_sha": "%s",\n' "$(git rev-parse HEAD 2>/dev/null || echo unknown)"
  printf '  "git_dirty": %s,\n' "$([ -n "$(git status --porcelain 2>/dev/null)" ] && echo true || echo false)"
  printf '  "ghostty_sha": "%s",\n' "$GHOSTTY_SHA"
  printf '  "host": {"model": "%s", "macos": "%s", "power": "%s", "refresh_hz": %s},\n' \
    "$(sysctl -n hw.model 2>/dev/null || uname -m)" \
    "$(sw_vers -productVersion 2>/dev/null || uname -sr)" \
    "$POWER" "$REFRESH"
  printf '  "nfr_mode": "%s",\n' "$NFR_MODE"
  printf '  "gates": [\n'
  sed '$!s/$/,/' "$RESULTS" | sed 's/^/    /'
  printf '  ],\n'
  printf '  "negative_controls": [\n'
  sed '$!s/$/,/' "$CONTROLS" | sed 's/^/    /'
  printf '  ]\n'
  printf '}\n'
} > "$EVIDENCE"

# ------------------------------------------------------------------- summary
# Rendered FROM the evidence file. Nothing is printed that was not measured.
echo
echo "==== Spike 0 — $UTC ===="
python3 - "$EVIDENCE" <<'PY'
import json, sys
d = json.load(open(sys.argv[1]))
print(f"git {d['git_sha'][:12]}{' (dirty)' if d['git_dirty'] else ''}  "
      f"ghostty {d['ghostty_sha'][:12]}  power={d['host']['power']}  "
      f"refresh={d['host']['refresh_hz']}Hz  nfr_mode={d['nfr_mode']}")
print()
for g in d["gates"]:
    mark = "ok " if g["exit"] == 0 else "RED"
    print(f"  {mark}  {g['id']:<16} {g['class']}")
nc = [c for c in d["negative_controls"] if c["went_red"] is not None]
if nc:
    print()
    for c in nc:
        mark = "ok " if c["went_red"] else "BROKEN INSTRUMENT"
        print(f"  {mark}  {c['gate']} / {c['mutation']}")
print()
print("Class 'Green-unproven' is NOT evidence. A gate reaches Proven only after")
print("being demonstrated red on a build lacking the behaviour (ADR 0002 D2).")
if d["host"]["power"] != "battery":
    print("T-NFR did not run on battery and therefore cannot close (PRD NFR-KEY).")
if d["nfr_mode"] != "hid":
    print("T-NFR ran in 'app' mode: diagnostic only, not gate-closing (ADR 0003 D7).")
PY
if [ "${RILL_NFR_OPTIONAL:-}" = 1 ]; then
  echo "Hosted macos-14: T-NFR, T-FS-EXIT, and T-GLYPH-SCALE are recorded but do not fail this job (no panel / Spaces / Retina)."
fi

echo
echo "evidence: $EVIDENCE"
[ "$ANY_FAIL" -eq 0 ] || { echo "one or more gates failed" >&2; exit 1; }
