# ADR 0022: Chip 1 answers DA and DSR through a bounded reply buffer

- **Status:** Accepted — 2026-08-18
- **Tree:** this repository only
- **Issue:** epic [#6](https://github.com/mahboobmonnamd/RILL/issues/6)
  (child issue to be filed: T-CHIP1-REPLY)
- **Requires:** [ADR 0012](0012-chip1-isolated-vt.md),
  [ADR 0020](0020-chip1-parser-in-tree.md)
- **Amends:** [SPEC-CHIP1](../spec/SPEC-CHIP1.md) §2 — adds one inherent method
  to Chip 1. The `TerminalEmulation` **trait** is unchanged, so Chip 0 is not
  touched and ADR 0012 D2's "same I/O shape" still holds for the trait.
- **Does not authorize:** Chip 1 owning a PTY or a socket, writing to any fd,
  Chip 1 as the live chip, a warm-path control RPC, mouse reporting, key
  encoding

## Context

[SPEC-CHIP1](../spec/SPEC-CHIP1.md) §3 says: "DA / DSR `6n`: MUST answer. A TUI
that hangs on DA is a v0 miss." The §2 trait is `feed`, `resize`, `snapshot`,
returning `Result<(), Error>` or a grid.

S-VT found that **there is no channel on which an answer can leave the crate**
([SPIKE-VT](../SPIKE-VT.md) Result 7). The contract as written requires a reply
it makes structurally impossible. A `vim` that queries primary DA and waits
would hang against a conforming implementation. Found before any code, so it
costs a paragraph rather than a refactor.

Chip 1 must not own the write side. It has no PTY and no socket (ADR 0012 D1),
and the kernel is the sole writer to the PTY master (ADR 0001 §5). So the reply
cannot be sent by the chip; it can only be *made available* to whoever owns the
write path.

## Decision

### D1 — Replies accumulate in the chip and the caller drains them

```rust
impl VtEngine {
    /// Bytes the terminal owes the program on the other side of the PTY.
    /// Drains: a second call returns empty unless more arrived.
    pub fn take_replies(&mut self) -> Result<Vec<u8>, Error>;
    pub fn has_replies(&self) -> bool;
}
```

Inherent methods on the Chip 1 type, not `TerminalEmulation`. The trait keeps
the shape Chip 0 already implements (ADR 0012 D2); Chip 0 is not modified by
this ADR and libghostty-vt keeps handling its own queries.

`feed` MUST NOT block, MUST NOT write to any file descriptor, and MUST NOT call
back into caller code. Producing a reply is appending to an internal buffer.

### D2 — v0 answers exactly three queries

| Query | Reply |
|---|---|
| `CSI c` / `CSI 0 c` (primary DA) | `CSI ? 6 c` — VT102 class, matching the v0 subset we actually implement |
| `CSI > c` (secondary DA) | `CSI > 0 ; 0 ; 0 c` |
| `CSI 6 n` (DSR cursor position) | `CSI <row> ; <col> R`, 1-based, current cursor |

`CSI 5 n` (device status) MAY answer `CSI 0 n`. Every other query is consumed
and **not** answered. We MUST NOT claim a capability we do not implement: no
DECRPM for modes we ignore, no `XTVERSION`, no DA reply advertising sixel or
colour capabilities we do not have.

DSR reports the position `snapshot()` reports (`cursor_row` / `cursor_col`,
1-based in the reply). It MUST NOT pre-resolve a pending wrap
([SPEC-VT-SCREEN](../spec/SPEC-VT-SCREEN.md) §2).

Normative detail: [SPEC-VT-REPLY](../spec/SPEC-VT-REPLY.md).

### D3 — The reply buffer is bounded and fails closed

Capacity is fixed (SPEC-VT-REPLY §4). A program that spams `CSI 6 n` without
reading MUST NOT grow the buffer without limit — that is a remote memory
exhaustion path, and the bytes arrive from whatever is running in the shell.

On overflow the chip **drops new replies and sets a counter**
(`replies_dropped`), reported like `grapheme_truncated`. It MUST NOT panic,
reallocate without bound, or discard the buffer contents. `take_replies`
returning `Err` MUST leave the buffer intact so the caller can retry.

A dropped reply is a real (if rare) hang risk for the program; it is counted so
it is visible rather than mysterious.

### D4 — Routing is the host's job at M7, and stays off the warm path shape

Chip 1 in M4 is a library: nothing drains it in production because nothing
constructs it in production (ADR 0012 D1). At M7 the host drains after `feed`
and sends the bytes as ordinary attach `DATA` frames — the same path as a
keystroke, no new frame tag, no JSON, no control RPC. T-NFR's zero-control-RPC
clause (ADR 0003 D9) is therefore untouched, and SPEC-ATTACH does not change.

`rilld`'s cold resync path (`resync_from_history`) MUST discard replies: history
replay is not a live program and must not inject bytes toward the PTY. The chip
counts them as dropped rather than pretending they were sent.

### D5 — Named gate

**T-CHIP1-REPLY** — feed `CSI 6 n` after `CSI 5 ; 3 H`; `take_replies()` yields
`CSI 5 ; 3 R`. Feed `CSI c`; the reply is `CSI ? 6 c`. A second
`take_replies()` is empty. Oracle is the drained bytes, parsed — not a constant
the test prepended, and not a flag saying a reply was queued.

**Required mutation.** Never enqueue a reply (`take_replies` always empty). The
gate goes red. A second mutation, unbounded reply buffer, must turn the
overflow-counter assertion red.

Feeding the reply bytes back into a second instance is **not** the oracle for
this gate: replies travel toward the program, not the screen.

## Consequences

- SPEC-CHIP1 §3's DA/DSR MUST becomes satisfiable.
- One more inherent method to keep in step with Chip 0 at M7, where Chip 0's
  equivalent is inside libghostty-vt. The M7 ADR must say how the host drains
  it; that is named work, not a surprise.
- `vim` and `htop` against Chip 1 stop being a coin flip on whether they probe.

## Rejected alternatives

- **`feed` returns the reply bytes.** Rejected: it diverges the trait Chip 0
  implements (ADR 0012 D2) and allocates on the hot path for the common case
  where there is no reply.
- **A callback or writer handed to the chip.** Rejected: it puts a write path
  inside a crate forbidden to own one, and invites the chip to touch an fd.
- **Drop the DA/DSR MUST from v0.** Rejected: the goal is `less`, `vim` and
  alt-screen TUIs (ADR 0012 D5), and those are exactly the programs that probe.
  A TUI hanging on DA is the miss SPEC-CHIP1 already named.
- **Answer every query xterm answers.** Rejected: advertising capabilities we
  have not implemented is worse than silence, and sixel/colour claims would be
  lies in v0.
- **Unbounded reply buffer.** Rejected: remote-input-driven growth. D3 bounds
  and counts.
