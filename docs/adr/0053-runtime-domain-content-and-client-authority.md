# ADR 0053: Runtime, domain, content and client authority

- **Status:** Accepted — 2026-08-21
- **Tree:** this repository only
- **Decision approval:** the repository-wide architecture decision gate answered
  by the product owner on 2026-08-21, including the accepted shell,
  configuration and privacy follow-up recorded in D13–D15, and the
  UI/workflow concept reconciliation recorded in D16–D21.
- **Implementation tracking:** none created or modified by this documentation
  decision. Each implementation slice still requires its own open GitHub issue,
  lane, milestone and evidence sequence.
- **Evidence record:** [Architecture evidence — 2026-08-21](../ARCHITECTURE-EVIDENCE-2026-08-21.md).
- **Requires:** [ADR 0001](0001-session-operating-system.md),
  [ADR 0002](0002-falsifiable-evidence.md),
  [ADR 0003](0003-display-pipeline.md),
  [ADR 0011](0011-session-graph.md),
  [ADR 0014](0014-m1-first-slice-closes.md),
  [ADR 0037](0037-chip1-live-swap.md),
  [ADR 0038](0038-session-graph-navigation-model.md),
  [ADR 0040](0040-terminal-fidelity-is-chip0.md),
  [ADR 0041](0041-remote-is-a-second-kernel.md),
  [ADR 0043](0043-one-look-schema-one-config-file.md),
  [ADR 0044](0044-trust-secrets-and-automation-boundary.md),
  [ADR 0045](0045-one-core-native-ui-per-os.md),
  [ADR 0048](0048-task-is-the-agent-runtime-object.md), and
  [ADR 0050](0050-blocks-are-a-cold-overlay.md).
- **Does not authorize:** production implementation, a public SDK, a hosted
  relay, an account system, or Chip 1 live swap. Each behavior still requires
  a GitHub issue, specification, demonstrated-red test, implementation, and
  the integration evidence required by ADR 0002.

## Context

Spike 0 and the M1 first slice proved the essential persistence wedge: a GUI
can die while a host process continues to own the PTY, and a client can render
raw bytes through an in-process VT and the Metal terminal-grid presenter.
Those are sound foundations, not the complete product architecture.

The repository subsequently used `Session` for both a PTY-owning execution
leaf and a user-facing durable grouping, described a Block as byte-ring offsets
that are replayed through a VT, treated a specialized grid renderer as the
whole display model, and specified remote reconnect from a bounded ring without
settling terminal-state authority. It also lacked explicit hidden-UI,
multi-client, runtime-update, daemon-crash, mobile and zero-footprint SSH
contracts.

The architecture was evaluated against traditional shells, tab and split
users, tmux and WezTerm multiplexers, SSH and SRE workflows, server/log
workflows, full-screen TUIs, single and concurrent coding agents, mobile
control, and users who want no workspace, session or agent chrome. The common
requirement is one durable host model with optional presentations, not separate
"simple" and "product" execution paths.

## Decision

### D1 — The canonical domain graph separates grouping from execution

The canonical ownership graph is:

```text
Runtime
└── Workspace
    └── Session
        └── Tab
            └── Split tree
                └── TerminalPane
                    └── TerminalExecution (zero or one)
```

- A `Workspace` is a stable organizational domain object.
- A `Session` is a durable grouping that owns tabs, restoration identity and
  transcript scope. It is not a PTY.
- A `TerminalPane` is the user-facing terminal surface slot. Each terminal pane
  owns at most one `TerminalExecution`.
- `TerminalExecution` is the host object that owns one PTY, its child process
  group, canonical terminal state and execution journal.
- `Leaf` remains internal split-tree terminology and MUST NOT be user-facing.
- Layout branches, tabs, sidebars, inspectors, rich-content views,
  conversations and tasks own no PTY.
- `Pane` is a typed layout leaf. `TerminalPane` is the only pane kind that may
  bind a `TerminalExecution`; agent, activity, diff, artifact, inspector,
  timeline and other typed panes own no PTY.
- Primary and alternate screen are states of the same terminal pane and
  execution. Alternate screen MUST NOT allocate a second PTY.

The existing kernel type named `Session` has `TerminalExecution` semantics.
Its rename and identifier migration require their own specification. No second
incompatible `Session` type may be added beside it.

Conversation and Task are attachable domain objects. They may target a
Workspace, Session or TerminalPane, but neither is the execution identity or
the transcript itself.

### D2 — Workspace and Session visibility are presentation state

Workspace and Session identities exist whether their management UI is visible.
There is one runtime and one object model in both modes.

- Unscoped creation resolves to stable implicit default Workspace and Session
  identities.
- Hiding chrome MUST NOT delete, recreate, detach, terminate, rename or migrate
  either object.
- Named objects may remain active and deep-linkable while management chrome is
  hidden. Hidden mode does not restrict the runtime to one existing Workspace.
- Enabling UI exposes the same identities and history. Disabling it again is
  lossless.
- Visibility, selected object, scroll position, zoom, sidebar state and
  inspector state are per-client view state, not runtime ownership.

### D3 — Persistence is the default; termination is explicit intent

`Quit RILL` detaches clients and leaves eligible Sessions and
TerminalExecutions running. Closing a window, GUI crash or `SIGKILL`, transport
loss, laptop sleep, mobile suspension, backgrounding, and lease expiry are not
termination intent.

The first `Close terminal pane` contract hides that client's presentation of
the same TerminalPane and TerminalExecution. It does not delete either object,
break their binding or signal the process. A later remove-from-layout operation
requires its own accepted domain transition; it cannot infer termination.

RILL MAY provide a separate **Quit and terminate selected sessions** action.
It MUST:

1. name the exact Sessions and TerminalExecutions affected;
2. show every attached controller and observer;
3. refuse ordinary termination while another controller is attached;
4. permit an explicit administrator/owner force path after showing those
   controllers and requiring a second destructive confirmation;
5. notify observers but not grant them a veto;
6. record the request, actor, signals, exit status and final durable state;
7. attempt a specified graceful PTY/process-group shutdown before bounded
   escalation; and
8. fail closed when ownership or foreground-process state is unknown.

There is no automatic terminate-on-quit preference in the first contract.
Host shutdown or logout cannot preserve a live process; it records an explicit
host-termination outcome and preserves only policy-permitted durable state.

### D4 — A supervised runtime and execution workers own survival

The host runtime runs on the machine where its PTYs and processes run. On
macOS it is a user-visible, user-controllable per-user service registered with
the supported Service Management/LaunchAgent mechanism. Linux and Windows use
equivalent per-user service mechanisms when specified.

The control daemon and every `TerminalExecution` have separate failure
boundaries. A worker owns the PTY master, child process group, canonical
terminal core, monotonic offset, bounded delta recovery and checkpoints for its
execution. Restarting or updating the control daemon MUST NOT terminate a
healthy worker or discard that live terminal authority. The restarted daemon
discovers, authenticates and reconciles workers from the durable journal. An
orphan or incompatible worker fails closed and remains observable; it is never
silently adopted by an unrelated runtime.

Updates require a versioned protocol and an N/N-1 compatibility window or a
refusal to update while incompatible live workers exist. Merely detaching the
current monolithic `rilld` from the GUI does not satisfy this decision.

### D5 — The host is authoritative

The process-owning host is authoritative for:

- PTY and process state;
- canonical terminal state, active screen, modes and canonical PTY geometry;
- monotonic execution offsets, checkpoints and byte deltas;
- authoritative semantic event/content runtime state and any
  policy-authorized durable records;
- Workspace, Session, Tab and pane graph;
- Conversation, Task and attention state; and
- client roles, input/resize leases and audit decisions.

The controller holding the input/resize lease determines the canonical PTY
geometry. Observers may pan, crop or letterbox the live terminal. They do not
independently resize or reflow a running full-screen TUI. Immutable structured
content may reflow per client.

### D6 — Client VT mirrors are disposable warm-path projections

Each terminal client MAY keep an in-process VT mirror for raw-byte rendering.
A mirror initializes from a versioned compact binary checkpoint and its ending
offset, consumes ordered byte deltas, and continuously reconciles offset and
state hashes with the host. Divergence stops presentation and requests a new
checkpoint; it never becomes authority.

Deleting or crashing a client mirror loses no authoritative state. Reconnect
after ring eviction MUST use a compatible checkpoint plus retained deltas or
show an explicit non-recoverable discontinuity. It MUST NOT fabricate output.

The warm path remains raw binary bytes to an in-process VT and POD damage to the
terminal-grid presenter. JSON, per-cell strings and control-plane RPC are still
forbidden on that path. Cold checkpoints MAY contain compact versioned POD/runs
and shared grapheme data; they are not per-frame cell IPC.

### D7 — The semantic transcript and ContentTimeline are the native content model

The authoritative semantic event ledger is an ordered, versioned runtime model
owned by the persistent runtime. Its durable persistence is governed by D8;
when durable persistence is disabled, only bounded memory required for live
operation, attach, reconciliation and the declared recovery window may remain.
The primary normal-shell presentation is Flow: a compact virtualized
`ContentTimeline` projection of typed semantic and runtime events, not a
byte-range-only BlockList. Initial item kinds include terminal input, terminal
output, background output, agent conversation, tool call and result, approval,
question, diff/change result and explicit discontinuity.

Every event has a stable event ID, owning runtime/domain IDs, per-stream
sequence, causal/correlation references where authoritative, payload version,
provenance and retention class. Append and recovery are idempotent. Snapshot
plus ordered delta recovery has an explicit cursor; gaps, conflicts and
truncation are visible. Terminal byte offsets correlate evidence to semantic
events, but neither offsets nor renderer geometry are semantic identity.

Terminal output items hold materialized semantic presentation data plus source
execution offsets and checkpoint identity. Command boundaries come only from
explicit shell/protocol marks or known RILL input events; prompt regex and
language heuristics are forbidden. Without marks, output remains an honest
terminal region rather than a fabricated command.

The mutable alternate-screen grid is a separate presentation mode of the same
TerminalPane. It is not inserted into the scrollable timeline as independent
Blocks. Raw bytes remain useful for live fidelity, reconstruction, audit and
disaster recovery. Replaying an arbitrary byte range through a fresh VT is a
recovery tool, not normal rendering and not a sufficient content identity.

`Block` may remain a product label or derived grouping, but ADR 0050's
range-only identity and normal replay requirements are superseded.

Raw is an exact, user-selectable compatibility/troubleshooting presentation of
the same execution and a mandatory fallback when semantic processing is absent,
late or unreliable. Alternate-screen or raw-mode ownership selects the full
terminal grid, suppresses the native composer and routes input directly to the
PTY. Flow resumes only after authoritative terminal modes permit it. No
presentation switch creates a pane, PTY, execution or Session.
Disabling durable semantic persistence does not disable raw terminal
correctness or live Flow presentation. The product reports the resulting
history and recovery limits instead of implying that unavailable content was
retained.

### D8 — Persistence and capture are explicit policy

Hot bounded memory required for attach and backpressure is independent of
durable capture policy. Durable raw replay data, semantic transcript, command
history, conversations and task history each declare retention class, limit,
location and deletion behavior.

- Local persistent retention is policy-controlled and MAY be disabled
  entirely for a Workspace, Session or policy domain.
- When disabled, the runtime retains only the bounded state needed for live
  operation and reconnect and MUST report the resulting recovery limit.
- Durable local data is encrypted at rest where the platform can protect the
  key, but encryption does not make captured corporate output automatically
  safe or authorize capture.
- Redaction creates a derived sink or export. It MUST NOT silently rewrite the
  canonical source, claim that all secrets were found, or justify collection.
- No transcript or replay data leaves the host without a separate explicit
  transmission decision.
- Segment compaction, pinning and deletion MUST remain bounded and MUST make
  truncation visible to every referring content item.

### D9 — The terminal renderer becomes one compositor primitive

The existing Metal glyph-atlas and instanced-cell renderer remains the
specialized terminal-grid primitive. RILL adds a retained compositor capable
of virtualization, shaped text runs, clips, layers, images, controls, diffs,
damage, hit testing and accessibility nodes. The compositor does not own
Workspace, Session, Task or transcript state.

Internal responsibility boundaries are:

| Boundary | Responsibility |
|---|---|
| `rill-domain` | stable IDs, graph, lifecycle vocabulary |
| `rill-protocol` | versioned binary frames, capabilities, transport-neutral messages |
| `rill-runtime` | service, workers, PTYs, leases, journal and persistence |
| `rill-terminal-core` | VT, screens, modes, input encoding, damage and checkpoint codec |
| `rill-text` | font discovery, fallback, shaping and reusable glyph data |
| `rill-terminal-surface` | grid scene, terminal selection, clipboard and accessibility mapping |
| `rill-content` | ContentTimeline, transcript and typed content schemas |
| `rill-editor` | structured input, IME and editor state outside raw TUI mode |
| `rill-compositor` | retained scene, virtualization, damage, hit testing and presenter contracts |

These are internal ownership boundaries in one monorepo, not nine deployment
units or nine promised public libraries. Existing crates move only through
separate accepted specifications. Core APIs MUST avoid application UI types and
preserve possible Rust, C ABI and WebAssembly seams, without promising public
stability. A browser client still requires WASM plus a browser presenter and a
TypeScript API; publishing a Rust crate alone is insufficient.

ADR 0045's prohibition on speculative cross-platform UI remains. It is amended
only to permit the product-required compositor and text boundaries on macOS;
native platform presenters remain the rule.

### D10 — Multi-client authority is explicit and per client

Every connection has a `ClientId`, authenticated device identity, role,
capabilities, independent flow-control window and client-specific view state.
Roles are `controller`, `observer` and policy-authorized `administrator/owner`.

- Exactly one client holds the input/resize lease for a TerminalExecution.
- Observers receive permitted output but cannot write, resize, grant credit for
  another client or keep a controller lease alive.
- Lease takeover is explicit, attributed, visible to all clients and atomic.
- Disconnect starts a bounded lease grace period; expiry releases the lease but
  never terminates the Session or process.
- Human takeover supersedes agent automation. No client may buffer keystrokes
  while offline for later injection.
- A malformed, unauthorized or stalled client is isolated and cannot terminate
  the runtime or block unrelated panes.

### D11 — Remote has two SSH paths and a future direct transport seam

When RILL is installed remotely, that remote RILL runtime is authoritative and
speaks the same versioned product protocol over a transport. Local Unix sockets
land first. SSH may carry an explicitly selected RILL protocol stream. A later
mutually authenticated direct transport stays behind the same boundary and
requires its own threat model and spike; no relay or account is authorized.

The SSH compatibility modes are distinct:

1. **Zero-footprint SSH.** RILL invokes only the user's requested SSH session.
   It MUST NOT upload, bootstrap, install, modify profiles, probe for RILL,
   inspect remote history, or run hidden remote commands. It exposes raw SSH
   terminal capability and an explicit capability downgrade: no RILL-owned
   remote persistence, transcript, rich content or multi-client semantics.
2. **Optional enhanced bootstrap.** This is explicit opt-in, names every remote
   artifact and command before execution, and runs only when local and remote
   policy permit. Cleanup is best effort and MUST be described as such; RILL
   MUST record residue or unverifiable cleanup rather than promise removal.

An iPhone or iPad is a client, never the owner of a remote Mac/Linux/VPS PTY.
Mobile v1 prioritizes view, attention, approval and deliberate lease takeover.
Backgrounding or network loss detaches it without termination. The host must be
awake, online and reachable for live control.

### D12 — Dependency order is binding

No implementation may infer authority from milestone numbers alone. The
domain/identity schema, configuration and privacy foundations are specified
first; behavioral slices then land in this dependency order:

1. terminal and PTY compatibility;
2. host-authoritative terminal state, supervision, checkpoints and leases;
3. authoritative semantic transcript runtime model, ordering and
   policy-governed retention;
4. Flow Block/ContentTimeline projection with independent Raw fallback;
5. persistent Workspace/Session/Tab/pane topology using the already-defined
   canonical identities;
6. durable agent Task state and isolation;
7. structured attention, requests and approvals;
8. artifact and diff state; and
9. optional derived workspace activity timeline.

Compositor, text, input, selection, accessibility, remote/mobile and
configuration work enters only when the authority it consumes is available;
none may reorder the dependencies above. Persistent topology is fifth even
though its type/identity contract is specified first: early schema definition
does not claim topology persistence is implemented before terminal content.

Shell compatibility is a foundation gate across every step. Privacy and
configuration isolation are cross-cutting prerequisites: a feature does not
advance merely because its functional dependency is ready if its data sinks or
configuration migration remain unspecified.

ADR 0037 remains Accepted, but Chip 1 live-swap implementation stays parked
until D5–D6 checkpoint/state compatibility is specified and its required
mutations have demonstrated red. The state contract must work for both Chip 0
and Chip 1; a swap must not create a second protocol.

### D13 — PTY-compatible shells remain native shell experiences

RILL launches the user's selected zsh, fish, bash or other PTY-compatible shell
with normal PTY, argv, environment, cwd, signal, job-control and terminal
capability semantics. Existing shell startup files, prompts, themes, plugins,
line editors, completions, ANSI colours and interactive programs MUST work
without RILL-specific replacement, rewriting or configuration changes.

Shell integration is optional and additive. Its absence may reduce semantic
metadata, but MUST NOT reduce raw terminal correctness. RILL MUST NOT inject
hidden commands, rewrite prompts, replace a shell theme/plugin, modify profiles
or require a RILL shell wrapper for correctness. Zero-footprint SSH preserves
the remote host's shell and configuration and remains the default remote path.

### D14 — One versioned TOML configuration model governs the product

RILL has one canonical, versioned TOML model for application and terminal
themes, fonts and sizes, keybindings, rendering, Workspace/Session behavior,
privacy/retention preferences and other user settings. Importers and a future
settings UI target this model; they do not create shadow stores.

A named theme resolves application chrome, terminal palette, ContentTimeline,
editor, diffs, controls and accessibility/contrast tokens consistently. A
surface may derive a role-specific token, but MUST NOT silently substitute an
unrelated theme or compiled palette.

Configuration loading is validated and fail-closed. Schema migration is
versioned, previewable, atomic and recoverable from a pre-migration backup.
Users can export and back up the canonical configuration. Optional sync is
allowlisted, opt-in and never required for local use. Configuration, export,
backup and sync MUST NOT contain credentials, private keys, access tokens,
secret values, device authentication material or host credentials; such data
uses a separately governed platform credential store and opaque references.

### D15 — Privacy and PII boundaries are architectural, not cleanup work

Terminal output, commands, transcripts, content, local history, clipboard
payloads, agent context, host/user/session identifiers and diagnostic metadata
are sensitive by default. Every collection, persistence, export, backup, sync,
telemetry, crash-report and external-agent sink declares purpose, minimum data,
scope, retention, encryption, access boundary, redaction and deletion behavior
before it receives data.

- Collection and transmission are minimized; unavailable data is not
  reconstructed or collected “just in case.”
- Policy may disable sensitive persistence entirely. The most restrictive
  user, host, Workspace, Session or enterprise policy wins.
- Policy-authorized durable sensitive data is encrypted at rest with
  platform-protected keys. Network transfer is authenticated and encrypted.
- Logs, telemetry and crash reports exclude terminal content, commands,
  clipboard payloads, credentials, secrets and raw agent context. Optional
  diagnostics require explicit scope disclosure and consent.
- Clipboard and agent context are explicit derived sinks. The exact scoped
  payload is previewed where practical, redacted under policy and never
  expanded to a whole Session or Workspace by implication.
- Runtime, storage, backup and sync boundaries isolate operating-system users,
  hosts, Workspaces, Sessions, clients, agents and external services. A failure
  or identifier collision MUST NOT disclose data across those boundaries.
- Credentials, secrets and PII MUST NOT be placed in configuration, URLs,
  process arguments, service logs, analytics identifiers or crash metadata.

Encryption and redaction reduce exposure only after collection is authorized;
they do not make collection safe by default or replace minimization.

### D16 — The UI concepts are projections, not architectural layers

The supplied prototype and screenshots are product-concept evidence, not
visual or API authority. Tabs, split geometry, Block styling, spines, cards,
gutters, timeline lanes, inspector layout, navigation chrome, popovers,
overlays and badges remain client projections over stable runtime objects.
Switching Raw, Flow or TUI presentation changes only the view of one
TerminalPane/TerminalExecution.

Inspector and navigation consume typed authoritative state and deep links; they
MUST NOT scrape terminal cells or become the only location for a critical
action. The central work surface remains terminal-first. Optional agent,
activity, attention, artifact, diff and timeline surfaces MUST NOT obstruct or
be required for raw terminal operation.

### D17 — The activity timeline is a derived cross-pane projection

The workspace activity timeline is optional and is not the terminal,
ContentTimeline, attention queue or an independent source of truth. It derives
auditable cross-pane summaries from transcript, process, Task, approval,
artifact and lifecycle events. It does not repeat every command or low-level
tool call by default. Visual chronology, graph nodes, lanes and causal lines are
client layout.

New authoritative causal/dependency edges may be added only when correctness
requires information that cannot be derived reliably. Such edges are ordinary
versioned runtime events with authorization, retention and recovery semantics,
not renderer-owned graph state.

### D18 — Attention and responses are structured runtime objects

Each actionable request has a stable `StructuredRequestId`; each attention item
has a stable `AttentionId`, exact domain/task/execution references where
applicable, type, urgency, lifecycle/expiry, authorization policy, navigation
target and allowed actions. Attention is a projection over source events and
request lifecycle, not a second copy of approval or task state.

Inline response is allowed only for safe, single-step structured requests.
Raw-terminal prompts, TUI-owned input, ambiguous/multi-step interactions and
secret/password input navigate to the owning pane. Secret values are never
duplicated in attention or notification previews. Responses are authenticated,
bound to the request ID and current generation, and reject stale, expired,
duplicate or replayed decisions. Cell scraping cannot create an actionable
request.

### D19 — Tasks and forks are durable, subordinate runtime objects

Task identity, parent/child relations, domain associations, status, isolation
context, messages, tool calls, approvals, artifacts, diffs, checkpoints,
attention and authorization survive client disconnection under retention
policy. A fork remains grouped beneath its parent and does not create a visible
tab or pane unless explicitly opened, pinned or requiring attention.

Cancellation and completion propagate only through explicit policy recorded in
the task graph; no implicit parent/child termination is allowed. Concurrent
write-capable tasks require distinct worktrees or equivalent filesystem
isolation. Conflicts become durable structured events requiring explicit
resolution; they are not silently merged by a client.

### D20 — Input arbitration is host-authoritative and mode-explicit

Input modes are native command composer, shell line editor, raw terminal,
alternate-screen/raw-mode application, agent prompt and structured approval.
The runtime terminal modes, task/request state and current input lease determine
which target may receive input; client focus alone never grants authority.
Transitions are ordered events and preserve focus restoration, IME composition,
paste and mouse routing or fail safely to direct raw-terminal input.

Composer drafts are sensitive, client-local and non-durable by default. They
are never shared, backed up or synchronized implicitly. Durable or cross-device
drafts require a later explicit policy and threat-model decision. Missing or
unreliable shell integration disables composer-derived semantics, not native
shell input.

### D21 — Protocol channels are typed, ordered, bounded and independently failing

The product protocol has capability-negotiated binary channels for topology,
execution lifecycle, terminal bytes, terminal checkpoints/deltas, semantic
events, ContentTimeline snapshots/deltas or bounded semantic-content
projections, Tasks, structured requests/approvals, attention, artifacts/diffs,
leases, policy and resume cursors. Latency-sensitive terminal traffic remains
binary and never waits on JSON or a semantic channel.

Clients derive Flow, accessibility views, mobile summaries, future
presentations and visual Block styling from authoritative semantic content.
Cards, spines, gutters, separators, timeline geometry and other presentation
choices MUST NOT enter protocol schemas. This derivation does not move semantic
authority into the client.

Each channel declares ordering domain, maximum frame/queue size,
acknowledgement/credit, idempotency key, snapshot cursor, missed-event recovery
and authorization. Correlation records define ordering between terminal byte
offsets and semantic events. Slow and mobile clients receive bounded projections
or resync requirements; one failing semantic channel cannot stall PTY drain,
raw input or another client. Protocol/checkpoint/content versions negotiate
explicitly and fail closed when no compatible recovery path exists.

## Explicit amendments

| Prior authority | Amendment |
|---|---|
| ADR 0038 D1/D3/D6 | `Session` becomes the durable grouping; the current PTY-owning object is `TerminalExecution`; presentation close no longer terminates owned executions. |
| ADR 0039 | Readers project the canonical domain while hidden; hidden UI does not constrain object existence. |
| ADR 0040 D1/D4/D6 | Host canonical VT/checkpoints and policy-controlled replay supersede ring-only recovery; D13 makes native shell compatibility explicit. |
| ADR 0041 D1/D2/D5/D7 | Host authority, disposable mirrors, two SSH paths, leases and mobile semantics supersede SSH-forward-plus-ring-only reconnect; zero-footprint preserves the remote shell unchanged. |
| ADR 0042 D7 | Mobile is an attaching client; it does not receive a separate ownership model. |
| ADR 0043 D1/D2/D7 | D14 fixes TOML, complete settings scope, named-theme consistency, migration/export/backup and secret-free optional sync. |
| ADR 0044 D4/D6 | Capture requires policy authority; update survival requires worker isolation and protocol compatibility; D15 adds end-to-end privacy and PII isolation. |
| ADR 0045 D2/D5 | Product-required compositor/text boundaries are allowed; speculative portable widget abstraction remains rejected. |
| ADR 0048 | Task remains distinct from Session, TerminalExecution and transcript; current library serialization is not product persistence evidence. |
| ADR 0050 D1/D2/D4 | Range-only Block identity and replay-as-normal-rendering are superseded by D7. |
| ADR 0051 | Structured editor content lives in ContentTimeline; raw TUI input still bypasses it. |
| ADR 0052 | Selection spans terminal and structured content through surface-specific anchors; raw-mode arbitration remains explicit. |
| ADR 0037 | Implementation is parked until checkpoint/reconciliation authority is accepted in specifications and red tests. |
| ADR 0047 | Attention becomes a stable structured runtime projection with exact source IDs, authenticated responses and replay protection; the existing queue implementation proves only its narrower library contract. |
| ADR 0048/0049 | Task forks gain durable parentage, hidden-by-default navigation, explicit propagation, isolation and conflict events. |
| ADR 0050 | Flow is the default normal-shell presentation over the authoritative transcript; Raw/TUI remain the exact independently operable fallback. |
| ADR 0051/0052 | Input modes and transitions are explicit; composer drafts default to client-local, sensitive and non-durable. |

## Consequences

- The current M1 kernel `Session` name requires a deliberate compatibility
  migration, but the PTY mechanism is preserved.
- Runtime supervision becomes more complex because PTY workers outlive daemon
  instances. That complexity is the cost of the approved crash/update survival
  property.
- Host canonical terminal state duplicates a VT projection in attached clients.
  Reconciliation makes that duplication disposable and keeps cells off the
  warm IPC path.
- Durable history may legitimately be absent by policy. UI and APIs must expose
  the resulting recovery boundary instead of inventing data.
- Rich content and the terminal grid share a compositor without forcing a TUI
  through structured-content abstractions.
- Remote support remains truthful: zero-footprint SSH is useful but degraded;
  enhanced bootstrap is explicit and cannot guarantee cleanup.
- Shell users keep their real shell ecosystem; semantic enrichment must degrade
  without changing prompts, plugins, profiles or interactive behavior.
- Configuration portability improves, but migrations, backup and optional sync
  become security-sensitive operations with explicit negative-data contracts.
- Privacy gates precede every new sink, including diagnostics and agent context;
  encryption/redaction cannot be used to waive minimization.
- The prototype's inspector, timeline, Flow styling and navigation can evolve
  without runtime identity or protocol migration.
- Typed semantic channels add protocol complexity but cannot become a
  dependency of raw terminal availability.

## Rejected alternatives

- A second "simple terminal" path with no Workspace or Session.
- `Session` meaning both PTY execution and durable grouping.
- Termination inferred from client loss or application quit.
- A monolithic daemon whose restart drops all PTYs.
- Client-authoritative terminal state or host-rendered per-frame cells.
- Byte-ring offsets as durable content identity.
- Prompt-regex command inference.
- Replacing the Metal terminal renderer or raw VT path.
- Copying another terminal's Block or compositor implementation.
- SSH probing or hidden bootstrap in zero-footprint mode.
- Replacing shell prompts, themes, plugins or startup files to make RILL work.
- Multiple config formats or a GUI-only settings database.
- Storing credentials in TOML so export, backup or sync is “complete.”
- Content-bearing telemetry or crash reports, even when nominally redacted.
- Claiming encryption or redaction makes capture safe by default.
- Publishing unstable internal crates as supported libraries.
- Treating the prototype layout, inspector, timeline or Block cards as durable
  runtime state or public API.
- Encoding approvals, agent state or actionable requests only in terminal cells.
- Making the activity timeline authoritative or mandatory for navigation.

## Evidence gates

Normative test names and required mutations are in
[TEST-CASES](../TEST-CASES.md#architecture-foundation--runtime-content-and-clients-red).
No item in this ADR is Proven merely because this document is Accepted.
