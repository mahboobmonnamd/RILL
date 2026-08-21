# SPEC-ATTACH — attach plane (`lane:attach`)

- **Status:** Accepted for Spike 0 Proven clauses — 2026-08-17
  ([ADR 0010](../adr/0010-spike-0-closes.md)). Written 2026-08-16 as the
  remediation draft.
- **Authority:** [ADR 0001](../adr/0001-session-operating-system.md) §2, §5, §6,
  [ADR 0015](../adr/0015-m1-persist-remainder.md) (protocol + observe), amended
  for future versions by
  [ADR 0053](../adr/0053-runtime-domain-content-and-client-authority.md)
- **Future contract:** [SPEC-CLIENT-AUTHORITY](SPEC-CLIENT-AUTHORITY.md) and
  [SPEC-TERMINAL-PERFORMANCE](SPEC-TERMINAL-PERFORMANCE.md). The existing
  protocol-1 clauses remain historical evidence; they do not prove roles,
  independent observer flow control, leases, checkpoints or semantic-channel
  isolation.
- **Crate:** `crates/rill-attach`
- **Gates:** T-ATTACH, T-EXIT, T-RESIZE, T-DROP, T-NFR (control-RPC clause) —
  **Proven**. T-ATTACH-PROTO, T-GRAPH-OBSERVE — persist remainder.

## 1. Transport

- Darwin has no `SOCK_SEQPACKET` (errno 43). The transport is a framed
  `SOCK_STREAM` over `AF_UNIX`.
- `SOCK_SEQPACKET` MUST NOT appear in the tree.
- The socket MUST be created with mode `0600` in a protected runtime directory
  the invoking user owns. A predictable endpoint directly under shared `/tmp`
  does not satisfy this requirement. Local peer credentials MUST match before
  ATTACH is processed.

## 2. Frame format

```text
+--------+------------------+------------------+
| tag u8 | len u32 (LE)     | payload (len)    |
+--------+------------------+------------------+
```

- `MAX_FRAME` is 4 MiB. A declared length above it is a protocol error and the
  connection MUST be closed — not skipped, not clamped.
- Not JSON. No cells. No per-cell `String`.

| Tag | Value | Direction | Payload |
|---|---|---|---|
| `DATA` | 1 | both | raw bytes |
| `CREDIT` | 2 | GUI → kernel | `u32` bytes granted |
| `RESIZE` | 3 | GUI → kernel | `cols u16, rows u16, px_w u16, px_h u16` |
| `EXIT` | 4 | kernel → GUI | `i32` raw wait status |
| `ATTACH` | 5 | GUI → kernel | `generation u64`; optional `session_id u64`; optional `protocol u8` + `flags u8` (18-byte). 8 bytes is Spike 0: default leaf. |
| `REFUSED` | 6 | kernel → GUI | `u8` reason |
| `CHECKPOINT` | 7 | kernel → GUI | `ending_offset u64`, `hash u64`, opaque blob |
| `DELTA` | 8 | kernel → GUI | `start_offset u64`, raw bytes |
| `RESYNC_REQUEST` | 9 | GUI → kernel | empty |

`CHECKPOINT`, `DELTA`, and `RESYNC_REQUEST` are cold. They MUST NOT be
classified as warm-path frames ([#314](https://github.com/mahboobmonnamd/RILL/issues/314)).

Reasons: `1 AlreadyAttached`, `2 Invalid`, `3 ProtocolMismatch` (ADR 0015 D1).

ATTACH payloads: 8 bytes generation (Spike 0, protocol 1 implied); 16 bytes
generation + session_id; 18 bytes generation + session_id + protocol u8 +
flags u8 (bit 0 = observe). Packaged host still sends 8-byte.

## 3. Decoder

- MUST be incremental and MUST tolerate arbitrary read boundaries, including a
  split inside the 5-byte header. This is the hazard `SOCK_STREAM` introduces
  over seqpacket and it is the decoder's whole reason to exist.
- MUST NOT retain unbounded buffered input: if the accumulated buffer exceeds
  `MAX_FRAME + 5` without yielding a frame, that is a protocol error.
- `Decoder::push` MUST NOT allocate per frame beyond the payload itself.
- Unknown tags are a protocol error (fail closed), not a skip.

## 4. Ordering

- `DATA` and `RESIZE` from the GUI are ordered by stream position and MUST be
  applied by the kernel in that order (SPEC-KERNEL §5, §7).
- The kernel MUST NOT reorder outbound `DATA`. `EXIT` MUST NOT overtake `DATA`
  already queued for the same client.

## 5. Credit

- Protocol 1 `CREDIT` is the sole attached client's replenished delivery window,
  not a one-shot grant (audit S3-5). The proven implementation couples its PTY
  read to that window.
- The protocol-1 GUI MUST NOT open with `Credit(u32::MAX)`. Initial window is
  256 KiB; the client grants `n` further bytes only after it has fed `n` bytes
  to the chip.
- Protocol 2 separates worker PTY drain/recovery bounds from per-ClientId
  delivery credit. Each client's credit governs only that client's outbound
  stream. A stalled observer or controller may lose deltas and resync; it cannot
  stall the worker's bounded PTY drain, another client or another pane.
- Semantic/content channel credit and queue capacity are separate from terminal
  DATA, checkpoint/delta and input credit. Semantic overflow produces an honest
  offset-correlated degradation record; it never pauses PTY read or
  drops/reorders terminal DATA.

## 6. Attach identity

- Protocol 1 permits exactly one **writer** attach per legacy execution. A
  second writer `ATTACH` MUST
  receive `REFUSED{AlreadyAttached}` and MUST NOT disturb the first client.
  A second connection MAY `ATTACH` with observe (flags bit 0) and MUST NOT
  write the PTY (ADR 0015 D7).
- A connection that has not sent `ATTACH` holds no claim, but MUST NOT be able
  to displace an attached client by connecting (audit S3-6). The daemon tracks
  connections and the attach claim separately; the attached connection is
  replaced only when it closes.
- A connection without a completed ATTACH MUST NOT route any pane-directed
  frame to a default execution. Such a frame closes only that connection.
- 8-byte ATTACH (generation only) MUST attach the default leaf. 16-byte ATTACH
  MUST name a `SessionId`. 18-byte ATTACH carries protocol and flags. Protocol
  other than 1 → `REFUSED{ProtocolMismatch}`. Unknown id → `REFUSED{Invalid}`
  on that connection. A second connection MAY attach a different live id
  (ADR 0011 D3) or observe the same id (ADR 0015 D7).
- `generation` is opaque to the kernel in Spike 0 and reserved for reconnect
  tokens under a later ADR.

Protocol 2 or later replaces writer/observe flags with authenticated ClientId,
role, capabilities, independent credit, explicit TerminalExecutionId and an
input/resize lease generation. It adds versioned checkpoint, delta-offset,
state-hash and lease events without putting cells, JSON or per-cell strings on
the warm path. Protocol 1 MUST NOT be silently interpreted as protocol 2.

## 7. What the attach plane may and may not do

- MAY classify the byte stream: alt-screen entry/exit, OSC 52 (denied until a
  policy UI exists), OSC 9, OSC 133, OSC 7, title. Classification is journalled.
  **Later — not Spike 0 Proven.** No classifier ships today; that is not a
  gate miss. OSC 7 is not the cwd source of truth
  ([ADR 0013](../adr/0013-cwd-tap.md)).
- MUST NOT build a grid the GUI consumes.
- MUST NOT carry cells, JSON, or any structure derived from parsing into a
  screen model.
- MUST NOT pass file descriptors.

## 8. Control-RPC prohibition

The warm path is `DATA` and `CREDIT` only. During a T-NFR measurement window
(ADR 0003 D9):

- frames sent by the client: `DATA`, `CREDIT` only;
- frames received: `DATA` only;
- no file descriptor other than the attach socket is written.

`Frame::is_control_rpc` — which returned a hardcoded `false` — is deleted
(audit S1-3). The property is asserted at the client's single `send`
chokepoint, not by inspecting bytes for JSON substrings.

Protocol 2 preserves the same prohibition across typed channels: terminal DATA,
credit needed by the terminal channel and the measured input/present path do
not wait for topology, semantic, content, Task, attention, artifact, policy or
control-plane serialization. A shared blocking outbound queue violates
T-PROTOCOL-SEMANTIC-INDEPENDENCE and T-PERF-PTY-DRAIN-INDEPENDENT.

## 9. Fuzzing

`cargo-fuzz` target over `Decoder::push` with arbitrary byte splits. Must
survive: truncated headers, declared lengths at and beyond `MAX_FRAME`, unknown
tags, payload lengths that disagree with the tag's fixed size, and interleaved
partial frames. No panic, no unbounded allocation. Runs in `fast.yml` for a
bounded corpus and nightly for longer.

## 10. Future performance gates

T-CLIENT-CREDIT-ISOLATION, T-PROTOCOL-SEMANTIC-INDEPENDENCE,
T-PROTOCOL-SLOW-CLIENT-CHANNEL-ISOLATION,
T-PERF-PTY-DRAIN-INDEPENDENT, T-PERF-CLIENT-ISOLATION and
T-PERF-BYTE-FIDELITY are all mandatory for Protocol 2 product acceptance. They
supplement rather than replace protocol-1 T-ATTACH/T-DROP/T-NFR evidence.
