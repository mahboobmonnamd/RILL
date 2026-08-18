# SPEC-SURFACES — non-terminal and development panes (`lane:host`)

- **Status:** Accepted — 2026-08-18. Gates **Red** until demonstrated
  red-then-green (ADR 0002 D2).
- **Authority:** [ADR 0024](../adr/0024-non-terminal-panes-are-cold.md),
  [ADR 0028](../adr/0028-development-surfaces-are-panes.md)
- **Requires:** [SPEC-NAV](SPEC-NAV.md) (pane slots),
  [SPEC-CHROME](SPEC-CHROME.md), [SPEC-TRUST](SPEC-TRUST.md)
- **Crates:** `crates/rill-host`, `host/macos/`
- **Milestone:** M2 — Chrome

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. Cold rule

- No surface here MAY own a PTY, an attach claim, or the master fd.
- No surface MAY place work on the presenter's display-link callback.
- While a leaf in the same window is attached and the user is typing, a
  non-terminal surface MUST NOT invalidate, lay out, or draw in response.
- An `--nfr-key` run with the surface present MUST still meet the p95 budget, or
  the surface does not ship.

## 2. Embedded browser

- MUST run out of process. A page crash or hang MUST NOT hang the window, stall
  the display link, or kill a leaf.
- Page content is untrusted. It MUST NOT open a RILL-handled URL scheme without
  the deep-link confirmation (SPEC-NAV §9), read the clipboard, reach OSC 52,
  reach an unopened tunnel, or write to any leaf.
- MUST NOT be composited over the Metal surface.

## 3. File, markdown, diff

- Read-only in M2. The file explorer MUST NOT edit in place.
- Markdown "run fenced command" MUST show the command, MUST be explicitly
  invoked, and MUST NOT run on open.
- Renderers MUST run without network access.
- Diff rendering is read-only; hunk revert is §9.

## 4. Worktrees

- Create/open/remove MUST show the resolved path first.
- Remove MUST refuse a worktree with uncommitted changes unless the user
  confirms that specific loss.
- A worktree MUST NOT be created as a side effect of opening a workspace
  (agrees with [#266](https://github.com/mahboobmonnamd/RILL/issues/266)).

## 5. Hosted views

- Simulator and Dock panes MUST declare an owner process and MUST degrade to an
  explicit "not available" state.
- Neither may spawn a user shell (NFR-SPAWN) or claim a pane slot's leaf.

## 6. Keep-awake

- An assertion MUST be scoped to a named reason, visible while held, and
  released when the reason ends — including on crash, via a watchdog.

## 7. Workspace metadata

- Pin, colors, icons, status lanes, todos and env attach to the `Workspace` node
  and persist with it.
- `needs-attention` MUST be read from the attention queue
  ([SPEC-ATTENTION](SPEC-ATTENTION.md) §1). Manual override MUST be marked
  manual.
- Env feeds `spawn_leaf` before spawn ([SPEC-FIDELITY](SPEC-FIDELITY.md) §5). It
  MUST NOT be written into a running shell.
- Advertised ports MUST be observed from the child, and MUST NOT open a tunnel.
- Workspace todos are user-owned files, distinct from agent task lists
  ([SPEC-TASK](SPEC-TASK.md) §8).

## 8. Refused development surfaces

- This tree MUST NOT contain a text editor implementation, a language-server
  client, or a debug-adapter client.
- The supported editing story is an editor in a leaf, or an external IDE (§10).

## 9. Read-only code surfaces

- Find MUST NOT write. Project-wide replace is not implemented in M2.
- Hunk revert MUST go through git, MUST show the exact hunk, MUST refuse when
  the file changed since the diff was computed, and MUST NOT revert more than
  the named hunk. It MUST NOT run on hover or selection.

## 10. Open in external IDE

- Invocation MUST come from a declared per-editor entry in config, from an
  allowlist plus the user's own entry.
- Arguments MUST be passed as an argv vector. RILL MUST NOT build a shell
  command from a path.
- `path:line:col` parses to structured fields; line and column MUST be validated
  as integers.

## 11. Index, zero-state, notebooks, DevTools

- The index covers **git-tracked** files in the workspace only. It MUST NOT
  index ignored or untracked files or anything outside the root.
- Index content MUST NOT leave the machine.
- Indexing MUST be cold, interruptible, MUST NOT run during an `--nfr-key`
  measurement, and MUST NOT block a spawn or a paint.
- Clone/open MUST show the destination and confirm; clone MUST refuse a
  non-empty destination.
- Notebooks are user-owned files; running a fence follows §3 and trust
  (SPEC-TRUST §2).
- DevTools MUST run in the browser's content process and MUST NOT reach the
  host, config, or any leaf.

## 12. Review comments

- Inline comments MUST enter the task object ([SPEC-TASK](SPEC-TASK.md) §4) and
  MUST NOT be injected as keystrokes into an agent PTY.

## 13. Gates

| ID | Status | Closes |
|---|---|---|
| T-SURF-COLD | Red | §1 |
| T-SURF-BROWSER | Red | §2 |
| T-SURF-MD | Red | §3 |
| T-SURF-WORKTREE | Red | §4 |
| T-SURF-AWAKE | Red | §6 |
| T-SURF-META | Red | §7 |
| T-DEV-NOEDITOR | Red | §8 |
| T-DEV-READONLY | Red | §9 |
| T-DEV-REVERT | Red | §9 |
| T-DEV-OPEN | Red | §10 |
| T-DEV-INDEX | Red | §11 |
| T-DEV-NOTEBOOK | Red | §11 |

## 14. Out of scope

An editor, LSP, DAP, agents, Blocks, Chip 1 live, cloud indexing, a second
window, a mobile companion implementation.

## 15. What we will not do

- Composite a web view over Metal.
- Auto-run a fenced command on open.
- Create a worktree implicitly.
- Build an editor command as a shell string.
- Send indexed code off the machine.
