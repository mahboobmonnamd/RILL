# M4 handoff — Chip 1 isolated VT

**Status: ready for handover. 2026-08-17.** `lane:chip1-vt-engine` / Milestone 4. This track
stops here. A new human or agent continues from this page and the GitHub epic.

Do **not** write `vt-engine` until S-VT is closed **and** T-CHIP1 tests exist
and have been observed red (ADR 0002 D2). Do **not** link the crate into the
window.

| | |
|---|---|
| Epic | [#6](https://github.com/mahboobmonnamd/RILL/issues/6) |
| First spike | [S-VT #21](https://github.com/mahboobmonnamd/RILL/issues/21) |
| Not this team | [S-TUI-BLOCK #22](https://github.com/mahboobmonnamd/RILL/issues/22), [S-CWD #23](https://github.com/mahboobmonnamd/RILL/issues/23), [M7 placeholder #24](https://github.com/mahboobmonnamd/RILL/issues/24) |
| ADR | [0012](adr/0012-chip1-isolated-vt.md) (Accepted — isolation; not live) |
| Spec | [SPEC-CHIP1](spec/SPEC-CHIP1.md) |
| Tests | [TEST-CASES](TEST-CASES.md) T-CHIP1-* (**Red**) |
| Tracker | GitHub Issues and Milestones **only**. No beads. |
| Live chip | Chip 0 until **M7** |

## You are / you are not

You own a **pure VT library**: bytes in, POD grid out.

You do **not** replace the live window. You do **not** own a PTY, paint, build
Blocks, host a TUI in a card, or tap cwd. Those are M6/M7. Mixing them into
this crate is how you get a second parser and cells in `Text`.

## Read first (this tree only)

1. [AGENTS.md](../AGENTS.md)
2. [CONTRIBUTING.md](../CONTRIBUTING.md)
3. [ADR 0001](adr/0001-session-operating-system.md) §1
4. [ADR 0002](adr/0002-falsifiable-evidence.md)
5. [LANES.md](LANES.md)
6. [ARCHITECTURE.md](ARCHITECTURE.md)
7. [SPEC-CHIP0](spec/SPEC-CHIP0.md) — match the **shape**, not Ghostty’s C API
8. [ADR 0012](adr/0012-chip1-isolated-vt.md) and [SPEC-CHIP1](spec/SPEC-CHIP1.md)
9. [`crates/rill-chip0/src/lib.rs`](../crates/rill-chip0/src/lib.rs) —
   `PodCell` / `TerminalEmulation` as they exist
10. [`crates/rill-chip0/src/adapter/rill_chip0_vt.h`](../crates/rill-chip0/src/adapter/rill_chip0_vt.h)

Do not cite another application tree.

## Sequence

Spike = research. It does not make production valid.

1. **[S-VT #21](https://github.com/mahboobmonnamd/RILL/issues/21)** (research) — parser pick (`vte` + our grid vs from
   scratch). Close with a note on [#6](https://github.com/mahboobmonnamd/RILL/issues/6).
2. Named T-CHIP1 tests in `vt-engine` / `rill-vt-types` — fail for the intended
   reason, mutation named, oracle is `snapshot()`.
3. Smallest implementation that turns those tests green.
4. `fast.yml` Linux, no Zig. Never add `rill-chip0` to that job.
5. Merge crate-only PRs to `main`. Rebase often. **No** `rill-host` / `rilld`
   dependency.

`Proposed` ADRs do not authorize code. 0012 is Accepted for **isolation**. The
first CSI parser PR MUST cite S-VT (0012 D6).

## Why

Stop living on pinned, unstable `libghostty-vt`. Kernel, attach, and Metal
stay. Chip 1 is swappable later (M7) behind the same traits. Full Ghostty exec
is rejected forever (ADR 0001 §1).

## Must / must not

| Must | Must not |
|---|---|
| `feed` unmodified | PTY, `posix_spawn`, `openpty` |
| `resize` to `cols * rows` | Paint, GPU, AppKit, Blocks, `Text` |
| `snapshot() -> PodGrid` | `String` on a snapshot type |
| `Result` on library paths | `unwrap` on reachable feed/snapshot |
| Visible cells only | Scrollback (kernel ring) |
| Pure Rust in `fast.yml` | Depend on `rill-chip0` / Zig / libghostty-vt |
| No `ghostty_` in domain | Ghostty FFI in Chip 1 |
| Not `Sync` | Share the VT across threads |
| Isolated workspace member | Dependency of host or `rilld` until M7 |

`rilld` today uses `Chip0::resync_from_history`. Leave it.

## Contract (short)

Trait: `feed`, `resize`, `snapshot`. Also `reset` / `repaint_bytes` /
`resync_from_history` as Chip 0.

`PodCell` 16 bytes: codepoint, fg/bg RGBA8888 `(r<<24)|(g<<16)|(b<<8)|0xff`,
attrs bit0 bold / bit1 underline / bit2 inverse. Defaults `#cccccc` / `#121212`.
Grapheme bound 32; truncate to base and count.

v0 sequences: SPEC-CHIP1 §3 (C0, CUP/ED/SGR, alt-screen 1049, DA/DSR). Not
sixel/images/full xterm.

Oracle: independent grid. Not Chip 0 equality. Not a copied input buffer. Not
the `\x1b[2J` prefix.

## First GitHub work

1. Close **[S-VT #21](https://github.com/mahboobmonnamd/RILL/issues/21)** (parser pick).
2. Child issues under [#6](https://github.com/mahboobmonnamd/RILL/issues/6) per
   slice (UTF-8, C0, CSI cursor, SGR, alt-screen, resync emit) — after tests
   exist as names, one issue per slice, `lane:chip1-vt-engine`, milestone M4.
   Palette-index cells / compositor opacity:
   **[#267](https://github.com/mahboobmonnamd/RILL/issues/267)** (colour ADR
   first; this handoff does not authorize that work).
3. Types crate + red tests, then impl.

## Done (M4) vs later

**M4 done:** `rill-vt-types` + `vt-engine` on `main`; T-CHIP1 demonstrated red
then green in `fast.yml`; host/`rilld` still do not depend on `vt-engine`;
lint-planes clean.

**M7 (not you):** live swap ADR, resync = Chip 1, packaged T-NFR hid.

**M6 (not you):** Blocks host the live chip; live TUI-in-block; cwd tap
([ADR 0013](adr/0013-cwd-tap.md): kernel fg `proc_pidinfo`, not
OSC 7; Block path header is [#22](https://github.com/mahboobmonnamd/RILL/issues/22)). May use Chip 0. Must not dump the grid into `Text`.

## Other milestones

| Milestone | This team? |
|---|---|
| M1 session graph [#16](https://github.com/mahboobmonnamd/RILL/issues/16) | No |
| M2 chrome | No |
| M4 this crate | Yes |
| M6 Blocks + TUI-in-block + cwd tap | No |
| M7 Chip 1 live | No — later, after M4 Proven vs T-CHIP1 |
