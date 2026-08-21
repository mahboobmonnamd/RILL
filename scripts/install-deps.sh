#!/bin/sh
# Install or verify Spike 0 build tools.
set -eu

RUST_MIN="1.85.0"

die() {
  printf 'setup: %s\n' "$1" >&2
  exit 1
}

ver_ge() {
  [ "$(printf '%s\n%s\n' "$2" "$1" | sort -V | head -n 1)" = "$2" ]
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "missing $1. $2"
}

os="$(uname -s)"
[ "$os" = "Darwin" ] || die "macOS first (uname=$os)"

need_cmd git "Install Xcode Command Line Tools, then retry."
need_cmd curl "Install Xcode Command Line Tools, then retry."

if ! xcode-select -p >/dev/null 2>&1; then
  die "Xcode Command Line Tools missing. Run: xcode-select --install"
fi

if ! command -v rustup >/dev/null 2>&1 && ! command -v cargo >/dev/null 2>&1; then
  printf 'setup: installing rustup (stable)\n'
  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
fi

if [ -f "$HOME/.cargo/env" ]; then
  # shellcheck disable=SC1091
  . "$HOME/.cargo/env"
fi

need_cmd rustc "Open a new terminal after rustup, or: . \$HOME/.cargo/env"
need_cmd cargo "Open a new terminal after rustup, or: . \$HOME/.cargo/env"

rustc_ver="$(rustc --version | awk '{print $2}')"
ver_ge "$rustc_ver" "$RUST_MIN" || die "rustc $rustc_ver is older than $RUST_MIN. Run: rustup update stable"

printf 'setup: rustc %s  cargo ok  xcode-select ok\n' "$rustc_ver"
