# ADR 0021: Inventories, pickers and summon are cold readers

- **Status:** Accepted — 2026-08-18
- **Tree:** this repository only
- **Issue:** catalog epic [#33](https://github.com/mahboobmonnamd/RILL/issues/33).
  Rows authorized by this ADR:
  F-010 [#47](https://github.com/mahboobmonnamd/RILL/issues/47), F-011
  [#48](https://github.com/mahboobmonnamd/RILL/issues/48), F-012
  [#49](https://github.com/mahboobmonnamd/RILL/issues/49), F-013
  [#50](https://github.com/mahboobmonnamd/RILL/issues/50), F-014
  [#51](https://github.com/mahboobmonnamd/RILL/issues/51), F-015
  [#52](https://github.com/mahboobmonnamd/RILL/issues/52), F-016
  [#53](https://github.com/mahboobmonnamd/RILL/issues/53), F-021
  [#58](https://github.com/mahboobmonnamd/RILL/issues/58), F-022
  [#59](https://github.com/mahboobmonnamd/RILL/issues/59), F-025
  [#62](https://github.com/mahboobmonnamd/RILL/issues/62), F-026
  [#63](https://github.com/mahboobmonnamd/RILL/issues/63), F-027
  [#64](https://github.com/mahboobmonnamd/RILL/issues/64), F-028
  [#65](https://github.com/mahboobmonnamd/RILL/issues/65), F-029
  [#66](https://github.com/mahboobmonnamd/RILL/issues/66).
- **Requires:** [ADR 0011](0011-session-graph.md),
  [ADR 0013](0013-cwd-tap.md) (cold `Session::cwd()`),
  [ADR 0018](0018-three-pane-host-chrome.md),
  [ADR 0020](0020-session-graph-navigation-model.md) (container tree)
- **Amends:** nothing.
- **Does not authorize:** agents or an agent runtime (ADR 0030, ADR 0031),
  remote hosts (ADR 0023), plugins (ADR 0026), Blocks, Chip 1 live, a second
  window, JSON on the warm path, writing to a leaf from a picker.

## Context

Fourteen catalog rows are the same shape: something enumerates the graph and
jumps to a node. Workspace dashboard (F-010), agent dashboard (F-011),
session/process switcher (F-012), command palette (F-013), quick switcher
(F-014), focus history (F-015), reopen closed (F-016), global summon (F-021),
deep links (F-022), task/resource manager (F-025), goto picker (F-026), custom
sidebars (F-027), nested multiplexers (F-028), transcript vault (F-029).

ADR 0020 put the container tree in the kernel. These rows are its readers. The
risk they carry is not topology — it is **cost and authority**: a resource
monitor that samples every pane at 10 Hz, or a palette that can reach into a
leaf and write bytes, would put an orchestration surface on the path NFR-KEY
protects.

## Decision

### D1 — Readers are cold, sampled, and bounded

Every surface in this ADR reads `layout_snapshot` and per-leaf cold state.
None of them MAY:

- add a frame to an attached leaf's warm path,
- hold the PTY master fd, or write bytes to a leaf,
- poll faster than **2 Hz** while visible, or at all while hidden.

The 2 Hz ceiling is a decision, not a hint. A dashboard is a glance, not an
instrument. `--nfr-key` MUST run with every surface here disabled, and the
measured run MUST show zero control-plane RPCs (PRD NFR-KEY).

Mutation `dashboard_polls_hot` (raise to per-frame sampling) MUST turn
T-INV-COLD red.

### D2 — Selection navigates; it does not act

Selecting a row in any picker (F-010, F-012, F-013, F-014, F-026) MUST resolve
to a `NodeId` and focus it. It MUST NOT spawn, terminate, resize, or send input.

Actions that do act (kill a process, restart a pane) are palette **commands**
with their own confirmation, not a side effect of selection. A picker that
kills on Enter is rejected.

Named test `t_picker_selection_focuses_without_writing`. Mutation
`select_sends_input` MUST turn T-INV-SELECT red.

### D3 — Focus history and reopen are host view state with a bounded ring

Focus history (F-015) is a bounded ring of `NodeId` in the host, capacity 64.
Entries whose node no longer exists MUST be skipped, not resurrected.

Reopen closed (F-016) restores a **layout template** (ADR 0020 D4), not a live
child. It MUST spawn a new leaf and MUST NOT claim the old pid. Reopen after
`terminate` MUST NOT resurrect scrollback that the ring already dropped.

### D4 — The resource manager reads the OS, per leaf, and names its cost

Task/resource manager (F-025) reports CPU and RSS per leaf child pid, sampled at
D1's ceiling. It MUST attribute to the pid the kernel owns, not to a chrome
estimate. It MUST NOT walk the full process table on the sample path.

Agent rows (F-011) are empty in M2 and MUST render as empty, not hidden — there
is no agent runtime until ADR 0030. A dashboard that shows fabricated agent rows
before M3 is rejected.

### D5 — Global summon and deep links are the two untrusted entry points

Global summon (F-021) is a system hotkey that shows/hides the window. It MUST
NOT spawn, and MUST NOT create a workspace.

Deep links (F-022) arrive from outside the app and are **untrusted input**. A
`rill://` URL MUST be parsed into an explicit action set and MUST require
confirmation before anything that spawns, attaches to a host, opens SSH, or
changes settings. A deep link MUST NOT be able to:

- run a command or write bytes to any leaf,
- add or modify a trusted project config (that is ADR 0026 D2),
- silently target a host the user has not already approved.

Unknown schemes, unknown verbs, and malformed payloads MUST fail closed
(PRD NFR-FAIL). Named test `t_deep_link_requires_confirm_and_cannot_run`.
Mutation `deep_link_autoruns` MUST turn T-INV-LINK red.

### D6 — Nested multiplexers are supported by not interfering

F-028: users may run `tmux`, `zellij`, `vim`, or another TUI inside a leaf. That
is already true and MUST stay true. RILL MUST NOT scrape, rewrite, or
special-case a nested multiplexer's output, and MUST NOT refuse to spawn one.

`RILL_INSIDE=1` nesting refusal (SPEC-GRAPH §5) applies to a nested **`rilld`**,
not to tmux. Adopting an existing tmux as native panes is F-179 and is refused
here — see ADR 0023 D6.

### D7 — Custom sidebars and the transcript vault are file readers, out of process

Custom interpreted sidebars (F-027) MUST NOT be in-process user code. They are
declarative, or they are a plugin under ADR 0026 D3's out-of-process boundary.
This ADR does not authorize an embedded interpreter.

The transcript vault (F-029) is a **read-only** index of provider session files
already on disk. It MUST NOT resume, replay, or start a provider. Opening an
entry hands the file to an agent surface once one exists (ADR 0030). Secret
redaction (ADR 0026 D4) applies to anything it renders.

### D8 — Oracle

| ID | Closes |
|---|---|
| T-INV-COLD | D1 — sample ceiling; zero RPCs on an `--nfr-key` run |
| T-INV-SELECT | D2 — selection focuses, never writes |
| T-INV-REOPEN | D3 — reopen spawns; no resurrected pid |
| T-INV-LINK | D5 — deep link confirms; cannot run |
| T-INV-NEST | D6 — `tmux` inside a leaf still works |

## Consequences

- [SPEC-NAV](../spec/SPEC-NAV.md) §5–§8 carry the reader contract.
- ADR 0013's cold `Session::cwd()` is the only cwd source for these surfaces.
- F-011 stays an empty inventory until ADR 0030 lands the `Task` object.

## Rejected alternatives

- **Live per-frame resource graphs.** Rejected: D1. The product is a terminal
  whose typing path is in-process; a monitor that costs the frame is the wrong
  trade.
- **Deep links that run a command.** Rejected: remote-code-execution by URL.
  Confirmation is not a dialog we can skip for convenience.
- **Palette selection that kills the process it found.** Rejected: D2.
- **An in-process scripting runtime for sidebars.** Rejected: ADR 0026 D3
  isolation; a crash in a user sidebar must not take the window.
- **Scraping tmux to present it as native panes.** Rejected here and in
  ADR 0023 D6: a mirrored tmux is not a leaf the kernel owns.
