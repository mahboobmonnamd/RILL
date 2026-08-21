# ADR 0054: Chip 0 / libghostty-vt retired

- **Status:** Accepted — 2026-08-21
- **Amends:** [ADR 0037](0037-chip1-live-swap.md) D1 (crate may remain for
  measurement) — that hold is lifted. The live chip is Chip 1 only.
- **Does not authorize:** recutting T-NFR, a second VT, Ghostty.app as a
  product, changing look-file grammar in this ADR

## Decision

`crates/rill-chip0`, `third_party/ghostty.pin`, and
`scripts/fetch-libghostty-vt.sh` are deleted. The packaged host and daemon MUST
NOT fetch Zig or `libghostty-vt`. `gates.yml` MUST NOT install Zig for Chip 0.

T-BYTES remains the kernel ring plus Chip 1 library feed. T-LOOK oracles run on
`rill-look` + `VtEngine`. T-CHIP0-C1-PAINT is withdrawn; T-CHIP1-C1 is the C1
paint gate.

Historical ADRs that named Chip 0 as the then-live emulator stay as history.
They do not restore the crate.
