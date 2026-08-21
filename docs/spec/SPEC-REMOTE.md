# SPEC-REMOTE — remote runtimes, SSH compatibility and mobile clients

- **Status:** Red. Specification only; no implementation is authorized.
- **Authority:** [ADR 0041](../adr/0041-remote-is-a-second-kernel.md), amended
  by [ADR 0053](../adr/0053-runtime-domain-content-and-client-authority.md)
  D5–D6, D10–D11 and D13.
- **Requires:** [SPEC-ATTACH](SPEC-ATTACH.md),
  [SPEC-RUNTIME-SUPERVISION](SPEC-RUNTIME-SUPERVISION.md),
  [SPEC-CLIENT-AUTHORITY](SPEC-CLIENT-AUTHORITY.md),
  [SPEC-DOMAIN-LIFECYCLE](SPEC-DOMAIN-LIFECYCLE.md).
- **Lane:** `lane:kernel` for runtime/transport and native client lanes for
  presentation.

## 1. Process-host authority

For a local Mac, another user-owned Mac/Linux machine, or a user-owned VPS/VM,
the RILL runtime on the machine where the PTY and process run is authoritative.
It owns canonical terminal state, checkpoints, transcript, graph, tasks and
leases. A client does not become authority by being local to the display.

There is one versioned RILL product protocol across local and remote transports.
Transport-specific setup does not create a second domain, resync or display
model. PTY bytes never pass through a RILL-hosted third-party relay under this
authority.

## 2. RILL-installed remote hosts

Local Unix transport lands first. SSH may carry an explicitly selected RILL
protocol stream to an installed runtime. A later direct mutually authenticated
transport remains behind the same boundary and requires a separate threat
model, device-pairing/revocation contract and spike.

Before attach, the client verifies host/device identity, runtime protocol,
checkpoint version and granted role. Identity changes and unsupported versions
fail closed with both sides named; no silent downgrade is permitted.

Reconnect attaches the same TerminalExecutionId, initializes a disposable VT
mirror from a compatible host checkpoint, applies ordered deltas and verifies
offset/hash. Missing data is a visible Discontinuity. Offline input is never
buffered for later injection.

## 3. Zero-footprint SSH

Zero-footprint SSH is a compatibility terminal, not a RILL remote runtime. RILL
invokes only the SSH session and remote shell/command the user requested. The
remote shell keeps its existing profiles, prompt, theme, plugins, aliases,
completion, ANSI behavior and interactive semantics. It MUST NOT:

- probe whether RILL or another helper is installed;
- upload, install, bootstrap or execute a helper;
- modify shell profiles, startup files or remote configuration;
- inspect remote terminal/session/history state; or
- execute hidden remote commands before, during or after the session.

Zero-footprint is the default for a host without an explicitly approved RILL
runtime plan. Selecting zsh, fish, bash or another PTY-compatible remote shell
does not opt into bootstrap or shell integration.

The UI and API expose a capability downgrade before connect: no RILL-owned
remote process persistence, canonical transcript, rich content, checkpoint
reconnect or multi-client lease semantics. Ordinary raw SSH/tmux behavior
inside the requested session remains compatible.

## 4. Optional enhanced bootstrap

Enhanced bootstrap is a separate explicit action. It runs only when local and
remote policy permit. Before execution it presents:

- every remote command;
- artifact identity, size, source and destination;
- required permissions and expected lifetime;
- runtime/protocol compatibility; and
- cleanup plan and known residue risks.

Consent is bound to that unchanged plan. A changed plan requires new consent.
Cleanup is best effort. RILL reports and journals success, residue or
unverifiable cleanup; it never promises that remote artifacts were removed.
Enhanced bootstrap MUST NOT modify profiles unless a later separately approved
feature names that exact change.

## 5. Multi-client and geometry

Every remote client follows SPEC-CLIENT-AUTHORITY. One ClientId holds the
input/resize lease; observers cannot write, resize or affect another client's
credit. The lease owner determines canonical PTY geometry. Other clients crop,
pan or letterbox the live terminal and may reflow immutable structured content.

Transport loss starts lease grace and detaches presentation. It does not
terminate Session or TerminalExecution. A takeover is explicit, atomic and
attributed to all clients.

## 6. Mobile

iPhone/iPad attaches as a client to an existing awake, online and reachable
runtime. Mobile v1 prioritizes viewing, attention, approvals, questions,
diff/change review and deliberate input-lease takeover. Backgrounding,
suspension or network loss releases presentation/lease according to policy and
never expresses process-termination intent.

Mobile does not own a Mac/Linux/VPS PTY, maintain an authoritative VT, or queue
offline keystrokes. Its local VT mirror is disposable and uses the same
checkpoint/reconciliation contract as desktop.

## 7. Measurement and conveniences

NFR-KEY is a local terminal-path gate. Remote latency is reported separately
with transport RTT and MUST NOT be presented as NFR-KEY.

Remote attention, file transfer, port forwarding and browsing are explicit
capabilities attributed to verified host identity. File destination and tunnel
ports are shown and confirmed. Mentioning a path or port in terminal output
never triggers an action.

## 8. Gates

| ID | Status | Closes |
|---|---|---|
| T-REM-HOST-AUTHORITY | Red | §§1–2 |
| T-REM-IDENTITY-VERSION | Red | §2 |
| T-REM-CHECKPOINT-RECONNECT | Red | §2 |
| T-SSH-ZERO-FOOTPRINT | Red | §3 |
| T-SSH-SHELL-UNCHANGED | Red | §3 |
| T-SSH-ENHANCED-PLAN-CLEANUP | Red | §4 |
| T-REM-OBSERVER-LEASE | Red | §5 |
| T-MOBILE-BACKGROUND-DETACH | Red | §6 |
| T-REM-NFR-SEPARATE | Red | §7 |

## 9. Out of scope

A RILL account, hosted control plane, relay, bundled SSH implementation, direct
transport selection, invisible helper installation and foreign tmux adoption
as native panes. Nested tmux remains supported as an ordinary child.

## 10. What we will not do

- Claim SSH alone provides RILL persistence or transcript semantics.
- Probe or bootstrap in zero-footprint mode.
- Promise enhanced-bootstrap cleanup.
- Stream host-rendered per-frame cells or JSON.
- Trust a changed host identity or silently downgrade protocol.
- Buffer input while disconnected.
