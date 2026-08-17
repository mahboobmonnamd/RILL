# ADR 0014: M1 first slice closes

- **Status:** Accepted — 2026-08-17
- **Tree:** this repository only
- **Issue:** [#254](https://github.com/mahboobmonnamd/RILL/issues/254)
- **Requires:** [ADR 0010](0010-spike-0-closes.md) (Spike 0 Proven),
  [ADR 0011](0011-session-graph.md) (session graph). Presenter remains
  [ADR 0009](0009-direct-to-display-echo.md).
- **Amended by:** [ADR 0015](0015-m1-persist-remainder.md) (persist remainder on M1)
- **Does not authorize:** chrome, Blocks, agents, Chip 1 as the live chip,
  JSON on the warm path, a second T-NFR instrument, `SCM_RIGHTS` of the PTY
  master. Packaged N-leaf persist and inventory persist rows are ADR 0015.

## Context

[ADR 0011](0011-session-graph.md) named the M1 kernel: `SessionId → Session`,
one attach per leaf, isolate histories, terminate one without killing another,
flood on A must not drop B. Those slices landed as [#16](https://github.com/mahboobmonnamd/RILL/issues/16),
[#28](https://github.com/mahboobmonnamd/RILL/issues/28),
[#29](https://github.com/mahboobmonnamd/RILL/issues/29),
[#31](https://github.com/mahboobmonnamd/RILL/issues/31).

The GitHub milestone still held fifteen Warp×cmux×Herdr catalog rows
(`inventory` + `blocked`). Those issues say they do **not** authorize code.
They are not a build list. Closing M1 by implementing agents, tabs, or
multi-client takeover would violate ADR 0011 D3–D6 and ADR 0002 D11.

## Decision

### D1 — The first slice is Proven

T-GRAPH-SPAWN, T-GRAPH-ISOLATE, T-GRAPH-ATTACH, T-GRAPH-TERMINATE,
T-ATTACH-NAMED, and T-GRAPH-FLOOD are **Proven** under ADR 0002 D2–D6.

Kernel gates cite `fast.yml` on `main` after those PRs, plus demonstrated red
under `RILL_MUTATE=single_session` and `terminate_all_leaves` recorded in
[#27](https://github.com/mahboobmonnamd/RILL/pull/27) and
[#30](https://github.com/mahboobmonnamd/RILL/pull/30). Attach named-id and flood
cite [#30](https://github.com/mahboobmonnamd/RILL/pull/30) and
[#32](https://github.com/mahboobmonnamd/RILL/pull/32). This closer wires those
names into `fast.yml` (kernel, no Zig) and `validate-spike0.sh` (including
rilld) so D8 keeps holding.

### D2 — Packaged N-leaf persist is ADR 0015

Superseded by [ADR 0015](0015-m1-persist-remainder.md) D8. The window still
sends 8-byte ATTACH to the default leaf (ADR 0011 D5). A second live child at
daemon bind (`RILL_TEST_SECOND_LEAF`) is how packaged T-KILL names N pids.
Socket-only tests still do not close it.

### D3 — Catalog rows: first-slice classification, persist remainder in 0015

The first-slice close classified rows. [ADR 0015](0015-m1-persist-remainder.md)
keeps the persist remainder on M1 and names tests. D9 of that ADR is the
honest refuse for agents, binary-replace handoff, and tab chrome.

| Issue | Catalog | This tree |
|---|---|---|
| [#68](https://github.com/mahboobmonnamd/RILL/issues/68) F-031 Reattach same IDs | workspace/tab identity | **Done for a leaf:** stable `SessionId`, 16-byte ATTACH, T-KILL reopen. Tabs/workspace chrome is M2. |
| [#69](https://github.com/mahboobmonnamd/RILL/issues/69) F-032 Event replay | stable event IDs | **ADR 0015 D4.** `GraphEvent` monotonic ids; terminate of a dead leaf is a no-op. |
| [#71](https://github.com/mahboobmonnamd/RILL/issues/71) F-034 Runtime crash honesty | no auto-respawn | **Done:** T-EXIT. A dead child stays dead. Window hollow cursor is [#17](https://github.com/mahboobmonnamd/RILL/issues/17). |
| [#72](https://github.com/mahboobmonnamd/RILL/issues/72) F-035 Layout snapshot | restore shape/focus | **ADR 0015 D6** kernel snapshot (ids, winsize, pid, cwd). Tab chrome is M2. |
| [#73](https://github.com/mahboobmonnamd/RILL/issues/73) F-036 Block/scrollback restore | restore recent output | **Done for bytes:** kernel ring + T-RESYNC. Blocks are M6 ([#22](https://github.com/mahboobmonnamd/RILL/issues/22)). |
| [#74](https://github.com/mahboobmonnamd/RILL/issues/74) F-037 Agent session resume | relaunch Claude/Codex | **ADR 0015 D9.** `SessionId` is the resume handle. No provider relaunch. |
| [#75](https://github.com/mahboobmonnamd/RILL/issues/75) F-038 Live server handoff | transfer PTYs on binary replace | **ADR 0015 D9.** Second `rilld` on a live socket is `AlreadyRunning`. No `SCM_RIGHTS`. |
| [#77](https://github.com/mahboobmonnamd/RILL/issues/77) F-040 Input delivery states | pending → dispatched | **ADR 0015 D3.** `InputDelivery` on the session, not a warm tag. |
| [#78](https://github.com/mahboobmonnamd/RILL/issues/78) F-041 Multi-client attach | observe / takeover | **ADR 0015 D7.** Observe is not a second writer. Takeover is still refused. |
| [#79](https://github.com/mahboobmonnamd/RILL/issues/79) F-042 Quit vs detach | GUI close detaches | **Done:** T-KILL. Stopping `rilld` is not a warn UI (M2). |
| [#80](https://github.com/mahboobmonnamd/RILL/issues/80) F-043 Nested-launch guard | block rill-in-rill | **ADR 0015 D2.** `RILL_INSIDE=1` refuses bind unless `RILL_ALLOW_NESTED=1`. |
| [#81](https://github.com/mahboobmonnamd/RILL/issues/81) F-044 Protocol version handshake | mismatch visible | **ADR 0015 D1.** 18-byte ATTACH; `REFUSED{ProtocolMismatch}`. |
| [#82](https://github.com/mahboobmonnamd/RILL/issues/82) F-045 Single-process escape hatch | `--no-session` debug | **ADR 0015 D5.** `RILL_EPHEMERAL=1` Drop terminates that child. |
| [#83](https://github.com/mahboobmonnamd/RILL/issues/83) F-046 Idle agent hibernation | SIGTERM restorable agents | **ADR 0015 D9.** MUST NOT SIGTERM user shells. |
| [#84](https://github.com/mahboobmonnamd/RILL/issues/84) F-047 Move pane without new PTY | move to another tab | **Done for identity:** `SessionId` is not a GUI index (ADR 0011 D1). Moving chrome is M2. |

### D4 — BGRA colour-emoji atlas is not M1

ADR 0003 D1's "Milestone 1" wording predates ADR 0011. Colour emoji remains a
later display slice. It does not block this close.

### D5 — Do not hide NFR-KEY

Unchanged from ADR 0011 D6 and ADR 0002 D11. Do not re-dispatch hosted
`gates.yml` to chase hid (ADR 0010 D4).

## Consequences

- TEST-CASES marks T-GRAPH-* **Proven**. SPEC-GRAPH first slice is closed.
- Persist remainder is [ADR 0015](0015-m1-persist-remainder.md), still M1.
- M2 may take chrome. Chip 1 stays isolated until M7.

## Rejected alternatives

- **Postpone persist catalog rows to later milestones so M1 can close on
  paper.** Rejected by ADR 0015: closing the milestone does not move the work.
- **Implement Warp agents, tab chrome, or PTY-master handoff as M1.** Rejected:
  those contradict ADR 0001 and ADR 0011. ADR 0015 D9 names the refuse.
- **Wait for packaged N-leaf persist before the first-slice close.** The kernel
  map was already Proven; N-leaf T-KILL is ADR 0015 D8 on the same milestone.
