# ADR 0012: Chip 1 isolated VT (M4)

- **Status:** Accepted — 2026-08-17
- **Tree:** this repository only
- **Issue:** M4 epic [#6](https://github.com/mahboobmonnamd/RILL/issues/6)
- **Requires:** [ADR 0001](0001-session-operating-system.md) §1,
  [ADR 0010](0010-spike-0-closes.md) (Spike 0 Proven)
- **Amends:** [ADR 0010](0010-spike-0-closes.md) D3 — Chip 1 stays isolated
  until **M7** (live swap), not merely until M4 exists. M4 is the crate.
- **Does not authorize:** Chip 1 as the live chip, a host/daemon dependency on
  `vt-engine`, Blocks, a second VT in the kernel, full libghostty exec,
  matching Ghostty bug-for-bug without a fixture

Handoff: [M4-HANDOFF](../M4-HANDOFF.md). Spec: [SPEC-CHIP1](../spec/SPEC-CHIP1.md).
Tests: [TEST-CASES](../TEST-CASES.md) T-CHIP1-* (**Red**).

## Context

Chip 0 is a borrowed VT (`libghostty-vt`, pin in `third_party/ghostty.pin`).
Upstream calls that API unstable. Pins move only in their own PR with the full
gate suite ([ADR 0002](0002-falsifiable-evidence.md) D7).

ADR 0001 already named Chip 1: an owned VT behind the same traits, later. Spike
0 Proven unblocks the isolated crate. It does not swap the window. `rilld`
still constructs `Chip0` for cold resync. Lane D must not call Chip 1 until
**M7**.

GitHub [#6](https://github.com/mahboobmonnamd/RILL/issues/6) was a stub
(“crate unit coverage”). This ADR is the contract a new Lane E team needs.

## Decision

### D1 — Isolated library only until M7

Chip 1 lives as `crates/vt-engine` (plus `crates/rill-vt-types` for the shared
POD/trait). `rill-host` and `rilld` MUST NOT depend on `vt-engine` until
[ADR 0037](0037-chip1-live-swap.md) is Accepted **and** the swap PR re-proves
packaged T-NFR hid. ADR 0037 is Accepted; the dependency lift is still the
[#24](https://github.com/mahboobmonnamd/RILL/issues/24) swap PR only.

Land the crate on `main` often so it does not rot. That is not a live swap.

### D2 — Same I/O shape as Chip 0, not Ghostty’s C API

`TerminalEmulation`: `feed`, `resize`, `snapshot`. Inherent `reset` and
`resync_from_history` / `repaint_bytes` as Chip 0 has. `snapshot_damaged` is
[#18](https://github.com/mahboobmonnamd/RILL/issues/18) (Lane C), not an M4
blocker.

`PodCell` is 16 bytes `repr(C)`: `codepoint u32`, `fg u32`, `bg u32` RGBA8888
with R in the high byte, `attrs u16` (bit0 bold, bit1 underline, bit2 inverse),
`_pad u16`. No `String` reachable from a snapshot.

Extract types into `rill-vt-types` so Lane E does not take Chip 0’s Zig
toolchain. `rill-chip0` implements the trait. `vt-engine` implements the trait.
`rill-chip0` MUST NOT depend on `vt-engine`. `vt-engine` MUST NOT depend on
`rill-chip0` except an optional macOS-only *dev* differential (never the
primary oracle).

### D3 — Chip does not own scrollback

Visible grid is `cols * rows`. History stays the kernel byte ring (ADR 0001 §4).
A snapshot MUST NOT grow with fed bytes.

### D4 — Resync stays one implementation

A second live VT in the kernel is forbidden. Until M7, Chip 0 is that
implementation. Chip 1 must still *be able* to emit VT for a second instance to
replay onto a matching grid (T-CHIP1-RESYNC). Kernel will not call it in M4.

### D5 — v0 sequence subset, not full xterm

Normative list: [SPEC-CHIP1](../spec/SPEC-CHIP1.md) §3. Goal: T-BYTES fixtures,
zsh print, `less`, `vim`, alt-screen TUIs such as `htop`. Explicitly out: sixel,
ReGIS, Kitty/iTerm images, Ghostty exec, Blocks dump.

### D6 — Parser pick is S-VT, then the first parser PR

Owned **grid and POD** are required. The byte parser may be written in this
tree or use the `vte` crate (parser only). Forbidden: `libghostty-vt`, full
Ghostty exec, another application tree.

[S-VT](https://github.com/mahboobmonnamd/RILL/issues/21) (research) records the
pick. The first PR that parses CSI MUST cite that close. Types crate + failing
named tests MAY land before the pick.

### D7 — Oracle is an independent `PodGrid`, not Chip 0 and not a copied buffer

Primary tests assert on `snapshot()` (codepoint, cursor, attrs, damage, length).
Banned: `Chip1.fed`, asserting on a `\x1b[2J` prefix the emit path prepends,
“equals this Ghostty pin” as the gate. Chip 0 differential MAY exist as a
macOS-only secondary check.

Every T-CHIP1 gate names a required mutation (ADR 0002 D3) and is demonstrated
red before green (D2). A skip is a failure (D5).

### D8 — `fast.yml` runs Chip 1 on Linux with no Zig

Add `-p rill-vt-types -p vt-engine` to the fast clippy/test list. Never add
`rill-chip0` to that list (SPEC-CHIP0 §9). Extend `lint-planes.sh`:
`no-cell-strings` and `no-unwrap` to Chip 1 src; `no-ghostty-in-domain` already
scans all crates.

### D9 — Fail closed

Library paths return `Result`. No `unwrap`/`expect` on reachable `feed` /
`resize` / `snapshot`. Not `Sync`. One thread feeds and snapshots (ADR 0003 D4).
`feed` MUST NOT allocate proportionally to input length. MUST NOT drop high
bytes before parse.

## Consequences

- M4 is staffable. First production work after S-VT (if the parser PR needs
  it): named T-CHIP1 tests red, then the smallest crate.
- M6 Blocks / live TUI-in-block / cwd tap are **not** this crate. They may be
  designed against Chip 0.
- M7 is a later ADR: swap the live chip, resync uses Chip 1, T-NFR hid again.
- Tracker is GitHub Issues and Milestones only. No beads.

## Rejected alternatives

- **Wait for Blocks or M1.** Rejected: Lane E is not blocked.
- **Link as live when the crate compiles.** Rejected: M7 + T-NFR.
- **Full libghostty exec as an intermediate.** Rejected by ADR 0001 §1.
- **Cell-for-cell Ghostty match as the M4 gate.** Rejected: unbounded, needs
  Zig, not a written subset.
- **Second tracker (beads).** Rejected: [CONTRIBUTING](../../CONTRIBUTING.md).
