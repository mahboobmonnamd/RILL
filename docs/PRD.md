# PRD — RILL

- **Status:** Authority for this repository. Implementation requires an Accepted ADR plus a GitHub issue in an open milestone.
- **Date:** 2026-08-21
- **Audience:** 3–5 engineers in parallel.

## 1. Product

RILL is a native terminal whose **work survives its clients**, and whose
**typing path is in-process**.

Users include traditional shell users, tab/split and multiplexer users, remote
and SRE workflows, TUI-heavy developers, single and concurrent coding-agent
work, mobile review/control, and people who want none of the Workspace, Session
or agent chrome visible. macOS is first; the domain is not macOS-only.

Promise:

> Close the app. The shell is still there. Open it again from an authorized
> client and you see the same work. Typing never felt like a remote desktop.

The accepted product direction includes durable grouping, optional hidden
chrome, structured terminal/agent content, host-authoritative recovery, rich
composition and remote/mobile attachment. Acceptance is architecture authority,
not implementation or release evidence. A live model provider, Linux/Windows
UI, public library and hosted relay remain unpromised.

## 2. Why a new tree

A previous prototype put the display emulator behind JSON (`pane_replay` as number arrays, every cell copied to `String`, SwiftUI observing the PTY buffer). Persist worked. Feel did not. Multi-agent on that path would have been undebuggable.

RILL starts from the lock, not from that composition. This repo does not depend on that tree.

## 3. In scope for Spike 0

One window. One runtime. One PTY. Chip 0 display. Framed attach. Quit/SIGKILL keeps the child. Reopen paints via a cold-path resync. Named tests in [SPIKE-0](SPIKE-0.md).

## 4. Delivery status and dependency order

Spike 0 and the M1 first slice are Proven under their recorded gates. Later
library slices or Accepted ADRs are not product E2E proof.

Implementation follows [ADR 0053](adr/0053-runtime-domain-content-and-client-authority.md)
D12:

1. domain/authority schema, canonical configuration and privacy;
2. terminal and PTY compatibility;
3. host terminal state, supervised workers, checkpoints and client leases;
4. authoritative semantic transcript runtime model and policy-governed
   retention;
5. Flow projection with independently operable Raw fallback;
6. persistent Workspace/Session/Tab/pane topology;
7. durable Task state and isolation;
8. structured attention and approvals;
9. artifact and diff state; and
10. optional derived workspace activity timeline.

Chip 1 remains isolated. Live swap is parked until the host-state and
checkpoint compatibility contract is specified and its required mutations are
demonstrated red. Accounts, billing, hosted control plane and a RILL relay are
out of scope.

## 5. Requirements

### Functional

| ID | Requirement |
|---|---|
| FR-PTY | Host TerminalExecution worker creates, owns, and reaps one terminal-pane PTY. GUI does not `posix_spawn` the user shell. |
| FR-ATTACH | Live keys and PTY bytes travel on a framed `SOCK_STREAM`. Darwin has no `SOCK_SEQPACKET`. |
| FR-SOLE | Kernel is the only writer on the PTY master. Do not pass the master fd to the GUI. |
| FR-CHIP0 | Window runs `libghostty-vt` + our Metal. `feed` takes bytes. Paint is a flat POD buffer + damage, not per-cell `String`. |
| FR-HISTORY | Runtime owns bounded hot recovery state. Durable raw replay, transcript, command, conversation and task retention are separately policy-controlled and may be disabled. |
| FR-RESYNC | Host owns canonical VT state and versioned checkpoints. A disposable client mirror initializes from a checkpoint, applies ordered byte deltas and reconciles offsets/hashes. Checkpoints are not on the warm keystroke path. |
| FR-EXIT | Child exit is an in-band frame. A dead pane does not look alive. |
| FR-RESIZE | Resize is in-band on the splice, ordered with keys. |
| FR-ONE | One terminal pane owns at most one TerminalExecution and PTY. Multiple clients use explicit roles and one input/resize lease. |
| FR-KILL | GUI `SIGKILL`: same child PID accepts input on reopen. |
| FR-DOMAIN | Runtime owns stable Workspace → Session → Tab → split tree → terminal-pane identities. Session is a durable grouping; TerminalExecution owns the PTY. |
| FR-HIDDEN | Hiding Workspace, Session or agent UI changes presentation only. Stable implicit defaults and named objects remain the same objects. |
| FR-QUIT | Normal quit detaches. Termination is a separate journaled action; another controller blocks ordinary termination and owner/admin force requires explicit impact confirmation. |
| FR-RUNTIME | A production per-user service supervises a control daemon and PTY-owning workers. Daemon restart or compatible update does not kill healthy workers. |
| FR-CONTENT | Primary presentation supports a virtualized typed ContentTimeline. Raw byte replay is recovery/audit, not normal content identity. Alternate screen remains the same pane and PTY. |
| FR-FLOW | Normal primary-screen shell activity defaults to compact Flow projected from authoritative semantic events. Exact Raw is selectable and remains independently operable when semantic processing fails. Flow/Raw/TUI switches never create another domain object or PTY. |
| FR-TRANSCRIPT | Runtime owns a versioned authoritative semantic event ledger with stable IDs, ordering, byte correlation, idempotency, bounded retention and snapshot/delta recovery. Durable persistence is separately policy-controlled; disabling it preserves raw correctness and bounded live Flow while reporting reduced history/recovery. Renderer geometry and cell scraping are never product authority. |
| FR-ACTIVITY | Workspace activity is an optional cross-pane projection over durable source events, not an authoritative timeline domain, attention duplicate or navigation requirement. |
| FR-ATTENTION | Attention and structured requests have stable IDs, exact deep links, lifecycle/expiry, authorization and allowed actions. Secret/TUI/raw prompts navigate to source; stale/replayed responses fail closed. |
| FR-TASK-FORK | Task forks are durable children, hidden/grouped by default, create no pane/tab, use explicit isolation and record propagation/conflict semantics. |
| FR-INPUT-MODE | Composer, shell editor, Raw, TUI, agent and approval input modes are explicit and host/lease-authorized. Composer drafts are client-local and non-durable by default. |
| FR-PROTOCOL | Typed binary channels carry authoritative semantic events, ContentTimeline snapshots/deltas or bounded semantic-content projections with independent ordering, bounds, credit, recovery and version negotiation. Clients derive Flow and other presentations; semantic failure cannot block raw terminal traffic. |
| FR-COMPOSITOR | Existing Metal terminal-grid rendering remains inside a broader RILL compositor for shaped text, rich content, editor, diffs, images, controls and accessibility. |
| FR-CLIENTS | Each client has identity, role, independent credit and view state. Observers cannot write, resize or affect controller flow control. |
| FR-REMOTE | A RILL runtime on the process host is authoritative. Zero-footprint SSH performs no probing/bootstrap/history/profile/hidden commands. Enhanced bootstrap is explicit opt-in with best-effort cleanup. |
| FR-MOBILE | Mobile attaches as a client to an awake/reachable host. Backgrounding or lease loss never terminates work; offline keystroke injection is forbidden. |
| FR-SHELL | zsh, fish, bash and other PTY-compatible shells retain their normal startup files, prompts, themes, plugins, ANSI colours, job control and interactive behavior without RILL replacement or modification. Shell integration is optional; zero-footprint SSH is default. |
| FR-CONFIG | One versioned TOML model governs themes, fonts/sizes, keybindings, rendering, Workspace/Session behavior, privacy/retention and other settings. It validates and migrates atomically, supports export/backup and optional allowlisted sync, and never contains credentials or secrets. |

### Non-functional

| ID | Requirement |
|---|---|
| NFR-KEY | Key-down `NSEvent.timestamp` → `presentedTime` of the drawable first containing the echoed glyph **at the cell the cursor occupied**. Packaged app. p95 < one display refresh interval over ≥1000 accepted samples, discards ≤ 2%. Warm and under load. **On battery.** Zero control-plane RPCs during the run. Superseded definition and measurement procedure: [ADR 0003](adr/0003-display-pipeline.md) D5–D9. |
| NFR-DROP | `yes` for 10s then `^C`: zero dropped bytes; prompt usable. Per-pane pumps so one flood cannot stall another pane (when panes exist). |
| NFR-BYTES | Invalid UTF-8 from the child reaches the emulator byte-identical. |
| NFR-SPAWN | Shipped GUI binary: no `posix_spawn` / `forkpty` / `openpty` used to start the user shell. Link-level test, not a source grep. |
| NFR-FAIL | Library and daemon paths return `Result`. No `unwrap` on reachable request handling. |
| NFR-ISOLATE | Malformed, unauthorized or stalled clients and workers are isolated; they do not terminate the runtime or block unrelated panes. |
| NFR-BOUND | Client queues, event ledgers, checkpoints, timeline materialization, images and retained history have explicit independent bounds. |
| NFR-STATE | Checkpoint/delta reconstruction matches continuous host VT state at the same offset; a mismatch fails closed and resyncs. |
| NFR-CAPTURE | Capture and retention obey the most restrictive applicable policy. Encryption or redaction is not authority to collect. |
| NFR-PRIVACY | Sensitive terminal, command, transcript, clipboard, agent and diagnostic data is minimized, policy-gated, encrypted when stored/transferred, and isolated across users, hosts, Sessions, clients, agents and external services. Logs, telemetry, crash reports, config, backup and sync exclude prohibited content and credentials. |

## 6. Input and content routing

Raw terminal/TUI mode routes encoded input to the leased TerminalExecution.
Structured editor submissions create explicit ContentTimeline or
Conversation/Task events. No PATH, prompt-regex, English/code or cursor-position
heuristic chooses the route. A scheduler may sit above explicit submissions; it
does not replace terminal input.

## 7. Success / stop

Spike 0 is **Proven** only when every named test in [SPIKE-0](SPIKE-0.md) has
been demonstrated **red and then green** on a packaged build
([ADR 0002](adr/0002-falsifiable-evidence.md) D2), including NFR-KEY on battery.

A green test that was never shown to fail is not evidence. The 2026-08-16 run
that reported eight of nine gates Proven is withdrawn; see
[SPIKE-0-AUDIT](SPIKE-0-AUDIT.md).

If a later NFR-KEY run misses: **stop that surface.** Do not add agents to hide
the miss, and do not re-cut the instrument to flatter it.
