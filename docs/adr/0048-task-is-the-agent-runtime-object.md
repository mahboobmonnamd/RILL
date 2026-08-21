# ADR 0048: Task is the agent runtime object

- **Status:** Accepted — 2026-08-18
- **Amended by:** [ADR 0053](0053-runtime-domain-content-and-client-authority.md)
  D1 and D5. Task is not Session, TerminalExecution or transcript, and library
  serialization alone is not durable runtime evidence.
- **Historical identifier:** merged as ADR 0030 in PR #278; renumbered to ADR
  0048 on 2026-08-21 with its series. Renumbering changed no decision.
- **Tree:** this repository only
- **Issue:** catalog epic [#33](https://github.com/mahboobmonnamd/RILL/issues/33).
  Rows authorized by this ADR:
  F-130 [#159](https://github.com/mahboobmonnamd/RILL/issues/159), F-131
  [#160](https://github.com/mahboobmonnamd/RILL/issues/160), F-132
  [#161](https://github.com/mahboobmonnamd/RILL/issues/161), F-133
  [#162](https://github.com/mahboobmonnamd/RILL/issues/162), F-134
  [#163](https://github.com/mahboobmonnamd/RILL/issues/163), F-140
  [#169](https://github.com/mahboobmonnamd/RILL/issues/169), F-142
  [#171](https://github.com/mahboobmonnamd/RILL/issues/171), F-145
  [#174](https://github.com/mahboobmonnamd/RILL/issues/174), F-146
  [#175](https://github.com/mahboobmonnamd/RILL/issues/175), F-147
  [#176](https://github.com/mahboobmonnamd/RILL/issues/176), F-148
  [#177](https://github.com/mahboobmonnamd/RILL/issues/177), F-152
  [#181](https://github.com/mahboobmonnamd/RILL/issues/181), F-161
  [#190](https://github.com/mahboobmonnamd/RILL/issues/190), F-162
  [#191](https://github.com/mahboobmonnamd/RILL/issues/191).
- **Requires:** [ADR 0001](0001-session-operating-system.md),
  [ADR 0011](0011-session-graph.md), [ADR 0013](0013-cwd-tap.md),
  [ADR 0038](0038-session-graph-navigation-model.md),
  [ADR 0044](0044-trust-secrets-and-automation-boundary.md),
  [ADR 0047](0047-attention-is-an-orchestration-queue.md)
- **Amends:** nothing.
- **Does not authorize:** a model provider, provider SDKs, agent detection
  (ADR 0049), Blocks (ADR 0050), cloud sync, an account, writing to a PTY from a
  task surface without ADR 0049's adapter, JSON on the warm path.
- **Milestone:** M3 — Conversations

## Context

Fourteen rows describe the object and its UI: task as a runtime object (F-130),
replay adapter (F-131), section types (F-132), approve/reject/cancel (F-133),
question cards (F-134), prompt queue (F-140), task lists (F-142), context
attachments (F-145), diff review (F-146), checkpoints (F-147), conversation fork
(F-148), custom labels (F-152), HITL feed cards (F-161), agent explain (F-162).

PRD §6 already fixed the input rule so nobody invents a classifier: **Enter →
PTY, ⌘Enter → conversation.** That decision needs an object on the other side of
⌘Enter, and this ADR is that object.

The ordering risk is the one this repo keeps naming: building agent UI against a
live provider means the UI's correctness is never separable from a network call.
F-131 (replay adapter) exists to prevent that, and this ADR makes it the
**first** thing built, not a testing convenience added later.

## Decision

### 2026-08-21 amendment — durable fork graph and navigation policy

ADR 0053 D19 extends the Task object with stable parent/child relations,
domain/execution associations, isolation context, lifecycle, authorization,
artifacts, diffs, checkpoints and attention references. Forks remain grouped
under their parent and are hidden from ordinary navigation until explicitly
opened, pinned or requiring attention; forking never creates a pane or tab.

Cancellation/completion propagation is explicit and journalled, never inferred
from parent status. Concurrent writers use distinct worktrees under ADR 0049
D7. Conflicts are stable structured events with explicit resolution rather
than client-local warnings or silent merges. Task/fork state survives client
disconnect only when the runtime persistence/recovery gates are Proven.

### D1 — `Task` is a kernel-side runtime object with a stable id

A `Task` has: a stable `TaskId` (`u64`, kernel-allocated, never reused while
live), status, cwd, host, a target `NodeId`, a label, and an ordered list of
sections (D3).

It is **orchestration-plane state**. It lives beside the session graph, not
inside a chrome view model, for the same reason ADR 0038 D1 put containers in
the kernel: a task that a window owns dies with the window, and the product
promise is that work survives the window.

A `Task` MAY target a leaf; it MUST NOT own one. The kernel still owns PTYs
(FR-PTY, FR-SOLE). A task ending MUST NOT kill a leaf unless the user asked.

Custom labels and metadata (F-152) are fields on this object. A label is
display-only and MUST NOT be used to address a task.

### D2 — The replay adapter is the reference implementation, and it ships first

F-131. A `Task` is driven by an **adapter** producing a stream of section
events. The replay adapter reads a recorded transcript file and emits exactly
the same events a live provider would.

Every surface in this ADR MUST be demonstrable — and MUST have its named tests
demonstrated red-then-green (ADR 0002 D2) — against the replay adapter alone,
with no provider, no network, and no API key.

This is a sequencing decision: the replay adapter lands before any live adapter
(ADR 0049). A test that needs a network call is not evidence, because it cannot
be made to fail deterministically.

Mutation `ui_requires_live_provider` MUST turn T-TASK-REPLAY red.

### D3 — Sections are a closed set with defined semantics

F-132. Exactly these section types:

`prompt` · `plan` · `tool` · `command` · `output` · `approval` · `diff` ·
`result` · `question`

Each is append-only with a monotonic index. A section MUST NOT be mutated after
it is closed; corrections arrive as new sections. Append-only is what makes
replay, fork (D6) and checkpoints (D5) coherent.

Adding a section type requires an ADR amendment. An adapter that receives
something it cannot map MUST emit `output` with the raw content rather than
inventing a type or dropping it (NFR-FAIL, no silent loss).

Section content is untrusted (ADR 0044 D1): it may originate from a model, a
tool, or a remote process. It MUST NOT be rendered as executable markup, and
MUST be redacted at every persisting sink (ADR 0044 D4).

### D4 — Approvals and questions are the only blocking sections, and they are explicit

F-133, F-134, F-161.

- An `approval` section blocks the task until the user approves, rejects, or
  cancels. There MUST be no timeout that auto-approves, and no "remember this
  choice" that silently approves a *different* action later. A remembered choice
  is a permission-profile change and goes through ADR 0049 D5.
- A `question` section blocks until answered. Answering creates the answer as a
  new section; it MUST NOT be injected as keystrokes into a PTY by this surface
  (ADR 0046 D7).
- Both raise attention immediately: `approval` and `needs_input` respectively
  (ADR 0047 D1). Neither may be rate-limited away (ADR 0047 D5).
- HITL feed cards (F-161) are the mailbox rendering of exactly these two
  section types. They are a view, not a second inbox.

The approve/reject/cancel decision MUST record who decided and when, and that
record MUST survive a window restart.

Mutation `approval_times_out_to_yes` MUST turn T-TASK-APPROVE red.

### D5 — Diff review and checkpoints go through git, and never silently

- **Diff review (F-146):** per-file and per-hunk apply/reject shares
  ADR 0046 D3's implementation and its guards — exact hunk, staleness refusal,
  never on hover or selection.
- **Checkpoints (F-147):** a checkpoint is a git-backed snapshot of the
  workspace at a task boundary. Revert MUST show what will change before it
  changes, MUST refuse when the working tree has drifted, and MUST NOT discard
  uncommitted work the checkpoint does not contain without naming it.

A checkpoint MUST NOT be created silently on a schedule that surprises the user
into a repository full of machine commits; creation points are task boundaries
and explicit requests only.

### D6 — Fork branches the transcript, not the process

F-148. Forking a conversation creates a new `Task` whose sections are a
**copy-on-write prefix** of the source up to the fork point.

It MUST NOT duplicate the source's PTY, MUST NOT attach a second writer to any
leaf (FR-ONE), and MUST NOT continue the source task. Both tasks are then
independent; a change in one MUST NOT appear in the other. Append-only sections
(D3) are what make this cheap and correct.

### D7 — The prompt queue is user-owned and inspectable before it runs

F-140. Queued follow-ups are visible, editable, reorderable, and removable
**before** they are sent. Nothing in the queue may be sent while the task is
blocked on `approval` or `question` — a queued prompt MUST NOT answer a
question the user has not seen.

The queue survives window restart with the task (D1).

### D8 — Task lists are the agent's, and are not the user's todos

F-142. An agent's to-do list is derived from task sections and is display state.
It MUST NOT be conflated with the user's workspace todos (F-203, ADR 0042 D7),
which are user-owned files.

Task list items MUST NOT be independently editable in a way that desynchronizes
them from the sections they came from.

### D9 — Context attachments are scoped, named, and redacted at attach time

F-145. Blocks, files, selections, URLs and images may be attached to a task.
Every attachment MUST:

- be explicit — nothing is attached implicitly by being on screen,
- show its scope before it is sent (which file, how much of it),
- pass redaction at attach (ADR 0044 D4), because attaching is transmission,
- respect the index rules (ADR 0046 D5) — an attachment MUST NOT smuggle
  untracked or ignored files into context.

A URL attachment fetches nothing until the user confirms; page content is
untrusted (ADR 0042 D2).

### D10 — Agent explain is a diagnostic over recorded state

F-162. "Why is this pane `blocked`/`idle`?" MUST answer from **recorded**
evidence — the state machine's inputs and transitions with timestamps — not from
a fresh inspection or a model call.

It is the debugging surface for ADR 0049's detection. If explain cannot answer,
detection is not observable enough, and that is a bug in detection.

### D11 — Oracle

| ID | Closes |
|---|---|
| T-TASK-REPLAY | D2 — every surface demonstrable with no provider |
| T-TASK-PERSIST | D1 — task and prompt queue survive window restart |
| T-TASK-SECTIONS | D3 — closed set, append-only, unknown maps to `output` |
| T-TASK-APPROVE | D4 — no auto-approve, no timeout, decision recorded |
| T-TASK-CHECKPOINT | D5 — revert previews; drift refuses |
| T-TASK-FORK | D6 — independent tasks; one writer per leaf |
| T-TASK-QUEUE | D7 — queued prompt cannot answer a pending question |
| T-TASK-ATTACH | D9 — scope shown; redaction at attach; no untracked files |

## Consequences

- [SPEC-TASK](../spec/SPEC-TASK.md) is the object and section contract.
- ADR 0047's queue gains two producers (`approval`, `question`) and no new
  states.
- ADR 0049's live adapters must emit D3's sections; the replay adapter is the
  conformance reference.
- PRD §6's ⌘Enter now has a defined destination.

## Rejected alternatives

- **Task as chrome view-model state.** Rejected: D1. Dies with the window; the
  product promise is that work does not.
- **Build against a live provider and add replay later.** Rejected: D2. Then no
  gate is deterministic, and ADR 0002 D2 cannot be satisfied.
- **Open-ended section types so adapters can express anything.** Rejected: D3.
  An open set means no surface can be complete.
- **Auto-approve after a timeout so long runs do not stall.** Rejected: D4. The
  entire value of an approval is that a human saw it.
- **Fork by duplicating the process.** Rejected: D6, FR-ONE.
- **Implicitly attach the visible pane as context.** Rejected: D9. The user must
  know what left the machine.
