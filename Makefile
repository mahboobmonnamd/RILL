# Spike 0 developer setup. Chip 0 links libghostty-vt; this is not Ghostty.app.
.PHONY: help setup deps libghostty-vt test package

help:
	@echo "make setup          install build tools and libghostty-vt"
	@echo "make deps           rustc, cargo, zig 0.16+, Xcode CLT"
	@echo "make libghostty-vt  fetch/build Chip 0 VT library only"
	@echo "make test           cargo test --workspace"
	@echo "make package        dist/Rill.app (after setup)"

setup: deps libghostty-vt
	@echo "setup ok"

deps:
	sh scripts/install-deps.sh

libghostty-vt: deps
	sh scripts/fetch-libghostty-vt.sh

test: libghostty-vt
	. "$$HOME/.cargo/env" 2>/dev/null; sh scripts/validate-spike0.sh

package: libghostty-vt
	sh scripts/package-macos.sh
