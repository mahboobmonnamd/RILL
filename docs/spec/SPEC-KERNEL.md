# SPEC-KERNEL — session kernel (`lane:kernel`)

- **Status:** Accepted for Spike 0 Proven clauses — 2026-08-17
  ([ADR 0010](../adr/0010-spike-0-closes.md)). Written 2026-08-16 as the
  remediation draft; those clauses closed. Multiple sessions: [ADR 0011](../adr/0011-session-graph.md),
  [SPEC-GRAPH](SPEC-GRAPH.md).
- **Authority:** [ADR 0001](../adr/0001-session-operating-system.md) §3–§7,
  [ADR 0002](../adr/0002-falsifiable-evidence.md)
- **Crate:** `crates/rill-kernel`
- **Gates:** T-BYTES, T-DROP, T-RESIZE, T-EXIT, T-KILL — **Proven**

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. Ownership

- The kernel MUST be the sole writer to the PTY master.
- The master fd MUST NOT be returned from any `pub` item. `Session::master_fd`
  and `pty::leak_master_forbidden` are removed (audit S3-4).
- Readiness is exposed as a capability, not a descriptor:

```rust
impl Session {
    /// Block up to `timeout` for the master to become readable.
    pub fn wait_readable(&mut self, timeout: Duration) -> Result<bool, Error>;
}
```

  `rilld` MUST drive the session through this, or through a `Session`-owned
  registration API. It MUST NOT hold a raw master fd. `scripts/lint-planes.sh`
  enforces this (ADR 0002 D9).
- `SCM_RIGHTS` of the master is forbidden anywhere in the tree.

## 2. Process lifetime

- `Pty` MUST NOT kill the child on `Drop`. The child is detached by default
  (audit S3-3).
- Intentional teardown is `Pty::terminate(&mut self, sig: Signal)`, called
  explicitly. Tests that need cleanup call it; `Daemon` error paths MUST NOT.
- The kernel MUST reap the child (`waitpid`) so it does not become a zombie,
  and MUST surface exit as `Frame::Exit`.
- Exit status MUST distinguish normal exit from signal death. `status` is
  encoded as the raw `wait` status; `code().unwrap_or(1)` is removed — it
  reports `1` for `SIGKILL`, which is a lie the GUI would display.

## 3. Byte history

- History is a bounded byte ring. Default 4 MiB, configurable.
- The ring MUST store raw bytes. No UTF-8 validation, no transformation.
- Overflow discards from the head. The ring is never used as a live pipe;
  live delivery uses credit (§4).
- `ByteRing::snapshot()` allocates and copies; it is a cold-path call only. The
  warm path MUST NOT call it.

## 4. Credit and backpressure

- `Session` holds a credit balance in bytes, granted by `CREDIT` frames.
- When credit is zero the kernel MUST stop reading the master. It MUST NOT
  read-and-discard, and MUST NOT read into an unbounded buffer.
- A single read MUST NOT exceed the remaining credit.
- `Session` MUST expose observable counters for the gates:

```rust
pub fn stalled_reads(&self) -> u64;  // times a read was skipped for zero credit
pub fn bytes_delivered(&self) -> u64;
pub fn resync_count(&self) -> u32;
```

  T-DROP asserts `stalled_reads > 0`; a flood that never stalls the kernel is
  an inconclusive test, not a pass (TEST-CASES T-DROP).

## 5. Frame handling

`Session::on_frame` MUST be total and MUST NOT panic.

| Inbound | Behaviour |
|---|---|
| `ATTACH{generation}` | First attach: record generation, set resync pending, and **re-emit a retained `EXIT` if the child already died** (§6). Second attach while attached: emit `REFUSED{AlreadyAttached}`, change nothing. |
| `CREDIT{n}` | Saturating add. |
| `RESIZE{...}` | Apply `TIOCSWINSZ` **after** all `DATA` frames received earlier on the stream have been written to the master. Ordering is recorded in the io journal (§7). |
| `DATA{bytes}` | If the child has exited, return `Error::Dead` and write nothing. Otherwise write all bytes to the master. |
| `EXIT`, `REFUSED` | Ignored inbound. |

## 6. Exit retention across detach

This is audit S3-2, the defect that breaks FR-EXIT on the persist path.

- `Session::detach()` MUST NOT discard a pending `EXIT`. It MAY discard pending
  `DATA` (the reattaching client resyncs from history instead).
- `Session` retains `child_exit: Option<i32>` for its lifetime.
- On `ATTACH`, if `child_exit` is `Some`, the kernel MUST enqueue `EXIT` so the
  reattaching client learns the pane is dead before it paints a cursor over a
  corpse.
- `Daemon` MUST NOT drain-and-drop outbound frames when no client is attached;
  it MUST leave retained control frames queued.

## 7. IO journal

An ordered, bounded, in-memory log of `(seq, event)` for the gates to assert
ordering without relying on timing:

```rust
pub enum IoEvent { PtyWrite(usize), PtyRead(usize), Winsize(Winsize), ChildExit(i32) }
pub fn io_journal(&self) -> &[(u64, IoEvent)];
```

T-RESIZE asserts `PtyWrite` precedes `Winsize` by sequence number.

## 8. Polling

- `select`/`fd_set` MUST NOT be used. `poll` only (audit S3-8c: `select` is
  undefined behaviour for fd ≥ `FD_SETSIZE`).

## 9. Errors

- Every public function returns `Result`.
- `unwrap`, `expect`, and `panic!` are forbidden outside `#[cfg(test)]`,
  enforced by `scripts/lint-planes.sh`.
- An error on a session path MUST NOT be able to terminate the child (§2).

## 10. Test-mode raw PTY

For T-BYTES the kernel MUST offer a raw-mode spawn that clears `ISIG`,
`ICANON`, `ECHO`, `OPOST`, `IXON`, `ISTRIP` on the slave before `exec`, so the
line discipline cannot rewrite the byte stream under test. The default
interactive spawn keeps the normal discipline.

## 11. Out of scope (Spike 0)

Reconnect tokens and daemon-restart / logout survival remain out (ADR 0001 §7).

**M1:** the kernel stores `SessionId → Session` ([ADR 0011](../adr/0011-session-graph.md),
[SPEC-GRAPH](SPEC-GRAPH.md)). Default daemon start still spawns one leaf so the
packaged one-window path stays Spike 0 frames. Session naming in the GUI is M2
chrome.
