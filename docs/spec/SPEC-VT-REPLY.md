# SPEC-VT-REPLY — Chip 1 device replies (`lane:chip1-vt-engine`, M4)

- **Status:** Accepted for the M4 contract — 2026-08-18. Named tests are **Red**.
- **Authority:** [ADR 0022](../adr/0022-chip1-reply-channel.md)
- **Issue:** epic [#6](https://github.com/mahboobmonnamd/RILL/issues/6)
- **Crate:** `crates/vt-engine`
- **Gates:** T-CHIP1-REPLY — **Red**. Not live. Not T-NFR.
- **Amends:** [SPEC-CHIP1](SPEC-CHIP1.md) §3's DA/DSR clause, which required an
  answer the §2 API could not deliver ([SPIKE-VT](../SPIKE-VT.md) Result 7)

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. Why there is a buffer at all

A terminal owes answers to some queries. `vim` and `htop` probe primary DA and
cursor position, and a program that waits for an answer it never gets **hangs** —
SPEC-CHIP1 already called that a v0 miss.

Chip 1 cannot send the answer. It has no PTY and no socket (ADR 0012 D1), and
the kernel is the sole writer to the PTY master (ADR 0001 §5). So the chip
**accumulates** replies and the owner of the write path drains them.

## 2. API

```rust
pub fn take_replies(&mut self) -> Result<Vec<u8>, Error>;
pub fn has_replies(&self) -> bool;
```

- Inherent on the Chip 1 type, **not** on `TerminalEmulation` (ADR 0022 D1), so
  Chip 0 and the trait are unchanged.
- `take_replies` **drains**: a second call returns empty unless more arrived.
- `feed` MUST NOT block, write to any file descriptor, or call back into caller
  code. Producing a reply is appending to an internal buffer.
- When there are no replies, `take_replies` MUST NOT allocate.

## 3. What v0 answers

| Query | Reply | Note |
|---|---|---|
| `CSI c`, `CSI 0 c` | `CSI ? 6 c` | Primary DA. VT102 class — the subset we implement. |
| `CSI > c` | `CSI > 0 ; 0 ; 0 c` | Secondary DA. |
| `CSI 6 n` | `CSI <row> ; <col> R` | DSR cursor position, **1-based**. |
| `CSI 5 n` | `CSI 0 n` | MAY answer. "No malfunction." |

Everything else is consumed and **not** answered.

- We MUST NOT claim a capability we have not implemented: no DECRPM for modes we
  ignore, no `XTVERSION`, and no DA reply advertising sixel, images or colour
  features absent from v0. Silence is safer than a lie — a program that believes
  a false capability sends output we cannot render.
- DSR MUST report the cursor **after** everything fed so far, including pending
  wrap resolution (SPEC-VT-SCREEN §2), so the answer agrees with what
  `snapshot()` would report. Off-by-one here makes full-width lines misplace the
  cursor in `vim`.
- Replies are emitted in the order the queries arrived.

## 4. Bounds and failure

- Capacity is fixed at **1024 bytes**. It MUST NOT grow.
- A program that spams `CSI 6 n` without reading MUST NOT be able to grow this
  buffer: the bytes arrive from whatever runs in the user's shell, so unbounded
  growth is a remote memory-exhaustion path.
- On overflow the chip **drops new replies** and increments `replies_dropped`,
  reported on `PodGrid` like `grapheme_truncated` (SPEC-VT-TYPES §3). It MUST NOT
  panic, reallocate without bound, or discard already-buffered bytes.
- A dropped reply is a real hang risk for the program. It is counted so the
  failure is visible rather than mysterious.
- `take_replies` returning `Err` MUST leave the buffer intact so the caller can
  retry.

## 5. Who drains it

- **M4:** nothing. Chip 1 is a library and nothing constructs it in production
  (ADR 0012 D1). The gate drains it in-process.
- **M7:** the host drains after `feed` and sends the bytes as ordinary attach
  `DATA` frames — the same path as a keystroke. No new frame tag, no JSON, no
  control RPC, so T-NFR's zero-control-RPC clause (ADR 0003 D9) and SPEC-ATTACH
  are untouched. The M7 ADR MUST say this explicitly.
- **Cold resync:** `resync_from_history` MUST discard replies. Replaying history
  is not a live program, and injecting bytes toward the PTY from a replay would
  send answers to queries no one asked. Discarded replies are counted, not
  pretended sent.

## 6. Gate

**T-CHIP1-REPLY.** Feed `CSI 5 ; 3 H` then `CSI 6 n`; `take_replies()` yields
`CSI 5 ; 3 R`. Feed `CSI c`; the reply is `CSI ? 6 c`. A second `take_replies()`
is empty.

**Oracle.** The drained bytes, parsed — not a constant the test prepended, not a
boolean saying a reply was queued. Feeding the reply into a second instance is
**not** the oracle: replies travel toward the program, not the screen.

**Required mutation.** Never enqueue a reply, so `take_replies` is always empty.
A second mutation — unbounded reply buffer — MUST turn the `replies_dropped`
assertion red.

## 7. Out of scope

Key encoding and mouse reporting (host), OSC responses such as clipboard `52`
and colour queries `4`/`10`/`11` (consumed and ignored, SPEC-VT-PARSER §8),
DECRQSS, `XTVERSION`, bracketed paste, the routing implementation (M7), the live
swap (M7).
