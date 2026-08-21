# ADR 0037: Chip 1 replaces Chip 0 on the warm path (M7 live swap)

- **Status:** Accepted — 2026-08-20
- **Implementation hold:** [ADR 0053](0053-runtime-domain-content-and-client-authority.md)
  D12 parks the live swap until host checkpoint and client reconciliation
  contracts are specified and demonstrated red.
- **Tree:** this repository only
- **Issue:** [#305](https://github.com/mahboobmonnamd/RILL/issues/305) (ADR),
  implementation [#24](https://github.com/mahboobmonnamd/RILL/issues/24)
- **Requires:** [ADR 0012](0012-chip1-isolated-vt.md) D1/D4,
  [ADR 0022](0022-chip1-reply-channel.md) D4,
  [ADR 0035](0035-chip1-character-width.md) D7 (T-CHIP1-WIDTH Proven),
  [ADR 0036](0036-chip1-mode-state-channel.md) D2/D4 (T-CHIP1-MODE Proven),
  T-CHIP0-C1-PAINT Proven ([#304](https://github.com/mahboobmonnamd/RILL/issues/304),
  [#308](https://github.com/mahboobmonnamd/RILL/pull/308))
- **Amends:** [ADR 0012](0012-chip1-isolated-vt.md) D1 — isolation lifts in the
  swap PR named below. [ADR 0040](0040-terminal-fidelity-is-chip0.md) D1 — at
  M7 the host queries Chip 1 `mode_state()`, not Chip 0. [M4-PLAN](../M4-PLAN.md)
  M7 preconditions 4–6.
- **Does not authorize:** starting [#24](https://github.com/mahboobmonnamd/RILL/issues/24)
  until a follow-up spec names the swap PR gates; a feature-flagged half swap;
  recutting T-NFR; a second VT on the warm path; JSON or a new attach frame tag;
  dumping the grid into `Text`; sixel / images / Ghostty exec

## Context

M4 landed an isolated `vt-engine` with every T-CHIP1 gate demonstrated red then
green in `fast.yml`. Chip 0 (`libghostty-vt` + Zig) still owns the window paint
path and `rilld` cold resync. ADR 0012 D1 forbids a host/daemon dependency on
`vt-engine` until this ADR is Accepted **and** packaged T-NFR hid is re-proven
after the swap.

The preconditions in [M4-PLAN](../M4-PLAN.md) are now satisfied:

| Precondition | Evidence |
|---|---|
| Every T-CHIP1 gate Proven | `fast.yml` negative controls; slice issues closed |
| T-CHIP1-WIDTH Proven | [#306](https://github.com/mahboobmonnamd/RILL/pull/306), ADR 0035 |
| T-CHIP1-LOOK-ANSI Proven | `t_chip1_look_ansi_sgr_colours_come_from_the_theme_file`, [#271](https://github.com/mahboobmonnamd/RILL/issues/271) |
| T-CHIP0-C1-PAINT | [#308](https://github.com/mahboobmonnamd/RILL/pull/308) — libghostty-vt **paints** decoded C1; no divergence register entry |
| Mode-state channel | ADR 0036 Accepted; T-CHIP1-MODE Proven ([#307](https://github.com/mahboobmonnamd/RILL/pull/307)) |
| This ADR | Accepted here |

What remains is the **implementation PR** ([#24](https://github.com/mahboobmonnamd/RILL/issues/24)):
wire `VtEngine` into `rill-host` and `rilld`, drain replies and poll modes, lift
the isolation lint, and re-run packaged gates without recutting the instrument.

## Decision

### D1 — One live type in the window and in `rilld` resync

The warm-path chip is `vt_engine::VtEngine` everywhere `Chip0` sits today:
`rill-host::Client` (attach paint) and `rilld::Daemon` (cold resync replay).
Both use `TerminalEmulation` from `rill-vt-types`. There MUST NOT be a second
live VT in the kernel (ADR 0012 D4).

The swap PR replaces `Chip0` with `VtEngine` in those two crates in **one**
change. A feature flag that leaves Chip 0 on the warm path while linking
`vt-engine` is forbidden.

`rill-chip0` remains in the tree for `gates.yml` measurement
(T-CHIP0-C1-PAINT, T-BYTES, packaged look baselines against libghostty-vt) until
a later retirement ADR. It MUST NOT appear in `rill-host` or `rilld`
`Cargo.toml` after the swap.

### D2 — `take_replies` drains onto ordinary attach `DATA`

After every successful `feed` on the attach client, the host MUST call
`take_replies()` and enqueue each drained byte sequence as `Frame::Data` toward
the PTY — the same path as keystrokes and paste. Order within a pump turn:

1. Read attach frames; for each `DATA` chunk, `feed` into the chip.
2. Drain `take_replies()` until empty; each drain is one or more `Frame::Data`
   writes toward the child (may be coalesced with the outbox queue, not with
   paint).
3. Invalidate the snapshot cache when `feed` or replies changed the grid.

No new attach frame tag. No JSON. No control RPC (ADR 0003 D9, ADR 0022 D4).

On `rilld`'s **cold resync** path (`resync_from_history`), replies MUST be
discarded: call `take_replies()` after replay and drop the bytes. History replay
is not a live program. Overflow counting from ADR 0022 D3 still applies if the
replay enqueues replies internally.

### D3 — Resync emits Chip 1 VT bytes

`rilld::Daemon::maybe_resync` MUST use `VtEngine::resync_from_history` instead
of `Chip0::resync_from_history`. The emitted bytes are ordinary `DATA` toward
the attach client, unchanged in framing. T-RESYNC packaged gates MUST stay green
with Chip 1 as the resync engine.

### D4 — Theme palette via `set_palette`, look gates unchanged

The host MUST load the resolved theme palette from the look **file**
([ADR 0043](0043-one-look-schema-one-config-file.md),
[ADR 0017](0017-ghostty-look-windowed-default.md)) and call `VtEngine::set_palette`
after connect and whenever the theme changes. `snapshot()` materialises SGR
against that palette ([ADR 0021](0021-chip1-colour-identity.md)).

Packaged **T-LOOK-ANSI**, **T-LOOK-CELL**, and **T-SPLIT-LOOK** MUST remain
green after the swap ([#272](https://github.com/mahboobmonnamd/RILL/issues/272)).
The swap PR does not recut those fixtures.

### D5 — Host encodes from `mode_state()` after `feed`

The host MUST NOT parse escape sequences for mouse mode, DECCKM, keypad mode,
or bracketed paste ([ADR 0040](0040-terminal-fidelity-is-chip0.md) D1, amended).
After each `feed` (and after resync replay on the attach client), the host reads
`VtEngine::mode_state()` and encodes keys and mouse from the returned
`TerminalModeState` ([ADR 0036](0036-chip1-mode-state-channel.md) D2).

Chip 1 tracks; the host encodes. Payload bytes still travel as attach `DATA`
toward the PTY.

### D6 — Wide cells on the Metal presenter

The host presenter MUST honour `PodCell` attrs bits 3–4 (`ATTR_WIDE_LEAD`,
`ATTR_WIDE_TAIL`, ADR 0035 D5/D7): skip wide-tail cells for damage and cursor
probes; advance the cursor across two columns for a wide lead. Chip 0 left those
bits zero; Chip 1 sets them. Failing to honour them regresses CJK paint at M7
against a live chip that already handles width today.

### D7 — Isolation lint lifts in the swap PR only

`scripts/lint-planes.sh` `no-host-dep-on-vt-engine` MUST be updated in the same
PR that adds `vt-engine` to `rill-host` and `rilld` `Cargo.toml`. Partial lifts
(an dependency without lint change, or the reverse) are forbidden.

That PR also drops the Zig toolchain requirement from the host/daemon build.
Zig remains required only for `gates.yml` / `rill-chip0` while that crate is
kept for measurement.

### D8 — C1 policy at swap time

T-CHIP0-C1-PAINT is **Proven**: libghostty-vt paints `0xc2 0x9b 0x41` as U+009B
then `A`. Chip 1's paint policy (T-CHIP1-C1) matches. No divergence register
entry is required for C1 at M7.

If a future libghostty pin changes Chip 0 behaviour, re-run T-CHIP0-C1-PAINT
before any pin bump (ADR 0002 D7); do not assume this ADR's row stays true across
pins.

### D9 — T-NFR and closure gates

The swap PR MUST re-prove packaged **T-NFR** (`Rill --nfr-key=hid`) on battery
with the **same instrument** — no recut, no new frame types (ADR 0004). Also
re-run **T-RESYNC** (idle shell and alt-screen), and confirm **T-LOOK-*** /
**T-SPLIT-LOOK** packaged gates.

Socket-only tests do not close the swap. Evidence is the uploaded artifact or
CI run, not a laptop transcript (ADR 0002 D8).

### D10 — Implementation sequence for [#24](https://github.com/mahboobmonnamd/RILL/issues/24)

This ADR authorizes the swap **spec and named tests** next, then the smallest
implementation that turns them green:

1. Spec — swap wiring in [SPEC-CHIP1](../spec/SPEC-CHIP1.md) §6 (added by the
   spec PR) or a dedicated `SPEC-VT-LIVE-SWAP.md` if the umbrella grows.
2. Named tests — extend packaged gates / `rill-host` tests; no grep for a type
   name as the oracle.
3. Implementation — D1–D7 in one PR.
4. Integration — packaged T-NFR on battery, T-RESYNC, T-LOOK-*.

## Consequences

- [#24](https://github.com/mahboobmonnamd/RILL/issues/24) is unblocked for
  spec → test → impl, not for drive-by dependency adds before tests exist.
- `no-host-dep-on-vt-engine` becomes a regression guard only until the swap PR
  lands; after that, accidental `rill-chip0` on the warm path should be linted
  separately (follow-up if needed).
- Blocks (historical M6) and live TUI-in-content (ADR 0050 as amended by ADR
  0053) still use whatever chip the warm
  path owns — after swap, that is Chip 1.

## Rejected alternatives

- **Feature-flag Chip 0 vs Chip 1 on the warm path.** Rejected: two live chips,
  divergent resync, and a false sense of green while Chip 0 still hides misses.
- **Drain replies on a new attach tag.** Rejected: ADR 0022 D4, ADR 0003 D9.
- **Host-side escape parser for modes.** Rejected: second VT (ADR 0036 D1).
- **Keep Chip 0 in the window and Chip 1 in `rilld`.** Rejected: ADR 0012 D4,
  T-RESYNC would compare unlike implementations.
- **Defer wide-bit presenter work.** Rejected: ADR 0035 D7; M7 would regress CJK
  against Chip 0 today.
- **Accept swap while T-CHIP0-C1-PAINT is red.** Rejected: M4-PLAN precondition
  4; fail closed on execute-vs-paint divergence.
