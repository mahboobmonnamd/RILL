# ADR 0038: Session graph is the navigation model

- **Status:** Accepted — 2026-08-18
- **Amended by:** [ADR 0053](0053-runtime-domain-content-and-client-authority.md)
  D1–D3. `Session` is the durable grouping; the PTY owner is
  `TerminalExecution`; hidden UI does not remove domain objects; presentation
  close does not imply execution termination.
- **Historical identifier:** merged as ADR 0020 in PR #278; renumbered to ADR
  0038 on 2026-08-21 to resolve a collision. Renumbering changed no decision.
- **Tree:** this repository only
- **Issue:** catalog epic [#33](https://github.com/mahboobmonnamd/RILL/issues/33).
  Rows authorized by this ADR: F-001 [#38](https://github.com/mahboobmonnamd/RILL/issues/38),
  F-002 [#39](https://github.com/mahboobmonnamd/RILL/issues/39),
  F-003 [#40](https://github.com/mahboobmonnamd/RILL/issues/40),
  F-004 [#41](https://github.com/mahboobmonnamd/RILL/issues/41),
  F-005 [#42](https://github.com/mahboobmonnamd/RILL/issues/42),
  F-006 [#43](https://github.com/mahboobmonnamd/RILL/issues/43),
  F-007 [#44](https://github.com/mahboobmonnamd/RILL/issues/44),
  F-008 [#45](https://github.com/mahboobmonnamd/RILL/issues/45),
  F-009 [#46](https://github.com/mahboobmonnamd/RILL/issues/46),
  F-017 [#54](https://github.com/mahboobmonnamd/RILL/issues/54),
  F-018 [#55](https://github.com/mahboobmonnamd/RILL/issues/55),
  F-019 [#56](https://github.com/mahboobmonnamd/RILL/issues/56),
  F-020 [#57](https://github.com/mahboobmonnamd/RILL/issues/57),
  F-024 [#61](https://github.com/mahboobmonnamd/RILL/issues/61)
- **Requires:** [ADR 0011](0011-session-graph.md) (kernel map, Proven via
  [ADR 0014](0014-m1-first-slice-closes.md)),
  [ADR 0015](0015-m1-persist-remainder.md) (`layout_snapshot`, observe),
  [ADR 0013](0013-cwd-tap.md), [ADR 0018](0018-three-pane-host-chrome.md)
- **Amends:** ADR 0011 D5 and ADR 0018 D1 — the window MAY attach more than one
  leaf once the container tree below exists in the kernel. Until it does, the
  shipped window stays one leaf.
- **Does not authorize:** Blocks, agents, conversations, Chip 1 live, remote
  hosts (ADR 0041), non-terminal surfaces (ADR 0042), JSON or cells on the warm
  path, a second window, a second VT, chrome that stores authoritative topology.

## Context

The kernel already owns `SessionId → Session` (ADR 0011 D1) and already emits a
`layout_snapshot` of ids, winsize, pid and cwd (ADR 0015, SPEC-GRAPH §5). The
window paints one leaf inside a three-column split (ADR 0018 D1).

Fourteen catalog rows describe the same object from the UI side: workspace
(F-004), workspace groups (F-005), tabs that own layout (F-006), nested splits
(F-007), surface stacks inside a split (F-008), close-narrowest (F-009), drag
rearrange (F-017), zoom/equalize (F-018), layout templates (F-019), sidebar hide
(F-020), vertical workspace tabs (F-024), plus session identity (F-002, F-003)
and the host indicator (F-001).

The failure mode is specific and the previous prototype died of it: chrome grows
its own tree, the kernel map becomes a detail the UI syncs against, and the two
disagree the first time a child exits while a tab is hidden. ADR 0011 D5 refused
chrome over a one-`Session` kernel for exactly this reason. The answer is not to
keep chrome thin forever — it is to put the containers in the kernel and let
chrome be a projection.

## Decision

### D1 — Containers are kernel nodes, not chrome state

The kernel graph gains **container** nodes above leaves: `Workspace`, `Group`,
`Tab`, `Split`. Each has a kernel-allocated stable `NodeId` (`u64`, disjoint
from `SessionId`, never reused while live). A leaf is addressed as it is today.

Chrome MUST NOT hold authoritative topology. Chrome MAY cache a snapshot for
paint. On disagreement the kernel wins; chrome re-reads. A host that can render
a tab the kernel does not have is a bug, not a race.

`layout_snapshot` (SPEC-GRAPH §5) MUST carry the container tree. It stays a
**cold** call. It MUST NOT appear on the typing path, and MUST NOT carry cells,
scrollback, or per-cell `String`.

### D2 — A pane slot is a stack; hiding detaches the presenter, never the leaf

F-008 (surface stack) and F-006 (inactive tabs keep live panes) are one rule.

A pane slot holds an ordered list of surfaces and shows one at a time. Hiding a
surface MUST detach only the **presenter**. The kernel MUST keep the leaf alive
and MUST keep draining its PTY into its ring. A hidden pane that stops draining
would stall the child and violate NFR-DROP; a hidden pane that terminates its
leaf would violate the persist wedge (ADR 0001).

Re-showing a surface MUST resync through the existing cold per-leaf path
(FR-RESYNC), once, and MUST NOT respawn.

Named test `t_hidden_surface_keeps_draining_and_same_pid`. Mutation
`hide_detaches_leaf` MUST turn T-NAV-STACK red.

### D3 — Close resolves the narrowest focused container, and only by `terminate`

⌘W (F-009) resolves innermost-first: surface → pane → tab → workspace → window.

Closing a container MUST terminate the leaves it owns by explicit
`Kernel::terminate(id)` (ADR 0011 D2). `Drop` MUST NOT kill a child, then or
ever.

Closing the **window** MUST NOT terminate anything. That is the product promise
(PRD §1) and T-KILL already guards it. A close path that reaches `terminate`
from window teardown is a regression of Spike 0, not a new feature.

Mutation `close_window_terminates_leaves` MUST turn T-NAV-CLOSE red, and MUST
also turn packaged T-KILL red — if it does not, T-NAV-CLOSE is not wired to the
real path.

### D4 — Rearranging is cold reparenting; identity does not move

Drag (F-017), zoom/equalize (F-018), split/unsplit (F-007) and template restore
(F-019) MUST be reparent/resize operations on the container tree.

They MUST NOT change any `SessionId`, MUST NOT `posix_spawn` a shell, MUST NOT
detach and re-attach a visible leaf, and MUST NOT put frames on the warm path
beyond the in-band `RESIZE` that already exists (FR-RESIZE).

Layout templates (F-019) serialize the container tree, per-leaf cwd, and startup
command. They MUST NOT serialize scrollback and MUST NOT claim to restore a live
child: reopening a template spawns new leaves, and says so.

Named test `t_reparent_preserves_session_ids_and_pids`. Mutation
`reparent_respawns` MUST turn T-NAV-REPARENT red.

### D5 — Sidebar visibility is view state

Sidebar hide (F-020) and vertical workspace tabs (F-024) are presentation of the
same projection. Toggling either MUST NOT detach, terminate, resize a leaf, or
touch the kernel at all.

Mutation `hide_sidebar_detaches` MUST turn T-NAV-VIEWSTATE red.

### D6 — Session identity is the kernel's, and the host indicator is read cold

Default session (F-002) is the daemon's default leaf, which SPEC-GRAPH §2
already names for 8-byte ATTACH. Named sessions (F-003) are a cold
kernel-owned label on a `Workspace` node; a name is **not** an id and MUST NOT
be used to address a leaf on the attach path.

The host indicator (F-001) reads the leaf's kernel identity cold. In this
milestone every kernel is local and the indicator MUST render `local`. Remote is
ADR 0041; this ADR does not authorize a second kernel.

### D7 — Oracle

Gates, all against the kernel and the packaged app, not against a chrome cache:

| ID | Closes |
|---|---|
| T-NAV-TOPOLOGY | `layout_snapshot` carries the container tree; chrome renders from it; a node chrome invented does not appear |
| T-NAV-STACK | D2 — hidden surface keeps pid, keeps draining |
| T-NAV-CLOSE | D3 — narrowest-first; window close terminates nothing |
| T-NAV-REPARENT | D4 — ids and pids survive drag/zoom/template |
| T-NAV-VIEWSTATE | D5 — sidebar toggle is inert at the kernel |

Socket-only tests do not close D2–D5 where they are user-visible; packaged
`Rill.app` e2e is the closer (ADR 0002 D8). T-NFR MUST NOT be re-cut: none of
these surfaces may add a control-plane RPC to an attached leaf's key path
(ADR 0011 D6).

## Consequences

- [SPEC-NAV](../spec/SPEC-NAV.md) is the host + kernel contract for the
  container tree.
- SPEC-GRAPH §5 `layout_snapshot` grows containers; §6 no longer lists tabs and
  splits as out of scope, and defers them here.
- ADR 0018's one-leaf window stands until the kernel tree lands. Chrome that
  paints tabs over a kernel with no `Tab` node is the ADR 0011 D5 lie again.

## Rejected alternatives

- **Chrome owns the tree, kernel stays a flat map.** Rejected: this is the
  prototype failure in PRD §2. Two trees disagree on child exit.
- **A pane stack multiplexes several leaves onto one attach.** Rejected: FR-ONE
  is per leaf (ADR 0011 D3). A second surface gets a second attach, not a tag.
- **Hidden tabs stop reading the PTY to save memory.** Rejected: stalls the
  child, breaks NFR-DROP. Bounded memory is F-099 (ADR 0040 D6), a ring policy,
  not a reason to stop draining.
- **Layout templates that restore live children.** Rejected: honest respawn.
  The persist promise is about the daemon outliving the window, not about a
  saved file resurrecting a dead pid.
- **`SessionId` as a path or a title so the UI can address it.** Rejected:
  SPEC-GRAPH §1.
