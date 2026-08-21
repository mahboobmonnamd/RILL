# Developer setup. Live chip is vt-engine (ADR 0054).
.PHONY: help setup deps fast lint test gates negative-controls package run

help:
	@echo "make setup             install rustc, cargo, Xcode CLT"
	@echo "make lint              plane invariants (AGENTS.md §5, ADR 0002 D9)"
	@echo "make fast              lint + codec/kernel tests"
	@echo "make gates             full Spike 0 gate suite, writes evidence/"
	@echo "make negative-controls assert each gate goes red under its own mutation"
	@echo "make package           dist/Rill.app"
	@echo "make run               package and launch (dev: spawn rilld from the GUI)"

setup: deps
	@echo "setup ok"

deps:
	sh scripts/install-deps.sh

lint:
	sh scripts/lint-planes.sh

fast: lint
	cargo fmt --all --check
	cargo clippy -p rill-attach -p rill-kernel --all-targets -- -D warnings
	cargo test -p rill-attach
	cargo test -p rill-kernel -- --test-threads=1

gates:
	. "$$HOME/.cargo/env" 2>/dev/null; sh scripts/validate-spike0.sh

negative-controls:
	. "$$HOME/.cargo/env" 2>/dev/null; sh scripts/validate-spike0.sh --negative-controls

test: gates

package:
	sh scripts/package-macos.sh

# `open dist/Rill.app` does not pass env and does not start rilld. Ad-hoc
# SMAppService registration is not enabled, so the GUI exits with
# "host io: No such file or directory". Direct spawn is the bounded
# development path (SPEC-RUNTIME-SUPERVISION §1).
run: package
	@echo "Foreground GUI (no extra terminal output). Interrupt (Ctrl+C) stops the window."
	@echo "Or: open dist/Rill.app  (dev.rill.spike0 sets RILL_DEV_DIRECT_RILLD in Info.plist)"
	RILL_DEV_DIRECT_RILLD=1 dist/Rill.app/Contents/MacOS/Rill
