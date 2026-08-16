#!/bin/sh
# Executable form of AGENTS.md §5 and ADR 0002 D9.
#
# These were review conventions. Review conventions do not survive five people
# working in parallel, so they are checks now. Runs in fast.yml on Linux with no
# Zig toolchain.
#
# Comments are stripped before matching: a file that *documents* a prohibition
# must not trip the check for it. Excluding this script from its own scans is
# for the same reason.
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

FAILED=0
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

fail() {
  printf 'lint-planes: FAIL [%s] %s\n' "$1" "$2" >&2
  FAILED=1
}

# Strip // /// //! # line comments, /* */ blocks, and doc prose, so a comment
# describing a rule is never mistaken for a violation of it.
strip_comments() {
  python3 - "$@" <<'PY'
import re, sys
for path in sys.argv[1:]:
    try:
        src = open(path, encoding="utf-8", errors="replace").read()
    except OSError:
        continue
    src = re.sub(r"/\*.*?\*/", "", src, flags=re.S)
    for i, line in enumerate(src.splitlines(), 1):
        code = re.sub(r"//.*$", "", line)
        if path.endswith((".sh", ".toml")):
            code = re.sub(r"#.*$", "", line)
        if code.strip():
            print(f"{path}:{i}:{code}")
PY
}

# scan <name> <message> <extended-regex> <file...>
scan() {
  name="$1"; msg="$2"; pattern="$3"; shift 3
  [ "$#" -gt 0 ] || return 0
  strip_comments "$@" > "$TMP/code" || true
  if hits="$(grep -E "$pattern" "$TMP/code")" && [ -n "$hits" ]; then
    fail "$name" "$msg"
    printf '%s\n' "$hits" | sed 's/^/    /' >&2
  fi
}

rust_src() { git ls-files 'crates/*/src/*.rs' 'crates/*/src/**/*.rs' 2>/dev/null; }
c_src()    { git ls-files 'crates/**/*.c' 'crates/**/*.h' 2>/dev/null; }
objc_src() { git ls-files 'host/**/*.m' 'host/**/*.h' 2>/dev/null; }
sh_src()   { git ls-files 'scripts/*.sh' 2>/dev/null | grep -v 'lint-planes.sh'; }

# --- no-master-export -------------------------------------------------------
# ADR 0001 D5, SPEC-KERNEL §1. The PTY master must not leave the kernel crate.
# shellcheck disable=SC2046
scan no-master-export \
  "kernel exports a raw fd; use a poll-only capability (SPEC-KERNEL §1)" \
  'pub (unsafe )?fn [^(]*\(.*\)[[:space:]]*->[^{;]*(RawFd|OwnedFd|BorrowedFd)' \
  $(git ls-files 'crates/rill-kernel/src/*.rs')

# shellcheck disable=SC2046
scan no-master-export-name \
  "leak_master_forbidden is the export it warns about; delete it" \
  'leak_master_forbidden' \
  $(rust_src)

# --- no-scm-rights ----------------------------------------------------------
# shellcheck disable=SC2046
scan no-scm-rights \
  "SCM_RIGHTS of the PTY master is forbidden (ADR 0001 D5)" \
  'SCM_RIGHTS' \
  $(rust_src) $(c_src) $(objc_src)

# --- no-seqpacket -----------------------------------------------------------
# shellcheck disable=SC2046
scan no-seqpacket \
  "Darwin has no SOCK_SEQPACKET (ADR 0001 D6)" \
  'SOCK_SEQPACKET' \
  $(rust_src) $(c_src) $(objc_src)

# --- no-ghostty-in-domain ---------------------------------------------------
# Ghostty FFI types live only in the adapter (ADR 0001 D1, SPEC-CHIP0 §1).
GHOSTTY_FILES="$(git ls-files 'crates/**/*.rs' 'crates/**/*.c' 'crates/**/*.h' 'host/**/*.m' \
  | grep -v '^crates/rill-chip0/src/adapter/' || true)"
if [ -n "$GHOSTTY_FILES" ]; then
  # shellcheck disable=SC2086
  scan no-ghostty-in-domain \
    "Ghostty identifiers outside crates/rill-chip0/src/adapter/" \
    'ghostty_[a-z_]+\(|Ghostty[A-Z][A-Za-z]*' \
    $GHOSTTY_FILES
fi

# --- no-cell-strings --------------------------------------------------------
# A String reachable from a POD snapshot is how the previous prototype died.
# Only the snapshot types themselves, not the whole crate.
if [ -f crates/rill-chip0/src/lib.rs ]; then
  hits="$(python3 - <<'PY'
import re
src = open("crates/rill-chip0/src/lib.rs", encoding="utf-8").read()
src = re.sub(r"/\*.*?\*/", "", src, flags=re.S)
for name in ("PodCell", "PodGrid"):
    m = re.search(r"pub struct %s\s*\{(.*?)\n\}" % name, src, flags=re.S)
    if not m:
        continue
    for i, line in enumerate(m.group(1).splitlines(), 1):
        code = re.sub(r"//.*$", "", line)
        if re.search(r"\b(String|&str|Cow<)", code):
            print(f"{name} field: {code.strip()}")
PY
)"
  if [ -n "$hits" ]; then
    fail no-cell-strings "String in a POD snapshot type (AGENTS.md §5)"
    printf '%s\n' "$hits" | sed 's/^/    /' >&2
  fi
fi

# --- no-unwrap-in-daemon ----------------------------------------------------
# PRD NFR-FAIL. src/ only; tests live in tests/ and may unwrap freely.
for crate in rill-kernel rill-attach rilld; do
  files="$(git ls-files "crates/$crate/src/*.rs" "crates/$crate/src/**/*.rs" 2>/dev/null || true)"
  [ -n "$files" ] || continue
  # shellcheck disable=SC2086
  strip_comments $files > "$TMP/code" || true
  # #[cfg(test)] blocks inside src/ are still exempt.
  hits="$(python3 - "$TMP/code" <<'PY'
import re, sys
in_test = {}
for raw in open(sys.argv[1], encoding="utf-8"):
    m = re.match(r"([^:]+):(\d+):(.*)", raw.rstrip("\n"))
    if not m:
        continue
    path, _, code = m.group(1), m.group(2), m.group(3)
    if "#[cfg(test)]" in code:
        in_test[path] = True
    if in_test.get(path):
        continue
    if re.search(r"\.unwrap\(\)|\.expect\(|(^|[^a-z_])panic!\(|unreachable!\(", code):
        print(raw.rstrip("\n"))
PY
)"
  if [ -n "$hits" ]; then
    fail no-unwrap-in-daemon "$crate: unwrap/expect/panic on a production path (PRD NFR-FAIL)"
    printf '%s\n' "$hits" | sed 's/^/    /' >&2
  fi
done

# --- no-json-on-warm-path ---------------------------------------------------
# shellcheck disable=SC2046
scan no-json-on-warm-path \
  "JSON is orchestration only; not in kernel/attach/display (AGENTS.md §5)" \
  'serde_json' \
  $(git ls-files 'crates/rill-kernel/**' 'crates/rill-attach/**' 'crates/rill-chip0/**' 'crates/rill-host/**' | grep -E '\.(rs|toml)$')

# --- no-gui-pty -------------------------------------------------------------
# The link-level gate is T-SPAWN; this catches it at review time too.
# shellcheck disable=SC2046
scan no-gui-pty \
  "GUI must not create a PTY (PRD FR-SPAWN)" \
  '(forkpty|openpty|posix_openpt|grantpt|unlockpt|login_tty)[[:space:]]*\(' \
  $(objc_src)

# --- no-select --------------------------------------------------------------
# select()/fd_set is UB for fd >= FD_SETSIZE (audit S3-8c).
# shellcheck disable=SC2046
scan no-select \
  "use poll, not select (SPEC-KERNEL §8)" \
  'libc::select|FD_SET\(|fd_set' \
  $(rust_src)

# --- no-skip-flags ----------------------------------------------------------
# ADR 0002 D5: a missing precondition is a failure, not a green skip.
# shellcheck disable=SC2046
scan no-skip-flags \
  "RILL_REQUIRE_* opt-in skip flags are deleted (ADR 0002 D5)" \
  'RILL_REQUIRE_' \
  $(rust_src) $(git ls-files 'crates/**/tests/*.rs') $(sh_src)

# --- no-self-certifying-oracle ----------------------------------------------
# ADR 0002 D4. The specific shapes the audit found.
# shellcheck disable=SC2046
scan no-self-certifying \
  "predicate hardcoded to the passing value (ADR 0002 D4)" \
  'fn is_control_rpc|fn [a-z_]+\(&self\) -> bool \{[[:space:]]*(false|true)[[:space:]]*\}' \
  $(rust_src)

# shellcheck disable=SC2046
scan no-fed-buffer \
  "Chip0 must not retain fed bytes; that oracle is self-referential and leaks (SPEC-CHIP0 §3)" \
  'fn bytes_fed\(&self\) -> &\[u8\]|fed: Vec<u8>' \
  $(rust_src)

# --- no-nm-U ----------------------------------------------------------------
# The exact mistake that made T-SPAWN unfalsifiable: -U lists *defined*
# symbols, and every symbol the gate asserts on is an import.
# shellcheck disable=SC2046
scan no-nm-defined-only \
  "T-SPAWN must inspect imports (nm -u / otool -Iv), never nm -U (audit S1-1)" \
  'nm[^\n]*"-U"|nm -U' \
  $(git ls-files 'crates/**/tests/*.rs') $(sh_src)

# TODO(lane-c): reject a fixed-size buffer passed to GRAPHEMES_BUF without a
#   preceding GRAPHEMES_LEN query. Needs a real C parse, not grep. Tracked here
#   rather than in a document so it stays visible (ADR 0002 D9).
# TODO(lane-b): reject naked read/write on the attach socket outside the codec.

if [ "$FAILED" -ne 0 ]; then
  echo "lint-planes: plane violations found" >&2
  exit 1
fi
echo "lint-planes: ok"
