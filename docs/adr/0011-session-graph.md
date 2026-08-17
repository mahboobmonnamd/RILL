# ADR 0011: Session graph

- **Status:** Accepted — 2026-08-17
- **Tree:** this repository only
- **Issue:** [#16](https://github.com/mahboobmonnamd/RILL/issues/16)
- **Requires:** [ADR 0001](0001-session-operating-system.md),
  [ADR 0010](0010-spike-0-closes.md) (Spike 0 Proven). Presenter remains
  [ADR 0009](0009-direct-to-display-echo.md).
- **Does not authorize:** chrome, Blocks, agents, Chip 1 as the live chip,
  JSON on the warm path, a second T-NFR instrument.

## Context

Spike 0 closed a **one-leaf wedge**: `rilld::Daemon` holds one `Session`, one
Unix listener, one attach claim, one headless `Chip0` for resync. That is
enough to prove persist and NFR-KEY. It is not a session operating system.

[SPEC-KERNEL](../spec/SPEC-KERNEL.md) §11 left multiple sessions out of Spike 0
on purpose. README still draws a pane graph. M1 is that graph in the kernel,
not a window chrome pass.

## Decision

### D1 — The kernel owns a map of leaves

A leaf is one PTY, one byte ring, one attach claim. The kernel addresses it by
a stable `SessionId` (opaque `u64`, allocated by the kernel, never reused for a
live child). `Daemon` holds `SessionId → Session`, not a single field.

Spike 0 behaviour is the map of size 1. Existing gates MUST remain green on
that map.

### D2 — Create and destroy are cold

Spawning a leaf is orchestration: not `DATA`, not `CREDIT`. The warm path does
not allocate sessions. Destroy is explicit `terminate` on that id; `Drop` still
MUST NOT kill the child (ADR 0001 persist wedge).

Daemon crash, logout, and app update stay out of wedge (ADR 0001 §7).

### D3 — One attach per leaf, N leaves per daemon

A second `ATTACH` for an id that already has a client is `REFUSED{AlreadyAttached}`
and MUST NOT disturb the first client (FR-ONE, per leaf). A second `ATTACH` for
a **different** id is accepted.

The listener remains one `SOCK_STREAM` accept path. How a connection names the
id (ATTACH payload vs a later multiplex tag) is specified in
[SPEC-GRAPH](../spec/SPEC-GRAPH.md). Frames on the warm path stay bytes, credit,
resize, exit — not JSON, not cells.

### D4 — Isolation

Histories, credit, stall counters, and child pids MUST NOT leak across ids.
A flood on one leaf MUST NOT drop bytes on another (PRD NFR-DROP when panes
exist). Resync is still per-leaf, same Chip 0 implementation, cold, once per
attach of that leaf.

### D5 — Display does not grow a graph UI

The shipped window may still show **one** attached leaf. Tabs, splits, sidebar,
and a second window are M2. M1 is false if it ships chrome to hide a kernel that
is still one `Session`.

### D6 — Do not hide NFR-KEY

Adding a leaf MUST NOT put JSON, cell dumps, or extra control RPCs on the key
path of an attached leaf. T-NFR is not re-cut. Chip 1 stays isolated until M7
([ADR 0012](0012-chip1-isolated-vt.md)).

## Consequences

- [#16](https://github.com/mahboobmonnamd/RILL/issues/16) is the first lane A
  slice: map, spawn two, refuse second attach on the same id.
- [SPEC-KERNEL](../spec/SPEC-KERNEL.md) §11 no longer forbids multiple sessions;
  it defers the contract to this ADR and SPEC-GRAPH.
- Named tests in [TEST-CASES](../TEST-CASES.md) (T-GRAPH-*) start **Red**.

## Rejected alternatives

- **Refactor crates before the map.** Rejected: planes already exist.
- **Multiplex cells or JSON so the GUI can paint N panes.** Rejected: ADR 0001.
- **M2 chrome first.** Rejected: chrome on one Session is a lie.
- **Wait for Chip 1.** Rejected: Chip 0 remains the live chip.
