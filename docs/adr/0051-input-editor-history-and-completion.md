# ADR 0051: Input editor, history and completion

- **Status:** Accepted — 2026-08-18
- **Amended by:** [ADR 0053](0053-runtime-domain-content-and-client-authority.md)
  D7 and D9. Structured editor content joins ContentTimeline; raw TUI input
  continues to bypass the editor.
- **Historical identifier:** merged as ADR 0033 in PR #278; renumbered to ADR
  0051 on 2026-08-21 with its series. Renumbering changed no decision.
- **Tree:** this repository only
- **Issue:** catalog epic [#33](https://github.com/mahboobmonnamd/RILL/issues/33).
  Rows authorized by this ADR:
  F-051 [#86](https://github.com/mahboobmonnamd/RILL/issues/86), F-052
  [#87](https://github.com/mahboobmonnamd/RILL/issues/87), F-058
  [#93](https://github.com/mahboobmonnamd/RILL/issues/93), F-059
  [#94](https://github.com/mahboobmonnamd/RILL/issues/94), F-060
  [#95](https://github.com/mahboobmonnamd/RILL/issues/95), F-061
  [#96](https://github.com/mahboobmonnamd/RILL/issues/96), F-062
  [#97](https://github.com/mahboobmonnamd/RILL/issues/97), F-063
  [#98](https://github.com/mahboobmonnamd/RILL/issues/98), F-064
  [#99](https://github.com/mahboobmonnamd/RILL/issues/99), F-065
  [#100](https://github.com/mahboobmonnamd/RILL/issues/100), F-066
  [#101](https://github.com/mahboobmonnamd/RILL/issues/101), F-067
  [#102](https://github.com/mahboobmonnamd/RILL/issues/102), F-068
  [#103](https://github.com/mahboobmonnamd/RILL/issues/103), F-252
  [#125](https://github.com/mahboobmonnamd/RILL/issues/125), F-253
  [#126](https://github.com/mahboobmonnamd/RILL/issues/126), F-254
  [#127](https://github.com/mahboobmonnamd/RILL/issues/127).
- **Requires:** [ADR 0009](0009-direct-to-display-echo.md),
  [ADR 0040](0040-terminal-fidelity-is-chip0.md) D3 (input arbitration, IME),
  [ADR 0043](0043-one-look-schema-one-config-file.md) D5 (keybindings),
  [ADR 0044](0044-trust-secrets-and-automation-boundary.md) D4 (redaction),
  [ADR 0050](0050-blocks-are-a-cold-overlay.md) (Blocks overlay, raw mode)
- **Amends:** nothing.
- **Does not authorize:** an editor or LSP (ADR 0046 D1), a natural-language
  classifier (ADR 0049 D9), sending history off the machine, an account, input
  work on the presenter's display-link callback, replacing the shell's own
  line editor in raw mode.
- **Milestone:** M6 — Blocks

## Context

Sixteen rows: click-to-place caret (F-051), multi-cursor and word ops (F-052),
alias expansion (F-058), command inspector (F-059), autosuggest/tab complete
(F-060), quote/bracket autocomplete (F-061), input Vim mode (F-062), command
history (F-063), unified command search (F-064), command corrections (F-065),
synchronized inputs (F-066), workflows (F-067), pin input top/bottom (F-068),
Up/Down recall (F-252), Ctrl+R search (F-253), natural caret navigation (F-254).

These describe a rich input field that exists **only when Blocks are on**. With
Blocks off (ADR 0050 D4), the shell's own line editor is the input, and RILL
MUST NOT compete with it — zsh's ZLE and fish's editor are better at their job
than anything in this tree, and users have configured them.

So this ADR is about a field that sits in front of the PTY, and the two rules
that keep it honest: it must not slow the key path, and it must not change what
the user meant before the PTY sees it.

## Decision

### 2026-08-21 amendment — explicit input modes and draft ownership

ADR 0053 D20 replaces the binary Blocks-on/off wording with explicit native
composer, shell line-editor, raw terminal, alternate-screen/raw-mode TUI,
agent-prompt and structured-approval modes. Authoritative terminal modes,
Task/request state and the current input lease decide routing. Missing shell
integration falls back to direct native shell input.

An unsent composer draft is sensitive client-local state and is non-durable by
default. It is not synchronized, backed up or inherited by another client.
Durable/cross-device drafts require a later policy and threat-model decision.
TUI ownership suppresses the composer; mode exit restores focus without
creating another pane, PTY, execution or Session.

### D1 — The input field exists only in Blocks mode, and never duplicates the shell

With Blocks off, keys go to the PTY (ADR 0050 D4) and every feature in this ADR
is inert. RILL MUST NOT intercept, echo, or buffer keys in raw mode.

With Blocks on, the field composes a line and submits it. Submission is Enter →
PTY (PRD §6). The field MUST NOT reimplement shell semantics — it does not
expand globs, resolve `$VAR`, or split words. What the user sees is what the
shell receives, byte for byte.

Mutation `field_active_in_raw_mode` MUST turn T-INPUT-RAW red.

### D2 — Editing never rewrites the submitted bytes without the user seeing it

F-058, F-061, F-065.

- **Alias expansion (F-058)** and **corrections (F-065)** MUST be *offered*, not
  applied. The user accepts an expansion or a correction explicitly, and the
  expanded text is then visible in the field before Enter.
- **Quote/bracket autocomplete (F-061)** inserts visible characters and MUST be
  disableable. It MUST NOT insert a closing character the user cannot see or
  delete normally.
- Nothing MUST be substituted at submit time. A field that rewrites on Enter
  makes the user's screen a lie about what ran.

Mutation `expand_alias_at_submit` MUST turn T-INPUT-WYSIWYG red.

### D3 — Caret, selection and modal editing are the platform's conventions

F-051, F-052, F-054 (with ADR 0052 D1), F-254.

Click-to-place, ⌥/⌃/⌘ + arrow word and line motions, Home/End, ⌘A/⌘E, and
Ctrl+A/Ctrl+E in the field follow macOS conventions and the standard responder
behaviour. They MUST be overridable through config bindings (ADR 0043 D5).

Multi-cursor and word ops (F-052) MUST have a working undo. Vim mode (F-062) is
opt-in, default off, and MUST NOT alter the field's behaviour when off.

Ctrl+A is the known collision: in the field it is start-of-line; in raw mode it
reaches the child (tmux prefix). D1 already resolves it — mode decides, and
ADR 0043 D5's load-time report catches a binding that would swallow it.

### D4 — History is local, per session then merged, and redacted at the sink

F-063. History records command, cwd, exit status and duration, per session,
merged for search. It is a **local file** the user owns (ADR 0044 D5). It MUST
NOT be uploaded, synced by default, or shared.

Redaction (ADR 0044 D4) applies: history is a persisting sink. A command line
containing a secret MUST NOT be written in the clear.

History MUST honour the shell's own privacy convention — a command the user's
shell would not record (leading space under `HIST_IGNORE_SPACE`) MUST NOT be
recorded here either. Silently keeping what the shell deliberately dropped is a
betrayal of an expectation users already rely on.

Mutation `record_space_prefixed_command` MUST turn T-INPUT-HIST red.

### D5 — Recall and search read history; they never execute

F-252, F-253, F-064.

- **Up/Down (F-252)** fills the field from history. It MUST NOT submit.
- **Ctrl+R (F-253)** is incremental/fuzzy search over local history. Selecting a
  result fills the field. It MUST NOT submit. Ctrl+R in raw mode still reaches
  the child (D1).
- **Unified search (F-064)** extends Ctrl+R across history, workflows and saved
  prompts in one ranked list, with each result's **source visibly labelled** —
  the user must know whether Enter will run a command or send a prompt.

Search MUST be cold and incremental over a bounded index. It MUST NOT rescan the
full history file per keystroke.

Mutation `recall_autosubmits` MUST turn T-INPUT-RECALL red.

### D6 — Completion and the inspector are cold, cancellable, and never execute to learn

F-059, F-060.

Autosuggest and tab completion draw on history, the filesystem, and declared
argument data. Completion MUST NOT run a program to discover completions — no
speculative `--help`, no probing a binary. Running a command the user did not
ask for, to make a menu, is unacceptable.

Completion work MUST be cancellable, MUST be off the key path, and MUST have a
deadline: a slow completion is dropped, never allowed to delay a keystroke.

The command inspector (F-059) renders declared documentation from local data. It
MUST NOT execute anything and MUST NOT fetch from the network.

Mutation `completion_execs_help` MUST turn T-INPUT-COMPLETE red.

### D7 — Synchronized input is explicit, visibly scoped, and confirms destructive sends

F-066. Typing into N panes at once MUST show which panes are targeted the whole
time it is active, MUST be entered and left explicitly, and MUST NOT persist
silently across a focus change.

Sending to more than one pane is amplification: it MUST NOT be the default for
any pane set, and disabling it MUST be reachable by keyboard (ADR 0044 D8).

### D8 — Workflows are local, parameterized, and reviewed before they run

F-067. Workflows are local YAML/notebook files the user owns. Running one MUST
show the fully resolved command — parameters substituted — before it executes.

Parameters MUST be substituted as **argv values**, not concatenated into a shell
string, wherever the workflow does not explicitly request shell evaluation. A
workflow from a repository is untrusted until that path is trusted
(ADR 0044 D2).

Mutation `workflow_runs_before_preview` MUST turn T-INPUT-WORKFLOW red.

### D9 — Input position is view state

F-068. Pinning the input top or bottom MUST NOT change submission semantics,
history, or focus behaviour. It is layout.

### D10 — Oracle

| ID | Closes |
|---|---|
| T-INPUT-RAW | D1 — field inert in raw mode; no interception |
| T-INPUT-WYSIWYG | D2 — no submit-time rewriting |
| T-INPUT-HIST | D4 — local, redacted, honours ignore-space |
| T-INPUT-RECALL | D5 — recall and search fill, never submit |
| T-INPUT-COMPLETE | D6 — no exec for completion; deadline honoured |
| T-INPUT-SYNC | D7 — targets visible; explicit exit |
| T-INPUT-WORKFLOW | D8 — resolved preview before run; argv substitution |

NFR-KEY MUST hold with the field active (ADR 0050 D3).

## Consequences

- [SPEC-INPUT](../spec/SPEC-INPUT.md) is the field contract.
- ADR 0050's Blocks mode gates every feature here.
- ADR 0052 owns selection and hyperlinks in the **output** region; this ADR owns
  the input field.

## Rejected alternatives

- **A rich input field that also works in raw mode.** Rejected: D1. Two line
  editors fighting over one keystroke.
- **Expand aliases and corrections at submit for convenience.** Rejected: D2.
- **Run `--help` to build completions.** Rejected: D6. Executing to autocomplete
  is executing without consent.
- **Sync history to a service for cross-machine recall.** Rejected: D4,
  ADR 0044 D5.
- **Recording commands the shell was told to ignore.** Rejected: D4.
- **Synchronized input as a sticky mode.** Rejected: D7.
- **Shell-string parameter substitution in workflows.** Rejected: D8, injection.
