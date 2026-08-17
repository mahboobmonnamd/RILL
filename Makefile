# Spike 0 developer setup. Chip 0 links libghostty-vt; this is not Ghostty.app.
.PHONY: help setup deps libghostty-vt fast lint test gates negative-controls package run

help:
	@echo "make setup             install build tools and libghostty-vt (at the pin)"
	@echo "make deps              rustc, cargo, zig 0.16+, Xcode CLT"
	@echo "make libghostty-vt     fetch/build Chip 0 VT library at third_party/ghostty.pin"
	@echo "make lint              plane invariants (AGENTS.md §5, ADR 0002 D9) — no Zig needed"
	@echo "make fast              lint + codec/kernel tests — no Zig, no macOS"
	@echo "make gates             full Spike 0 gate suite, writes evidence/"
	@echo "make negative-controls assert each gate goes red under its own mutation"
	@echo "make package           dist/Rill.app (after setup)"
	@echo "make run               package and launch dist/Rill.app"
	@echo ""
	@echo "Spike 0 is GREEN ([ADR 0010](docs/adr/0010-spike-0-closes.md)). See docs/SPIKE-0.md."

setup: deps libghostty-vt
	@echo "setup ok"

deps:
	sh scripts/install-deps.sh

libghostty-vt: deps
	sh scripts/fetch-libghostty-vt.sh

# The fast tier must never need Lane C's toolchain, or LANES is fiction
# (SPEC-CHIP0 §9).
lint:
	sh scripts/lint-planes.sh

fast: lint
	cargo fmt --all --check
	cargo clippy -p rill-attach -p rill-kernel --all-targets -- -D warnings
	cargo test -p rill-attach
	cargo test -p rill-kernel -- --test-threads=1

gates: libghostty-vt
	. "$$HOME/.cargo/env" 2>/dev/null; sh scripts/validate-spike0.sh

negative-controls: libghostty-vt
	. "$$HOME/.cargo/env" 2>/dev/null; sh scripts/validate-spike0.sh --negative-controls

# Kept for muscle memory; `gates` is the accurate name.
test: gates

package: libghostty-vt
	sh scripts/package-macos.sh

# One titled fullscreen window. Attaches to rilld; does not spawn the user shell.
run: package
	open dist/Rill.app
