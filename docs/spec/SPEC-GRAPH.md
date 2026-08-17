# SPEC-GRAPH — session graph (Milestone 1, `lane:kernel`)

- **Status:** Accepted for the first slice — 2026-08-17
- **Authority:** [ADR 0011](../adr/0011-session-graph.md)
- **Issue:** [#16](https://github.com/mahboobmonnamd/RILL/issues/16), attach naming: [#28](https://github.com/mahboobmonnamd/RILL/issues/28), terminate: [#29](https://github.com/mahboobmonnamd/RILL/issues/29)
- **Crates:** `crates/rill-kernel`, `crates/rilld`, `crates/rill-attach`
- **Gates:** T-GRAPH-SPAWN, T-GRAPH-ISOLATE, T-GRAPH-ATTACH, T-GRAPH-TERMINATE,
  T-ATTACH-NAMED. `Kernel` holds the map; default daemon start still spawns one
  leaf. Packaged host still sends 8-byte ATTACH (default leaf).

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. In this slice

- Kernel MUST store `SessionId → Session`.
- `SessionId` is a `u64` allocated by the kernel. It MUST NOT be a path, a
  title, or a GUI index.
- `Session` rules in [SPEC-KERNEL](SPEC-KERNEL.md) hold **per id** (sole writer,
  ring, credit, EXIT retention, `wait_readable`, no master export).
- Creating a leaf MUST be a cold call (`Kernel::spawn_leaf` or equivalent), not
  a warm `DATA` frame.
- Destroying a leaf MUST be an explicit `Kernel::terminate(id)`. It MUST NOT
  kill any other live child. `Drop` still MUST NOT kill the child.
- Default daemon start MAY spawn one leaf so Spike 0 packaged tests keep a
  single attach.

## 2. Attach

- 8-byte ATTACH is generation only and MUST attach the default leaf (Spike 0
  packaged path).
- 16-byte ATTACH is generation + `session_id` and MUST attach that leaf.
- Unknown `session_id` MUST yield `REFUSED{Invalid}` on that connection.
- A second connection MAY ATTACH a different live id while the first stays
  attached (ADR 0011 D3). FR-ONE is per leaf, not per daemon.
- The packaged one-window path MUST still send 8-byte ATTACH until a later
  host issue names a second leaf.

## 3. Isolation

- Distinct ids MUST have distinct child pids while both are alive.
- Bytes written by one child MUST NOT appear in the other leaf's `history()`.
- `stalled_reads` and `credit` are per leaf.
- `terminate(id)` MUST NOT kill any other live child.

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
