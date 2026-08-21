# ADR 0053: Runtime, domain, content and client authority

- **Status:** Accepted — 2026-08-21
- **Tree:** this repository only
- **Decision approval:** the repository-wide architecture decision gate answered
  by the product owner on 2026-08-21.
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
- durable transcript and structured content;
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

### D7 — ContentTimeline is the native content model

The primary terminal presentation is a virtualized `ContentTimeline` of typed
items, not a byte-range-only BlockList. Initial item kinds include terminal
input, terminal output, background output, agent conversation, tool call and
result, approval, question, diff/change result and explicit discontinuity.

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

No implementation may infer authority from milestone numbers alone. Work lands
in this dependency order:

1. authority mapping and domain/lifecycle specification;
2. supervised local runtime, worker recovery and client leases;
3. host terminal checkpoints, offsets and reconciliation;
4. ContentTimeline, transcript and retention policy;
5. compositor, text, editor, input, selection and accessibility;
6. remote/mobile transports and clients; and
7. agent product surfaces.

ADR 0037 remains Accepted, but Chip 1 live-swap implementation stays parked
until D5–D6 checkpoint/state compatibility is specified and its required
mutations have demonstrated red. The state contract must work for both Chip 0
and Chip 1; a swap must not create a second protocol.

## Explicit amendments

| Prior authority | Amendment |
|---|---|
| ADR 0038 D1/D3/D6 | `Session` becomes the durable grouping; the current PTY-owning object is `TerminalExecution`; presentation close no longer terminates owned executions. |
| ADR 0039 | Readers project the canonical domain while hidden; hidden UI does not constrain object existence. |
| ADR 0040 D1/D6 | Host canonical VT/checkpoints and policy-controlled replay supersede ring-only recovery and the fixed retention assumption. |
| ADR 0041 D1/D2/D5/D7 | Host authority, disposable mirrors, two SSH paths, leases and mobile semantics supersede SSH-forward-plus-ring-only reconnect. |
| ADR 0042 D7 | Mobile is an attaching client; it does not receive a separate ownership model. |
| ADR 0044 D4/D6 | Capture requires policy authority; update survival requires worker isolation and protocol compatibility. |
| ADR 0045 D2/D5 | Product-required compositor/text boundaries are allowed; speculative portable widget abstraction remains rejected. |
| ADR 0048 | Task remains distinct from Session, TerminalExecution and transcript; current library serialization is not product persistence evidence. |
| ADR 0050 D1/D2/D4 | Range-only Block identity and replay-as-normal-rendering are superseded by D7. |
| ADR 0051 | Structured editor content lives in ContentTimeline; raw TUI input still bypasses it. |
| ADR 0052 | Selection spans terminal and structured content through surface-specific anchors; raw-mode arbitration remains explicit. |
| ADR 0037 | Implementation is parked until checkpoint/reconciliation authority is accepted in specifications and red tests. |

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
- Claiming encryption or redaction makes capture safe by default.
- Publishing unstable internal crates as supported libraries.

## Evidence gates

Normative test names and required mutations are in
[TEST-CASES](../TEST-CASES.md#architecture-foundation--runtime-content-and-clients-red).
No item in this ADR is Proven merely because this document is Accepted.
