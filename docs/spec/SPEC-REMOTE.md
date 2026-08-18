# SPEC-REMOTE — remote kernels and transports (`lane:kernel`)

- **Status:** Accepted — 2026-08-18. Gates **Red** until demonstrated
  red-then-green (ADR 0002 D2).
- **Authority:** [ADR 0023](../adr/0023-remote-is-a-second-kernel.md)
- **Requires:** [SPEC-ATTACH](SPEC-ATTACH.md), [SPEC-KERNEL](SPEC-KERNEL.md),
  [SPEC-GRAPH](SPEC-GRAPH.md), [SPEC-CWD](SPEC-CWD.md)
- **Crates:** `crates/rill-attach`, `crates/rill-kernel`, `crates/rilld`
- **Milestone:** M2 — Chrome

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. One protocol

- A remote host is `rilld` on that machine, reached by the **same** frame codec
  over a transport.
- There MUST be exactly one attach protocol. There MUST NOT be a remote-specific
  frame, resync, or Chip 0.
- Chip 0 runs **locally**, fed bytes that arrived over the transport.
- Processes and credentials MUST stay on the remote machine. The client MUST NOT
  ship keys anywhere and MUST NOT proxy PTY bytes through a third party.

## 2. Transports

- SSH and Mosh implement one `Transport` abstraction: framed, ordered,
  reliable-or-explicitly-broken.
- A transport MUST NOT reinterpret frames, MUST NOT redeliver `DATA` it already
  delivered, and MUST fail closed on ambiguous state (PRD NFR-FAIL).
- SSH remains the control and authentication channel when Mosh carries the PTY.
- The thin client is the local host process with a remote transport: local
  keybindings, local clipboard, remote leaf. It MUST NOT be a second UI.

## 3. Measurement

- `--nfr-key` MUST refuse to run against a remote leaf.
- A remote latency number MUST NOT be reported as NFR-KEY.
- Remote echo latency is its own named budget, reported with transport RTT
  alongside.

## 4. Identity

- Host key verification MUST complete before any byte is written to the remote
  and before any credential is offered.
- A changed host key MUST stop and require explicit user action. It MUST NOT be
  a dismissible toast. There MUST NOT be an "always trust" default.
- Protocol version mismatch MUST fail closed with both versions named. It MUST
  NOT negotiate down silently.
- The host indicator (SPEC-NAV §6) MUST show the verified identity.

## 5. Reconnect

- Reconnect MUST use bounded exponential backoff and MUST re-attach the same
  `SessionId`.
- Replay comes from the remote kernel's ring (FR-HISTORY), resynced cold, once,
  like any attach (FR-RESYNC).
- A gap larger than the ring MUST render as a visible discontinuity. Partial
  scrollback MUST NOT be presented as continuous.
- While disconnected the pane MUST render disconnected and MUST NOT buffer input
  for a blind flush.

## 6. Conveniences

- Remote attention events travel as orchestration on the same connection, carry
  the verified host identity, and MUST NOT claim to be local
  ([SPEC-ATTENTION](SPEC-ATTENTION.md) §4).
- File drop MUST show the destination path and confirm. It MUST NOT overwrite
  without asking. Remote cwd reads the remote cold tap.
- A tunnel for remote browsing MUST be explicit, confirmed, and scoped to a
  named port. RILL MUST NOT open a tunnel because a pane mentioned a port.

## 7. Refused

- Adopting a foreign tmux server as native panes. A mirrored pane has no
  `SessionId`, no ring, no sole-writer guarantee, and no `terminate`.
- Running tmux **inside** a leaf remains supported (SPEC-NAV §10).

## 8. Gates

| ID | Status | Closes |
|---|---|---|
| T-REM-CODEC | Red | §1 |
| T-REM-NFR | Red | §3 |
| T-REM-HOSTKEY | Red | §4 |
| T-REM-RECONNECT | Red | §5 |
| T-REM-TUNNEL | Red | §6 |

## 9. Out of scope

An account, a control plane, cloud relay of PTY bytes, agents over SSH, remote
Blocks, bundling `ssh`.

## 10. What we will not do

- Marshal cells or JSON over the wire.
- Run Chip 0 on the remote and stream frames.
- Report remote latency as NFR-KEY.
- Default to trusting a changed host key.
