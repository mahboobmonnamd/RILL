# SPEC-INPUT — the Blocks input field (`lane:host`)

- **Status:** Accepted — 2026-08-18. Gates **Red** until demonstrated
  red-then-green (ADR 0002 D2).
- **Authority:** [ADR 0033](../adr/0033-input-editor-history-and-completion.md)
- **Requires:** [SPEC-BLOCKS](SPEC-BLOCKS.md), [SPEC-FIDELITY](SPEC-FIDELITY.md),
  [SPEC-CONFIG](SPEC-CONFIG.md), [SPEC-TRUST](SPEC-TRUST.md)
- **Milestone:** M6 — Blocks

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. Mode

- With Blocks off, keys go to the PTY and every feature here is inert. RILL MUST
  NOT intercept, echo, or buffer keys in raw mode.
- With Blocks on, the field composes a line and submits on Enter (PRD §6).
- The field MUST NOT reimplement shell semantics: no glob expansion, no `$VAR`
  resolution, no word splitting. What the user sees is what the shell receives,
  byte for byte.

## 2. No hidden rewriting

- Alias expansion and command corrections MUST be **offered**, not applied. The
  accepted text MUST be visible in the field before Enter.
- Quote/bracket autocomplete inserts visible characters, MUST be disableable,
  and MUST NOT insert a character the user cannot see or delete normally.
- Nothing MUST be substituted at submit time.

## 3. Caret and modal editing

- Click-to-place, ⌥/⌃/⌘ + arrow word and line motions, Home/End, ⌘A/⌘E and
  Ctrl+A/Ctrl+E follow macOS conventions and standard responder behaviour.
- All MUST be overridable through config bindings (SPEC-CONFIG §5).
- Multi-cursor and word ops MUST have working undo.
- Vim mode MUST be opt-in, default off, and MUST NOT alter behaviour when off.
- Ctrl+A resolves by mode: field start-of-line with Blocks on, child with Blocks
  off. SPEC-CONFIG §5's load-time report covers a binding that would swallow it.

## 4. History

- History records command, cwd, exit status, duration — per session, merged for
  search.
- It is a local user-owned file. It MUST NOT be uploaded, synced by default, or
  shared.
- Redaction applies; history is a persisting sink.
- A command the user's shell would not record (leading space under
  `HIST_IGNORE_SPACE`) MUST NOT be recorded here.

## 5. Recall and search

- Up/Down fills the field from history. It MUST NOT submit.
- Ctrl+R is incremental/fuzzy search over local history; selecting fills the
  field and MUST NOT submit. In raw mode Ctrl+R reaches the child (§1).
- Unified search spans history, workflows and saved prompts in one ranked list,
  and each result's **source MUST be visibly labelled** so the user knows
  whether Enter runs a command or sends a prompt.
- Search MUST be cold and incremental over a bounded index. It MUST NOT rescan
  the full history file per keystroke.

## 6. Completion and inspector

- Completion draws on history, the filesystem, and declared argument data.
- Completion MUST NOT run a program to discover completions — no speculative
  `--help`, no probing a binary.
- Completion work MUST be cancellable, off the key path, and deadlined. A slow
  completion is dropped, never allowed to delay a keystroke.
- The command inspector renders declared local documentation. It MUST NOT
  execute anything and MUST NOT fetch from the network.

## 7. Synchronized input

- Targeted panes MUST be visible the whole time it is active.
- It MUST be entered and left explicitly and MUST NOT persist across a focus
  change.
- It MUST NOT be the default for any pane set, and disabling MUST be reachable
  by keyboard (SPEC-TRUST §8).

## 8. Workflows

- Workflows are local user-owned files.
- Running one MUST show the fully resolved command, parameters substituted,
  before it executes.
- Parameters MUST be substituted as argv values, not concatenated into a shell
  string, unless the workflow explicitly requests shell evaluation.
- A workflow from a repository is untrusted until that path is trusted
  (SPEC-TRUST §2).

## 9. Position

- Pinning the input top or bottom MUST NOT change submission semantics, history,
  or focus behaviour.

## 10. Gates

| ID | Status | Closes |
|---|---|---|
| T-INPUT-RAW | Red | §1 |
| T-INPUT-WYSIWYG | Red | §2 |
| T-INPUT-HIST | Red | §4 |
| T-INPUT-RECALL | Red | §5 |
| T-INPUT-COMPLETE | Red | §6 |
| T-INPUT-SYNC | Red | §7 |
| T-INPUT-WORKFLOW | Red | §8 |

NFR-KEY MUST hold with the field active (SPEC-BLOCKS §3).

## 11. What we will not do

- Run a rich field in raw mode.
- Expand or correct at submit time.
- Execute anything to build a completion.
- Sync history to a service.
- Record what the shell was told to ignore.
- Make synchronized input sticky.
- Substitute workflow parameters into a shell string.
