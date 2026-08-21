# SPEC-INPUT — structured editor and raw terminal routing (`lane:host`)

- **Status:** Accepted — 2026-08-18. Gates **Red** until demonstrated
  red-then-green (ADR 0002 D2).
- **Authority:** [ADR 0051](../adr/0051-input-editor-history-and-completion.md),
  amended by
  [ADR 0053](../adr/0053-runtime-domain-content-and-client-authority.md) D7–D10
- **Requires:** [SPEC-BLOCKS](SPEC-BLOCKS.md), [SPEC-FIDELITY](SPEC-FIDELITY.md),
  [SPEC-CONFIG](SPEC-CONFIG.md), [SPEC-TRUST](SPEC-TRUST.md),
  [SPEC-CONTENT](SPEC-CONTENT.md), [SPEC-COMPOSITOR](SPEC-COMPOSITOR.md),
  [SPEC-CLIENT-AUTHORITY](SPEC-CLIENT-AUTHORITY.md)
- **Milestone:** after content, compositor and lease foundations; historical M6
  numbering does not override ADR 0053 D12.

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 0. Authority and modes

Input modes are native command composer, shell line editor, raw terminal,
alternate-screen/raw-mode application, agent prompt and structured approval.
Host terminal modes, Task/request lifecycle and the current input lease decide
the legal target. Focus is necessary presentation state but never grants write
authority. Mode transitions are ordered and define keyboard, mouse, focus,
paste and IME ownership; an uncertain transition falls back to direct raw
terminal input without submitting buffered composer text.

Composer drafts are sensitive client-local state and non-durable by default.
They are not synchronized, backed up or restored on another client. A future
durable draft requires an explicit retention, encryption and cross-client
authorization contract.

## 1. Mode

- In raw terminal/TUI mode, keys go through the current input lease to the PTY
  and every structured-editor feature is inert. RILL MUST NOT intercept, echo,
  rewrite or buffer keys in raw mode.
- In structured mode, `rill-editor` composes content and emits an explicit
  submission into ContentTimeline, Conversation/Task routing, or the leased
  PTY according to the visible selected action. No classifier chooses.
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

- History records command, cwd, exit status, duration — per Session, merged for
  search.
- Durable history is local and policy-controlled and MAY be disabled entirely.
  It MUST NOT be uploaded, synced by default, or shared.
- Capture obeys SPEC-CONTENT. Redaction applies to derived export/transmission
  sinks and is not authority to collect.
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
- The controlling ClientId MUST hold a valid input lease for every target
  TerminalExecution. Failure to obtain one target lease aborts the whole send;
  partial synchronized input is forbidden.

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
- Persist or synchronize an unsent composer draft implicitly.
- Route input from focus alone without the authoritative mode and lease.
- Expand or correct at submit time.
- Execute anything to build a completion.
- Sync history to a service.
- Record what the shell was told to ignore.
- Make synchronized input sticky.
- Substitute workflow parameters into a shell string.
