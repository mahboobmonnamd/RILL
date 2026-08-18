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
# Require an identifier boundary so Chip 0 tests named t_ghostty_look_* are
# not mistaken for ghostty_*() FFI calls.
GHOSTTY_FILES="$(git ls-files 'crates/**/*.rs' 'crates/**/*.c' 'crates/**/*.h' 'host/**/*.m' \
  | grep -v '^crates/rill-chip0/src/adapter/' || true)"
if [ -n "$GHOSTTY_FILES" ]; then
  # shellcheck disable=SC2086
  scan no-ghostty-in-domain \
    "Ghostty identifiers outside crates/rill-chip0/src/adapter/" \
    '(^|[^A-Za-z_])ghostty_[a-z_]+\(|(^|[^A-Za-z_])Ghostty[A-Z][A-Za-z]*' \
    $GHOSTTY_FILES
fi

# --- no-cell-strings --------------------------------------------------------
# A String reachable from a POD snapshot is how the previous prototype died.
# Only the snapshot types themselves, not the whole crate.
hits="$(python3 - <<'PY'
import os, re
files = [
    "crates/rill-chip0/src/lib.rs",
    "crates/rill-vt-types/src/lib.rs",
    "crates/vt-engine/src/lib.rs",
]
for path in files:
    if not os.path.isfile(path):
        continue
    src = open(path, encoding="utf-8").read()
    src = re.sub(r"/\*.*?\*/", "", src, flags=re.S)
    for name in ("PodCell", "PodGrid"):
        m = re.search(r"pub struct %s\s*\{(.*?)\n\}" % name, src, flags=re.S)
        if not m:
            continue
        for line in m.group(1).splitlines():
            code = re.sub(r"//.*$", "", line)
            if re.search(r"\b(String|&str|Cow<)", code):
                print(f"{path}: {name} field: {code.strip()}")
PY
)"
if [ -n "$hits" ]; then
  fail no-cell-strings "String in a POD snapshot type (AGENTS.md §5)"
  printf '%s\n' "$hits" | sed 's/^/    /' >&2
fi

# --- no-unwrap-in-daemon / no-unwrap (Chip 1) -------------------------------
# PRD NFR-FAIL. src/ only; tests live in tests/ and may unwrap freely.
# SPEC-VT-CONFORMANCE §5 extends this to rill-vt-types and vt-engine.
for crate in rill-kernel rill-attach rilld rill-vt-types vt-engine; do
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
    case "$crate" in
      rill-vt-types|vt-engine) lint=no-unwrap ;;
      *) lint=no-unwrap-in-daemon ;;
    esac
    fail "$lint" "$crate: unwrap/expect/panic on a production path (PRD NFR-FAIL)"
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

# Grapheme overflow (S3-1): GRAPHEMES_BUF without a prior GRAPHEMES_LEN query.
if [ -f crates/rill-chip0/src/adapter/rill_chip0_vt.c ]; then
  if grep -q 'GRAPHEMES_BUF' crates/rill-chip0/src/adapter/rill_chip0_vt.c \
    && ! grep -q 'GRAPHEMES_LEN' crates/rill-chip0/src/adapter/rill_chip0_vt.c; then
    fail grapheme-len "GRAPHEMES_BUF used without GRAPHEMES_LEN (SPEC-CHIP0 §5)"
  fi
fi

# Naked write_all on the attach client (Q1). Allowed only inside the mutate
# block that implements replay_full_frame.
if [ -f crates/rilld/src/lib.rs ]; then
  prod="$(grep -n 'stream.write_all' crates/rilld/src/lib.rs || true)"
  if [ -n "$prod" ] && ! grep -q 'replay_full_frame' crates/rilld/src/lib.rs; then
    fail no-write-all-nonblock "rilld must not write_all the attach socket (quality Q1)"
    printf '%s\n' "$prod" | sed 's/^/    /' >&2
  fi
fi

# --- no-vte-at-runtime ------------------------------------------------------
# ADR 0020 D2, SPEC-VT-CONFORMANCE §5. vte is a test oracle only.
hits="$(python3 - <<'PY'
import re, subprocess
files = subprocess.check_output(
    ["git", "ls-files", "**/Cargo.toml"], text=True
).splitlines()
section_re = re.compile(r'^\[([^]]+)\]\s*$')
dep_re = re.compile(r'^vte(?:\.workspace|\s*=)')
for path in files:
    section = None
    for i, raw in enumerate(open(path, encoding="utf-8"), 1):
        line = re.sub(r"#.*$", "", raw).rstrip()
        m = section_re.match(line)
        if m:
            section = m.group(1).strip()
            continue
        if not line.strip():
            continue
        if not dep_re.match(line.strip()):
            continue
        if section in ("dev-dependencies", "workspace.dev-dependencies"):
            continue
        print(f"{path}:{i}: vte under [{section}]")
PY
)"
if [ -n "$hits" ]; then
  fail no-vte-at-runtime "vte must live only in [dev-dependencies] (ADR 0020 D2)"
  printf '%s\n' "$hits" | sed 's/^/    /' >&2
fi

# --- no-host-dep-on-vt-engine -----------------------------------------------
# ADR 0012 D1, SPEC-VT-CONFORMANCE §5. Isolation until M7.
hits="$(python3 - <<'PY'
import re
for path in (
    "crates/rill-host/Cargo.toml",
    "crates/rilld/Cargo.toml",
):
    try:
        src = open(path, encoding="utf-8").read()
    except OSError:
        continue
    src = re.sub(r"#.*$", "", src, flags=re.M)
    if re.search(r'(?m)^vt-engine(?:\.workspace|\s*=)', src):
        print(f"{path}: depends on vt-engine")
PY
)"
if [ -n "$hits" ]; then
  fail no-host-dep-on-vt-engine "rill-host / rilld must not depend on vt-engine until M7"
  printf '%s\n' "$hits" | sed 's/^/    /' >&2
fi

# --- no-theme-rgb-in-rust ---------------------------------------------------
# ADR 0021 D3, SPEC-VT-CONFORMANCE §5. Theme RGB is data in fixtures/look.
hits="$(python3 - <<'PY'
import os, re

KEY = re.compile(
    r'(?:palette\s*=\s*\d+\s*=|foreground\s*=|background\s*=|'
    r'cursor-color\s*=|selection-background\s*=|selection-foreground\s*=)'
    r'\s*#([0-9A-Fa-f]{6})'
)
LIT = re.compile(r'(?:#|0x)([0-9A-Fa-f]{6})([0-9A-Fa-f]{2})?\b')

# SPEC-VT-COLOR §4. Exempt only inside Palette::vt_default().
EXEMPT = {
    0xCCCCCC, 0x121212,
    0x1D1F21, 0xCC6666, 0xB5BD68, 0xF0C674,
    0x81A2BE, 0xB294BB, 0x8ABEB7, 0xC5C8C6,
    0x666666, 0xD54E53, 0xB9CA4A, 0xE7C547,
    0x7AA6DA, 0xC397D8, 0x70C0B1, 0xEAEAEA,
}

forbidden = set()
theme_dir = "fixtures/look/themes"
if os.path.isdir(theme_dir):
    for name in os.listdir(theme_dir):
        path = os.path.join(theme_dir, name)
        if not os.path.isfile(path):
            continue
        for line in open(path, encoding="utf-8", errors="replace"):
            for m in KEY.finditer(line):
                forbidden.add(int(m.group(1), 16))

def in_vt_default(src, pos):
    m = re.search(r'fn vt_default\s*\(', src)
    if not m or pos < m.start():
        return False
    depth = 0
    started = False
    i = m.end()
    while i < len(src):
        ch = src[i]
        if ch == '{':
            depth += 1
            started = True
        elif ch == '}':
            depth -= 1
            if started and depth == 0:
                return m.start() <= pos <= i
        i += 1
    return False

crate_globs = []
for root, _, files in os.walk("crates/rill-vt-types"):
    for fn in files:
        if fn.endswith(".rs"):
            crate_globs.append(os.path.join(root, fn))
for root, _, files in os.walk("crates/vt-engine"):
    for fn in files:
        if fn.endswith(".rs"):
            crate_globs.append(os.path.join(root, fn))

for path in crate_globs:
    src = open(path, encoding="utf-8", errors="replace").read()
    offset = 0
    for lineno, line in enumerate(src.splitlines(True), 1):
        code = re.sub(r"//.*$", "", line)
        for m in LIT.finditer(code):
            rgb = int(m.group(1), 16)
            alpha = m.group(2)
            if alpha is not None and alpha.lower() != "ff":
                continue
            if rgb not in forbidden:
                continue
            pos = offset + m.start()
            if rgb in EXEMPT and path.endswith("rill-vt-types/src/lib.rs") and in_vt_default(src, pos):
                continue
            print(f"{path}:{lineno}: theme RGB #{m.group(1).lower()}")
        offset += len(line)
PY
)"
if [ -n "$hits" ]; then
  fail no-theme-rgb-in-rust "theme RGB belongs in the look file, not Rust (ADR 0021 D3)"
  printf '%s\n' "$hits" | sed 's/^/    /' >&2
fi

if [ "$FAILED" -ne 0 ]; then
  echo "lint-planes: plane violations found" >&2
  exit 1
fi
echo "lint-planes: ok"
