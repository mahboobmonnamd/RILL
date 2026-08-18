# SPEC-TASK — the agent runtime object (orchestration plane)

- **Status:** Accepted — 2026-08-18. Gates **Red** until demonstrated
  red-then-green (ADR 0002 D2).
- **Authority:** [ADR 0030](../adr/0030-task-is-the-agent-runtime-object.md)
- **Requires:** [SPEC-GRAPH](SPEC-GRAPH.md), [SPEC-NAV](SPEC-NAV.md),
  [SPEC-ATTENTION](SPEC-ATTENTION.md), [SPEC-TRUST](SPEC-TRUST.md)
- **Milestone:** M3 — Conversations

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. The object

- A `Task` has a stable `TaskId` (`u64`, kernel-allocated, not reused while
  live), status, cwd, host, target `NodeId`, label, and ordered sections.
- It is orchestration-plane state living beside the session graph, not inside a
  chrome view model.
- A `Task` MAY target a leaf; it MUST NOT own one. The kernel still owns PTYs
  (FR-PTY, FR-SOLE).
- Task end MUST NOT kill a leaf unless the user asked.
- A label is display-only and MUST NOT address a task.
- Tasks and their prompt queues MUST survive a window restart.

## 2. Replay first

- Every `Task` is driven by an adapter emitting section events.
- The replay adapter reads a recorded transcript and emits the same events a
  live provider would.
- Every surface in this spec MUST be demonstrable, and every gate MUST be
  demonstrated red-then-green, against the replay adapter alone — no provider,
  no network, no API key.
- The replay adapter lands **before** any live adapter.

## 3. Sections

Exactly: `prompt`, `plan`, `tool`, `command`, `output`, `approval`, `diff`,
`result`, `question`.

- Sections are append-only with a monotonic index.
- A closed section MUST NOT be mutated. Corrections are new sections.
- A new section type requires an ADR amendment.
- An adapter receiving something unmappable MUST emit `output` with the raw
  content. It MUST NOT invent a type or drop it.
- Section content is untrusted: it MUST NOT render as executable markup and MUST
  be redacted at every persisting sink (SPEC-TRUST §4).

## 4. Approvals and questions

- `approval` blocks until approve, reject, or cancel. There MUST be no timeout
  that auto-approves.
- "Remember this choice" MUST NOT silently approve a different action later; a
  remembered choice is a permission-profile change (SPEC-AGENT §5).
- `question` blocks until answered. The answer becomes a new section and MUST
  NOT be injected as keystrokes into a PTY.
- Both raise attention immediately (`approval`, `needs_input`) and MUST NOT be
  rate-limited away (SPEC-ATTENTION §5).
- HITL feed cards are the mailbox rendering of these two types, not a second
  inbox.
- The decision MUST record who decided and when, and MUST survive restart.

## 5. Diffs and checkpoints

- Per-file and per-hunk apply/reject uses SPEC-SURFACES §9 and its guards.
- A checkpoint is a git-backed snapshot at a task boundary.
- Revert MUST preview, MUST refuse on working-tree drift, and MUST NOT discard
  uncommitted work outside the checkpoint without naming it.
- Checkpoints MUST NOT be created on a schedule; task boundaries and explicit
  requests only.

## 6. Fork

- Fork creates a new `Task` with a copy-on-write section prefix to the fork
  point.
- It MUST NOT duplicate the PTY, MUST NOT attach a second writer to a leaf
  (FR-ONE), and MUST NOT continue the source task.
- The two tasks MUST be independent afterwards.

## 7. Prompt queue

- Queued follow-ups MUST be visible, editable, reorderable and removable before
  sending.
- Nothing MAY be sent while the task blocks on `approval` or `question`.

## 8. Task lists

- An agent's to-do list is derived from sections and is display state.
- It MUST NOT be conflated with user workspace todos (SPEC-SURFACES §7).
- Items MUST NOT be editable in a way that desynchronizes them from their
  sections.

## 9. Context attachments

- Attachment MUST be explicit; nothing attaches by being on screen.
- Scope MUST be shown before sending (which file, how much).
- Redaction applies at attach, because attaching is transmission.
- Attachments MUST NOT smuggle untracked or ignored files (SPEC-SURFACES §11).
- A URL attachment fetches nothing until confirmed; page content is untrusted.

## 10. Explain

- "Why is this pane `blocked`/`idle`?" MUST answer from recorded state-machine
  inputs and transitions with timestamps.
- It MUST NOT perform a fresh inspection or a model call.

## 11. Gates

| ID | Status | Closes |
|---|---|---|
| T-TASK-REPLAY | Red | §2 |
| T-TASK-PERSIST | Red | §1 |
| T-TASK-SECTIONS | Red | §3 |
| T-TASK-APPROVE | Red | §4 |
| T-TASK-CHECKPOINT | Red | §5 |
| T-TASK-FORK | Red | §6 |
| T-TASK-QUEUE | Red | §7 |
| T-TASK-ATTACH | Red | §9 |

## 12. What we will not do

- Hold task state in a chrome view model.
- Build against a live provider before replay exists.
- Open the section set.
- Auto-approve on a timeout.
- Fork by duplicating a process.
- Attach the visible pane implicitly.
