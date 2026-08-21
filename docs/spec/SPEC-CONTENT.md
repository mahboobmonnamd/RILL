# SPEC-CONTENT — terminal event ledger, ContentTimeline and retention

- **Status:** Red. Specification only; no implementation is authorized.
- **Authority:** [ADR 0053](../adr/0053-runtime-domain-content-and-client-authority.md)
  D5–D8, D15 and D22; supersedes range-only normal rendering in
  [ADR 0050](../adr/0050-blocks-are-a-cold-overlay.md).
- **Requires:** [SPEC-TERMINAL-PERFORMANCE](SPEC-TERMINAL-PERFORMANCE.md).
- **Lane:** `lane:kernel` for authoritative events and retention;
  `lane:host` for virtualized presentation.

## 1. Separate records

RILL maintains separate concepts:

| Record | Purpose |
|---|---|
| Terminal event ledger | ordered PTY bytes, explicit marks, resize/mode/exit events and offsets |
| Terminal checkpoint | bounded reconstruction starting point for canonical VT and client mirrors |
| ContentTimeline | ordered typed content used for normal primary-screen presentation |
| Semantic event ledger/transcript | authoritative typed runtime meaning; durable retention only under capture policy |
| Conversation and Task | structured orchestration objects referenced by timeline items |
| Client view state | presentation-only focus, scroll, selection, filters and chrome |

None substitutes for another.

## 2. Offsets and checkpoints

Every TerminalExecution has a monotonically increasing byte/event offset and
generation. Ring eviction advances a retained-base offset; it never renumbers
surviving data. A checkpoint names its ending offset and state hash. A source
range is `(TerminalExecutionId, generation, start, end, checkpoint-id?)`.

A range may support audit or reconstruction but is not durable display content
by itself. Starting replay in the middle of a control sequence or without prior
terminal state is invalid. Eviction or deletion changes the range to explicit
`Unavailable`/`Truncated`; it does not silently render different bytes.

## 3. ContentTimeline

The timeline is a virtualizable ordered collection. Initial item variants are:

- `TerminalInput`
- `TerminalOutput`
- `BackgroundOutput`
- `AgentMessage`
- `ToolCall`
- `ToolResult`
- `Approval`
- `Question`
- `DiffResult`
- `LifecycleEvent`
- `Discontinuity`
- `Unstructured`

Each item has a stable ContentItemId, ordering key, owning SessionId, optional
TerminalPaneId/TerminalExecutionId/TaskId/ConversationId, timestamps, retention
class, provenance and payload version.

Terminal output stores materialized grapheme/style runs or another specified
semantic representation suitable for normal reflow and accessibility, plus its
source ranges and checkpoint identity. It is not re-rendered by repeatedly
feeding an arbitrary byte slice to a fresh VT.

### 3.1 Semantic event envelope and invariants

The authoritative semantic event envelope contains:

- stable EventId, schema version and event kind;
- RuntimeId, WorkspaceId, SessionId and applicable TabId, PaneId,
  TerminalExecutionId, TaskId, StructuredRequestId and ArtifactId;
- stream identity plus strictly monotonic sequence and snapshot/resume cursor;
- terminal generation/offset range when terminal bytes are source evidence;
- explicit parent/correlation/causal IDs when emitted by an authoritative
  mechanism rather than inferred by presentation;
- provenance, timestamp/clock domain, retention class and payload hash; and
- a versioned typed payload with no renderer geometry or terminal cells as the
  only copy of agent, approval or artifact state.

Appending the same EventId and payload is idempotent. Reusing an EventId with a
different payload fails closed. Recovery loads a versioned snapshot and applies
strictly ordered deltas after its cursor. A gap, conflicting order or missing
retained dependency produces an explicit discontinuity/tombstone; clients do
not guess. Terminal bytes and semantic events correlate through execution
generation and offsets, while independent Task/approval events use explicit
causal references.

### 3.2 Flow and workspace activity

Flow is the default presentation for normal primary-screen shell activity when
semantic projection is healthy. It is a compact continuous document; cards,
spines, separators and gutters are client styling. Flow Blocks group stable
ContentItemIds/events and remain useful after policy-authorized raw byte
compaction.

The optional workspace activity timeline is a cross-pane projection over
transcript, process, Task, approval, artifact and lifecycle events. It is not a
second ledger, attention queue or navigation requirement, and it does not show
every command/tool event by default. Visual lanes/graphs are client state. A
new durable causal edge is allowed only when correctness needs information that
cannot be derived reliably.

## 4. Boundaries and raw mode

Command boundaries derive only from explicit shell/protocol marks or a known
RILL structured-input submission. Prompt regex, English/code classifiers and
cursor-position guesses are forbidden. Without a boundary, RILL creates an
honest terminal-output or `Unstructured` region. Displayed timestamps, command
text, duration, exit status, test results, diffs, agent status, cwd, branch and
approvals MUST use the source table in SPEC-TERMINAL-PERFORMANCE §3. Terminal
cells are never scraped to create those fields.

Full-screen alternate mode remains a mutable VT grid for the same terminal pane
and PTY. Entering alternate screen does not create a timeline Block. Leaving it
returns to the primary timeline without copying the alternate grid into history
unless a distinct policy-authorized capture action is specified.

Raw terminal mode bypasses the structured editor and feeds the PTY directly.
Nested tmux, Vim, Neovim and other TUIs retain normal terminal semantics.
Raw is also user-selectable for compatibility/troubleshooting and is the safe
fallback whenever semantic classification or Flow projection is unavailable.

## 5. Retention and deletion

Retention policy is resolved per policy domain, Workspace and Session, with the
most restrictive applicable rule winning. It independently controls raw replay
segments, checkpoints, semantic transcript, command history, conversations and
tasks.

Policy values include disabled, memory-only bounded, bounded durable and an
explicitly configured duration/size. Disabled durable retention is a supported
state. The UI and protocol report recovery and history consequences before the
policy changes.

When durable persistence is disabled, only bounded memory required for live
operation, attach, reconciliation and the declared recovery window may remain.
The authoritative semantic runtime model and live ContentTimeline/Flow
projection continue within those bounds, and raw terminal correctness is
unchanged. UI and protocol continuously expose the reduced history/recovery
capability and render unavailable history honestly.

When durable capture is enabled:

- data remains local unless a separate transmission action is approved;
- storage encryption and key state are reported, not assumed;
- loss of the key fails closed;
- deletion removes unpinned segments and materialized records according to the
  declared policy and reports pinned blockers;
- references to removed data become visible tombstones; and
- compaction remains bounded and crash recoverable.

Corporate policy may prohibit capture regardless of local encryption. RILL does
not present encryption or redaction as permission to collect.

Terminal content is sensitive by default. Retention, backup or sync MUST NOT
cross an operating-system user, runtime, host, Workspace, Session, client or
external-service boundary without explicit authority and the isolation gates in
SPEC-TRUST. Credentials and secret values are never configuration or sync data.

## 6. Redaction and export

Redaction operates on a derived sink such as clipboard, share, context attach,
log or export. The result records policy/rule versions and warns that detection
is incomplete. It does not alter canonical source evidence unless the user
performs an explicit destructive deletion governed by retention policy.

## 7. Recovery and fidelity

Normal presentation reads ContentTimeline or the live VT mirror. Byte replay is
allowed for checkpoint construction, conformance, disaster recovery and an
explicit raw/audit view. Reconstruction must start from a compatible checkpoint
or stream origin and verify its ending hash.

If compatible state is unavailable, RILL renders a Discontinuity item naming
the missing range and reason. It never invents text, command boundaries or
completion.

## 8. Gates

- T-CONTENT-MONOTONIC-OFFSETS
- T-CONTENT-RANGE-REQUIRES-STATE
- T-CONTENT-SURVIVES-RING-EVICTION
- T-CONTENT-NO-PROMPT-HEURISTIC
- T-CONTENT-ALT-SAME-PTY
- T-CONTENT-RETENTION-DISABLED
- T-CONTENT-RETENTION-RESTRICTIVE-WINS
- T-CONTENT-REDACTION-DERIVED
- T-CONTENT-TRUNCATION-VISIBLE
- T-CONTENT-BOUNDED-RECOVERY
- T-TRANSCRIPT-EVENT-IDEMPOTENCY
- T-TRANSCRIPT-BYTE-EVENT-ORDER
- T-FLOW-RAW-SEMANTIC-FAILURE
- T-ACTIVITY-DERIVED-NOT-AUTHORITY
- T-CONTENT-SOURCE-AUTHORITY
- T-PERF-PTY-DRAIN-INDEPENDENT
- T-PERF-SEMANTIC-DEGRADATION
- T-PERF-BYTE-FIDELITY

## 9. Out of scope

This spec does not select a database, promise perfect secret detection, define
cloud synchronization or make every raw terminal program semantically
structured.
