# SPEC-CLIENT-AUTHORITY — host state, mirrors, leases and reconnect

- **Status:** Red. Specification only; no implementation is authorized.
- **Authority:** [ADR 0053](../adr/0053-runtime-domain-content-and-client-authority.md)
  D5–D6 and D10–D11.
- **Lane:** `lane:attach` for protocol and flow control; `lane:kernel` for
  authority; platform host lanes for disposable presentation.

## 1. State allocation

| State | Authority |
|---|---|
| PTY, process, exit and canonical geometry | host TerminalExecution worker |
| terminal screen, modes, cursor, offsets and checkpoint | host terminal core |
| transcript, ContentTimeline, graph, Task and attention | host runtime |
| ClientId, role, grants, lease and audit | host runtime |
| VT mirror, selected object, scroll, zoom, local selection and chrome | client |
| composer draft | client-local, sensitive, non-durable by default |

Client state may be persisted locally for convenience but is disposable. A
client cannot make its screen, offset or lease authoritative by reconnecting.

## 2. Checkpoint and delta protocol

A terminal checkpoint contains format version, TerminalExecutionId, canonical
rows/columns, active screen, modes, cursor, compact cell/run and grapheme data,
palette identity, ending monotonic offset and state hash. It contains no
per-cell strings and is not a warm-path frame.

Attach proceeds:

1. authenticate and negotiate protocol/checkpoint versions;
2. receive graph and role/capability state;
3. receive the latest compatible checkpoint;
4. initialize a new local VT mirror;
5. apply ordered deltas strictly after the checkpoint offset;
6. verify offset and periodic state hash; and
7. present only after the mirror is consistent.

On a gap, duplicate, incompatible checkpoint or hash mismatch, the client
stops that pane's presentation and requests resync. It does not guess or keep
accepting input against a divergent mirror. Destroying the mirror and repeating
attach loses no authoritative state.

## 3. Clients, roles and flow control

Each connection has a unique ClientId and authenticated device identity. The
host assigns `controller`, `observer` or `administrator/owner` capabilities.
Each connection has independent receive credit and bounded queues.

Observers cannot send PTY DATA, RESIZE, lease renewal, signals or termination.
Their credit affects only their own stream. A stalled/disconnected observer may
lose live deltas and resync later; it cannot backpressure the controller or
other panes.

Before ATTACH completes, no pane-directed frame is accepted. There is no
implicit default execution for unattached traffic.

### 3.1 Typed channels

Protocol 2+ negotiates typed binary channels for topology, execution lifecycle,
terminal bytes, terminal snapshots/deltas, semantic transcript, Flow
projection, Tasks, structured requests/approvals, attention, artifacts/diffs,
leases, capability/policy and resume cursors. Each channel declares ordering
scope, frame and queue bounds, acknowledgement/credit, idempotency key,
authorization and snapshot/resume behavior.

Semantic events correlated to terminal output name the execution generation and
byte offset boundary. One failed, unauthorized or slow semantic channel cannot
withhold PTY DATA/CREDIT, raw input, terminal resync or another client's
traffic. Slow/mobile clients receive bounded projections and must resume from a
cursor or request a compatible snapshot. Unsupported protocol, checkpoint or
content versions fail closed without interpreting one frame type as another.

## 4. Input and resize lease

At most one ClientId owns a TerminalExecution's input/resize lease. The lease
contains generation, owner, expiry and canonical geometry. DATA and RESIZE are
accepted only with the current generation.

Takeover is an atomic host decision and produces an attributed event for every
attached client. A human request supersedes automated agent input under the
permission policy. Disconnect begins a bounded grace period. Expiry releases
the lease but does not terminate any domain object or process.

The lease owner determines PTY geometry. Observers render the live grid at that
geometry using crop, pan or letterbox. They may independently reflow immutable
ContentTimeline items. Offline input buffering is forbidden.

## 5. Local and remote transports

The protocol semantics above are transport independent. Local Unix sockets are
first. SSH may carry an explicitly selected enhanced RILL stream. A later
direct mutually authenticated transport requires its own threat model and
spike. Transport loss has the same detach semantics in every case.

Zero-footprint SSH is outside the product protocol and exposes its capability
downgrade before connect. It runs only the user-requested SSH session and does
not probe, upload, bootstrap, inspect history or execute hidden commands.

Enhanced bootstrap requires explicit opt-in and policy approval. Its plan lists
remote commands and artifacts before execution. Cleanup is best effort; residue
or unverifiable cleanup is reported and journaled.

Mobile uses the same client model. Backgrounding may drop the connection and
lease, never the Session. Mobile control requires the host to be awake, online
and reachable.

## 6. Gates

- T-CLIENT-MIRROR-DISPOSABLE
- T-CLIENT-MIRROR-RECONCILE
- T-CLIENT-RING-EVICTION-RESYNC
- T-CLIENT-OBSERVER-ISOLATION
- T-CLIENT-CREDIT-ISOLATION
- T-CLIENT-UNATTACHED-REFUSAL
- T-CLIENT-LEASE-ATOMIC
- T-CLIENT-LEASE-EXPIRY-DETACH
- T-CLIENT-VIEWPORT-AUTHORITY
- T-SSH-ZERO-FOOTPRINT
- T-SSH-ENHANCED-PLAN-CLEANUP
- T-MOBILE-BACKGROUND-DETACH
- T-PROTOCOL-SEMANTIC-INDEPENDENCE
- T-PROTOCOL-BYTE-EVENT-ORDER
- T-PROTOCOL-SLOW-CLIENT-CHANNEL-ISOLATION
- T-PROTOCOL-VERSION-MISMATCH

## 7. Out of scope

This spec does not choose a direct network protocol, authorize a relay, define
device-pairing UI or promise independent reflow of a live alternate-screen TUI.
