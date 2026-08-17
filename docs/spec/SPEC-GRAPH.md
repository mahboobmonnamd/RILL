# SPEC-GRAPH — session graph (Milestone 1, `lane:kernel`)

- **Status:** Accepted for the first slice — 2026-08-17
- **Authority:** [ADR 0011](../adr/0011-session-graph.md)
- **Issue:** [#16](https://github.com/mahboobmonnamd/RILL/issues/16)
- **Crates:** `crates/rill-kernel`, `crates/rilld` (attach payload: `lane:attach` follow-on)
- **Gates:** T-GRAPH-SPAWN, T-GRAPH-ISOLATE, T-GRAPH-ATTACH (all **Red** until
  demonstrated). No production map until Spike 0 close docs are on `main`.

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. In this slice

- Kernel MUST store `SessionId → Session`.
- `SessionId` is a `u64` allocated by the kernel. It MUST NOT be a path, a
  title, or a GUI index.
- `Session` rules in [SPEC-KERNEL](SPEC-KERNEL.md) hold **per id** (sole writer,
  ring, credit, EXIT retention, `wait_readable`, no master export).
- Creating a leaf MUST be a cold call (`Kernel::spawn_leaf` or equivalent), not
  a warm `DATA` frame.
- Default daemon start MAY spawn one leaf so Spike 0 packaged tests keep a
  single attach.

## 2. Attach (this slice vs follow-on)

This issue MAY keep one live attach in `rilld` while the **kernel map** is
tested in-process (two `Session` values, no second socket). If `ATTACH` grows a
session-id field, that change is `lane:attach` and a second issue; it MUST NOT ship as
an untested tag.

Until that follow-on:

- In-process tests MUST attach/refuse by id on the kernel map.
- The packaged one-window path MUST still speak Spike 0 frames.

## 3. Isolation

- Distinct ids MUST have distinct child pids while both are alive.
- Bytes written by one child MUST NOT appear in the other leaf's `history()`.
- `stalled_reads` and `credit` are per leaf.

## 4. Persist

- GUI quit / `SIGKILL` of the window process MUST NOT kill any leaf (existing
  T-KILL, now for every live id once a packaged graph exists).
- This slice's tests are library-level. Packaged multi-leaf persist is a later
  issue; socket-only tests do not close it.

## 5. Out of scope

Tabs, splits, sidebar, session titles in the window, conversations, agents,
Chip 1 live, daemon-crash survival, reconnect tokens, JSON on the typing path.

## 6. What we will not do

- Dump a graph into `Text` or a second VT.
- Replace Chip 0.
- Re-dispatch hosted `gates.yml` to chase hid.
