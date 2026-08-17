# SPEC-ATTACH — attach plane (`lane:attach`)

- **Status:** Accepted for Spike 0 Proven clauses — 2026-08-17
  ([ADR 0010](../adr/0010-spike-0-closes.md)). Written 2026-08-16 as the
  remediation draft.
- **Authority:** [ADR 0001](../adr/0001-session-operating-system.md) §2, §5, §6
- **Crate:** `crates/rill-attach`
- **Gates:** T-ATTACH, T-EXIT, T-RESIZE, T-DROP, T-NFR (control-RPC clause) —
  **Proven**

## 1. Transport

- Darwin has no `SOCK_SEQPACKET` (errno 43). The transport is a framed
  `SOCK_STREAM` over `AF_UNIX`.
- `SOCK_SEQPACKET` MUST NOT appear in the tree.
- The socket MUST be created with mode `0600` in a directory the invoking user
  owns. A world-writable socket path is a shell-execution vector.

## 2. Frame format

```
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
| `ATTACH` | 5 | GUI → kernel | `generation u64`; optional `session_id u64` (16-byte payload). 8 bytes is Spike 0: default leaf. |
| `REFUSED` | 6 | kernel → GUI | `u8` reason |

Reasons: `1 AlreadyAttached`, `2 Invalid`.

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

- `CREDIT` is a **window the client replenishes as it consumes**, not a
  one-shot grant (audit S3-5).
- The GUI MUST NOT open with `Credit(u32::MAX)`. Initial window is 256 KiB;
  the client grants `n` further bytes only after it has fed `n` bytes to the
  chip.
- The kernel MUST NOT read more than the outstanding credit.

## 6. Attach identity

- Exactly one attach per session. A second `ATTACH` while attached MUST receive
  `REFUSED{AlreadyAttached}` and MUST NOT disturb the first client.
- A connection that has not sent `ATTACH` holds no claim, but MUST NOT be able
  to displace an attached client by connecting (audit S3-6). The daemon tracks
  connections and the attach claim separately; the attached connection is
  replaced only when it closes.
- 8-byte ATTACH (generation only) MUST attach the default leaf. 16-byte ATTACH
  MUST name a `SessionId`. Unknown id → `REFUSED{Invalid}` on that connection.
  A second connection MAY attach a different live id (ADR 0011 D3).
- `generation` is opaque to the kernel in Spike 0 and reserved for reconnect
  tokens under a later ADR.

## 7. What the attach plane may and may not do

- MAY classify the byte stream: alt-screen entry/exit, OSC 52 (denied until a
  policy UI exists), OSC 9, OSC 133, title. Classification is journalled.
  **Later — not Spike 0 Proven.** No classifier ships today; that is not a
  gate miss.
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

## 9. Fuzzing

`cargo-fuzz` target over `Decoder::push` with arbitrary byte splits. Must
survive: truncated headers, declared lengths at and beyond `MAX_FRAME`, unknown
tags, payload lengths that disagree with the tag's fixed size, and interleaved
partial frames. No panic, no unbounded allocation. Runs in `fast.yml` for a
bounded corpus and nightly for longer.
