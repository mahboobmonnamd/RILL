# SPEC-GRAPH — session graph (Milestone 1, `lane:kernel`)

- **Status:** Accepted — first slice **Proven** 2026-08-17
  ([ADR 0014](../adr/0014-m1-first-slice-closes.md)); persist remainder
  [ADR 0015](../adr/0015-m1-persist-remainder.md).
- **Authority:** [ADR 0011](../adr/0011-session-graph.md),
  [ADR 0014](../adr/0014-m1-first-slice-closes.md),
  [ADR 0015](../adr/0015-m1-persist-remainder.md)
- **Issue:** [#16](https://github.com/mahboobmonnamd/RILL/issues/16), attach naming: [#28](https://github.com/mahboobmonnamd/RILL/issues/28), terminate: [#29](https://github.com/mahboobmonnamd/RILL/issues/29), flood isolation: [#31](https://github.com/mahboobmonnamd/RILL/issues/31), persist remainder: [#69](https://github.com/mahboobmonnamd/RILL/issues/69)–[#84](https://github.com/mahboobmonnamd/RILL/issues/84), N-leaf persist: [#255](https://github.com/mahboobmonnamd/RILL/issues/255), close: [#254](https://github.com/mahboobmonnamd/RILL/issues/254)
- **Crates:** `crates/rill-kernel`, `crates/rilld`, `crates/rill-attach`
- **Gates:** T-GRAPH-* first slice **Proven** (ADR 0014). Persist remainder
  gates in TEST-CASES (protocol, nested, delivery, events, layout, ephemeral,
  observe, N-leaf T-KILL). Packaged host still sends 8-byte ATTACH (default
  leaf). Window chrome stays one attached leaf (ADR 0011 D5).

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
- A flood on one leaf MUST NOT drop bytes on another (PRD NFR-DROP when panes
  exist).
- `terminate(id)` MUST NOT kill any other live child.

## 4. Persist

- GUI quit / `SIGKILL` of the window process MUST NOT kill any live leaf
  (T-KILL, N children via `RILL_TEST_SECOND_LEAF`, ADR 0015 D8).
- Socket-only tests do not close packaged persist. Packaged T-KILL must keep
  every pid named in the test pidfiles.

## 5. Persist remainder (ADR 0015)

- Nested `rilld` MUST refuse when `RILL_INSIDE=1` unless `RILL_ALLOW_NESTED=1`.
- Input delivery is `Pending` / `Dispatched` / `Unknown` on the session, not a
  warm frame.
- Event ids are unique; `terminate` of a dead leaf is a no-op.
- `RILL_EPHEMERAL=1` makes `Drop` kill that child. Default remains persist.
- `layout_snapshot` is kernel state (ids, winsize, pid, cwd), not chrome.
- A second connection MAY observe a leaf that already has a writer. A second
  writer is still `REFUSED{AlreadyAttached}`.

## 6. Out of scope

Tabs, splits, sidebar, session titles in the window, conversations, agents,
Chip 1 live, daemon-crash survival, reconnect tokens, JSON on the typing path,
`SCM_RIGHTS` of the PTY master.

## 7. What we will not do

- Dump a graph into `Text` or a second VT.
- Replace Chip 0.
- Re-dispatch hosted `gates.yml` to chase hid.
- Relaunch Claude/Codex, SIGTERM user shells to hibernate agents, or pass the
  master fd to a replacement binary (ADR 0015 D9).
