# SPEC-NAV — container tree and navigation surfaces (`lane:host`, `lane:kernel`)

- **Status:** Accepted — 2026-08-18. `crates/rill-kernel` now implements §1's
  container tree (`NodeId`, `NodeKind`, `create_node`, `attach_leaf`,
  `reparent_node`, `close_node`, `container_snapshot`). Two kernel-plane
  sub-gates remain **Proven at the library level** —
  `cargo test -p rill-kernel --test nav_gates`, red-then-green demonstrated
  under `--features mutate` (evidence below). The full gates in §11's table
  remain **Red**: each also names a chrome-renders-from-it or
  window-close-terminates-nothing half that needs `host/macos/` wiring and a
  packaged `Rill.app` e2e (ADR 0038 D7, ADR 0002 D8). A kernel-plane library
  test does not close a gate whose oracle names packaged behaviour — it only
  proves the layer under it is sound.
- **Authority:** [ADR 0038](../adr/0038-session-graph-navigation-model.md),
  [ADR 0039](../adr/0039-inventories-are-cold-readers.md), amended by
  [ADR 0053](../adr/0053-runtime-domain-content-and-client-authority.md) D1–D3,
  [ADR 0056](../adr/0056-vertical-slices-backend-and-host.md)
- **Requires:** [SPEC-GRAPH](SPEC-GRAPH.md), [SPEC-KERNEL](SPEC-KERNEL.md),
  [SPEC-CHROME](SPEC-CHROME.md),
  [SPEC-DOMAIN-LIFECYCLE](SPEC-DOMAIN-LIFECYCLE.md)
- **Crates:** `crates/rill-kernel`, `crates/rilld`, `crates/rill-host`,
  `host/macos/`
- **Milestone:** M2 — Chrome

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. Durable hierarchy and existing container tree

- The target durable hierarchy is `Workspace → Session → Tab → Split tree →
  TerminalPane → TerminalExecution?`.
- The existing `Workspace`, `Group`, `Tab`, `Split` node slice is preserved as
  implementation evidence, not declared equal to the target hierarchy.
- `Group` is an optional non-owning organizational projection under Workspace;
  it MUST NOT become a lifecycle owner between Workspace and Session.
- `NodeId` is disjoint from durable `SessionId` and `TerminalExecutionId` and
  MUST NOT be reused while its node is live.
- `NodeId` MUST NOT be a path, a title, or a GUI index (same rule SPEC-GRAPH §1
  sets for `SessionId`).
- `layout_snapshot` MUST carry the container tree. It stays cold. It MUST NOT
  carry cells, scrollback, or per-cell `String`.
- Chrome MUST NOT hold authoritative topology. Chrome MAY cache for paint and
  MUST re-read on disagreement.
- A node chrome invented and the kernel does not have MUST NOT render.

## 2. Terminal panes and surface stacks

- A TerminalPane holds an ordered presentation stack and displays one surface.
- Its terminal surface references zero or one TerminalExecution. Rich surfaces
  and inspectors own no PTY.
- Hiding a surface MUST detach only the presenter.
- When more tabs exist than fit the strip, chrome MUST keep every tab reachable
  by horizontal scrolling. Clipping tabs with no scroll is forbidden.
- Holding Command MAY show small chord badges on tabs (`⌘1`–`⌘9`) and the
  workspace row (`⌥⌘1`). Badges MUST NOT become first responder or consume
  the flags-changed event. Releasing Command hides them.
- The runtime MUST keep a hidden TerminalExecution alive and drain its PTY under
  bounded recovery policy. Hidden panes MUST NOT stall the child.
- Re-showing initializes a disposable client mirror from the host checkpoint
  and deltas. It MUST NOT respawn or change the child pid.
- A stack MUST NOT multiplex several TerminalExecutions onto one terminal
  attach or assign a PTY to a non-terminal surface.

## 3. Close

- ⌘W resolves innermost-first as a per-client presentation close: surface →
  terminal pane → tab → Session presentation → Workspace presentation → window.
- Each step hides that client's presentation while preserving the same domain
  IDs and any TerminalPane-to-TerminalExecution binding. It MUST NOT infer
  Session or TerminalExecution termination or remove an object from the shared
  layout. A later remove-from-layout action requires its own Accepted domain
  transition. A destructive terminate action invokes SPEC-DOMAIN-LIFECYCLE §5
  with the exact affected identities and attached clients shown.
- Closing the window MUST NOT terminate any TerminalExecution. Packaged T-KILL
  remains binding.

## 4. Rearrange, zoom, templates

- Drag, split, unsplit, zoom and equalize MUST be reparent/resize operations.
- They MUST NOT change a durable `SessionId` or `TerminalExecutionId`, MUST NOT
  `posix_spawn` a shell, and MUST NOT recreate a visible execution.
- The only warm-path frame they may cause is the in-band `RESIZE` (FR-RESIZE).
- Layout templates serialize only declared template state. Reattaching a live
  Session resolves the existing IDs. Starting from a template is a separate
  action that spawns new TerminalExecutions and MUST say so. A template MUST
  NOT claim that it restored a live process or transcript.

## 5. View state

- Workspace UI, Session UI, sidebar and vertical-tab visibility are per-client
  view state. Toggling them MUST NOT create, delete, migrate, detach, terminate
  or resize domain objects. Explicit deep links still resolve hidden objects.

## 6. Identity

- The current default execution remains the legacy 8-byte ATTACH target only
  until the versioned ClientId/TerminalExecutionId protocol migration.
- Workspace and Session labels are cold runtime-owned properties of their own
  objects. A label MUST NOT address an execution on the attach path.
- The host indicator MUST read the execution's verified host identity cold. In M2 it MUST
  render `local`. Remote identity is [SPEC-REMOTE](SPEC-REMOTE.md) §4 and MUST
  be the verified identity, never a user-supplied label.

## 7. Readers (dashboards, pickers, palette, switchers)

- Every reader MUST be cold: no PTY, no master fd, no attach claim, no frame on
  an attached leaf's warm path.
- Sampling MUST NOT exceed **2 Hz** while visible and MUST be **0** while
  hidden.
- An `--nfr-key` run MUST report zero control-plane RPCs with readers present.
- Selecting a row MUST resolve to a `NodeId` and focus it. It MUST NOT spawn,
  terminate, resize, or send input.
- Actions that act MUST be explicit palette commands with their own
  confirmation, never a side effect of selection.
- The resource view MUST attribute to the kernel-owned child pid and MUST NOT
  walk the full process table on the sample path.
- The agent inventory MUST render empty until ADR 0048's `Task` exists. It MUST
  NOT show fabricated rows.

## 8. Focus history and reopen

- Focus history is a bounded host-side ring, capacity **64**.
- Entries whose node no longer exists MUST be skipped, not resurrected.
- Reopen of a still-live Session reattaches its existing identities. Restore
  from a template explicitly spawns new executions. Neither may fabricate a
  dead pid, missing transcript or evicted recovery data.

## 9. Global summon and deep links

- Summon MUST show/hide the window only. It MUST NOT spawn or create a
  workspace.
- A `rill://` URL is untrusted input. It MUST parse to an explicit action set and
  MUST require confirmation before anything that spawns, attaches to a host,
  opens SSH, or changes settings.
- A deep link MUST NOT run a command, write bytes to a leaf, modify trusted
  project config, or target an unapproved host.
- Unknown schemes, unknown verbs and malformed payloads MUST fail closed
  (PRD NFR-FAIL).

## 10. Nested tools, sidebars, transcripts

- RILL MUST NOT scrape, rewrite, or special-case a nested multiplexer's output,
  and MUST NOT refuse to spawn one.
- `RILL_INSIDE=1` refusal (SPEC-GRAPH §5) applies to nested `rilld` only.
- Custom sidebars MUST NOT be in-process user code. They are declarative or they
  are out-of-process plugins ([SPEC-TRUST](SPEC-TRUST.md) §3).
- The transcript vault MUST be a read-only index. It MUST NOT resume, replay, or
  start a provider. Redaction (SPEC-TRUST §4) applies to what it renders.

## 11. Gates

| ID | Status | Closes |
|---|---|---|
| T-NAV-TOPOLOGY | Red (kernel-plane half Proven, library) | §1 |
| T-NAV-STACK | Red | §2 |
| T-NAV-CLOSE | Red | §3 |
| T-NAV-REPARENT | Red (kernel-plane half Proven, library) | §4 |
| T-NAV-VIEWSTATE | Red | §5 |
| T-INV-COLD | Red | §7 |
| T-INV-SELECT | Red | §7 |
| T-INV-REOPEN | Red | §8 |
| T-INV-LINK | Red | §9 |
| T-INV-NEST | Red | §10 |

Socket-only tests do not close §2–§5 where user-visible; packaged `Rill.app`
e2e is the closer (ADR 0002 D8).

**Kernel-plane evidence (2026-08-18).** `crates/rill-kernel/tests/nav_gates.rs`
demonstrates three sub-properties red-then-green, `--test-threads=1`, real
`posix_spawn` children checked live with `kill(pid, 0)`:

| Test | Green | Required mutation | Demonstrated red |
|---|---|---|---|
| `t_nav_topology_snapshot_reflects_created_nodes_and_leaves` | `cargo test -p rill-kernel --test nav_gates` | `RILL_MUTATE=omit_node_children` | `cargo test -p rill-kernel --features mutate --test nav_gates` |
| `t_nav_close_terminates_only_the_closed_subtrees_leaves` | historical only; superseded by ADR 0053 D3 | `RILL_MUTATE=close_node_terminates_all_leaves` | same |
| `t_nav_reparent_preserves_session_identity` | same | `RILL_MUTATE=reparent_recreates_node` | same |

The three mutations were historically confirmed to turn **only** their own
test red, with the other two staying green (ADR 0002 D3's isolation
requirement) and the full pre-existing `rill-kernel` suite (23 Spike-0/M1 gates
and 2 unit tests) unaffected. ADR 0053 D3 supersedes the close test's product
oracle: presentation close no longer owns or terminates executions. That test
preserves decision history but is not evidence for the current T-NAV-CLOSE and
its behavior MUST NOT be wired into chrome. The topology and reparent tests
close only their data-structure prerequisites. None of the full gates has a
window yet: `host/macos/ChromeHost` does not call `create_node`, `reparent_node`
or `close_node`, and no packaged e2e has run. That wiring is [#345](https://github.com/mahboobmonnamd/RILL/issues/345)
(ADR 0056): cold `.nav` command in rilld plus host New Tab in the same issue.

## 12. Out of scope

The proven M2 library slice does not implement ContentTimeline, agents,
conversations, Chip 1 live, remote hosts, non-terminal surfaces or a second
window. Those items are governed by later specs. JSON or cells on the warm path
remain forbidden.

## 13. What we will not do

- Keep an authoritative topology in chrome.
- Stop draining a hidden pane to save memory.
- Terminate leaves from window teardown.
- Resurrect a pid on reopen.
- Run a command from a deep link.
