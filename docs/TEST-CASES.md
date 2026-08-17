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

Ledger below cites GitHub Actions
[run 31993832263](https://github.com/mahboobmonnamd/RILL/actions/runs/31993832263)
(`spike0-20260817T041912Z.json`) for the library suite, plus the 2026-08-17
battery hid on ADR 0009. Every gate is **Proven** ([ADR 0010](adr/0010-spike-0-closes.md)).
The withdrawn `p95=0.032ms` run must not be cited. Do not dispatch `gates.yml`
again.

---

## Quality follow-ups (post Spike 0)

These are not Spike 0 reopeners. They are defects the post-close audit found
in production code that Spike 0 did not name.

### T-PARTIAL-WRITE — non-blocking flush must not replay a DATA frame

**Oracle.** Child-emitted tokens `RILL-UNIQ-<n>-END` in decoded `DATA`. Only
complete `-END` tokens count: a read cutoff can bisect `RILL-UNIQ-16073` into
a line that looks like `160`. Downstream of the PTY child, not of a buffer
the daemon copied for the test.

**Procedure.** In-process `Daemon` over `AF_UNIX`. Shrink the client's
`SO_RCVBUF`, ATTACH+CREDIT, **pump without reading** until the socket fills,
then drain. The child emits ~20k numbered lines. Assert each complete token
appears once, and that enough tokens arrived to have filled the socket.

**Required mutation.** `write_all` of a whole frame on `WouldBlock`, then
re-queue that same frame (`RILL_MUTATE=replay_full_frame`, feature `mutate`).
The observed red is a decode failure (`UnknownTag` from payload bytes treated
as a frame header) or a duplicated `-END` token.

**Negative control.** `replay_full_frame` — automated. The test sets
`RILL_TEST_TINY_SNDBUF` so the accepted socket actually short-writes;
integration tests do not compile the lib with `cfg(test)`.

### T-ATTACHED-POLL — live attach must not sleep in `poll`

**Oracle.** After ATTACH+CREDIT, `Daemon::step_timeout_ms()` is `0`. Before
attach it is `50` (idle must not busy-loop). Packaged T-NFR hid is the vsync
oracle this lock exists to protect.

**Procedure.** In-process `Daemon`. Bind, assert idle timeout > 0, connect,
ATTACH+CREDIT, step until credit is applied, assert timeout is 0.

**Required mutation.** Always return 50 (`RILL_MUTATE=idle_poll_while_attached`,
feature `mutate`). That is the Q5 regression: hid p95 12–13 ms vs closer 7.011 ms.

**Negative control.** `idle_poll_while_attached` — automated.

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

**Required mutation.** Drop both session isolations: `POSIX_SPAWN_SETSID` in
`spawn_rilld` (`main.m`) **and** `setsid()` in `rilld`. Either one alone keeps
the daemon out of the GUI process group, so the instrument would stay green.

**Negative control.** `RILL_MUTATE=drop_POSIX_SPAWN_SETSID` — forwarded to the
packaged GUI and inherited by rilld; `persist_e2e` must go red.

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
The production mutation is `RILL_MUTATE=openpty_in_main_m` at package time
(links `crates/rill-host/tests/fixtures/mutate_openpty.c`, never `host/`);
`t_spawn_gui_binary_does_not_import_pty_creation_symbols` must go red against
that binary.

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

Present is ADR 0009: `toggleFullScreen:` + opaque `CAMetalLayer` + echo, one
in flight, same-stack pump. ADRs 0004–0008 are exhausted paths, not the closer.
The 8.33ms budget does not move. GitHub-hosted `macos-14` cannot close hid.

**Control-RPC oracle (replaces the self-certifying check).** During the window:
frames sent are only `DATA` and `CREDIT`; frames received are only `DATA`; the
process opened no socket other than the attach socket, compared by a
before/after fd snapshot.

**Required mutation.** Restore the 60 Hz `NSTimer` in place of ADR 0003 D2's
dispatch source **and** skip same-stack `paintEchoAfterInput` plus the NFR
loop's 0.5 ms pump, so the sample waits on the timer. A one-frame polling
interval on a one-frame budget must be visible in p95.

**Negative control.** `RILL_MUTATE=timer_pump` — `sh scripts/run-t-nfr-timer-pump.sh`
from Terminal.app on the packaged app. Must miss p95 (or fail to accept 1000
samples because of the 60 Hz poll). App-mode invert on a headless runner is
not this control.

**Observed invert (2026-08-17).** `timer_pump=1`, p95 **30.823ms** vs 8.33ms,
cadence p50=33.33ms (~30 Hz), 1000/2, `ax_trusted=1`. Unmutated battery hid
on the same presenter was p95 **7.011ms**. The instrument detects the poll.

**Why the old test could not fail.** It searched the whole grid for
`b'a' + i%26`, which the shell had already echoed there on a previous cycle, so
the wait loop exited before any PTY round trip. It also stopped at the POD
snapshot, inside the Rust client, never reaching the host or the GPU.

---

## Gate ledger

| ID | Status | Automated negative control | Blocking defect it also covers |
|---|---|---|---|
| T-BYTES | **Proven** | `drop_high_bytes` went red (named test) | S3-1 (overflow) via the emoji fixture |
| T-DROP | **Proven** | `drop_on_full` went red (`stalled_reads` stayed 0) | S3-5 (nominal backpressure) |
| T-RESIZE | **Proven** | `resize_before_data` went red | — |
| T-EXIT | **Proven** | `clear_outbound_on_detach` went red | **S3-2 (EXIT lost on detach)** |
| T-ATTACH | **Proven** | `accept_replaces_client` went red | **S3-6 (refusal hole)** |
| T-RESYNC | **Proven** | `no_resync` went red (blank reopen) | — |
| T-KILL | **Proven** | `drop_POSIX_SPAWN_SETSID` automated | S3-3 (drop kills child) |
| T-SPAWN | **Proven** | fixture control + `openpty_in_main_m` automated | S1-1 |
| T-NFR | **Proven** | `timer_pump` went red on laptop (p95 30.823ms); hid Manual per ADR 0009 D4 | S3-8b, S3-8g, S3-8h |

Library D8 artifact: [run 31993832263](https://github.com/mahboobmonnamd/RILL/actions/runs/31993832263).
T-NFR battery hid (ADR 0009 / 0010, 2026-08-17): p50=6.743ms p95=**7.011ms**
p99=14.220ms max=22.670ms, samples=1000 discarded=2 (0.20%), 120 Hz
budget=8.33ms, cadence p50=p95=8.33ms, `ax_trusted=1`, `pmset` Battery Power
28% discharging. `timer_pump` invert: p95 **30.823ms**, cadence 33.33ms,
`timer_pump=1`. Hosted `macos-14` T-NFR timed out at 45s and is not the closer.

---

## Milestone 1 — session graph

Authority: [ADR 0011](adr/0011-session-graph.md), [SPEC-GRAPH](spec/SPEC-GRAPH.md),
[#16](https://github.com/mahboobmonnamd/RILL/issues/16),
[#28](https://github.com/mahboobmonnamd/RILL/issues/28),
[#29](https://github.com/mahboobmonnamd/RILL/issues/29).
Library tests live in `crates/rill-kernel/tests/gates.rs` and
`crates/rilld/tests/gates.rs`. Packaged multi-leaf persist is **not** these
gates (SPEC-GRAPH §4).

### T-GRAPH-SPAWN — two leaves, two pids

**Oracle.** After `spawn_leaf` twice, `child_pid` values differ and both
`child_alive()`. Downstream of `posix_spawn`, not of a counter the test wrote.

**Procedure.** In-process kernel (no GUI). Two interactive or raw spawns of
`/bin/sleep` (or `/bin/sh -c 'exec sleep 60'`).

**Required mutation.** A kernel that stores one `Session` and ignores the
second spawn MUST turn this red. Negative control: `RILL_MUTATE=single_session`
(feature `mutate`).

### T-GRAPH-ISOLATE — histories do not mix

**Oracle.** Child A emits a unique marker that MUST appear in A's `history()`
and MUST NOT appear in B's. Child B vice versa. Markers come from the children,
not from a test-owned copy of the feed buffer.

**Procedure.** Raw discipline; each child `cat`s a distinct fixture.

**Required mutation.** A shared history for all ids MUST turn this red
(`single_session` when implemented).

### T-GRAPH-ATTACH — refuse same id, accept other id

**Oracle.** Second attach to id A yields `REFUSED{AlreadyAttached}` and A's
first client still receives DATA. Attach to id B succeeds. A bare connection
MUST NOT steal A's claim (existing S3-6, per id).

**Procedure.** In-process `on_frame` / map API for this slice. Socket
naming is [#28](https://github.com/mahboobmonnamd/RILL/issues/28).

**Required mutation.** Treating every attach as one session MUST turn the
second-id clause red.

### T-GRAPH-TERMINATE — destroy one leaf, leave the other alive

**Oracle.** After `Kernel::terminate(A)`, `kill(pid_A, 0)` fails and
`kill(pid_B, 0)` succeeds. Downstream of the OS, not of `child_alive()`.
`Drop` still MUST NOT kill (existing T-KILL).

**Procedure.** In-process kernel. Two `/bin/sh -c 'exec sleep 60'` leaves.
Terminate A only.

**Required mutation.** `RILL_MUTATE=terminate_all_leaves` (feature `mutate`)
MUST turn this red by killing B as well.

### T-ATTACH-NAMED — ATTACH payload names a leaf

**Oracle.** 8-byte ATTACH is generation only. 16-byte ATTACH carries a
`session_id`. A socket ATTACH with B's id receives B's child marker and MUST
NOT receive the default leaf's marker. Unknown id → `REFUSED{Invalid}`. A
second connection ATTACH to B is accepted while A stays attached.

**Procedure.** Codec unit tests plus in-process `Daemon` over `AF_UNIX`.

**Required mutation.** `RILL_MUTATE=ignore_session_id` (feature `mutate`):
always attach the default leaf. The two-id socket tests MUST go red.

---

## Milestone 6 — cwd tap (Red)

Authority: [ADR 0013](adr/0013-cwd-tap.md) (Accepted),
[SPEC-CWD](spec/SPEC-CWD.md),
[#23](https://github.com/mahboobmonnamd/RILL/issues/23). Named here first.
Not M1. Path header chrome is [#22](https://github.com/mahboobmonnamd/RILL/issues/22).
Not T-NFR. Not Chip 1.

### T-CWD-FG — foreground job chdir is visible

**Oracle.** Interactive `zsh` stays in dir A. A fg child `chdir`s to
`/private/tmp` (or another path the test did not put in a buffer it then
asserts). `Session::cwd()` MUST equal that child's vnode path
(`/private/tmp`). The session-leader pid's cwd MUST still be A — if the
implementation reports the leader, this test is red.

**Procedure.** In-process kernel PTY. No GUI. Child script is a fixture file,
not a prompt parse.

**Required mutation.** `RILL_MUTATE=leader_cwd` (feature `mutate`): return
`proc_pidinfo` of the posix_spawn child only. This test MUST go red.

### T-CWD-NO-OSC7 — alt-screen / TUI chdir without OSC 7

**Oracle.** Leaf is a process that `chdir`s and never writes `ESC ] 7`.
History MUST NOT contain OSC 7. `Session::cwd()` MUST still be the new path.

**Procedure.** In-process kernel. Python (or equivalent) as the leaf.

**Required mutation.** `RILL_MUTATE=osc7_only`: cwd updates only when OSC 7
is classified. This test MUST go red.

### T-CWD-FAIL-CLOSED — unreadable cwd does not invent a path

**Oracle.** When `proc_pidinfo` fails, `Session::cwd()` is `Err` and last
known is unchanged. Never an empty path as `Ok`, never a prompt substring.

**Procedure.** Mutation or a pid that cannot be inspected.

**Required mutation.** `RILL_MUTATE=cwd_fail_open`: return `Ok("")` or the
prompt. This test MUST go red.

---

## Milestone 4 — Chip 1 isolated VT (Red)

Authority: [ADR 0012](adr/0012-chip1-isolated-vt.md), [SPEC-CHIP1](spec/SPEC-CHIP1.md),
[M4-HANDOFF](M4-HANDOFF.md), [#6](https://github.com/mahboobmonnamd/RILL/issues/6).
These gates are **Red**. Named here first. No `vt-engine` behaviour until they
exist and have been observed failing (ADR 0002 D2). Not live. Not T-NFR.

Oracle for every gate: `snapshot()` (codepoint, cursor, attrs, `cells.len()`),
or a second instance’s grid after resync emit. Never a copy of the input, never
the `\x1b[2J` prefix the emit path prepends.

### T-CHIP1-ASCII — printable lands in the POD grid

**Oracle.** After `feed(b"Hello")`, row 0 cells 0..4 are `H e l l o`.

**Procedure.** In-process Chip 1, 80×24 (or 40×5). No PTY.

**Required mutation.** Drop `feed` / do not write cells.

### T-CHIP1-BYTES — invalid UTF-8 reaches the parser

**Oracle.** Same fixtures as T-BYTES. ASCII `A` present when the fixture
contains `0x41`. High bytes produce a non-ASCII cell except CSI-high-param,
which MAY consume the high byte without a cell. MUST NOT drop `>= 0x80`
before parse.

**Required mutation.** Drop bytes `>= 0x80` before parse.
**Negative control.** `RILL_MUTATE=drop_high_bytes` (`feature = "mutate"`).

### T-CHIP1-GRAPHEME — long cluster does not overrun

**Oracle.** `e` + 40× U+0301: snapshot survives; `grapheme_truncated >= 1`
or the base is still a visible cell. No panic.

**Required mutation.** Fixed 8-slot stack buffer / silent drop of extras.

### T-CHIP1-CRLF — CR LF moves the cursor

**Oracle.** `A\r\nB` → `B` at row 1 col 0 (or documented equivalent).

**Required mutation.** Ignore CR/LF.

### T-CHIP1-CUP — CSI CUP positions the cursor

**Oracle.** `ESC[5;10H` → cursor at 1-based (5,10), i.e. row 4 col 9
0-based, documented in the test.

**Required mutation.** Ignore CSI.

### T-CHIP1-SGR — bold sets attrs bit 0

**Oracle.** `ESC[1mX` → that cell `attrs & 1 != 0`.

**Required mutation.** Ignore SGR.

### T-CHIP1-ED — erase display clears to space

**Oracle.** Feed text, `ESC[2J`, every cell codepoint is space `32`.

**Required mutation.** ED is a no-op.

### T-CHIP1-ALT — 1049 preserves primary

**Oracle.** Feed A, `?1049h`, feed B, `?1049l` → A visible, B gone.

**Required mutation.** Single buffer.

### T-CHIP1-SIZE — snapshot is exactly cols×rows

**Oracle.** After resize 40×5, `cells.len() == 200`.

**Required mutation.** Unbounded history in the snapshot.

### T-CHIP1-POD — PodCell is 16 bytes

**Oracle.** `size_of::<PodCell>() == 16`. Lint `no-cell-strings`.

**Required mutation.** Add a `String` field.

### T-CHIP1-RESYNC — emit bytes reconstruct the grid

**Oracle.** Feed a marker, emit, second instance feeds only the emit bytes,
row 0 matches. MUST NOT assert on `\x1b[2J`.

**Required mutation.** Emit empty or emit only the prefix.

