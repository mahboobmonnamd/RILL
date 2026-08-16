# Spike 0 — test cases

Authority: [ADR 0001](adr/0001-session-operating-system.md),
[ADR 0002](adr/0002-falsifiable-evidence.md),
[ADR 0003](adr/0003-display-pipeline.md). Findings:
[SPIKE-0-AUDIT](SPIKE-0-AUDIT.md).

Every gate below carries four things. A gate missing any of them is not a gate.

- **Oracle** — what is observed, and why it is downstream of the mechanism
  (ADR 0002 D4). If the oracle can be satisfied by the test's own inputs, it is
  rejected.
- **Procedure** — what the test does.
- **Required mutation** — a minimal change to production code that this test
  **must** turn red (ADR 0002 D3). Reviewed like the assertion.
- **Negative control** — how the mutation is applied automatically, where it
  can be. `manual` means a reviewer applies it and pastes the red.

Status vocabulary: **Red** (no evidence) · **Green-unproven** (passes, never
demonstrated red) · **Proven** (demonstrated red, then green, in CI or a
recorded evidence artifact).

All gates are currently **Red** per ADR 0002 D1.

---

## T-BYTES — invalid UTF-8 reaches the emulator

Spec: PRD NFR-BYTES. Replaces the tautologies in audit S2-2.

**Oracle.** Two independent observations, neither of which is our copy of the
input:

1. **Kernel.** `Session::history()` contains the fixture verbatim. The child
   *emits* the bytes with the PTY in raw mode, so the line discipline cannot
   rewrite them. This is NFR-BYTES.
2. **Chip 0.** After `feed(fixture)`, the snapshot grid still shows the ASCII
   that was in the fixture, and any high byte that was not a CSI parameter
   produces a non-ASCII cell (U+FFFD or C1). libghostty-vt **may** substitute
   U+FFFD — that is decoding, not dropping. Forbidding U+FFFD made the gate
   unsatisfiable against this VT.

**Procedure.**

- Fixtures (inline in the tests, same corpus):
  - `lone_continuation` — `80 41`
  - `truncated_3byte` — `e2 82 41` (incomplete 3-byte sequence then ASCII, so
    the VT must flush)
  - `overlong_slash` — `c0 af`
  - `lone_surrogate` — `ed a0 80`
  - `bom_then_high` — `ff fe 80 41`
  - `csi_high_param` — `1b 5b 80 6d 41` (chip 0 only)
  - `c1_in_utf8` — `c2 9b 41` (chip 0 only)
- Kernel path: spawn `/bin/sh -c 'cat fixture.bin'` with `Discipline::Raw`.
- Chip 0 path: `feed` → `snapshot` → ASCII present; high bytes left a non-ASCII
  cell (except CSI, which may consume the high byte without a cell).

**Required mutation.** In `Chip0::feed`, drop bytes `>= 0x80` before
`vt_write`. `from_utf8_lossy` is a no-op against this VT (it already emits
U+FFFD) and cannot turn the gate red.

**Negative control.** `RILL_MUTATE=drop_high_bytes` — automated.

**Why the old test could not fail.** It compared `Chip0.fed` — a `Vec` the
function filled by `extend_from_slice` before touching the VT — to the input.
The rewrite then forbade U+FFFD in `repaint_bytes()`, which this emulator
produces for illegal UTF-8, so the gate was red for the wrong reason.

---

## T-DROP — flood, interrupt, keep typing, lose nothing

Spec: PRD NFR-DROP. Replaces audit S2-1.

**Oracle.** A checksum the child computes and reports, plus the child's own
liveness after the interrupt:

1. The child emits a numbered sequence (`seq 1 N`), so the reader can name the
   **first missing line number** rather than counting bytes.
2. After `^C`, the child prints a token that only a shell processing a fresh
   command line can produce.
3. The kernel's read loop is observed to **stop** while credit is exhausted —
   `Session` exposes a `stalled_reads` counter that must be non-zero.

**Procedure.**

- Spawn an interactive `/bin/sh -i`. Run `yes` (unbounded — not `head`).
- Client grants a **finite** credit window (64 KiB) and replenishes only as it
  consumes, per the real GUI policy.
- Run 10 wall-clock seconds. Assert `stalled_reads > 0`; if the kernel never
  stalls, the flood was not fast enough and the test **fails as inconclusive**.
- Send `0x03`. Within 2s the child must stop producing.
- Send `printf 'RILL-ALIVE-%s\n' "$$"\n`. The output must appear and the pid
  must equal the pre-interrupt child pid.
- Restart the child on a numbered flood (`seq 1 500000`) with the same finite
  credit; assert every line number `1..500000` is present, in order, and report
  the first gap on failure.

**Required mutation.** In `Session::on_pty_readable`, replace the credit gate
with a fixed 4 KiB read and drop the remainder — i.e. `try_send` semantics.

**Negative control.** `RILL_MUTATE=drop_on_full` — automated.

**Why the old test could not fail.** It re-granted `Credit(u32::MAX)` every
iteration, so the backpressure path never executed, and it asserted
`y_count >= 20000` against a `head -n 20000` that guarantees exactly that.

---

## T-RESIZE — the child's own winsize matches the display

Spec: PRD FR-RESIZE. Replaces audit S2 row 3.

**Oracle.** The **child's** `TIOCGWINSZ` on its own controlling terminal,
reported back over the PTY. Never the master fd we just wrote.

**Procedure.**

- Spawn `/bin/sh -i` with a `SIGWINCH` trap:
  `trap 'stty size > /tmp/rill-winsize-$$' WINCH`.
- Write a partial line to leave input pending mid-command (the "vim has pending
  input" clause), then send `RESIZE{cols:91, rows:31}` **immediately after** a
  `DATA` frame, on the same stream, with no drain between them.
- Await `SIGWINCH` handling; assert the child reports `31 91`.
- Assert ordering: the bytes written to the PTY before the resize were written
  **before** `TIOCSWINSZ`, verified by a kernel-side ordered event log
  (`Session::io_journal()`), not by timing.
- Repeat inside a real alt-screen TUI (`vim`) started in the same session, to
  cover reflow: after resize, the child's reported size matches and the VT's
  `snapshot()` reports `cols == 91`.

**Required mutation.** In `Session::on_frame`, reorder `Frame::Resize` handling
to apply `TIOCSWINSZ` *before* draining pending `DATA` writes.

**Negative control.** `RILL_MUTATE=resize_before_data` — automated.

**Why the old test could not fail.** It called `TIOCGWINSZ` on the same master
fd it had just called `TIOCSWINSZ` on, with `sleep 8` as the child. It verified
that Darwin's ioctl round-trips.

---

## T-EXIT — a dead pane never looks alive, including across detach

Spec: PRD FR-EXIT. Extended by audit S3-2.

**Oracle.** The GUI client's `alive` flag and the frames it actually received,
after a reattach that spans the child's death.

**Procedure.**

- Case A (attached): child runs `exit 7`. Client receives `EXIT{status:7}`.
  A subsequent `DATA` frame is rejected with `Error::Dead`. `Client::alive()`
  is false.
- Case B (**the persist path — this is the one that was broken**): attach,
  detach, *then* the child exits with no client connected, then reattach.
  The reattaching client must receive `EXIT{status}` before or with its resync
  bytes, and `alive()` must be false on the first pump.
- Case C: exit status fidelity for `0`, `7`, and death by `SIGKILL` (signal
  status, not `code().unwrap_or(1)`).

**Required mutation.** Restore `self.outbound.clear()` in `Session::detach`
(the current production behaviour).

**Negative control.** `RILL_MUTATE=clear_outbound_on_detach` — automated.
**Case B fails today without any mutation.** It is a red test against `main`.

---

## T-ATTACH — one attach, second refused, grids do not diverge

Spec: PRD FR-ONE. Extended by audit S3-6.

**Oracle.** The second connection's received frames, and a cell-by-cell
comparison of two independently constructed grids.

**Procedure.**

- Attach A, produce known content, detach, attach B, resync. Compare A's final
  `PodGrid` to B's post-resync `PodGrid` **cell by cell** — codepoint, fg, bg,
  attrs, cursor position — not by substring search of concatenated codepoints.
- With A still attached, connect B and send `ATTACH`. B must receive
  `REFUSED{AlreadyAttached}` and A must remain functional (a key sent on A
  still echoes).
- **Connect-without-attaching hole:** connect B, send **nothing**, then send a
  key on A. A must still work and B must not have displaced it.

**Required mutation.** In `Daemon::accept_client`, drop the `attached()`
condition so any second connection replaces the client.

**Negative control.** `RILL_MUTATE=accept_replaces_client` — automated.
The connect-without-attaching case fails today without mutation.

---

## T-RESYNC — reopen is never blank over a live process

Spec: PRD FR-RESYNC.

**Oracle.** The reattached grid's cells, compared against the pre-detach grid,
for an idle shell **and** for a full-screen alt-screen application.

**Procedure.**

- Idle `zsh`: produce a marker, detach, reattach, assert the marker is at the
  same cell coordinates it occupied before detach.
- `vim`: enter alt-screen, type known text, detach, reattach. Assert the grid is
  non-blank, that the alt-screen is active in the resynced state, and that the
  cursor is at the pre-detach position.
- Assert the resync consumed **zero** warm-path budget: the resync bytes arrive
  in response to `ATTACH`, and no resync work occurs on any subsequent keystroke
  (`Session::resync_count()` is unchanged after 100 keys).
- Assert the window cannot distinguish resync bytes from live bytes: the client
  receives only `DATA` frames, with no resync-specific tag.

**Required mutation.** Make `Session::take_resync_history` return `None`
unconditionally.

**Negative control.** `RILL_MUTATE=no_resync` — automated.

---

## T-KILL — quit or SIGKILL the GUI, the shell survives

Spec: PRD FR-KILL. Renamed per ADR 0002 D6.

New name: `t_kill_gui_process_group_child_pid_survives_and_reattach_shows_prior_output`.
Doc comment records the original report: *"quit app and reload does not persist
the session."*

**Oracle.** `kill(pid, 0)` from a third process, and the reattached grid.

**Procedure.** The existing `crates/rilld/tests/persist_e2e.rs` procedure is
kept — it is the one test in the tree that earns its name. Extended with:

- The packaged `Rill.app` binary is the GUI, not a `sh` stand-in, so the real
  `posix_spawn(POSIX_SPAWN_SETSID)` path in `main.m` is exercised.
- `SIGKILL` to the process group **and** a separate case for AppKit `Quit`
  (`osascript -e 'quit app "Rill"'`), which takes a different teardown path.
- Assert the child's **parent** after the kill is `rilld`, not `launchd`
  reparenting from a leaked orphan — `ps -o ppid= -p <child>` equals the rilld
  pid.
- Assert the shell's working directory and environment survived, by asking the
  shell itself after reattach.

**Required mutation.** Remove `POSIX_SPAWN_SETSID` from `spawn_rilld` in
`main.m`.

**Negative control.** `manual` — requires an ObjC rebuild; reviewer pastes the
red.

---

## T-SPAWN — the GUI never creates the user shell's PTY

Spec: PRD FR-SPAWN, NFR-SPAWN. Replaces audit S1-1.

**Oracle.** Three independent checks, because a symbol list alone cannot express
this — `main.m` legitimately calls `posix_spawn` to launch `rilld`.

1. **Imports, not exports.** `nm -u` and `otool -Iv` (the lazy/non-lazy bind
   tables) on `dist/Rill.app/Contents/MacOS/Rill`. Assert none of `_forkpty`,
   `_openpty`, `_posix_openpt`, `_grantpt`, `_unlockpt`, `_ptsname`,
   `_login_tty` appear. These are PTY **creation** primitives; `rilld` cannot
   have been the one to spawn a shell if the GUI cannot make a PTY.
2. **Positive control.** The same check is run against a purpose-built fixture
   binary that *does* call `forkpty` (`crates/rill-host/tests/fixtures/spawner.c`),
   and **must report a violation**. If the positive control passes clean, the
   check is broken and the gate fails. This is what the old test lacked.
3. **Runtime.** With the app running, resolve the user shell's process and
   assert its parent is `rilld` and its session id differs from the GUI's.

**Required mutation.** Call `openpty()` once in `main.m` and discard the result.

**Negative control.** Check 2 is the permanent, always-on negative control.
The mutation is `manual`.

**Why the old test could not fail.** `nm -U` lists **defined** symbols. The
asserted symbols can only ever be **undefined** imports. The command excluded
exactly the set the assertion inspected.

---

## T-NFR — key-down → drawable presented

Spec: PRD NFR-KEY as redefined by [ADR 0003](adr/0003-display-pipeline.md).
Replaces audit S1-2 and S1-3.

**Oracle.** `NSEvent.timestamp` → `drawable.presentedTime`, with a
**cell-position-specific** sentinel that cannot pre-exist (ADR 0003 D6).

**Procedure.**

- Packaged `Rill.app`, real window, real display.
- Per sample: record cursor cell `(c,r)` and the codepoint currently at `(c,r)`;
  choose a printable codepoint that differs from it; inject the key; complete
  the sample when the presented frame holds the chosen codepoint **at `(c,r)`**.
- Frames are tagged with the sentinel they carry so `addPresentedHandler:`
  attributes each presentation to the right keystroke.
- Discard samples where the cursor moved unexpectedly, the screen scrolled, or
  the shell wrapped. **Discards > 2% fail the run** — an unreliable oracle does
  not get to report a p95.
- `n ≥ 1000` accepted samples. Report p50/p95/p99/max, discard count, refresh
  rate, vsync state, power source, injection mode, libghostty-vt SHA.
- Three runs: idle, under flood, **on battery**. Battery is the gate.

**Pass:** p95 < one refresh interval at the display's actual rate (ADR 0003 D8).

**Control-RPC oracle (replaces the self-certifying check).** During the window:
frames sent are only `DATA` and `CREDIT`; frames received are only `DATA`; the
process opened no socket other than the attach socket, compared by a
before/after fd snapshot.

**Required mutation.** Restore the 60 Hz `NSTimer` in place of ADR 0003 D2's
dispatch source. A one-frame polling interval on a one-frame budget must be
visible in p95.

**Negative control.** `RILL_MUTATE=timer_pump` — automated in `app` mode.

**Why the old test could not fail.** It searched the whole grid for
`b'a' + i%26`, which the shell had already echoed there on a previous cycle, so
the wait loop exited before any PTY round trip. It also stopped at the POD
snapshot, inside the Rust client, never reaching the host or the GPU.

---

## Gate ledger

| ID | Status | Automated negative control | Blocking defect it also covers |
|---|---|---|---|
| T-BYTES | Red | `drop_high_bytes` | S3-1 (overflow) via the emoji fixture |
| T-DROP | Red | `drop_on_full` | S3-5 (nominal backpressure) |
| T-RESIZE | Red | `resize_before_data` | — |
| T-EXIT | Red | `clear_outbound_on_detach` | **S3-2 (EXIT lost on detach)** |
| T-ATTACH | Red | `accept_replaces_client` | **S3-6 (refusal hole)** |
| T-RESYNC | Red | `no_resync` | — |
| T-KILL | Red | manual | S3-3 (drop kills child) |
| T-SPAWN | Red | permanent positive control | S1-1 |
| T-NFR | Red | `timer_pump` | S3-8b, S3-8g, S3-8h |

T-EXIT across detach and T-ATTACH's connect-without-attaching hole were
production defects; both have tests that now pass. They are **Green-unproven**
until a negative-control run records them going red (ADR 0002 D2).
