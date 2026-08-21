# ADR 0041: Remote is a second kernel, not a second display path

- **Status:** Accepted — 2026-08-18
- **Amended by:** [ADR 0053](0053-runtime-domain-content-and-client-authority.md)
  D5–D6 and D10–D11 for host authority, disposable client mirrors, leases,
  mobile semantics and the two distinct SSH paths.
- **Historical identifier:** merged as ADR 0023 in PR #278; renumbered to ADR
  0041 on 2026-08-21 to resolve a collision. Renumbering changed no decision.
- **Tree:** this repository only
- **Issue:** catalog epic [#33](https://github.com/mahboobmonnamd/RILL/issues/33).
  Rows authorized by this ADR:
  F-170 [#194](https://github.com/mahboobmonnamd/RILL/issues/194), F-171
  [#195](https://github.com/mahboobmonnamd/RILL/issues/195), F-172
  [#196](https://github.com/mahboobmonnamd/RILL/issues/196), F-173
  [#197](https://github.com/mahboobmonnamd/RILL/issues/197), F-174
  [#198](https://github.com/mahboobmonnamd/RILL/issues/198), F-175
  [#199](https://github.com/mahboobmonnamd/RILL/issues/199), F-176
  [#200](https://github.com/mahboobmonnamd/RILL/issues/200), F-177
  [#201](https://github.com/mahboobmonnamd/RILL/issues/201), F-178
  [#202](https://github.com/mahboobmonnamd/RILL/issues/202), F-179
  [#203](https://github.com/mahboobmonnamd/RILL/issues/203), F-180
  [#204](https://github.com/mahboobmonnamd/RILL/issues/204).
- **Requires:** [ADR 0001](0001-session-operating-system.md),
  [ADR 0011](0011-session-graph.md), [ADR 0015](0015-m1-persist-remainder.md),
  [ADR 0038](0038-session-graph-navigation-model.md) (host indicator, F-001)
- **Amends:** nothing. NFR-KEY remains a **local** measurement (PRD §5).
- **Does not authorize:** an account or control plane, cloud relay of PTY bytes,
  agents over SSH (ADR 0049), remote Blocks, shipping a bundled `ssh`, tunnels
  opened without confirmation, weakening host-key verification.

## Context

Eleven rows describe remote work: runtime on the host (F-170), SSH attach
(F-171), thin local client (F-172), ssh-then-attach (F-173), reconnect with
backoff (F-174), notify relay (F-175), remote cwd and scp drop (F-176), browser
via remote network (F-177), Mosh (F-178), remote tmux mirror (F-179), host key
and version check (F-180).

RILL's architecture already answers most of this, because the kernel is already
a separate process reached over a framed `SOCK_STREAM` (FR-ATTACH). A remote
session is that same protocol over a different transport. The wrong answer —
the one every terminal that "supports remote" eventually regrets — is a second
display path: bytes marshalled differently, a second resync, a second VT, and an
NFR that only holds on localhost.

The `lane:` on all eleven rows is `lane:kernel`. That is correct and this ADR
keeps it there.

## Decision

### D1 — A remote host is another kernel speaking the same protocol

F-170, F-171, F-173. Remote is `rilld` running on the remote machine, reached by
the **same** frame codec (`crates/rill-attach`) over an SSH-forwarded stream.

There MUST be exactly one attach protocol. There MUST NOT be a remote-specific
frame, a remote-specific resync, or a remote-specific Chip 0. Chip 0 runs
**locally**, fed by bytes that arrived over the transport, exactly as it is fed
by bytes from a local socket.

Processes and credentials stay on the remote machine (F-170). The local client
MUST NOT ship the user's keys anywhere, and MUST NOT proxy PTY bytes through any
third party. There is no cloud in this path.

Mutation `remote_uses_second_codec` MUST turn T-REM-CODEC red.

### D2 — The transport is pluggable; the plane boundary is not

SSH (F-171) and Mosh (F-178) are transports under one `Transport` trait: framed,
ordered, reliable-or-explicitly-broken. Mosh gives roaming; SSH remains the
control and authentication channel (F-178's own note). A transport MUST NOT
reinterpret frames, MUST NOT resend `DATA` it has already delivered, and MUST
fail closed on an ambiguous state (PRD NFR-FAIL).

The thin local client (F-172) is the local host process with a remote transport:
keybindings, clipboard and chrome are local; the leaf is remote. It MUST NOT
become a second UI codebase.

### D3 — NFR-KEY is a local gate and remote MUST NOT be measured against it

PRD's NFR-KEY is key-down to `presentedTime` on a packaged local app. A remote
session crosses a network; that budget is not achievable and pretending
otherwise would corrupt the instrument (ADR 0002, and the withdrawn-run lesson
in SPIKE-0-AUDIT).

Remote therefore gets its **own** named budget, measured and reported
separately: echo latency to the local drawable, with the transport RTT reported
alongside so a slow link is visibly a slow link and not a regression in Chip 0.

`--nfr-key` MUST refuse to run against a remote leaf. A remote NFR-KEY number
MUST NOT be cited as NFR-KEY. Mutation `nfr_accepts_remote` MUST turn
T-REM-NFR red.

### D4 — Identity is verified before attach, and a change stops the session

F-180. Host key verification MUST happen before any byte is written to the
remote and before any credential is offered. A changed host key MUST stop and
require explicit user action. It MUST NOT be a dismissible toast, and there MUST
NOT be an "always trust" default.

Protocol version mismatch between local client and remote `rilld` MUST fail
closed with both versions named, not negotiate down silently.

Named test `t_changed_host_key_blocks_attach`. Mutation
`host_key_change_warns_only` MUST turn T-REM-HOSTKEY red.

### D5 — Reconnect replays from the ring; it never fabricates

F-174. On a dropped transport the client reconnects with bounded exponential
backoff and re-attaches the same `SessionId`. The remote kernel's ring is the
source of replay (FR-HISTORY): the client resyncs cold, once, like any attach
(FR-RESYNC).

If the gap exceeds what the ring holds, the client MUST show a discontinuity. It
MUST NOT silently present a partial scrollback as continuous. A dead pane must
not look alive (FR-EXIT); a lossy pane must not look lossless.

While disconnected the pane MUST render as disconnected and MUST NOT accept
input into a buffer it will later flush blind.

### D6 — Refused: adopting a foreign tmux as native panes

F-179 asks to attach an existing tmux server and present its panes as RILL
panes. **Rejected.** A mirrored tmux pane is not a leaf the kernel owns: no
`SessionId`, no ring, no sole-writer guarantee, no `terminate`. Presenting it in
the same tree as real leaves would make every guarantee in ADR 0011 conditional.

Running tmux *inside* a leaf is supported and MUST keep working (ADR 0039 D6).
That is the supported path and it is enough.

This row closes as **wontfix** rather than staying blocked forever.

### D7 — Remote conveniences are explicit, confirmed, and scoped

- **Notify relay (F-175):** remote attention events travel as orchestration on
  the same connection and land in the local queue (ADR 0047 D1). They carry the
  host identity so a notification cannot impersonate the local machine.
- **cwd / scp drop (F-176):** drag-to-remote is an explicit file transfer with a
  visible destination path and a confirmation. It MUST NOT overwrite without
  asking. Remote cwd reads the remote kernel's cold tap (ADR 0013).
- **Browser via remote network (F-177):** requires an **explicit** SSH tunnel
  the user confirms, scoped to a named port. RILL MUST NOT open tunnels
  implicitly because a pane mentioned `localhost:3000`. The embedded browser
  itself is ADR 0042 D2.

### D8 — Oracle

| ID | Closes |
|---|---|
| T-REM-CODEC | D1 — one codec, one Chip 0, no remote-only frame |
| T-REM-NFR | D3 — `--nfr-key` refuses remote |
| T-REM-HOSTKEY | D4 — changed key blocks; version mismatch fails closed |
| T-REM-RECONNECT | D5 — replay from ring; visible discontinuity |
| T-REM-TUNNEL | D7 — no implicit tunnel |

## Consequences

- [SPEC-REMOTE](../spec/SPEC-REMOTE.md) is the transport and identity contract.
- SPEC-ATTACH gains the transport abstraction; the frame codec is unchanged.
- F-001's host indicator (ADR 0038 D6) becomes meaningful here and MUST show the
  verified remote identity, never a user-supplied label.
- F-179 is closed `wontfix`; the other ten proceed.

## Rejected alternatives

- **Marshal cells or JSON over the wire so the remote can be thin.** Rejected:
  ADR 0001, FR-CHIP0. Bytes travel; cells never do.
- **Run Chip 0 on the remote and stream frames.** Rejected: that is the remote
  desktop the PRD promises this is not.
- **Relay through a hosted service for NAT traversal.** Rejected: D1. No third
  party on the PTY path, and no account (ADR 0044 D5).
- **TOFU host keys with an "always trust" default.** Rejected: D4.
- **Report remote latency as NFR-KEY.** Rejected: D3, ADR 0002.
- **Mirror a foreign tmux.** Rejected: D6.
