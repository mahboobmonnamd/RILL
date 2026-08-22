# ADR 0056: Vertical slices — backend and host projection together

- **Status:** Accepted — 2026-08-21
- **Tree:** this repository only
- **Decision approval:** product owner, 2026-08-21: the mock is not the product
  without its other parts; waiting for a complete backend before any matching
  UI does not feel like the product. Backend and UI for the same object ship
  together. Independent objects may proceed in parallel.
- **Issue:** vertical tab slice
  [#345](https://github.com/mahboobmonnamd/RILL/issues/345). Mouse encoder
  remains [#344](https://github.com/mahboobmonnamd/RILL/issues/344). Deferred
  compositor/Blocks tracker [#338](https://github.com/mahboobmonnamd/RILL/issues/338).
  ContentTimeline library [#331](https://github.com/mahboobmonnamd/RILL/issues/331).
- **Requires:** [ADR 0053](0053-runtime-domain-content-and-client-authority.md)
  D12, D16, D22; [ADR 0055](0055-mockup-is-destination-mouse-first.md);
  [ADR 0018](0018-three-pane-host-chrome.md); [ADR 0038](0038-session-graph-navigation-model.md)
- **Amends:** ADR 0053 D12 — **journaled** topology persist stays step 5.
  Host presentation of the **in-memory** kernel container tree (including
  chrome-driven `create_node` / `spawn_leaf`) MUST NOT wait for step 5.
  ADR 0055 D1 — tabs/splits are the next vertical slice (#345), not “after
  persist.” ADR 0018 — the one-leaf window was the M2 layout closer, not a
  permanent ban on a second kernel leaf presented in the same window.
- **Does not authorize:** Flow compositor or Command Block cards before
  ContentTimeline exists; fake tab/file-tree/Attention rows the kernel does
  not have; GUI `posix_spawn` of the user shell; JSON on the warm attach path;
  skipping named tests.

## Context

ADR 0055 correctly refused to dump the whole mock onto Chip 1 in one change.
That delivery reading — backend foundations for months, chrome last — leaves a
window that is not the product: no tabs, no Blocks, no second pane, while the
kernel already has `Workspace` / `Tab` / `spawn_leaf`.

The mock without those parts is not the product. Parallel **lanes** already
exist ([LANES](../LANES.md)). What was missing is a rule that a
**user-visible object** is one issue: kernel/attach/VT mechanism **and** host
projection, or an honest empty state — never a painted lie, never mechanism
with no way to use it.

## Decision

### D1 — A slice is vertical

For each user-visible capability:

1. Accepted ADR + spec already name the object (or this ADR plus a spec delta).
2. One GitHub issue, all lanes that the object crosses (`lane:kernel` and
   `lane:host` when chrome creates a leaf).
3. Named tests cover the mechanism **and** the host projection (packaged e2e
   if the user can see it). Socket-only tests still do not close paint/spawn.
4. Implementation is the smallest change that turns those tests green on both
   sides.

A kernel library Proven with no window is a layer, not a shipped feature
(SPEC-NAV §11). A host control with no kernel call is a lie (ADR 0011 D5).

### D2 — Parallel means independent objects, not skipped prerequisites

Lanes MAY work at the same time on **different** objects:

| Track | Object | UI in the same issue | Must not ship yet |
|---|---|---|---|
| A | Pointer reports (#344) | Host encodes clicks/wheel | Selection chrome (ADR 0052) |
| B | In-memory tabs/panes (#345) | File → New Tab, tab strip, second attach | Daemon journal restore (D12 step 5) |
| C | ContentTimeline library (#331) | Honest empty Flow; Raw remains Chip 1 | Block cards / compositor |
| D | Task / Attention | Empty Agents row until Task exists | HITL fake queue |

Authority order in ADR 0053 D12 still applies **inside** an object: Flow UI
cannot precede ContentTimeline. Independent objects do not queue behind each
other (mouse need not wait for tabs; tabs need not wait for Blocks).

### D3 — Honest empty is the UI for missing authority

If the runtime object does not exist, chrome shows the specified empty state
(SPEC-CHROME §4 Agents). It MUST NOT scrape the grid, invent ids, or clone
mock bitmaps.

### D4 — In-memory topology is live; persist is later

`rilld` already `create_node`s Workspace+Tab and `spawn_leaf` at bind. Chrome
MUST grow a cold command on the existing `.nav` socket (not the warm attach
splice) so New Tab / New Pane call `spawn_leaf` + `create_node` + `attach_leaf`
in the daemon, then attach that `SessionId`. Extra windows MAY follow the same
pattern (second presenter, same runtime).

Closing a tab is presentation-first (SPEC-NAV §3). Terminate remains explicit
(ADR 0053 D3).

Journaled restore across daemon death stays D12 step 5 and MUST NOT be faked
by host-local tab lists.

### D5 — Blocks still ride ContentTimeline

Wanting the mock “full” does not move Flow onto the warm path (D22). Track C
builds the ledger in `lane:kernel` **in parallel** with Tracks A/B. Host Flow
chrome is the vertical half of the issue that closes step 4, not a side
project that paints cards over `PodGrid`.

## Consequences

- Next implementation issue is [#345](https://github.com/mahboobmonnamd/RILL/issues/345),
  not more deferred-docs-only.
- [#344](https://github.com/mahboobmonnamd/RILL/issues/344) stays the pointer
  vertical slice (host + Chip 1 modes).
- SPEC-NAV’s “no window yet” clause is the gap #345 closes.

## Rejected alternatives

- **Backend-complete, then a chrome epic.** Rejected: the window never feels
  like the product.
- **Paint the mock now and wire it later.** Rejected: invented topology.
- **Reorder D12 so Blocks precede transcript.** Rejected: D22 and ADR 0050.
