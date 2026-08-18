#!/bin/sh
# SPEC-PLATFORM T-PLAT-CORE: core crates carry no UI/platform dependency
# (ADR 0027 D1). Build-level check on the resolved dependency graph, not a
# source grep — no Zig, no macOS toolchain needed.
set -eu
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"
cargo metadata --format-version 1 | python3 scripts/check_core_boundary.py
