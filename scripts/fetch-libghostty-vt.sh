#!/bin/sh
# Fetch and build libghostty-vt at the pinned revision. ADR 0002 D7.
#
# Fails closed on: missing pin, SHA mismatch, or an existing archive built from
# an unknown revision. An unpinned emulator means a green run today and a red
# run tomorrow share no referent.
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

PIN_FILE="$ROOT/third_party/ghostty.pin"
[ -f "$PIN_FILE" ] || { echo "fetch: missing $PIN_FILE (ADR 0002 D7)" >&2; exit 1; }

PIN_REPO="$(sed -n 's/^repo *= *//p' "$PIN_FILE" | tr -d '[:space:]')"
PIN_SHA="$(sed -n 's/^sha *= *//p' "$PIN_FILE" | tr -d '[:space:]')"
[ -n "$PIN_REPO" ] || { echo "fetch: no repo in pin" >&2; exit 1; }
case "$PIN_SHA" in
  ????????????????????????????????????????) : ;;
  *) echo "fetch: sha in pin is not a full 40-char commit id: '$PIN_SHA'" >&2; exit 1 ;;
esac

GHOSTTY="${RILL_GHOSTTY_DIR:-$ROOT/third_party/ghostty}"
STAMP="$GHOSTTY/.rill-built-sha"
ARCHIVE="$GHOSTTY/zig-out/lib/libghostty-vt.a"

# An archive with no provenance stamp, or the wrong one, is not trusted.
if [ -f "$ARCHIVE" ]; then
  if [ -f "$STAMP" ] && [ "$(cat "$STAMP")" = "$PIN_SHA" ]; then
    echo "fetch: libghostty-vt.a already built at $PIN_SHA"
    exit 0
  fi
  echo "fetch: archive present but built from an unknown or stale revision; rebuilding" >&2
  rm -rf "$GHOSTTY/zig-out"
  rm -f "$STAMP"
fi

if [ ! -d "$GHOSTTY/.git" ]; then
  mkdir -p "$GHOSTTY"
  git -C "$GHOSTTY" init -q
  git -C "$GHOSTTY" remote add origin "$PIN_REPO"
fi

git -C "$GHOSTTY" fetch --depth 1 origin "$PIN_SHA"
git -C "$GHOSTTY" checkout -q --detach FETCH_HEAD

GOT="$(git -C "$GHOSTTY" rev-parse HEAD)"
if [ "$GOT" != "$PIN_SHA" ]; then
  echo "fetch: SHA mismatch. pin=$PIN_SHA checked-out=$GOT" >&2
  exit 1
fi

command -v zig >/dev/null 2>&1 || { echo "fetch: zig not found; run scripts/install-deps.sh" >&2; exit 1; }
(cd "$GHOSTTY" && zig build -Demit-lib-vt -Doptimize=ReleaseFast)

[ -f "$ARCHIVE" ] || { echo "fetch: build produced no $ARCHIVE" >&2; exit 1; }
printf '%s' "$PIN_SHA" > "$STAMP"
echo "fetch: libghostty-vt.a built at $PIN_SHA"
