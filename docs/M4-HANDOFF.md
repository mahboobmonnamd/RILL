# M4 handoff — Chip 1 isolated VT

**Status: handover context. 2026-08-17. S-VT closed 2026-08-18.**
`lane:chip1-vt-engine` / Milestone 4.

**Read [M4-PLAN](M4-PLAN.md) first — it is the current order of work.**
[S-VT #21](https://github.com/mahboobmonnamd/RILL/issues/21) is closed
([SPIKE-VT](SPIKE-VT.md)): the parser is written **in this tree** and `vte` is a
`[dev-dependencies]` differential oracle only. Four ADRs followed —
[0020](adr/0020-chip1-parser-in-tree.md) parser and C1 policy,
[0021](adr/0021-chip1-colour-identity.md) colour identity,
[0022](adr/0022-chip1-reply-channel.md) DA/DSR replies,
[0023](adr/0023-chip1-v0-defers-character-width.md) width deferred (amended by
[0035](adr/0035-chip1-character-width.md)) — plus six slice specs under
[SPEC-CHIP1](spec/SPEC-CHIP1.md) §0.

Do **not** write `vt-engine` until the T-CHIP1 tests for that slice exist and
have been observed red (ADR 0002 D2). Do **not** link the crate into the window.

| | |
|---|---|
| Epic | [#6](https://github.com/mahboobmonnamd/RILL/issues/6) |
| Plan | [M4-PLAN](M4-PLAN.md) |
| First spike | [S-VT #21](https://github.com/mahboobmonnamd/RILL/issues/21) — **closed**, [SPIKE-VT](SPIKE-VT.md) |
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

1. ~~**S-VT #21** — parser pick.~~ **Done** 2026-08-18:
   [SPIKE-VT](SPIKE-VT.md), ADR 0020.
2. Named T-CHIP1 tests in `vt-engine` / `rill-vt-types` — fail for the intended
   reason, mutation named, oracle is `snapshot()`. Slice order:
   [M4-PLAN](M4-PLAN.md).
3. Smallest implementation that turns those tests green.
4. `fast.yml` Linux, no Zig. Never add `rill-chip0` to that job.
5. Merge crate-only PRs to `main`. Rebase often. **No** `rill-host` / `rilld`
   dependency.

`Proposed` ADRs do not authorize code. 0012 is Accepted for **isolation**. The
first CSI parser PR MUST cite S-VT and ADR 0020 (0012 D6).

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
| `vte` in `[dev-dependencies]` only | `vte` in `[dependencies]` (ADR 0020 D2) |
| Palette identity until snapshot | Theme RGB compiled into Rust (ADR 0021 D3) |
| `take_replies` for DA/DSR | A write path or fd inside the chip (ADR 0022) |

`rilld` today uses `Chip0::resync_from_history`. Leave it.

## Contract (short)

Trait: `feed`, `resize`, `snapshot`. Also `reset` / `repaint_bytes` /
`resync_from_history` as Chip 0, plus Chip 1-only `set_palette` and
`take_replies` / `has_replies`.

`PodCell` 16 bytes: codepoint, fg/bg RGBA8888 `(r<<24)|(g<<16)|(b<<8)|0xff`,
attrs bit0 bold / bit1 underline / bit2 inverse / bit3 wide-lead / bit4 wide-tail.
Grapheme bound 32; truncate to
base and count.

Colour: cells hold `Default | Indexed(u8) | Rgb` and materialise at `snapshot()`
against a `Palette` the host loaded from the theme **file**.
`Palette::vt_default()` is `#cccccc` / `#121212` — a VT default, not a theme
(ADR 0021).

C1: an invalid `0x80..=0x9f` byte is one U+FFFD; a decoded U+0080..=U+009F scalar
paints; `0x9b` does not open a CSI (ADR 0020 D3).

Width: **cluster then East Asian Width W/F → 2; Ambiguous → 1** (ADR 0035).
`PodCell` 16 bytes; attrs bit3 wide-lead, bit4 wide-tail. T-CHIP1-WIDTH
(`日本X` → column 5) is an M7 precondition. Not live.

v0 sequences: SPEC-CHIP1 §3 as amended, detail in the six slice specs. Not
sixel/images/full xterm.

Oracle: independent grid. Not Chip 0 equality, not `vte` equality alone, not a
copied input buffer, not the `\x1b[2J` prefix.

## First GitHub work

1. ~~Close S-VT #21.~~ **Done** — record the pick on
   [#6](https://github.com/mahboobmonnamd/RILL/issues/6).
2. Nine slice issues under [#6](https://github.com/mahboobmonnamd/RILL/issues/6),
   one per [M4-PLAN](M4-PLAN.md) slice, `lane:chip1-vt-engine`, milestone M4.
   Colour is no longer blocked: ADR 0021 is the ADR
   [#267](https://github.com/mahboobmonnamd/RILL/issues/267) required, so
   [#271](https://github.com/mahboobmonnamd/RILL/issues/271) loses `blocked`.
   M7 must keep packaged T-LOOK-ANSI:
   [#272](https://github.com/mahboobmonnamd/RILL/issues/272).
3. Slice 1 scaffolding + lints, then red tests, then impl.

## Done (M4) vs later

**M4 done:** `rill-vt-types` + `vt-engine` on `main`; T-CHIP1 demonstrated red
then green in `fast.yml`; host/`rilld` still do not depend on `vt-engine`;
lint-planes clean. **Width is not required for M4.**

**M7 (next):** [ADR 0037](adr/0037-chip1-live-swap.md) Accepted — implement
[#24](https://github.com/mahboobmonnamd/RILL/issues/24): spec → named tests →
swap PR (`VtEngine` in host + `rilld`, reply drain, `mode_state`, wide-bit
presenter, lift `no-host-dep-on-vt-engine`), then packaged T-NFR hid on battery.
All [M4-PLAN](M4-PLAN.md) M7 preconditions except the swap PR itself are met.

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
