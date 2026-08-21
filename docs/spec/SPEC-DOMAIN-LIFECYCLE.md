# SPEC-DOMAIN-LIFECYCLE — durable grouping, visibility and termination

- **Status:** Red. Specification only; no implementation is authorized.
- **Authority:** [ADR 0053](../adr/0053-runtime-domain-content-and-client-authority.md)
  D1–D3.
- **Lane:** `lane:kernel` for the domain model; `lane:host` only for projections
  and explicit user intent.

## 1. Canonical types and ownership

```text
RuntimeId
└── WorkspaceId
    └── SessionId
        └── TabId
            └── SplitNodeId
                └── PaneId (typed)
                    └── TerminalPaneId
                        └── TerminalExecutionId?  -> PTY + process group
```

IDs are stable opaque values. A label, path, title, selected row, attach token,
socket address or process ID is never an object's identity.

`Workspace` and `Session` are durable organizational records. `Tab`, split
nodes and typed panes are their layout. `TerminalExecution` alone owns one
PTY master and process group. A terminal pane owns zero or one execution. No
other domain or view object owns a PTY. Agent, activity, diff, artifact,
inspector and timeline panes therefore have stable PaneIds and lifecycle but no
TerminalExecutionId. Changing a pane's presentation never allocates another
pane or domain object.

Every Workspace, Session, Tab, Pane, TerminalExecution, Task, Activity,
StructuredRequest, AttentionItem and Artifact type specifies stable identity,
owner, lifecycle, persistence class, protocol representation, authorization,
failure result, recovery behavior and named tests before implementation.

The current kernel `Session` and `SessionId` have TerminalExecution semantics.
Before a durable `Session` type can land, a migration spec MUST name:

- serialized and protocol identifiers affected;
- compatibility aliases and their removal gate;
- database/journal migration and rollback;
- code call sites that remain internal `Leaf` tree terminology; and
- downstream tests that distinguish SessionId from TerminalExecutionId.

Adding a second Session meaning without that migration is forbidden.

## 2. Implicit objects and hidden UI

Each Runtime has one stable implicit default Workspace and, within it, one
stable implicit default Session for unscoped creation. They are ordinary domain
objects with durable IDs, not sentinels, empty IDs or separate types.

Visibility modes are per-client settings:

| Setting | Domain effect | Presentation effect |
|---|---|---|
| Workspace UI enabled | none | management and naming surfaces may be shown |
| Workspace UI disabled | none | workspace chrome is absent |
| Session UI enabled | none | session management/history may be shown |
| Session UI disabled | none | session chrome/history picker is absent |

Named Workspaces and Sessions remain live and addressable while UI is hidden.
Deep links, restoration and remote clients resolve the same IDs. Toggling
visibility MUST NOT create, delete, rename, merge, detach or terminate objects.

Unscoped commands use the implicit defaults. A command containing an explicit
ID uses that object regardless of visibility. Deleting the implicit default is
refused while it is the unscoped target; an explicit replacement transaction
must commit before deletion.

## 3. View state

Selected Workspace/Session, focused pane, sidebar visibility, inspector state,
scroll position, zoom, mobile navigation stack and content filters are keyed by
ClientId. They are neither kernel ownership nor lifecycle state. A client may
choose to persist its own view state, but another client does not inherit it
unless a later explicit sharing contract says so.

## 4. Lifecycle events

| Event | Required result |
|---|---|
| Close one window | detach that presentation only |
| Close one terminal pane | hide/detach that client's presentation of the same pane; preserve its IDs, binding and process |
| Quit RILL | detach the quitting client's presentations |
| GUI crash or `SIGKILL` | detach after connection loss; processes continue |
| Network interruption | retain processes and state; lease grace may begin |
| Laptop sleep | processes suspend/resume with the host; no termination intent |
| Mobile backgrounding | detach or suspend presentation; no termination intent |
| Client lease expiry | release input/resize lease only |
| Runtime control-daemon restart | workers and PTYs continue; daemon reconciles |
| Host logout/shutdown | record host termination; live-process survival is not promised |
| Explicit terminate | run the workflow in §5 |

Stale implicit Sessions are ordinary Sessions subject to explicit retention and
cleanup policy. Age, absence of clients or hidden UI alone never authorizes
termination.

`Close terminal pane` is not remove-from-layout in this first contract. A later
domain operation that removes or rebinds a pane requires an Accepted transition
that names the execution destination and recovery behavior; it never gains
implicit process-termination authority.

## 5. Explicit termination

The request payload identifies exact SessionIds or TerminalExecutionIds,
requesting ClientId, device identity, role and `force` flag. Before acting the
runtime returns an impact record containing all executions, foreground-process
knowledge, controllers, observers and unknowns.

Ordinary termination is refused when another controller is attached. An
administrator/owner force request requires a second confirmation bound to the
unchanged impact record. If the impact record changes, confirmation expires.

The termination algorithm is separately specified per platform but MUST:

1. journal intent before signalling;
2. request a graceful terminal/process-group shutdown;
3. wait a bounded, observable interval;
4. escalate through specified signals/actions;
5. reap every child it owns;
6. record final status and incomplete cleanup; and
7. preserve policy-permitted transcript/final state.

Unknown ownership or an inability to journal fails closed before the first
signal. Observers are notified but do not veto an authorized termination.

## 6. Gates

Normative gates:

- T-DOMAIN-IDENTITY-MIGRATION
- T-WORKSPACE-HIDDEN-IDENTITY
- T-SESSION-HIDDEN-IDENTITY
- T-LIFECYCLE-UNINTENTIONAL-DETACH
- T-TERMINATE-OTHER-CONTROLLER
- T-TERMINATE-FORCE-IMPACT
- T-TERMINATE-JOURNAL
- T-ALT-SAME-EXECUTION

Each is Red until its TEST-CASES mutation is demonstrated.

## 7. Out of scope

This spec does not choose storage technology, implement the identifier rename,
define collaboration between different operating-system users, or authorize a
cloud account or relay.
