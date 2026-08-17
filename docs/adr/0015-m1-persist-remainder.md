# ADR 0015: M1 persist remainder

- **Status:** Accepted — 2026-08-17
- **Tree:** this repository only
- **Issue:** persist catalog on M1 ([#69](https://github.com/mahboobmonnamd/RILL/issues/69)–[#84](https://github.com/mahboobmonnamd/RILL/issues/84) as classified below), packaged N-leaf [#255](https://github.com/mahboobmonnamd/RILL/issues/255)
- **Requires:** [ADR 0011](0011-session-graph.md), [ADR 0014](0014-m1-first-slice-closes.md)
- **Amends:** ADR 0011 D3 — a second connection MAY **observe** a leaf that already has a writer. A second **writer** is still `REFUSED{AlreadyAttached}` (FR-ONE).
- **Does not authorize:** tab/split chrome, agents, `SCM_RIGHTS` of the PTY master, JSON on the warm path, Chip 1 live, daemon-crash / logout survival (ADR 0001 §7).

## Context

Closing M1 by moving persist catalog rows to other milestones postponed the work. This ADR keeps that work on M1 and names the kernel/attach slices. It does not ship Warp-class tabs or a Claude relauncher.

## Decision

### D1 — Protocol version is visible

ATTACH payloads: 8-byte (generation, Spike 0), 16-byte (generation + session), 18-byte (generation + session + `protocol u8` + `flags u8`). Protocol **1** is this tree. Any other protocol byte MUST `REFUSED{ProtocolMismatch}`. 8- and 16-byte payloads imply protocol 1. Packaged host still sends 8-byte.

### D2 — Nested launch is refused unless opted in

The kernel sets `RILL_INSIDE=1` on the user shell. `rilld` MUST refuse to bind if that is set and `RILL_ALLOW_NESTED` is unset. Mutation `skip_nested_guard` MUST turn the gate red.

### D3 — Input delivery is kernel-visible, not JSON

Per leaf: `Pending` (queued, not yet written), `Dispatched` (written to the PTY master), `Unknown` (detach while still queued). Not an attach warm tag. Not a control RPC.

### D4 — Event ids are stable; terminate is idempotent

`Kernel` records spawn / attach / terminate / exit with monotonic ids. A second `terminate` of a dead leaf MUST NOT emit a second terminate event and MUST NOT kill another leaf.

### D5 — Ephemeral is opt-in debug

`RILL_EPHEMERAL=1`: `Drop` of a `Session` terminates that child. Default remains persist (T-KILL). Detach in ephemeral mode terminates that leaf only. Mutation `ignore_ephemeral` MUST turn the gate red.

### D6 — Layout snapshot is kernel state, not chrome

`Kernel::layout_snapshot()` returns each live id, winsize, child pid, cwd. No window, no tabs. Restore-after-daemon-crash stays ADR 0001 §7.

### D7 — Observe is not a second writer

`flags` bit 0 = observe. Observer receives DATA/EXIT, MUST NOT write the PTY. Writer claim unchanged. Fan-out DATA to every client on that leaf (pop once). Mutation `allow_observer_write` MUST turn the gate red.

### D8 — N live children survive GUI death

`Daemon::spawn_leaf` plus GUI `SIGKILL` MUST leave every live child pid alive. Test env `RILL_TEST_SECOND_LEAF=1` spawns a second leaf at bind so packaged T-KILL can observe two pids. Not a second window.

### D9 — Out of this ADR

- F-037 / F-046 agents: SessionId is the resume handle. No provider relaunch, no SIGTERM of shells.
- F-038 binary replace `--handoff`: still ADR 0001 §7. Second `rilld` on a live socket remains `AlreadyRunning`. No `SCM_RIGHTS`.
- F-047 / F-035 chrome: moving panes between tabs is M2. Kernel identity and `layout_snapshot` are this ADR.

## Consequences

Named T-GRAPH / T-ATTACH tests below. Inventory rows close against those tests or stay open only for D9.

## Rejected alternatives

- **Wait for M2 chrome to persist N leaves.** Rejected: the kernel already has N leaves.
- **Takeover as the second attach.** Rejected: FR-ONE. Observe is the M1 addition.
- **Pass the master fd to a new binary.** Rejected: ADR 0001 §5.
