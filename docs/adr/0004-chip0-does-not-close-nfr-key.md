# ADR 0004: Chip 0 does not close NFR-KEY

- **Status:** Accepted — 2026-08-17
- **Tree:** this repository only
- **Issue:** [#8](https://github.com/mahboobmonnamd/RILL/issues/8)
- **Amends:** nothing in ADR 0001–0003. Those decisions stand. This ADR
  records what the honest T-NFR number means for Spike 0 closure.
- **Evidence:** battery hid, 1000 accepted samples, 0 discards, `ax_trusted=1`,
  p95 **23.525ms** vs 8.33ms (120 Hz). After warmup, `key_to_commit` ≈ 1.4ms,
  `commit_to_presented` ≈ 20–22ms, present cadence ~40 Hz. Artifact:
  `evidence/t-nfr-hid-battery.{out,err}` (laptop; ADR 0002 D8 still applies).

## Context

[ADR 0003](0003-display-pipeline.md) redefined T-NFR as key-down
`NSEvent.timestamp` → drawable `presentedTime`, vsync on, p95 < one refresh
interval on battery. It already said the honest first number would likely miss,
and forbade flattening the instrument (ADR 0002 D11).

Chip 0 now has the atlas presenter 0003 D1 required. The remaining miss is not
the VT and not a 60 Hz `NSTimer`:

| Segment | Typical, after warmup |
|---|---|
| key → `commit` | ~1.4ms |
| `commit` → `presentedTime` | ~20–22ms |
| present cadence | ~25ms (~40 Hz) — one present per key |

libghostty-vt is not the slow part. `CAMetalLayer` inside an `NSWindow`, with
`displaySyncEnabled`, is.

Attempts to pin ProMotion at 120 Hz by presenting extra frames either queued
three–four vsyncs (p95 ~38ms) or broke the sentinel (discard abort). Skipping
those presents returns the 23ms / 40 Hz miss. That is a contradiction, not a
missed `tryPresent` flag.

## Decision

### D1 — Spike 0 stays Red

T-NFR is **Red**. No gate is **Proven** (ADR 0002 D8). Milestone 1 stays closed.
The 23.5ms battery hid run is the Spike 0 result for NFR-KEY. The withdrawn
`p95=0.032ms` run must not be cited.

### D2 — ADR 0003's oracle and budget do not move

T-NFR remains `NSEvent.timestamp` → `presentedTime`, vsync on, n ≥ 1000,
discards ≤ 2%, p95 < one refresh interval at the display's actual maximum rate,
battery as the closing run. We do not grade 120 Hz hardware against 16.7ms. We
do not measure to `commit`. We do not present off-vsync to shrink the number.

### D3 — Chip 0's present path is not the closing presenter

The glyph-atlas draw (0003 D1) stays. The **present** path that ships in
`TerminalView` today — echo → `CAMetalLayer` `presentDrawable:` in an
`NSWindow` — does not close NFR-KEY. Further heartbeat, coalesce, or in-flight
knobs on that path are not Spike 0 work.

### D4 — A later presenter is a new spike, then a new ADR

If we want NFR-KEY green, the next step is a **research spike** (may explore;
must not merge). After that spike, and only then:

1. An **Accepted** ADR that names the presenter and what it will not do
   (full libghostty exec remains rejected by ADR 0001 §1).
2. Spec.
3. Named T-NFR still failing for p95 23.5ms vs 8.33ms on the Chip 0 present
   path (the bug the test was born from).
4. Smallest implementation that turns that test green.
5. Packaged hid on battery.

This ADR (0004) does **not** pick that presenter and does **not** authorize
step 4.

### D5 — Library gates are not a substitute

T-BYTES, T-DROP, T-ATTACH, T-RESIZE, T-EXIT, T-SPAWN, T-KILL, and T-RESYNC may
sit at **Green-unproven**. They do not open Milestone 1 while T-NFR is Red.

## Consequences

- Stop adding agents, Blocks, chrome, or Chip 1 as the live chip.
- `host/macos/TerminalView.m` present-path PRs that do not follow D4 are
  rejected.
- A Proposed presenter ADR is research documentation, not a license to land
  Metal.

## Rejected alternatives

- **Widen the budget to 16.7ms or ~23ms.** Rejected: ADR 0002 D11 and 0003 D8.
- **Measure to `commit`.** Already rejected by 0003.
- **Off-vsync.** Already rejected by 0003.
- **Keep turning `TerminalView` present knobs until 8.33ms.** Rejected: the
  2026-08-16 hid series showed 120 Hz and one-frame latency do not coexist on
  this path.
- **Take Ghostty's GPU exec.** Already rejected by ADR 0001 §1.
- **Mark Spike 0 Proven except T-NFR.** Rejected: the stop rule is every gate
  Proven, including T-NFR on battery.
