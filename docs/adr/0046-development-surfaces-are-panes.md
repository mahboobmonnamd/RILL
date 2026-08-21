# ADR 0046: Development surfaces are panes, not a second editor plane

- **Status:** Accepted — 2026-08-18
- **Historical identifier:** merged as ADR 0028 in PR #278; renumbered to ADR
  0046 on 2026-08-21 with its series. Renumbering changed no decision.
- **Tree:** this repository only
- **Issue:** catalog epic [#33](https://github.com/mahboobmonnamd/RILL/issues/33).
  Rows authorized by this ADR:
  F-240 [#243](https://github.com/mahboobmonnamd/RILL/issues/243), F-241
  [#244](https://github.com/mahboobmonnamd/RILL/issues/244), F-242
  [#245](https://github.com/mahboobmonnamd/RILL/issues/245), F-243
  [#246](https://github.com/mahboobmonnamd/RILL/issues/246), F-244
  [#247](https://github.com/mahboobmonnamd/RILL/issues/247), F-245
  [#248](https://github.com/mahboobmonnamd/RILL/issues/248), F-246
  [#249](https://github.com/mahboobmonnamd/RILL/issues/249), F-247
  [#250](https://github.com/mahboobmonnamd/RILL/issues/250), F-248
  [#251](https://github.com/mahboobmonnamd/RILL/issues/251), F-249
  [#252](https://github.com/mahboobmonnamd/RILL/issues/252), F-250
  [#253](https://github.com/mahboobmonnamd/RILL/issues/253).
- **Requires:** [ADR 0038](0038-session-graph-navigation-model.md) (pane slots),
  [ADR 0042](0042-non-terminal-panes-are-cold.md) (cold surfaces, browser out of
  process), [ADR 0044](0044-trust-secrets-and-automation-boundary.md) (trust,
  redaction)
- **Amends:** nothing.
- **Does not authorize:** authoring a text editor in this tree, an LSP client, a
  debug adapter client, agents (ADR 0048, ADR 0049), cloud indexing, sending
  code off the machine, an editor on the warm path.

## Context

Eleven rows are the "Warp Code / cmux browser" cluster: native code editor
(F-240), language servers (F-241), find and replace (F-242), code review panel
(F-243), interactive review comments (F-244), open in external IDE (F-245),
zero-state open/clone (F-246), local codebase index (F-247), local notebooks
(F-248), browser DevTools (F-249), debugger/DAP (F-250).

Two of them already carry the owner's own instruction in the catalog: F-240 and
F-241 say **"Do not author."** That is the right call and this ADR makes it a
decision rather than a note, because the pull is strong and constant: once a
diff viewer exists, an editable diff is one commit away, and then an editor is
one more.

RILL's promise (PRD §1) is a terminal whose session survives the window and
whose typing path is in-process. An editor is not that product, and `nvim` and
`hx` already run correctly in a leaf (F-102, ADR 0040 D4).

## Decision

### D1 — We do not author an editor, an LSP client, or a debugger

F-240, F-241, F-250. This tree MUST NOT contain a text editor implementation, a
language-server client, or a debug-adapter client.

The supported editing story is: run your editor in a leaf, or open the file in
your own IDE (D4). Both already work; neither costs this tree a second plane.

This is a closing decision. F-240, F-241 and F-250 are resolved as **wontfix**
in this tree rather than held open indefinitely — an issue that will never be
implemented should say so, not sit blocked forever. Reopening requires a new
ADR that supersedes this one.

### D2 — Read-only code surfaces are allowed, and stay read-only

Find (F-242) and the review panel's rendering (F-243) are read-only readers of
the working tree, on ADR 0042 D1's cold terms.

- **Find** (F-242) searches files and scrollback. Regex and smart-case are fine.
  **Replace** is refused in M2: a replace across a working tree from a terminal,
  with no editor and no undo model, is a data-loss feature. F-242 ships as find
  only; the row's replace half is deferred to a later ADR that must first name
  an undo model.
- Rendering diffs, blame, and file content is allowed. Writing files is not,
  except D3's explicit revert.

### D3 — Hunk revert is the one write, and it is guarded

F-243's hunk revert writes to the working tree. It is allowed because git gives
it an undo model, and it MUST be implemented through git, not by patching bytes.

It MUST: show the exact hunk, refuse when the file has changed since the diff
was computed, and never revert more than the named hunk. It MUST NOT run on
selection or on hover.

Mutation `revert_without_staleness_check` MUST turn T-DEV-REVERT red.

### D4 — Open-in-external-IDE resolves an editor, never guesses a command

F-245. Opening `path:line:col` in Cursor, VS Code, Zed, JetBrains or `$EDITOR`
MUST use a declared per-editor invocation from config (ADR 0043 D1), resolved
from an allowlist of known editors plus the user's own entry.

RILL MUST NOT build a shell command from a path and hand it to a shell — that is
an injection surface on filenames the user does not control. Arguments are
passed as an argv vector. Path with line/column (F-056) shares this resolver and
is decided in ADR 0052 D2.

Mutation `open_editor_via_shell_string` MUST turn T-DEV-OPEN red.

### D5 — The codebase index is local, git-scoped, and never leaves the machine

F-247. The index covers **git-tracked** files in the workspace. It MUST NOT
index ignored files, untracked files, or anything outside the workspace root.

It MUST stay on the user's machine (the row says so and this makes it binding):
no upload, no remote embedding service, no telemetry of content. Redaction
(ADR 0044 D4) applies to anything the index hands to another surface.

Indexing MUST be cold and interruptible, MUST NOT run during an `--nfr-key`
measurement, and MUST NOT block a spawn or a paint. A stale index is acceptable;
a slow terminal is not.

Mutation `index_untracked_files` MUST turn T-DEV-INDEX red.

### D6 — Zero-state and notebooks are user-owned files with confirmed actions

- **Zero-state open / clone (F-246):** creating a project, opening a repo, or
  cloning MUST show the resolved destination path and MUST confirm before
  writing. Clone MUST refuse a non-empty destination. A remote URL is untrusted
  input (ADR 0044 D1).
- **Local notebooks (F-248):** runnable markdown is a **file the user owns**,
  not a hosted document and not an account feature (ADR 0044 D5). Running a
  fenced command follows ADR 0042 D3: visible command, explicit invocation,
  never on open. A notebook from a repository is untrusted until that path is
  trusted (ADR 0044 D2).

### D7 — Review comments steer an agent only through the agent's own contract

F-244. Inline comments that steer a running agent are an **input** to the task
object (ADR 0048 D2), not a private channel into a CLI agent's PTY.

A comment MUST NOT be injected as keystrokes into an agent's terminal by this
surface. Agent input goes through ADR 0049's adapter, with its permission
profile (ADR 0049 D5) applying. Nothing here ships before M3.

### D8 — DevTools are the browser's, in the browser's process

F-249. Inspect, device preview and design tools belong to the embedded browser
pane and MUST run in its out-of-process content process (ADR 0042 D2). They MUST
NOT gain access to RILL's host, config, or any leaf.

### D9 — Oracle

| ID | Closes |
|---|---|
| T-DEV-NOEDITOR | D1 — no editor/LSP/DAP client in the dependency graph |
| T-DEV-READONLY | D2 — find does not write; replace absent |
| T-DEV-REVERT | D3 — staleness refuses; only the named hunk changes |
| T-DEV-OPEN | D4 — argv vector, no shell string |
| T-DEV-INDEX | D5 — tracked files only; nothing leaves the machine |
| T-DEV-NOTEBOOK | D6 — no auto-run; untrusted path inert |

## Consequences

- [SPEC-SURFACES](../spec/SPEC-SURFACES.md) §8–§11 carry these panes.
- F-240, F-241, F-250 close as `wontfix` in this tree.
- F-242 ships find; replace is deferred pending an undo model.
- F-244 ships nothing until M3 and ADR 0049.

## Rejected alternatives

- **Embed an existing editor component.** Rejected: D1. It brings an LSP client,
  a settings surface, and a second input model with it.
- **Editable diff viewer.** Rejected: D2, D3. Git is the undo model; freehand
  editing has none.
- **Project-wide find and replace in M2.** Rejected: D2, data loss.
- **Build the editor command as a shell string.** Rejected: D4, injection.
- **Cloud/remote codebase indexing for better context.** Rejected: D5 and
  ADR 0044 D5. The row's own promise is that it stays local.
- **Inject review comments as keystrokes into an agent PTY.** Rejected: D7 — it
  routes around the permission profile.
