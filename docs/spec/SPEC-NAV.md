# SPEC-NAV — container tree and navigation surfaces (`lane:host`, `lane:kernel`)

- **Status:** Accepted — 2026-08-18. `crates/rill-kernel` now implements §1's
  container tree (`NodeId`, `NodeKind`, `create_node`, `attach_leaf`,
  `reparent_node`, `close_node`, `container_snapshot`). Three kernel-plane
  sub-gates are **Proven at the library level** —
  `cargo test -p rill-kernel --test nav_gates`, red-then-green demonstrated
  under `--features mutate` (evidence below). The full gates in §11's table
  remain **Red**: each also names a chrome-renders-from-it or
  window-close-terminates-nothing half that needs `host/macos/` wiring and a
  packaged `Rill.app` e2e (ADR 0020 D7, ADR 0002 D8). A kernel-plane library
  test does not close a gate whose oracle names packaged behaviour — it only
  proves the layer under it is sound.
- **Authority:** [ADR 0020](../adr/0020-session-graph-navigation-model.md),
  [ADR 0021](../adr/0021-inventories-are-cold-readers.md)
- **Requires:** [SPEC-GRAPH](SPEC-GRAPH.md), [SPEC-KERNEL](SPEC-KERNEL.md),
  [SPEC-CHROME](SPEC-CHROME.md)
- **Crates:** `crates/rill-kernel`, `crates/rilld`, `crates/rill-host`,
  `host/macos/`
- **Milestone:** M2 — Chrome

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. Container tree

- The kernel MUST store container nodes above leaves: `Workspace`, `Group`,
  `Tab`, `Split`.
- `NodeId` is a kernel-allocated `u64`, disjoint from `SessionId`, and MUST NOT
  be reused while the node is live.
- `NodeId` MUST NOT be a path, a title, or a GUI index (same rule SPEC-GRAPH §1
  sets for `SessionId`).
- `layout_snapshot` MUST carry the container tree. It stays cold. It MUST NOT
  carry cells, scrollback, or per-cell `String`.
- Chrome MUST NOT hold authoritative topology. Chrome MAY cache for paint and
  MUST re-read on disagreement.
- A node chrome invented and the kernel does not have MUST NOT render.

## 2. Pane slots and surface stacks

- A pane slot holds an ordered list of surfaces and displays one.
- Hiding a surface MUST detach only the presenter.
- The kernel MUST keep the hidden leaf alive and MUST keep draining its PTY into
  its ring. Hidden panes MUST NOT stall the child (PRD NFR-DROP).
- Re-showing MUST resync once through the existing cold per-leaf path
  (FR-RESYNC). It MUST NOT respawn and MUST NOT change the pid.
- Each terminal surface is its own leaf with its own attach. A stack MUST NOT
  multiplex several leaves onto one attach (FR-ONE, ADR 0011 D3).

## 3. Close

- ⌘W MUST resolve innermost-first: surface → pane → tab → workspace → window.
- Closing a container MUST terminate its leaves by explicit
  `Kernel::terminate(id)`. `Drop` MUST NOT kill a child.
- Closing the **window** MUST NOT terminate any leaf. Packaged T-KILL is the
  closer and MUST stay green.

## 4. Rearrange, zoom, templates

- Drag, split, unsplit, zoom and equalize MUST be reparent/resize operations.
- They MUST NOT change a `SessionId`, MUST NOT `posix_spawn` a shell, and MUST
  NOT detach and re-attach a visible leaf.
- The only warm-path frame they may cause is the in-band `RESIZE` (FR-RESIZE).
- Layout templates MUST serialize the container tree, per-leaf cwd, and startup
  command. They MUST NOT serialize scrollback and MUST NOT claim to restore a
  live child. Restoring spawns new leaves and MUST say so.

## 5. View state

- Sidebar hide and vertical workspace tabs MUST NOT detach, terminate, resize a
  leaf, or reach the kernel.

## 6. Identity

- The default leaf remains the 8-byte ATTACH target (SPEC-GRAPH §2).
- A session name is a cold kernel-owned label on a `Workspace`. A name MUST NOT
  be used to address a leaf on the attach path.
- The host indicator MUST read the leaf's kernel identity cold. In M2 it MUST
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
- The agent inventory MUST render empty until ADR 0030's `Task` exists. It MUST
  NOT show fabricated rows.

## 8. Focus history and reopen

- Focus history is a bounded host-side ring, capacity **64**.
- Entries whose node no longer exists MUST be skipped, not resurrected.
- Reopen-closed MUST restore a layout template and spawn a new leaf. It MUST NOT
  claim the old pid and MUST NOT resurrect ring-dropped scrollback.

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
| T-NAV-CLOSE | Red (kernel-plane half Proven, library) | §3 |
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
| `t_nav_close_terminates_only_the_closed_subtrees_leaves` | same | `RILL_MUTATE=close_node_terminates_all_leaves` | same |
| `t_nav_reparent_preserves_session_identity` | same | `RILL_MUTATE=reparent_recreates_node` | same |

Each mutation was confirmed to turn **only** its own test red, with the other
two staying green (ADR 0002 D3's isolation requirement) and the full
pre-existing `rill-kernel` suite (23 Spike-0/M1 gates + 2 unit tests)
unaffected. This closes the data-structure prerequisite for T-NAV-TOPOLOGY,
T-NAV-CLOSE and T-NAV-REPARENT. It does **not** close those gates: none of
this has a window yet. `host/macos/ChromeHost` does not call `create_node`,
`reparent_node`, or `close_node`, and no packaged e2e has run. That wiring is
open work — see [#260](https://github.com/mahboobmonnamd/RILL/issues/260)'s
lane for the host side.

## 12. Out of scope

Blocks, agents, conversations, Chip 1 live, remote hosts, non-terminal surfaces,
a second window, a second VT, JSON or cells on the warm path.

## 13. What we will not do

- Keep an authoritative topology in chrome.
- Stop draining a hidden pane to save memory.
- Terminate leaves from window teardown.
- Resurrect a pid on reopen.
- Run a command from a deep link.
