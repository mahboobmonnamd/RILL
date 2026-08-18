# ADR 0024: Non-terminal panes are cold surfaces

- **Status:** Accepted — 2026-08-18
- **Tree:** this repository only
- **Issue:** catalog epic [#33](https://github.com/mahboobmonnamd/RILL/issues/33).
  Rows authorized by this ADR:
  F-190 [#205](https://github.com/mahboobmonnamd/RILL/issues/205), F-191
  [#206](https://github.com/mahboobmonnamd/RILL/issues/206), F-192
  [#207](https://github.com/mahboobmonnamd/RILL/issues/207), F-193
  [#208](https://github.com/mahboobmonnamd/RILL/issues/208), F-194
  [#209](https://github.com/mahboobmonnamd/RILL/issues/209), F-195
  [#210](https://github.com/mahboobmonnamd/RILL/issues/210), F-196
  [#211](https://github.com/mahboobmonnamd/RILL/issues/211), F-197
  [#212](https://github.com/mahboobmonnamd/RILL/issues/212), F-198
  [#213](https://github.com/mahboobmonnamd/RILL/issues/213), F-199
  [#214](https://github.com/mahboobmonnamd/RILL/issues/214), F-201
  [#216](https://github.com/mahboobmonnamd/RILL/issues/216), F-202
  [#217](https://github.com/mahboobmonnamd/RILL/issues/217), F-203
  [#218](https://github.com/mahboobmonnamd/RILL/issues/218), F-204
  [#219](https://github.com/mahboobmonnamd/RILL/issues/219).
- **Requires:** [ADR 0009](0009-direct-to-display-echo.md),
  [ADR 0011](0011-session-graph.md),
  [ADR 0018](0018-three-pane-host-chrome.md),
  [ADR 0020](0020-session-graph-navigation-model.md) (pane slots, D2 stacks)
- **Amends:** nothing.
- **Does not authorize:** an editor or LSP (ADR 0028), agents, Blocks, Chip 1
  live, a second window, a web view over Metal, a surface that owns a PTY
  outside the kernel, cloud sync.

## Context

Fourteen rows put something other than a terminal in a pane: embedded browser
(F-190), file explorer (F-191), markdown viewer (F-192), diff viewer (F-193),
built-in editor (F-194, deferred to ADR 0028), git worktree UI (F-195), mobile
companion (F-196), simulator panes (F-197), Dock panes (F-198), keep-awake
(F-199), plus workspace metadata: pin/colors/icons (F-201), status lanes
(F-202), todos (F-203), env and ports (F-204).

ADR 0020 D2 already gave a pane slot a stack of surfaces. This ADR decides what
a non-terminal surface is *allowed to be*, because the risk is concentrated and
specific: a `WKWebView` or a `NSTextView` sharing a window with an `MTKView`
competes for the same main thread and the same display link that ADR 0008 and
ADR 0009 spent Spike 0 protecting. A browser pane that costs the terminal its
frame budget is a regression of the only NFR that matters.

## Decision

### D1 — A non-terminal surface never touches the warm path

Every surface here MUST be cold: no PTY, no attach claim, no master fd, no
frames on an attached leaf's path, no work on the presenter's display-link
callback.

They MUST render on their own schedule. When a leaf in the same window is
attached and the user is typing, a non-terminal surface MUST NOT invalidate,
lay out, or draw in response to that typing.

Mutation `surface_draws_on_display_link` MUST turn T-SURF-COLD red, and an
`--nfr-key` run with a non-terminal surface present MUST still meet the p95
budget or the surface does not ship. That is the gate, not a benchmark
afterwards.

### D2 — The embedded browser is a separate process and is untrusted

F-190. The browser pane MUST run out of process (`WKWebView` with its own
content process, or equivalent). A crash or a hang in a page MUST NOT hang the
window, MUST NOT stall the display link, and MUST NOT kill a leaf.

Page content is **untrusted data**. It MUST NOT be able to:

- open a URL scheme that RILL handles (deep links, ADR 0021 D5) without the same
  confirmation a foreign deep link gets,
- read the clipboard, or reach OSC 52 (ADR 0022 D3),
- reach a remote host tunnel that the user has not explicitly opened
  (ADR 0023 D7),
- write to any leaf.

It MUST NOT be composited over the Metal surface. It occupies its own pane slot.
`NSVisualEffectView` over Metal was already rejected in ADR 0018 for the same
class of reason.

### D3 — File, markdown and diff surfaces are read-only in this milestone

F-191, F-192, F-193 render files from disk. In M2 they are **read-only**:

- The file explorer opens a file into a viewer or hands the path to an external
  editor (F-245, ADR 0028 D4). It MUST NOT edit in place.
- The markdown viewer renders. "Run fenced command" is a **confirmed** action
  that sends the command to a named leaf, with the command visible before it
  runs, and never automatically on open. A markdown file is untrusted input.
- The diff viewer reads `git` output. Hunk revert (F-243) is ADR 0028 D3, and is
  not authorized here.

Mermaid and any other renderer MUST run without network access.

Mutation `markdown_autoruns_fence` MUST turn T-SURF-MD red.

### D4 — Git worktree UI creates real worktrees and never silently

F-195. Create/open/remove map to `git worktree` invocations with the resolved
path shown first. Remove MUST refuse when the worktree has uncommitted changes
unless the user confirms that specific loss.

A worktree MUST NOT be created as a side effect of opening a workspace. This is
the same rule [#266](https://github.com/mahboobmonnamd/RILL/issues/266) already
states for workspace grouping by folder path, and the two must agree.

### D5 — Simulator and Dock panes are hosted views with an owner

F-197 (iOS Simulator) and F-198 (Dock panes) embed something the OS owns. Each
MUST declare an owner process and MUST degrade to an explicit "not available"
state rather than an empty rectangle. Neither may spawn a user shell (NFR-SPAWN
stands), and neither may claim a pane slot's leaf.

### D6 — Keep-awake is scoped, visible, and released

F-199. A keep-awake assertion MUST be scoped to a named reason (a running
command, an agent task), MUST be visible in the UI while held, and MUST be
released when that reason ends — including on crash, via a watchdog. An
assertion that outlives its reason drains a battery silently; NFR-KEY is
measured on battery, and so is the user's day.

### D7 — Workspace metadata is kernel state, not chrome decoration

F-201 (pin, colors, icons), F-202 (status lanes), F-203 (todos), F-204 (env and
ports) attach to the `Workspace` node from ADR 0020 D1 and persist with it.

- Status lanes (`todo` / `working` / `needs-attention` / `done`) MUST be derived
  where a derivation exists — `needs-attention` is the attention queue's
  (ADR 0029 D1), not a second classifier. Manual override is allowed and MUST be
  marked as manual.
- Env (F-204) feeds `spawn_leaf` **before** spawn (ADR 0022 D5). It MUST NOT be
  written into a running shell.
- Advertised ports are **observed** from the child process, not declared by the
  user and assumed. An advertised port MUST NOT open a tunnel (ADR 0023 D7).
- Todos (F-203) are local files in the workspace, user-owned. Not a cloud
  object, not an agent's task list (that is ADR 0030 D5).

### D8 — Mobile companion is out of scope for this tree in M2

F-196 describes a phone as a full workspace. Attaching from a phone is
ADR 0023's protocol over a transport, and needs an identity story this tree does
not have. This ADR authorizes **only** the design constraint: any future
companion MUST speak the same attach protocol (ADR 0023 D1) and MUST NOT require
an account or a relay for local-network use.

No macOS-side implementation is authorized by this ADR. The row stays open
against a later ADR rather than pretending M2 delivers it.

### D9 — Oracle

| ID | Closes |
|---|---|
| T-SURF-COLD | D1 — no surface work on the key path; `--nfr-key` holds with a surface present |
| T-SURF-BROWSER | D2 — out of process; crash does not take the window; no clipboard, no leaf write |
| T-SURF-MD | D3 — no auto-run of a fenced command |
| T-SURF-WORKTREE | D4 — no silent create; dirty remove refuses |
| T-SURF-AWAKE | D6 — assertion released when the reason ends |
| T-SURF-META | D7 — env before spawn; `needs-attention` derived from the queue |

## Consequences

- [SPEC-SURFACES](../spec/SPEC-SURFACES.md) carries §1–§7 for these panes and
  §8–§11 for ADR 0028's development surfaces.
- ADR 0020's pane stack is the only place a surface lives; there is no floating
  surface and no second window.
- F-196 remains unimplemented in M2 by decision, with its constraint recorded.

## Rejected alternatives

- **In-process web view composited over the terminal.** Rejected: D1, D2, and
  ADR 0018's rejection of `NSVisualEffectView` over Metal.
- **Editable file explorer in M2.** Rejected: D3. Editing is ADR 0028, and it
  needs the editor decision first.
- **Auto-running fenced commands in markdown.** Rejected: D3. Opening a file is
  not consent to execute it.
- **Creating a git worktree implicitly when a workspace opens.** Rejected: D4.
- **A global always-on keep-awake toggle.** Rejected: D6, unscoped assertions.
- **Deriving `needs-attention` in chrome, separately from the queue.** Rejected:
  D7 — two classifiers disagree, and the user believes the wrong one.
