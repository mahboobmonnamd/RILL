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

### T-NAV-IDENTITY — default leaf host indicator comes from the kernel

**Status.** Green-unproven — F-001
[#38](https://github.com/mahboobmonnamd/RILL/issues/38) and F-002
[#39](https://github.com/mahboobmonnamd/RILL/issues/39), authorized by ADR 0038
D6 and SPEC-NAV §6. A packaged local run demonstrated the behavior-absent
baseline red, then green, and the required mutation red on 2026-08-18. Hosted
CI has not run this gate because the macOS runner was unavailable, so ADR 0002
D8 still forbids marking it Proven.

**Oracle.** Packaged `Rill.app` starts with a deliberately non-host `$HOME`.
The live AppKit host-indicator label is observed through the test heartbeat and
must read `local`. The label is populated by a cold daemon reply for the default
leaf, rather than by a string the test supplied or a chrome cache. The host's
normal attach remains the 8-byte `ATTACH` for that daemon default leaf.

**Procedure.** Launch the packaged app with a unique socket and `HOME` set to a
temporary path named `rill-not-a-host-*`; wait for the live window heartbeat's
rendered host indicator.

**Precondition.** `dist/Rill.app/Contents/MacOS/Rill` exists and is executable.
The test fails if packaging did not provide it; it never skips the AppKit,
daemon, or cold-socket path.

**Required mutation.** `RILL_MUTATE=host_indicator_from_home` selects `$HOME`
instead of the cold kernel identity. The host-indicator test MUST go red.

**Negative control.** Automated by `validate-spike0.sh --negative-controls`,
which runs the packaged test with `RILL_MUTATE=host_indicator_from_home`.

### T-PARTIAL-WRITE — non-blocking flush must not replay a DATA frame

**Oracle.** Child-emitted tokens `RILL-UNIQ-<n>-END` in decoded `DATA`. Only
complete `-END` tokens count: a read cutoff can bisect `RILL-UNIQ-16073` into
a line that looks like `160`. Downstream of the PTY child, not of a buffer
the daemon copied for the test.

**Procedure.** In-process `Daemon` over `AF_UNIX`. Shrink the client's
`SO_RCVBUF`, ATTACH+CREDIT, **pump without reading** until the socket fills,
then drain. The child emits ~20k numbered lines. Assert each complete token
appears once, and that enough tokens arrived to have filled the socket.

**Required mutation.** `write_all` of a whole frame, then re-queue that same
frame (`RILL_MUTATE=replay_full_frame`, feature `mutate`). The replay is not
gated on `WouldBlock`: a kernel that ignores a tiny `SO_SNDBUF` never short-
writes, and that left the instrument green on hosted `macos-14`. The observed
red is a decode failure (`UnknownTag` from payload bytes treated as a frame
header) or a duplicated `-END` token.

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

## T-FS-EXIT — leaving fullscreen must not hang

Authority: [ADR 0016](adr/0016-exit-fullscreen-must-not-hang.md),
[SPEC-DISPLAY](spec/SPEC-DISPLAY.md) §3,
[#257](https://github.com/mahboobmonnamd/RILL/issues/257).

**Bug (doc comment).** After `make run` the window is fullscreen. Clicking
the button to return to a normal window hangs; force quit is required.

**Oracle.** A main-thread heartbeat file (`RILL_TEST_HEARTBEAT`) keeps
advancing after `toggleFullScreen:` leaves the Space (`fullscreen=0` in the
file, then `seq` increases). The GUI pid stays alive (`kill(pid, 0)`).
Downstream of the run loop, not of a flag the test wrote into the presenter.

**Procedure.** Packaged `Rill.app`. `RILL_TEST_EXIT_FULLSCREEN=1` issues the
same `toggleFullScreen:` as the green button. No GUI click in CI. Socket-only
tests do not close this.

**Required mutation.** `RILL_MUTATE=wait_forever_on_inflight`: the in-flight
wait is unbounded again. This test MUST go red.

**Launch.** Default `make run` is windowed (ADR 0017). This gate sets
`RILL_TEST_EXIT_FULLSCREEN=1`, which **enters** a Space then leaves. It does
not require default launch to be fullscreen. Hosted `macos-14` has no Spaces;
`validate-spike0.sh` records the Red and does not fail the job when
`RILL_NFR_OPTIONAL=1` (same contract as T-NFR, ADR 0009 D4).

---

## T-LOOK-OVERLAY — Ghostty look keys win over host-surface.toml

Authority: [ADR 0017](adr/0017-ghostty-look-windowed-default.md) D2,
[SPEC-DISPLAY](spec/SPEC-DISPLAY.md) §10,
[#259](https://github.com/mahboobmonnamd/RILL/issues/259).

**Bug (doc comment).** `host-surface.toml` was Menlo 13 on Chip 0 dark
defaults. The user's look keys (Catppuccin Latte, JetBrainsMono Nerd Font,
size 16, padding 8) in `~/.config/rill/config` were ignored.

**Oracle.** A Ghostty-grammar fixture (unquoted `theme = Catppuccin Latte`,
`font-size = 16`) overlaid on a host-surface with `font-size = 13` and no
theme, with `themes/` pointing at `fixtures/look/themes/`. Resolved
`font_size` is 16. Resolved background equals the `background =` hex
parsed from that theme **file**, which is not the Chip 0 default
`#121212`. Downstream of `overlay_look` reading the file, not of a Rust
`LATTE_BG` constant and not of a string the parser copied from the
fixture into `theme`.

**Procedure.** Library test. In-memory fixture matching `~/.config/rill/config`.
No packaged app. `look_file_candidates` names `$HOME/.config/rill/config`,
not Ghostty or cmux paths.

**Required mutation.** `RILL_MUTATE=skip_ghostty_overlay`. Font size stays
13 and background is not Latte.

---

## T-LOOK-UNKNOWN — unknown theme does not replace host-surface colors

Authority: ADR 0017 D2.

**Oracle.** host-surface resolved from the Latte **file**, then overlay
`theme = NotARealTheme`. Background stays the file's `background =`.
Downstream of resolve-or-keep, not of a hardcoded pass.

**Required mutation.** `RILL_MUTATE=unknown_theme_wipes`. Colors become
unset / Chip 0 dark.

---

## T-LOOK-CELL — empty cell is not Chip 0 default dark

Authority: ADR 0017 D3.

**Oracle.** `Chip0` snapshot of an empty grid, then `apply_theme` with colours
loaded from the Latte **file**. Cell (0,0) `bg` equals that file's
`background =`, not `#121212`. Downstream of the VT default and the remap.
A test that only checks `HostSurface.colors` is not this gate.

**Required mutation.** `RILL_MUTATE=skip_theme_apply`.

---

## T-LOOK-FILE — theme file wins over a compiled-in RGB table

Authority: ADR 0017 D2, SPEC-DISPLAY §10,
[#259](https://github.com/mahboobmonnamd/RILL/issues/259).

**Bug (doc comment).** `theme = Catppuccin Latte` resolved a Rust `match` of
Catppuccin RGB, so a `themes/` file whose `background =` differed from that
table was ignored. Values must come from the theme file.

**Oracle.** A temp `themes/Catppuccin Latte` whose `background = #a1b2c3`
(not official Latte). `parse_look_keys("theme = Catppuccin Latte")` with
that directory. Resolved background is `#a1b2c3`, not `#eff1f5`. Downstream
of reading the file. A test that asserts the official Latte constant can
pass with a hardcoded table.

**Procedure.** Library test. Temp directory. No packaged app.

**Required mutation.** `RILL_MUTATE=invent_theme_rgb`. Resolve returns
invented Latte RGB without reading the file.

---

## T-LOOK-ANSI — SGR colours are the theme-file palette (Ghostty/cmux)

Authority: [ADR 0017](adr/0017-ghostty-look-windowed-default.md) D3,
[SPEC-DISPLAY](spec/SPEC-DISPLAY.md) §10,
[#274](https://github.com/mahboobmonnamd/RILL/issues/274).

**Bug (doc comment).** The same Catppuccin Latte file is readable in Ghostty
and cmux (dark body, green `)`, red unknown command). Rill painted
libghostty-vt's built-in SGR green (`#b5bd68`) and default white on
`#eff1f5`, so typed commands were unreadable.

**Oracle.** `Chip0::apply_look` from `fixtures/look/themes/Catppuccin Latte`,
then feed `CSI 32 m G`. Snapshot cell `G` `fg` equals `palette = 2=` parsed
from that **file**. Contrast vs file `background` must beat Chip 0's built-in
green `#b5bd68` (the wash-out). Unstyled `A` equals file `foreground =` with
WCAG contrast ≥ 4.5 vs that background. Downstream of the VT snapshot.

**Procedure.** Library test. No packaged app.

**Required mutation.** `RILL_MUTATE=skip_vt_look_colors`. SGR 32 stays the
built-in green.

Demonstrated **red** (`fg=0xb5bd68ff` vs file `#40a02b`; unstyled
`0xffffffff` vs `#4c4f69`).

Chip 1 counterpart (not this gate):
[#267](https://github.com/mahboobmonnamd/RILL/issues/267) palette identity,
[#271](https://github.com/mahboobmonnamd/RILL/issues/271) T-CHIP1-LOOK-ANSI
(library, blocked on colour ADR),
[#272](https://github.com/mahboobmonnamd/RILL/issues/272) M7 live must keep
packaged T-LOOK-ANSI. Chrome inset is host [#270](https://github.com/mahboobmonnamd/RILL/issues/270).

---

## T-GLYPH-SCALE — atlas glyphs match backing-scale cell pixels

Authority: [ADR 0003](adr/0003-display-pipeline.md) D1, [SPEC-DISPLAY](spec/SPEC-DISPLAY.md) §4–5,
[#273](https://github.com/mahboobmonnamd/RILL/issues/273),
[#275](https://github.com/mahboobmonnamd/RILL/issues/275).

**Bug (doc comment).** Latte colours were correct and the cursor filled the
cell, but typed letters were tiny specks: CoreText rasterised at font
point size while `cellPx` used `_cellW * backingScaleFactor`.

**Oracle.** Packaged GUI on a Retina backing scale (`cell_px` > 1.5 ×
look `font-size`). Heartbeat `glyph_m` (atlas height of `M`) / `cell_px`
≥ 0.7. Downstream of the atlas, not of `font-size` in the config file.

**Procedure.** Packaged GUI. Same heartbeat path as T-WINDOWED.

**Required mutation.** `RILL_MUTATE=skip_glyph_backing_scale`. Atlas stays
1× while `cell_px` stays backing pixels; ratio falls below 0.7. Hosted
`macos-14` is 1× (`cell_px=16` at 16pt) and cannot detect the bug;
`validate-spike0.sh` records the Red and does not fail the job when
`RILL_NFR_OPTIONAL=1`.

---

## T-LOOK-GLASS — background-opacity must not make the window translucent

Authority: ADR 0017 D3, ADR 0009, SPEC-DISPLAY §3 / §10,
[#259](https://github.com/mahboobmonnamd/RILL/issues/259).

**Bug (doc comment).** Windowed launch set `NSWindow.alphaValue` from
`background-opacity = 0.95`, so the Metal surface was glass and the theme
looked washed out (desktop showed through).

**Oracle.** Packaged `Rill.app`, `RILL_CONFIG` with `background-opacity = 0.95`,
not fullscreen. Heartbeat reports `opaque=1` and `alpha=100`. Downstream of
the window, not of a flag the test wrote. Socket-only tests do not close this.

**Procedure.** Packaged GUI. Same heartbeat path as T-WINDOWED.

**Required mutation.** `RILL_MUTATE=window_alpha_from_opacity`. Heartbeat
`alpha` is not 100.

---

## T-WINDOWED — launch is not fullscreen

Authority: ADR 0017 D1, SPEC-DISPLAY §3,
[#259](https://github.com/mahboobmonnamd/RILL/issues/259).

**Bug (doc comment).** `make run` called `toggleFullScreen:` after
`makeKeyAndOrderFront:`, so every launch entered a Space.

**Oracle.** Packaged `Rill.app` with no `--nfr-key` and no
`RILL_TEST_EXIT_FULLSCREEN`. Heartbeat file reports `fullscreen=0` and `seq`
advances. Downstream of the window style mask, not of a flag the test wrote.

**Procedure.** Packaged GUI. Socket-only tests do not close this.

**Required mutation.** `RILL_MUTATE=always_toggle_fullscreen`. Heartbeat
shows `fullscreen=1`.

---

## T-SPLIT — window is three panes around Chip 0

Authority: [ADR 0018](adr/0018-three-pane-host-chrome.md),
[SPEC-CHROME](spec/SPEC-CHROME.md),
[#260](https://github.com/mahboobmonnamd/RILL/issues/260).

**Bug (doc comment).** `contentView` was the `MTKView` alone, so there was no
navigation column, no inspector, and no place for a workspace list.

**Oracle.** Packaged `Rill.app` heartbeat (`RILL_TEST_HEARTBEAT`) reports
`chrome=3`, `left` and `right` widths &gt; 0, `center` width &gt; 0, and
`first=terminal`. Those numbers are the `NSSplitView` subview frames and the
window first responder after layout, not a constant the test wrote.

**Procedure.** Packaged GUI. No `--nfr-key`. No `RILL_TEST_EXIT_FULLSCREEN`.
Socket-only tests do not close this.

**Required mutation.** `RILL_MUTATE=no_chrome`: `contentView` is
`TerminalView` again. Heartbeat `chrome` is not 3 (left/right collapse).

Demonstrated **red** on packaged `Rill.app` at `cdac6c5` (heartbeat
`seq=… fullscreen=1` with no chrome fields). Demonstrated **green** on this
branch; `no_chrome` went red (`chrome=1 left=0 right=0 first=terminal`).
CI on `gates.yml` is the D8 closer.

---

## T-SPLIT-LOOK — chrome background is the derived surface, Latte and Mocha

Authority: [ADR 0018](adr/0018-three-pane-host-chrome.md) D5,
[ADR 0017](adr/0017-ghostty-look-windowed-default.md) D3,
[SPEC-CHROME](spec/SPEC-CHROME.md) §4a,
[#269](https://github.com/mahboobmonnamd/RILL/issues/269),
[#270](https://github.com/mahboobmonnamd/RILL/issues/270).

**Bug (doc comment).** Sidebars used `colorWithCalibratedWhite:0.09` while
Chip 0 remapped to Catppuccin Latte, so the window was a dark frame around
a light terminal. After look-file paint, sidebars still used the file
`background`, so Latte chrome and Chip 0 were the same cream.

**Oracle.** Packaged `Rill.app` heartbeat `nav_bg` is the left pane
`CALayer.backgroundColor` as RRGGBB. Parse `background =` from
`fixtures/look/themes/Catppuccin Latte` (and separately Mocha). Expected
chrome is that hex with each channel saturating-minus 9. `nav_bg` MUST
equal that derived hex and MUST NOT equal the file background. Not Chip 0
`#121212`, not a compiled mantle table. Downstream of the layer.

**Procedure.** Two launches. `RILL_CONFIG` with `theme = Catppuccin Latte`
then `theme = Catppuccin Mocha`. Packaged `Resources/themes/` supplies the
files. Socket-only tests do not close this.

**Required mutation.** `RILL_MUTATE=hardcoded_chrome_gray`. `nav_bg` stays
near `#171717` for both launches.

Demonstrated **red** with hardcoded chrome (`nav_bg=1e1e1e` vs Latte
derived surface). Demonstrated **red** again when chrome matched file
`background` (`nav_bg=eff1f5` vs derived `#e6e8ec`; Mocha `1e1e2e` vs
`#151525`). Demonstrated **green** after derived-surface paint
(`nav_bg=e6e8ec` / `151525`); `hardcoded_chrome_gray` stays the invert.

---

## T-CHROME-INSET — section labels match look padding-y

Authority: [ADR 0018](adr/0018-three-pane-host-chrome.md) D6,
[SPEC-CHROME](spec/SPEC-CHROME.md) §4b,
[#270](https://github.com/mahboobmonnamd/RILL/issues/270).

**Bug (doc comment).** Workspaces sat in leftover space under the titlebar
because labels were framed for a 680pt pane (`y = 680 - 36`) while Chip 0
used `padding-y = 8`.

**Oracle.** Packaged heartbeat `nav_top` is the left pane's distance from
its top to `chrome-left-heading`'s frame (flipped: `origin.y`; unflipped:
`NSMaxY(pane) - NSMaxY(heading)`). `pad_y` is Chip 0's look padding.
`|nav_top - pad_y| ≤ 1`. Downstream of the frames, not of a constant the
chrome copied into the heartbeat.

**Procedure.** Packaged GUI. Socket-only tests do not close this.

**Required mutation.** `RILL_MUTATE=hardcoded_chrome_y`. `nav_top` stays
near 20 (the 680 − 36 template).

Demonstrated **red** on packaged `Rill.app` (`nav_top=20` `pad_y=8`
`chrome_font=11`). Demonstrated **green** after live-bounds layout
(`nav_top=8` `pad_y=8`); `hardcoded_chrome_y` went red (`nav_top=20`).

---

## T-CHROME-FONT — section labels are control size, not caption size

Authority: [ADR 0018](adr/0018-three-pane-host-chrome.md) D6,
[SPEC-CHROME](spec/SPEC-CHROME.md) §4b,
[#270](https://github.com/mahboobmonnamd/RILL/issues/270).

**Bug (doc comment).** Workspaces / On \<home\> used 11pt caption size next
to a 16pt JetBrains Mono grid.

**Oracle.** Packaged heartbeat `chrome_font` is
`chrome-left-heading`'s `NSTextField.font.pointSize` after layout. It MUST
be `NSFont.systemFontSize` (≥ 13). Not 11. Downstream of the field, not of
a number the chrome wrote for the test.

**Procedure.** Packaged GUI. Socket-only tests do not close this.

**Required mutation.** `RILL_MUTATE=tiny_chrome_font`. `chrome_font` is 9.

Demonstrated **red** on packaged `Rill.app` (`chrome_font=11`).
Demonstrated **green** after control-size type (`chrome_font=13`);
`tiny_chrome_font` went red (`chrome_font=9`).

---

## T-DOCK-REOPEN — Dock click shows the window

Authority: [ADR 0019](adr/0019-dock-reopen-shows-window.md),
[SPEC-DISPLAY](spec/SPEC-DISPLAY.md) §3,
[#262](https://github.com/mahboobmonnamd/RILL/issues/262).

**Bug (doc comment).** After `make run`, switching to another app and
clicking Rill in the Dock does not show the window. Quit and `make run`
again is required.

**Oracle.** Packaged `Rill.app`. After the window is ordered out, the Dock
reopen selector (`applicationShouldHandleReopen:hasVisibleWindows:`) makes
it visible and key. Heartbeat reports `visible=1` and `key=1` from
`NSWindow.isVisible` / `isKeyWindow`, then `seq` increases. Downstream of
the window, not of a flag the test wrote.

**Procedure.** Packaged GUI. `RILL_TEST_DOCK_REOPEN=1` orders the window
out, then sends the same reopen selector Dock uses. No GUI click in CI.
Socket-only tests do not close this.

**Required mutation.** `RILL_MUTATE=skip_dock_reopen`: reopen does not
`makeKeyAndOrderFront:`. This test MUST go red.

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

Authority: [ADR 0011](adr/0011-session-graph.md),
[ADR 0014](adr/0014-m1-first-slice-closes.md),
[ADR 0015](adr/0015-m1-persist-remainder.md),
[SPEC-GRAPH](spec/SPEC-GRAPH.md),
[#16](https://github.com/mahboobmonnamd/RILL/issues/16),
[#28](https://github.com/mahboobmonnamd/RILL/issues/28),
[#29](https://github.com/mahboobmonnamd/RILL/issues/29),
[#31](https://github.com/mahboobmonnamd/RILL/issues/31),
[#254](https://github.com/mahboobmonnamd/RILL/issues/254),
[#255](https://github.com/mahboobmonnamd/RILL/issues/255).
Library tests live in `crates/rill-kernel/tests/gates.rs` and
`crates/rilld/tests/gates.rs`. Packaged N-leaf persist is T-KILL with
`RILL_TEST_SECOND_LEAF` (ADR 0015 D8).

Every first-slice gate below is **Proven** ([ADR 0014](adr/0014-m1-first-slice-closes.md) D1).
Kernel suite: `fast.yml` on `main` after those PRs, mutations red in this closer.
Named-id / flood: PRs #30 / #32, wired into `validate-spike0.sh`.
Persist remainder gates follow; mutations are wired into `fast.yml` (kernel)
and `validate-spike0.sh` (rilld).

| ID | Status | Negative control | Notes |
|---|---|---|---|
| T-GRAPH-SPAWN | **Proven** | `single_session` automated | [#16](https://github.com/mahboobmonnamd/RILL/issues/16) / [PR #27](https://github.com/mahboobmonnamd/RILL/pull/27) |
| T-GRAPH-ISOLATE | **Proven** | `single_session` automated | PR #27 |
| T-GRAPH-ATTACH | **Proven** | `single_session` automated | PR #27 |
| T-GRAPH-TERMINATE | **Proven** | `terminate_all_leaves` automated | [#29](https://github.com/mahboobmonnamd/RILL/issues/29) / [PR #30](https://github.com/mahboobmonnamd/RILL/pull/30) |
| T-ATTACH-NAMED | **Proven** | `ignore_session_id` automated | [#28](https://github.com/mahboobmonnamd/RILL/issues/28) / PR #30 |
| T-GRAPH-FLOOD | **Proven** | `starve_other_leaves` automated | [#31](https://github.com/mahboobmonnamd/RILL/issues/31) / [PR #32](https://github.com/mahboobmonnamd/RILL/pull/32) |

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

### T-GRAPH-FLOOD — flood on A must not drop B

**Oracle.** Leaf A runs `yes`. A's attach stream MUST contain that child's
`y` output (if it does not, the flood never ran and the test fails as
inconclusive). Leaf B's child `cat`s a unique fixture. Every byte of that
marker MUST appear in B's attach stream. Downstream of the children, not of
a buffer the test copied.

**Procedure.** In-process `Daemon` over `AF_UNIX`. Two connections. Finite
credit on A so the producer outruns the consumer. No GUI.

**Required mutation.** `RILL_MUTATE=starve_other_leaves` (feature `mutate`):
the daemon reads only the default leaf's PTY. B's marker MUST be absent.

### T-ATTACH-PROTO — protocol mismatch is refused

**Oracle.** An 18-byte ATTACH with protocol ≠ 1 yields
`REFUSED{ProtocolMismatch}` on that connection. The default leaf stays
unclaimed by that client.

**Procedure.** In-process `Daemon` over `AF_UNIX`.

**Required mutation.** `RILL_MUTATE=ignore_protocol_version`: accept any
protocol byte. This test MUST go red.

### T-GRAPH-NESTED — nested rilld bind is refused

**Oracle.** With `RILL_INSIDE=1` and `RILL_ALLOW_NESTED` unset, `Daemon::bind`
returns `NestedLaunch`. Downstream of the env the kernel sets on the child,
not of a flag the test wrote into a struct.

**Procedure.** In-process daemon. No GUI.

**Required mutation.** `RILL_MUTATE=skip_nested_guard`: bind succeeds. This
test MUST go red.

### T-GRAPH-DELIVERY — DATA write is Dispatched

**Oracle.** After a writer ATTACH and a DATA frame, `last_delivery()` is
`Dispatched` and `io_journal` contains `PtyWrite`. Not a copy of the input
buffer.

**Procedure.** In-process `Session`. No GUI.

**Required mutation.** `RILL_MUTATE=always_pending`: skip the PTY write. This
test MUST go red.

### T-GRAPH-EVENTS — unique ids; terminate is idempotent

**Oracle.** Event ids are unique. A second `terminate` of a dead leaf does not
emit a second Terminate event and MUST NOT kill another live child (`kill(pid, 0)`
on B). Reap records one Exit for A.

**Procedure.** In-process kernel. Two `/bin/sh -c 'exec sleep 60'` leaves.

**Required mutation.** `RILL_MUTATE=duplicate_event_ids`: every event reuses
id 1. This test MUST go red.

### T-GRAPH-LAYOUT — snapshot names every live leaf

**Oracle.** After two `spawn_leaf` calls, `layout_snapshot()` has two rows with
distinct `child_pid` values and both ids.

**Procedure.** In-process kernel.

**Required mutation.** `RILL_MUTATE=omit_second_leaf`: snapshot length 1. This
test MUST go red.

### T-GRAPH-EPHEMERAL — historical library fixture makes Drop kill

**Oracle.** With `RILL_EPHEMERAL=1`, dropping a `Session` makes
`kill(pid, 0)` fail. Default persist remains T-KILL.

**Procedure.** In-process kernel. `/bin/sh -c 'exec sleep 30'`.

**Required mutation.** `RILL_MUTATE=ignore_ephemeral`: Drop is a no-op. This
test MUST go red.

**Authority boundary.** This preserves evidence for the current library test
branch only. It is not a product preference or client-lifecycle contract. ADR
0053 D3 forbids automatic terminate-on-quit; production termination requires
the explicit flow in SPEC-DOMAIN-LIFECYCLE §5.

### T-GRAPH-OBSERVE — observer sees DATA and cannot write

**Oracle.** Writer attach plus observe attach: observer stream contains the
child's fixture marker. Observer DATA MUST NOT appear in that stream (it was
not written to the PTY).

**Procedure.** In-process `Daemon` over `AF_UNIX`. Two connections.

**Required mutation.** `RILL_MUTATE=allow_observer_write`: observer DATA is
written through. This test MUST go red.

### T-GRAPH-KILL-N — GUI SIGKILL must not kill any live leaf

**Oracle.** Packaged T-KILL: after SIGKILL of the GUI process group, both
pidfile pids and rilld remain alive (`kill(pid, 0)`). Reattach shows prior
output. Socket-only drop of `Daemon` is supporting evidence, not the closer.

**Procedure.** Packaged `Rill.app` with `RILL_TEST_SECOND_LEAF=1`. Existing
T-KILL mutation `drop_POSIX_SPAWN_SETSID` MUST still go red.

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

Authority: [ADR 0012](adr/0012-chip1-isolated-vt.md),
[ADR 0020](adr/0020-chip1-parser-in-tree.md),
[ADR 0021](adr/0021-chip1-colour-identity.md),
[ADR 0022](adr/0022-chip1-reply-channel.md),
[ADR 0023](adr/0023-chip1-v0-defers-character-width.md) as amended by
[ADR 0035](adr/0035-chip1-character-width.md),
[SPEC-CHIP1](spec/SPEC-CHIP1.md) and its six slice specs,
[M4-HANDOFF](M4-HANDOFF.md), [M4-PLAN](M4-PLAN.md),
[#6](https://github.com/mahboobmonnamd/RILL/issues/6).

[S-VT #21](https://github.com/mahboobmonnamd/RILL/issues/21) is closed
([SPIKE-VT](SPIKE-VT.md)): parser in-tree, `vte` dev-only differential.
The colour ADR [#267](https://github.com/mahboobmonnamd/RILL/issues/267)
required now exists (ADR 0021), so
[#271](https://github.com/mahboobmonnamd/RILL/issues/271) T-CHIP1-LOOK-ANSI is
**unblocked**. Live swap must not regress packaged T-LOOK-ANSI:
[#272](https://github.com/mahboobmonnamd/RILL/issues/272), and per ADR 0035
character width (T-CHIP1-WIDTH Proven) is also an M7 precondition.

These gates are **Red**. Named here first. No `vt-engine` behaviour until they
exist and have been observed failing (ADR 0002 D2). Not live. Not T-NFR.
S-VT numbers are research and MUST NOT be cited as gate evidence (ADR 0002 D8).

Oracle for every gate: `snapshot()` (codepoint, cursor, attrs, materialised
colour, `cells.len()`, damage), a second instance’s grid after resync emit, or
drained replies. Never a copy of the input, never the `\x1b[2J` prefix the emit
path prepends, never "equals `vte`" alone
([SPEC-VT-CONFORMANCE](spec/SPEC-VT-CONFORMANCE.md) §1).

### T-CHIP1-ASCII — printable lands in the POD grid

**Oracle.** After `feed(b"Hello")`, row 0 cells 0..4 are `H e l l o`.

**Procedure.** In-process Chip 1, 80×24 (or 40×5). No PTY.

**Required mutation.** `RILL_MUTATE=drop_print` — drop `feed` / do not write cells.

### T-CHIP1-BYTES — invalid UTF-8 reaches the parser

**Oracle.** Same fixtures as T-BYTES. ASCII `A` present when the fixture
contains `0x41`. High bytes produce a non-ASCII cell except CSI-high-param,
which MAY consume the high byte without a cell. MUST NOT drop `>= 0x80`
before parse.

**Required mutation.** Drop bytes `>= 0x80` before parse.
**Negative control.** `RILL_MUTATE=drop_high_bytes` (`feature = "mutate"`).

**The mutation MUST be detected by** `lone_continuation`, `truncated_3byte`,
`overlong_slash`, `lone_surrogate`, `bom_then_high`, `c1_in_utf8`,
`zwj_emoji.bin`, `invalid_utf8.bin`.

`csi_high_param` is **blind** to it and MUST NOT be cited as carrying it: S-VT
measured that a high byte inside a CSI parameter changes no cell whether parsed
or dropped, for every candidate (ADR 0020 D7,
[SPEC-VT-CONFORMANCE](spec/SPEC-VT-CONFORMANCE.md) §3). It stays in the corpus
as a no-crash, no-spurious-cell case.

### T-CHIP1-C1 — a decoded C1 scalar paints and does not open a CSI

Authority: [ADR 0020](adr/0020-chip1-parser-in-tree.md) D3,
[SPEC-VT-PARSER](spec/SPEC-VT-PARSER.md) §2.

**Bug (doc comment).** `vte` 0.15 dispatches `0x80..=0x9f` to `execute()` as an
8-bit control, so `[0x80, 0x41]` produced a grid identical to one where the byte
was dropped, and T-CHIP1-BYTES could not see its own mutation.

**Oracle.** Feed `[0xc2, 0x9b, 0x41]`: row 0 is U+009B then `A`, and the cursor
advanced two columns — proving `0x9b` did not open a CSI that consumed the `A`.
Feed `[0x80, 0x41]`: exactly one U+FFFD, then `A`.

**Required mutation.** `RILL_MUTATE=c1_as_control` — treat `0x80..=0x9f` as
controls. Row 0 loses its non-ASCII cell.

### T-CHIP0-C1-PAINT — Chip 0 paints decoded C1 scalars (macOS / Zig)

Authority: [ADR 0020](adr/0020-chip1-parser-in-tree.md) D3,
[SPEC-VT-CONFORMANCE](spec/SPEC-VT-CONFORMANCE.md) §4 Chip 0 differential,
[#304](https://github.com/mahboobmonnamd/RILL/issues/304).

**Status.** Red until demonstrated on `gates.yml` (Zig + libghostty-vt). MUST
NOT run in `fast.yml` (SPEC-CHIP0 §9). Secondary to T-CHIP1-C1. Does not swap
the live chip.

**Oracle.** Feed `[0xc2, 0x9b, 0x41]` to Chip 0; row 0 is U+009B then `A`;
`cursor_col == 2`. If libghostty-vt *executes* C1 instead, this gate stays red
and the live-swap ADR must name that divergence — fail closed, do not guess.

**Required mutation.** `RILL_MUTATE=c1_as_control` — blank painted C1 cells
in the snapshot. The U+009B cell disappears.

### T-CHIP1-GRAPHEME — long cluster does not overrun

**Oracle.** `e` + 40× U+0301: snapshot survives; `cursor_col == 1`;
`grapheme_truncated >= 1` or the base is still a visible cell. No panic.
ZWJ family (`fixtures/bytes/zwj_emoji.bin`; fail if absent) occupies **2**
columns after width (ADR 0035 D1), not 1.

**Required mutation.** Fixed 8-slot stack buffer / silent drop of extras.

### T-CHIP1-CRLF — CR LF moves the cursor

**Oracle.** `A\r\nB` → `B` at row 1 col 0 (or documented equivalent).

**Required mutation.** `RILL_MUTATE=ignore_crlf` — ignore CR/LF.

### T-CHIP1-CUP — CSI CUP positions the cursor

**Oracle.** `ESC[5;10H` → cursor at 1-based (5,10), i.e. row 4 col 9
0-based, documented in the test.

**Required mutation.** `RILL_MUTATE=ignore_csi` — ignore CSI.

### T-CHIP1-SGR — bold sets attrs bit 0

**Oracle.** `ESC[1mX` → that cell `attrs & 1 != 0`.

**Required mutation.** `RILL_MUTATE=ignore_sgr` — ignore SGR.

### T-CHIP1-ED — erase display clears to space

**Oracle.** Feed text, `ESC[2J`, every cell codepoint is space `32`.

**Required mutation.** `RILL_MUTATE=noop_ed` — ED is a no-op.

### T-CHIP1-ALT — 1049 preserves primary

**Oracle.** Feed A, `?1049h`, feed B, `?1049l` → A visible, B gone.

**Required mutation.** Single buffer.

### T-CHIP1-SIZE — snapshot is exactly cols×rows

**Oracle.** After resize 40×5, `cells.len() == 200`.

**Required mutation.** `RILL_MUTATE=unbounded_history` — unbounded history in the snapshot.

### T-CHIP1-POD — PodCell is 16 bytes

**Oracle.** `size_of::<PodCell>() == 16`. Lint `no-cell-strings`.

**Required mutation.** Add a `String` field.

### T-CHIP1-RESYNC — emit bytes reconstruct the grid

**Oracle.** Feed a marker, emit, second instance feeds only the emit bytes,
row 0 matches. MUST NOT assert on `\x1b[2J`.

**Required mutation.** `RILL_MUTATE=empty_resync` — emit empty or emit only the prefix.

### T-CHIP1-WRAP — the last column defers its wrap

Authority: [SPEC-VT-SCREEN](spec/SPEC-VT-SCREEN.md) §2.

**Oracle.** On a 10-column grid, feed 10 printables then one more. The 11th lands
at row 1 column 0; row 0 holds all ten. Downstream of the cursor, not of a
counter the test kept.

**Required mutation.** `RILL_MUTATE=eager_wrap` — wrap on reaching the last
column. Row 0 holds nine and the grid scrolls a row early.

### T-CHIP1-SCROLL — DECSTBM confines the scroll

Authority: [SPEC-VT-SCREEN](spec/SPEC-VT-SCREEN.md) §5.

**Oracle.** On a 6-row grid set `CSI 2;4 r`, fill rows, force a scroll. Rows 0
and 5 are unchanged; rows 1..3 shifted. This is what `less` and `vim` status
lines depend on.

**Required mutation.** `RILL_MUTATE=ignore_decstbm` — scroll the whole grid.
Rows 0 and 5 move.

### T-CHIP1-DAMAGE — an untouched frame can be skipped

Authority: [SPEC-VT-SCREEN](spec/SPEC-VT-SCREEN.md) §7,
[SPEC-VT-TYPES](spec/SPEC-VT-TYPES.md) §3.

**Oracle.** Feed one character on row 3 and snapshot: damage covers row 3 and not
row 0. Snapshot again with no feed: `full_damage == false` and
`damage_row0 > damage_row1`, so the caller may skip.

**Required mutation.** `RILL_MUTATE=always_full_damage`. The skip assertion goes
red.

### T-CHIP1-BOUNDS — hostile sequences stay bounded

Authority: [SPEC-VT-PARSER](spec/SPEC-VT-PARSER.md) §6,
[ADR 0012](adr/0012-chip1-isolated-vt.md) D9.

**Oracle.** 8 MiB unterminated OSC, 8 MiB unterminated DCS, and a CSI with
1,000,000 parameters: a counting allocator around `feed` reports no growth
attributable to `feed`, and a following `feed(b"A")` still prints. S-VT measured
both candidates as bounded here, so this gate protects a property we have.
The counting allocator is process-global: this gate MUST run with
`--test-threads=1`, or the allocator MUST attribute allocations per thread.

**Required mutation.** `RILL_MUTATE=unbounded_osc` — accumulate OSC into an
uncapped `Vec`. Allocation grows with input.

### T-CHIP1-COLOR-IDENTITY — SGR keeps its palette index until materialisation

Authority: [ADR 0021](adr/0021-chip1-colour-identity.md) D1–D2,
[SPEC-VT-COLOR](spec/SPEC-VT-COLOR.md) §6,
[#267](https://github.com/mahboobmonnamd/RILL/issues/267).

**Bug (doc comment).** Chip 0 snapshots are already RGB, so the host could only
remap cells still equal to the VT default and ANSI 0–15 could not be themed
without a compiled RGB catalog.

**Oracle.** Feed `CSI 32 m G`. The cell's fg is `Indexed(2)` before
materialisation. Materialise the same engine state against a palette parsed from
`fixtures/look/themes/Catppuccin Latte`, then `Catppuccin Mocha`: the two `fg`
values differ, and each equals that file's `palette = 2=`.

**Required mutation.** `RILL_MUTATE=sgr_rgb_at_parse` — resolve SGR to RGB at
parse time. Both materialisations return the same value.

### T-CHIP1-LOOK-ANSI — SGR colours come from the theme file (Latte and Mocha)

Authority: [ADR 0021](adr/0021-chip1-colour-identity.md) D4,
[SPEC-VT-COLOR](spec/SPEC-VT-COLOR.md) §6,
[#271](https://github.com/mahboobmonnamd/RILL/issues/271).

**Bug (doc comment).** User-reported 2026-08-17: Catppuccin Latte was readable in
Ghostty and cmux and unreadable in Rill until the Chip 0 adapter loaded the theme
file palette. SGR greens stayed the built-in `#b5bd68` and washed out on Latte's
`#eff1f5`. Chip 1 must not regress that when it becomes live.

**Oracle.** With the Latte palette applied, `CSI 32 m G` gives `fg` equal to that
file's `palette = 2=`. An unstyled `A` gives `fg` equal to the file's
`foreground =`, with WCAG contrast ≥ 4.5 against the file's `background =`.
Repeat for Mocha so one constant cannot fake it. Library gate, no packaged app,
no Ghostty FFI.

**Required mutation.** `RILL_MUTATE=skip_file_palette`. SGR 32 stays the VT
default and Latte contrast fails.

Packaged T-LOOK-ANSI / T-LOOK-CELL / T-SPLIT-LOOK stay Chip 0 / `lane:host`;
this gate does not close them ([#272](https://github.com/mahboobmonnamd/RILL/issues/272)).

### T-CHIP1-REPLY — DA and DSR are answered

Authority: [ADR 0022](adr/0022-chip1-reply-channel.md) D5,
[SPEC-VT-REPLY](spec/SPEC-VT-REPLY.md) §6.

**Bug (doc comment).** SPEC-CHIP1 §3 required answering DA/DSR while the §2 API
had no channel for a reply to leave the crate, so a `vim` that probes would hang
against a conforming implementation (SPIKE-VT Result 7).

**Oracle.** Feed `CSI 5 ; 3 H` then `CSI 6 n`; `take_replies()` yields
`CSI 5 ; 3 R`. Feed `CSI c`; the reply is `CSI ? 6 c`. A second `take_replies()`
is empty. The drained bytes are parsed — not a constant the test prepended, and
not a flag saying a reply was queued.

**Required mutation.** `RILL_MUTATE=no_reply` — never enqueue.
Second mutation `unbounded_replies` MUST turn the `replies_dropped` assertion red.

### T-CHIP1-WIDTH — CJK occupies two columns; `日本X` cursor at column 5

Authority: [ADR 0035](adr/0035-chip1-character-width.md) D7,
[SPEC-VT-SCREEN](spec/SPEC-VT-SCREEN.md) §9.
Replaces T-CHIP1-WIDTH-DEFERRED ([ADR 0023](adr/0023-chip1-v0-defers-character-width.md) D3);
do not delete that history quietly.

**Bug (doc comment).** v0 advanced one column per scalar, so `'日本X'` left the
cursor at column 3. A conforming terminal advances 5. Summing per-scalar
widths is also wrong: a ZWJ family is 2 columns, not 8 (SPIKE-WIDTH Result 2).

**Oracle.** Feed `日本X` into an 80×24 Chip 1. `cursor_col == 5`. Cells:
`日` lead at col 0 (`attrs` bit3) + tail at col 1 (`attrs` bit4, codepoint
not 0), `本` lead col 2 + tail col 3, `X` at col 4 with neither wide bit.
Primary oracle is those snapshot fields, written in the test — not
"equals `unicode-width`". Extra: ZWJ fixture occupies 2 columns; a wide
glyph at the last column wraps instead of splitting. ECH, DCH, or overwrite
of a wide lead or tail clears **both** halves to space and the current
background; no orphan tail (ADR 0035). Pending cluster survives `feed()`:
combining / ZWJ in the next `feed()` still appends; `snapshot()` after the
first `feed()` of `日` already shows width 2 (ADR 0035 D8).

**Required mutation.** `RILL_MUTATE=narrow_cjk` — force one column per
scalar (v0). That must turn this gate red. Smash mutation
`RILL_MUTATE=orphan_wide_tail` must turn the ECH-of-lead assertion red.

### T-CHIP1-MODE — host encoder flags tracked and polled

Authority: [ADR 0036](adr/0036-chip1-mode-state-channel.md),
[SPEC-VT-MODE](spec/SPEC-VT-MODE.md).

**Oracle.** After `CSI ? 1 h/l`, `ESC =` / `ESC >`, `?2004`, mouse `?1000/1002/1003/1006`,
`?1004`, `?1049`, and `?25`, `mode_state()` matches the expected booleans.
`reset()` restores `TerminalModeState::fresh()`.

**Required mutation.** `RILL_MUTATE=ignore_mode_updates` — modes do not change.

### T-CHIP1-DIFF — an independent parser agrees over the corpus

Authority: [ADR 0020](adr/0020-chip1-parser-in-tree.md) D2,
[SPEC-VT-CONFORMANCE](spec/SPEC-VT-CONFORMANCE.md) §4.

**Oracle.** `vte` (`[dev-dependencies]`, `default-features = false`) and the
in-tree parser drive the same `Actions` sink over the fixture corpus and the v0
sequence cases; the resulting grids and cursors agree, with divergence 1 (C1
handling) applied as an explicit remap.

**Secondary oracle only.** This is the named exception to SPEC-VT-CONFORMANCE
§1's ban on "equals `vte`" as a gate. It remains secondary: no *other* gate may
be expressed solely as "equals `vte`", and where they disagree the spec wins
and the divergence is registered. Mutations MUST hit the in-tree parser front
only (SPEC-VT-CONFORMANCE §4).

**Required mutation.** Any parser mutation above must also turn this red; if a
mutation leaves the differential green, the corpus is too small.

---

## Milestone gates (M2–M6)

Spike 0 and M1 gates are above. Gates for the catalog milestones are defined in
their own specs, each with the same four things this document requires — oracle,
procedure, required mutation, negative control — and each carrying a status.

Every gate listed in these specs is **Red**: defined, not yet demonstrated. None
may be cited as evidence until it has been observed failing on a build where the
behaviour is absent, with the failure output in the PR (ADR 0002 D2).

### M2 F-003–F-020 catalog-to-gate map (Red)

The following are test-case definitions only. They do not authorize executable
tests or production behavior. Every user-visible closer is a packaged
`Rill.app` test; a socket or library result may prove a lower-plane
sub-property but cannot close it (ADR 0002 D8).

| Feature | Catalog issue | Test case | Accepted scope |
|---|---:|---|---|
| F-003 Named sessions | [#40](https://github.com/mahboobmonnamd/RILL/issues/40) | T-NAV-NAME | ADR 0038 D6; SPEC-NAV §6 |
| F-004 Workspaces | [#41](https://github.com/mahboobmonnamd/RILL/issues/41) | T-NAV-WORKSPACE-PROJECTION | ADR 0038 D1/D6; SPEC-NAV §§1, 6 |
| F-005 Groups | [#42](https://github.com/mahboobmonnamd/RILL/issues/42) | Blocked | Group topology is accepted; collapse presentation is not |
| F-006 Tabs | [#43](https://github.com/mahboobmonnamd/RILL/issues/43) | T-NAV-STACK (F-006/F-008) | ADR 0038 D2; SPEC-NAV §2 |
| F-007 Nested splits | [#44](https://github.com/mahboobmonnamd/RILL/issues/44) | T-NAV-REPARENT (F-007/F-017/F-018) | ADR 0038 D4; SPEC-NAV §4 |
| F-008 Surface stacks | [#45](https://github.com/mahboobmonnamd/RILL/issues/45) | T-NAV-STACK; T-SURF-BROWSER blocked | ADR 0038 D2; SPEC-NAV §2 |
| F-009 Close | [#46](https://github.com/mahboobmonnamd/RILL/issues/46) | T-NAV-CLOSE | ADR 0038 D3; SPEC-NAV §3 |
| F-010 Dashboard | [#47](https://github.com/mahboobmonnamd/RILL/issues/47) | T-INV-SELECT (F-010/F-012/F-013/F-014) | ADR 0039 D1/D2; SPEC-NAV §7 |
| F-011 Agent dashboard | [#48](https://github.com/mahboobmonnamd/RILL/issues/48) | T-INV-AGENT-EMPTY | ADR 0039 D4; SPEC-NAV §7 |
| F-012 Session/process switcher | [#49](https://github.com/mahboobmonnamd/RILL/issues/49) | T-INV-SELECT | ADR 0039 D1/D2/D4; SPEC-NAV §7 |
| F-013 Command palette | [#50](https://github.com/mahboobmonnamd/RILL/issues/50) | T-INV-SELECT | ADR 0039 D2; SPEC-NAV §7; SPEC-TRUST §§1, 7 |
| F-014 Quick switcher | [#51](https://github.com/mahboobmonnamd/RILL/issues/51) | T-INV-SELECT | ADR 0039 D2; SPEC-NAV §7 |
| F-015 Focus history | [#52](https://github.com/mahboobmonnamd/RILL/issues/52) | T-INV-HISTORY | ADR 0039 D3; SPEC-NAV §8 |
| F-016 Reopen closed | [#53](https://github.com/mahboobmonnamd/RILL/issues/53) | T-INV-REOPEN | ADR 0039 D3; SPEC-NAV §8 |
| F-017 Drag rearrange | [#54](https://github.com/mahboobmonnamd/RILL/issues/54) | T-NAV-REPARENT | ADR 0038 D4; SPEC-NAV §4 |
| F-018 Zoom/equalize | [#55](https://github.com/mahboobmonnamd/RILL/issues/55) | T-NAV-REPARENT | ADR 0038 D4; SPEC-NAV §4 |
| F-019 Layout templates | [#56](https://github.com/mahboobmonnamd/RILL/issues/56) | T-NAV-TEMPLATE | ADR 0038 D4; SPEC-NAV §4 |
| F-020 Sidebar visibility | [#57](https://github.com/mahboobmonnamd/RILL/issues/57) | T-NAV-VIEWSTATE | ADR 0038 D5; SPEC-NAV §5 |

#### T-NAV-NAME — a Session label is runtime-owned and never an execution id

**Feature.** F-003.

**Oracle.** A packaged host renders the cold snapshot's durable Session label,
while the independently observed attach request addresses the selected
`TerminalExecutionId`; neither the Session label nor `SessionId` addresses a
leaf.

**Procedure.** Create two named durable Sessions with distinct executions, read
the cold snapshot, render it, then attach one execution through the normal host
path.

**Required mutation.** `session_label_from_chrome_cache`: render a host-local
label rather than the cold kernel snapshot.

**Negative control.** Manual until the accepted kernel label API and packaged
host wiring exist; the reviewer applies the mutation and records the red
packaged result.

**Precondition.** A packaged app, a runtime with two durable Session objects,
and the accepted runtime-owned Session-label API must exist. Their absence
fails this gate; it does not permit a fabricated chrome label.

#### T-NAV-WORKSPACE-PROJECTION — chrome projects the kernel Workspace tree

**Feature.** F-004.

**Oracle.** The rendered Workspace/Tab rows match an independently fetched cold
kernel snapshot, including the default leaf's cold identity and cwd where
accepted; a chrome-invented row is absent.

**Procedure.** Launch a packaged app over a seeded container tree, fetch the
kernel snapshot on the cold path, and compare its node ids and parentage with
the visible projection.

**Required mutation.** `chrome_invents_workspace_row`: add a host-only row.

**Negative control.** Manual until the cold snapshot-to-host projection exists.

**Precondition.** Packaged app, kernel container snapshot, and accepted cold
cwd/identity readers are all present. No test may substitute an AppKit cache
for the kernel snapshot.

#### T-NAV-GROUP-COLLAPSE — blocked pending an accepted presentation contract

**Feature.** F-005.

**Oracle.** Blocked: ADR 0038 and SPEC-NAV §1 authorize kernel `Group` nodes,
but do not specify collapsible-group view state, persistence, or an observable
collapse result.

**Procedure.** Blocked until an Accepted ADR/spec defines that behavior.

**Required mutation.** Blocked; no production mutation may be invented.

**Negative control.** Blocked; no control exists without an accepted invariant.

**Precondition.** An Accepted contract for group-collapse semantics is required.
Missing authority is a blocked gate, not a skipped implementation.

#### T-NAV-STACK — hidden terminal surfaces keep the same live leaf

**Features.** F-006 and F-008 share this one atomic liveness invariant.

**Oracle.** After an inactive tab or stacked terminal surface is hidden, its
real child continues emitting numbered tokens and retains its pid; re-showing
cold-resyncs the same leaf without spawning it.

**Procedure.** Packaged app with two real leaves. Hide one terminal surface,
observe the hidden child's subsequent tokens and pid with a second process,
then re-show it and observe one cold resync.

**Required mutation.** `hide_detaches_leaf`.

**Negative control.** Manual packaged control until this host path exists.

**Precondition.** Packaged app, two live PTY children, bounded-ring/resync
instrumentation, and an external pid oracle must exist. The horizontal tab-strip
placement is not specified and is not claimed by this test.

#### T-NAV-REPARENT — rearrangement preserves leaf identity and the warm path

**Features.** F-007, F-017, and F-018 share ADR 0038 D4's one atomic
reparent/resize invariant.

**Oracle.** A cold snapshot shows the intended changed parentage or geometry,
while independently observed `SessionId`, `TerminalExecutionId` and child pids
survive. The warm-frame trace contains no new attach/spawn; only in-band
`RESIZE` is allowed.

**Procedure.** In a packaged app with multiple real leaves, split, drag, zoom,
and equalize. Compare pre/post cold snapshots, pids, ids, and warm-frame trace.

**Required mutation.** `reparent_respawns`.

**Negative control.** The library sub-property is automatically checked by
`RILL_MUTATE=reparent_recreates_node`; the packaged mutation control remains
manual until chrome wiring exists.

**Precondition.** Packaged tree host, two live children, snapshot access, and
frame tracing must exist. The current library proof is not a packaged closer.

#### T-SURF-BROWSER — blocked pending the accepted browser-surface contract

**Feature.** F-008's browser-surface portion.

**Oracle.** Blocked: terminal-surface hiding is covered by T-NAV-STACK. A
browser's out-of-process crash, lifecycle, and leaf-write boundary require the
accepted SPEC-SURFACES behavior to be wired before a downstream oracle can be
named.

**Procedure.** Blocked pending that host implementation and its accepted
surface-specific contract.

**Required mutation.** Blocked; do not invent a browser process model.

**Negative control.** Blocked.

**Precondition.** A packaged browser-surface implementation authorized by
SPEC-SURFACES must exist. Until then F-008 does not authorize browser behavior.

#### T-NAV-CLOSE — presentation close resolves inward and preserves executions

**Feature.** F-009.

**Oracle.** A packaged `⌘W` sequence hides surface, pane, tab, Session
presentation, then Workspace presentation without changing the durable
identities. After every step, `kill(pid, 0)` proves every TerminalExecution is
still alive. Closing the window also leaves every child alive and packaged
T-KILL remains green.

**Procedure.** Launch multiple real executions arranged in nested containers.
Close each focused presentation layer in turn, recording pids, domain ids and
the cold snapshot after each operation; then close the window and reattach the
same Session.

**Required mutation.** `close_presentation_terminates_execution`.

**Negative control.** Manual packaged control. The historical
`close_node_terminates_all_leaves` library mutation tests a superseded product
oracle and is not supporting evidence for this gate.

**Precondition.** Packaged app, nested kernel containers, multiple real child
pids, and packaged T-KILL must all run. Missing any one is a failed
precondition, never a skip.

#### T-INV-COLD — readers are cold, bounded, and inert while hidden

**Features.** Cross-cutting prerequisite for F-010 through F-016.

**Oracle.** An independently recorded daemon counter shows reader sampling at
most 2 Hz while visible and zero while hidden; `--nfr-key` records zero
control-plane RPCs with readers visible.

**Procedure.** Packaged app opens then hides every reader surface while a child
is attached; measure reader queries and attach-stream control frames.

**Required mutation.** `dashboard_polls_hot`.

**Negative control.** Manual until cold-reader instrumentation exists.

**Precondition.** Packaged reader host, daemon-side sampling counter, and the
existing warm-path frame audit must exist. A missing counter fails the gate.

#### T-INV-SELECT — inventory selection focuses only

**Features.** F-010, F-012, F-013 navigation selection, and F-014 share this
atomic selection invariant.

**Oracle.** Selecting a snapshot-backed row focuses its actual live `NodeId`;
an input-observing child receives no bytes and no leaf is spawned, terminated,
or resized. A palette action remains separate and requires its own confirmation.

**Procedure.** Seed a tree with distinct Workspace, session, process, and
palette-navigation rows. Select each in the packaged app, then observe focus,
pid lifecycle, winsize, and child input independently.

**Required mutation.** `select_sends_input`.

**Negative control.** Manual until the reader and focus host paths exist.

**Precondition.** Packaged app, seeded kernel tree, input-observing child,
focus heartbeat, and T-INV-COLD instrumentation must exist. “Every important
action” has no accepted complete action set and is not claimed here.

#### T-INV-AGENT-EMPTY — absent Task renders no agent rows

**Feature.** F-011.

**Oracle.** With an explicit zero-Task runtime fixture, the visible agent
inventory is empty and no fabricated row appears; the reader also remains cold
under T-INV-COLD's independent counter.

**Procedure.** Launch the packaged inventory with the zero-Task fixture and
inspect the rendered rows plus cold-reader audit.

**Required mutation.** `fabricate_agent_row`.

**Negative control.** Manual until the agent inventory host projection exists.

**Precondition.** Packaged app and an explicit zero-Task fixture are required.
The absence of the Task runtime must make any unavailable fixture a failure,
not permission to show sample data or skip the test.

#### T-INV-HISTORY — focus history is bounded and never resurrects nodes

**Feature.** F-015.

**Oracle.** After visiting 65 nodes and deleting one, back/forward resolves the
actual focused live `NodeId`, skips the deleted node, and retains at most 64
entries.

**Procedure.** Seed at least 65 nodes in a packaged app, focus each, close one,
then traverse history while reading the resulting focus from the kernel.

**Required mutation.** `history_resurrects_deleted_node`.

**Negative control.** Manual until host focus-history wiring exists.

**Precondition.** Packaged host, at least 65 fixture nodes, and an independent
focus heartbeat are required. The existing reopen gate cannot substitute for
this bounded-history oracle.

#### T-INV-REOPEN — reopen reattaches the same live Session

**Feature.** F-016.

**Oracle.** Reopening a UI-hidden Session resolves the same `SessionId`,
`TerminalExecutionId` and child pid, then initializes a disposable mirror from
the host checkpoint and subsequent deltas. No spawn occurs and retained
content is unchanged.

**Procedure.** Hide a live Session presentation in the packaged app, record its
ids, pid, output offset and spawn counter, continue producing output, then
reopen and compare all observations.

**Required mutation.** `reopen_spawns_replacement_execution`.

**Negative control.** Manual until durable Session and host reopen wiring exist.

**Precondition.** Packaged app, a live durable Session, checkpoint/delta
instrumentation and independent pid/spawn probes are required. Absence fails;
a template-restore test cannot substitute for this gate.

#### T-NAV-TEMPLATE — templates serialize only the accepted durable shape

**Feature.** F-019.

**Oracle.** A saved template contains the container tree, per-leaf cwd, and
startup command, but no scrollback. Restoring it produces new leaves and
explicitly identifies restoration as spawn rather than live-child recovery.

**Procedure.** Save a multi-leaf layout after producing unique scrollback,
inspect the persisted template through its public cold reader, then restore in
the packaged app and compare old/new pids and child output.

**Required mutation.** `template_restores_old_pid`.

**Negative control.** Manual until the accepted template persistence surface
exists.

**Precondition.** Packaged template implementation, cold template reader, and
real child pid probes must exist. No test may inspect private chrome state or
claim scrollback restoration.

#### T-NAV-VIEWSTATE — hidden chrome changes only per-client view state

**Feature.** F-020.

**Oracle.** Toggling Workspace UI, Session UI, sidebar and vertical tabs leaves
domain-mutation and warm-frame counters at zero, while the same object ids,
child pid, winsize and attach state remain unchanged as output continues.

**Procedure.** In a packaged app with a live emitting child, toggle each chrome
region visible/hidden repeatedly and independently record domain calls, attach
frames, ids, pid, winsize and output; then deep-link to the hidden Session.

**Required mutation.** `hide_sidebar_detaches`.

**Negative control.** Manual until the sidebar host path and kernel-call counter
exist.

**Precondition.** Packaged app, live child, kernel/frame instrumentation, and a
pid oracle must exist. An unavailable counter is a failed precondition, never a
green skip.

| Spec | Authority | Milestone |
|---|---|---|
| [SPEC-NAV](spec/SPEC-NAV.md) | ADR 0038, ADR 0039 | M2 (container-tree kernel plane Proven at library level; chrome wiring open) |
| [SPEC-FIDELITY](spec/SPEC-FIDELITY.md) | ADR 0040 | M2 |
| [SPEC-REMOTE](spec/SPEC-REMOTE.md) | ADR 0041 | M2 |
| [SPEC-SURFACES](spec/SPEC-SURFACES.md) | ADR 0042, ADR 0046 | M2 |
| [SPEC-CONFIG](spec/SPEC-CONFIG.md) | ADR 0043 | M2 (resolution engine Proven at library level; look/appearance/updater host work open) |
| [SPEC-TRUST](spec/SPEC-TRUST.md) | ADR 0044 | M2 (project trust + redaction Proven at library level; plugins/socket/a11y open) |
| [SPEC-PLATFORM](spec/SPEC-PLATFORM.md) | ADR 0045 | M2 (T-PLAT-CORE fully Proven — dependency-graph check; FFI/per-platform gates open) |
| [SPEC-ATTENTION](spec/SPEC-ATTENTION.md) | ADR 0047 | M3 (queue/rollup/read-clearing Proven at library level; OSC/socket/hooks open) |
| [SPEC-TASK](spec/SPEC-TASK.md) | ADR 0048, ADR 0053 | M3 library mechanics partly Proven; complete object and runtime persistence Red |
| [SPEC-AGENT](spec/SPEC-AGENT.md) | ADR 0049 | M3 |
| [SPEC-BLOCKS](spec/SPEC-BLOCKS.md) | ADR 0050 | M6 |
| [SPEC-INPUT](spec/SPEC-INPUT.md) | ADR 0051 | M6 |
| [SPEC-MOUSE](spec/SPEC-MOUSE.md) | ADR 0052 | M6 |
| [SPEC-DOMAIN-LIFECYCLE](spec/SPEC-DOMAIN-LIFECYCLE.md) | ADR 0053 | foundation, Red |
| [SPEC-RUNTIME-SUPERVISION](spec/SPEC-RUNTIME-SUPERVISION.md) | ADR 0053 | foundation, Red |
| [SPEC-CLIENT-AUTHORITY](spec/SPEC-CLIENT-AUTHORITY.md) | ADR 0053 | foundation, Red |
| [SPEC-CONTENT](spec/SPEC-CONTENT.md) | ADR 0053 | after runtime/checkpoints, Red |
| [SPEC-COMPOSITOR](spec/SPEC-COMPOSITOR.md) | ADR 0053 | after content/checkpoints, Red |
| [SPEC-TERMINAL-PERFORMANCE](spec/SPEC-TERMINAL-PERFORMANCE.md) | ADR 0053 D22 | cross-cutting; Red until each named T-PERF gate has demonstrated-red evidence |

T-NFR, T-KILL, T-SPAWN, T-DROP, T-BYTES and the T-GRAPH / T-LOOK families are
not re-cut by any of these. A milestone gate that would require modifying a
Proven Spike 0 gate is rejected (ADR 0002).

## Architecture foundation — runtime, content and clients (Red)

Authority: [ADR 0053](adr/0053-runtime-domain-content-and-client-authority.md).
All gates in this section are **Red**. The tables specify future tests; they do
not authorize implementation and are not evidence until the exact required
mutation has been demonstrated to turn the named test red. Each implemented
test MUST carry a doc comment naming the defect/gap recorded here. Missing
platform services, packaged apps, credentials, devices or network conditions
are failed preconditions, never green skips.

### Domain, visibility and lifecycle

| Gate | Downstream oracle | Required mutation |
|---|---|---|
| T-DOMAIN-IDENTITY-MIGRATION — Session is durable grouping and TerminalExecution owns the PTY | Persist and restore both IDs; attach/terminate resolves only TerminalExecutionId; labels cannot address either | deserialize legacy SessionId as new durable SessionId |
| T-WORKSPACE-HIDDEN-IDENTITY — hiding Workspace UI preserves identity | Deep link and second client resolve the same WorkspaceId before/after hide/reveal; tab/pane/history hashes unchanged | hide creates a replacement default Workspace |
| T-SESSION-HIDDEN-IDENTITY — hiding Session UI preserves identity | Same SessionId, child pid, tabs, transcript root and attachments after hide/reveal | hide detaches or recreates Session |
| T-LIFECYCLE-UNINTENTIONAL-DETACH — unintended client loss never terminates | Original child pid survives window close, GUI SIGKILL, network cut, sleep/resume, mobile background and lease expiry | lease expiry calls terminate |
| T-TERMINATE-OTHER-CONTROLLER — another controller blocks ordinary termination | Runtime returns refusal naming the other controller; child pid remains responsive | observers/controllers list is ignored |
| T-TERMINATE-FORCE-IMPACT — owner/admin force binds to shown impact | Changed controller set invalidates confirmation; unchanged second confirmation terminates only named executions | confirmation token omits impact hash |
| T-TERMINATE-JOURNAL — explicit termination escalates and records final state | Process-group signal observer plus durable journal records actor, stages, exit and final content offset | journal write occurs after first signal |
| T-ALT-SAME-EXECUTION — alternate screen reuses pane, execution and PTY | IDs, PTY device and child pid remain identical across enter/exit while primary restores | alt entry allocates another execution |

### Supervised runtime and failure isolation

| Gate | Downstream oracle | Required mutation |
|---|---|---|
| T-RUNTIME-GUI-INDEPENDENT — packaged GUI uses the registered per-user service | Service-manager state and original worker/child identity survive all GUI processes exiting | packaged path directly spawns an unregistered daemon |
| T-RUNTIME-DAEMON-RESTART — daemon restart preserves worker-owned PTY | Kill control daemon, restart it, reconcile worker, then exchange a nonce with the original child pid | worker exits when daemon channel closes |
| T-RUNTIME-DAEMON-STATE — output produced while daemon is absent remains host-authoritative | Child emits numbered VT/mode changes during daemon outage; restarted daemon exports worker checkpoint+deltas matching an independent continuous VT at the same offset/hash | restarted daemon initializes a blank terminal core |
| T-RUNTIME-UPDATE-COMPAT — compatible daemon update preserves workers | N/N-1 fixture reconnects to original worker; incompatible fixture refuses before replacement | version check accepts incompatible checkpoint format |
| T-RUNTIME-MALFORMED-CLIENT-ISOLATION — malformed client cannot kill runtime | Send oversized/unknown/truncated frames; second valid client and unrelated child continue | decoder error escapes connection task into daemon run loop |
| T-RUNTIME-PROTECTED-ENDPOINT — local endpoint verifies owner and peer | Foreign-uid fixture is refused before ATTACH; endpoint parent is user-owned and non-world-writable | peer credential check returns constant success |
| T-RUNTIME-HOST-SHUTDOWN-JOURNAL — restart reports host-caused process loss honestly | Simulated shutdown marker plus missing worker restores graph/transcript and explicit `host_terminated`, never a live pid | missing worker is reported running |

### Host authority, mirrors and clients

| Gate | Downstream oracle | Required mutation |
|---|---|---|
| T-CLIENT-MIRROR-DISPOSABLE — deleting a client mirror loses no state | Destroy/recreate mirror from host checkpoint+deltas and compare independent VT state/hash | host reads state back from client mirror |
| T-CLIENT-MIRROR-RECONCILE — mirror divergence fails closed and resyncs | Corrupt one mirror cell/mode; hash mismatch stops input/presentation and requests checkpoint | mismatch is logged but mirror keeps presenting |
| T-CLIENT-RING-EVICTION-RESYNC — long disconnect uses checkpoint plus deltas | Disconnect beyond hot-ring eviction, reconnect and match continuous independent VT oracle | reconnect starts replay at retained ring base without checkpoint |
| T-CLIENT-OBSERVER-ISOLATION — observer cannot write or resize | Attempt DATA/RESIZE; PTY bytes and winsize remain unchanged while controller continues | observer RESIZE is accepted |
| T-CLIENT-CREDIT-ISOLATION — one client's credit cannot gate worker drain or peers | Hold observer credit at zero during numbered output; host offset and controller stream advance without gaps, observer later resyncs | minimum client credit gates PTY read |
| T-CLIENT-UNATTACHED-REFUSAL — unattached frames cannot target a default pane | DATA/RESIZE before ATTACH closes only attacker connection; all child histories unchanged | missing attachment falls back to default ID |
| T-CLIENT-LEASE-ATOMIC — exactly one input/resize lease exists | Racing takeover requests yield one generation/owner; only its nonce reaches PTY | both contenders retain valid generation |
| T-CLIENT-LEASE-EXPIRY-DETACH — lease expiry releases input but keeps process | After grace, writes are refused and another client takes lease; original child pid remains | expiry calls session cleanup |
| T-CLIENT-VIEWPORT-AUTHORITY — controller alone sets canonical geometry | Observer viewport changes leave child `TIOCGWINSZ` unchanged; controller resize changes it | largest observer wins geometry |
| T-MOBILE-BACKGROUND-DETACH — mobile suspension is not termination | Background mobile fixture loses lease/connection; desktop reconnects to original pid/state | app background callback terminates Session |

### SSH and remote

| Gate | Downstream oracle | Required mutation |
|---|---|---|
| T-REM-HOST-AUTHORITY — remote process host owns canonical state | Two clients discard/rebuild mirrors and converge on remote checkpoint while remote child continues | client snapshot overwrites host state |
| T-REM-IDENTITY-VERSION — changed identity or version fails before attach | Fake changed key/device and incompatible protocol receive no DATA/credential/attach frame | identity prompt defaults to trust |
| T-REM-CHECKPOINT-RECONNECT — remote long gap is explicit and recoverable | Checkpoint+deltas match continuous VT or render named Discontinuity | missing bytes are presented as continuous |
| T-SSH-ZERO-FOOTPRINT — compatibility SSH runs no hidden remote work | Instrumented SSH server sees only the exact user-requested session/argv and zero upload/probe/profile/history/helper operations | client runs `command -v rilld` probe |
| T-SSH-SHELL-UNCHANGED — zero-footprint preserves remote shell configuration | zsh/fish/bash fixtures hash profiles before/after and observe the same prompt/plugin/ANSI/job-control behavior as direct SSH | connector appends a RILL profile hook |
| T-SSH-ENHANCED-PLAN-CLEANUP — bootstrap is consented and cleanup is honest | Executed commands/artifacts equal approved plan; injected cleanup failure is reported as residue | cleanup failure is reported successful |
| T-REM-OBSERVER-LEASE — remote observer cannot control | Remote observer DATA/RESIZE has no PTY/winsize effect; controller remains live | remote transport upgrades observer role |
| T-REM-NFR-SEPARATE — remote latency is never NFR-KEY | Remote target refuses `--nfr-key` and report contains RTT-labelled remote metric only | reporter labels remote p95 NFR-KEY |

### Content, retention and recovery

| Gate | Downstream oracle | Required mutation |
|---|---|---|
| T-CONTENT-MONOTONIC-OFFSETS — eviction never renumbers retained bytes | Append/evict/append ranges retain absolute offsets and generation; readers detect old base | ring reports snapshot-relative offsets |
| T-CONTENT-RANGE-REQUIRES-STATE — arbitrary range is not renderable without prior VT state | Slice beginning mid-sequence is refused without compatible checkpoint and succeeds from checkpoint against independent VT | renderer always resets VT before slice |
| T-CONTENT-SURVIVES-RING-EVICTION — retained timeline content outlives hot ring | Materialized content remains identical after source eviction when policy retained it | timeline reads moving ring on every render |
| T-CONTENT-NO-PROMPT-HEURISTIC — command boundaries require explicit events | Prompt-shaped output without mark remains terminal region; explicit submission/mark creates boundary | regex matching the dollar-space prompt creates command |
| T-CONTENT-SOURCE-AUTHORITY — enhanced fields require named producers | See terminal-performance section; cells/timing/regex cannot fabricate command, tests, duration, branch, cwd, approval or agent status | scrape_cells_for_command_and_pass_count |
| T-CONTENT-ALT-SAME-PTY — full-screen TUI stays one mutable terminal surface | PTY/device/TerminalExecution unchanged; no timeline Block for live alternate grid | alt grid copied into new Block each frame |
| T-CONTENT-RETENTION-DISABLED — durable capture can be entirely disabled | Run/output/restart leaves no durable raw/transcript/history/task payload while bounded live reconnect works | disabled policy still writes transcript segment |
| T-CONTENT-RETENTION-RESTRICTIVE-WINS — corporate/session policy cannot be widened locally | More restrictive parent rule wins resolution and blocks capture/export | closest Workspace setting always wins |
| T-CONTENT-REDACTION-DERIVED — redaction does not rewrite source or claim capture authority | Export is transformed and labelled; canonical hash unchanged; capture-disabled source was never stored | redactor mutates canonical record |
| T-CONTENT-TRUNCATION-VISIBLE — deleted/evicted sources remain honest | Referring item becomes explicit tombstone/discontinuity with range/reason | UI omits gap marker |
| T-CONTENT-BOUNDED-RECOVERY — logs/checkpoints/materialization stay bounded | Long-output/restart fixture remains within declared byte/item bounds and reconstructs or names discontinuity | checkpoint created for every frame |
| T-BLOCK-CONTENT-IDENTITY — Block groups stable ContentItems, not only ring offsets | Evict hot ring and retain policy content; Block identity/output remains stable | Block stores only `(start,end)` |
| T-BLOCK-RERUN — rerun fills input and never executes | Child receives no bytes until separate submit; exact retained command visible | rerun action writes directly to PTY |
| T-TRANSCRIPT-EVENT-IDEMPOTENCY — event identity is stable and append is idempotent | Replay each event twice and recover from snapshot; final transcript IDs/order/hash equal single append | duplicate EventId creates a second item |
| T-TRANSCRIPT-BYTE-EVENT-ORDER — semantic events correlate to exact terminal offsets | Real PTY fixture interleaves marks/output; recovered event ranges match independently captured byte offsets | semantic event is emitted before its source byte offset |
| T-FLOW-RAW-SEMANTIC-FAILURE — semantic failure never breaks raw terminal | Kill/fault semantic projector during interactive shell/TUI; raw bytes, input, paint and original pid continue and Raw becomes available | semantic queue backpressure pauses PTY drain |
| T-ACTIVITY-DERIVED-NOT-AUTHORITY — timeline teardown loses no source state | Destroy/rebuild/filter timeline from durable source events; source IDs/hashes and process state stay unchanged | timeline node owns and deletes approval event |

### Compositor, text, editor and library seams

| Gate | Downstream oracle | Required mutation |
|---|---|---|
| T-COMPOSITOR-PRESERVES-METAL-GRID — existing grid remains the terminal primitive | Packaged raw/TUI fixture uses Metal grid and keeps T-NFR/T-ALT behavior | terminal content rendered through generic text nodes |
| T-COMPOSITOR-VIRTUALIZED-CONTENT — offscreen content is not fully materialized | Million-item fixture keeps scene/item/glyph counts within declared viewport+overscan bounds | compositor builds every timeline item |
| T-COMPOSITOR-NO-DOMAIN-OWNERSHIP — scene teardown cannot delete domain state | Destroy/rebuild scene; Workspace/Session/Task/Content IDs and worker pid unchanged | scene-node drop closes Session |
| T-TEXT-CLUSTER-SHAPING — text shaping operates on clusters/runs | Ligature, combining, emoji-ZWJ and fallback fixtures match platform shaping oracle | shaper maps only the first scalar |
| T-EDITOR-RAW-BYPASS — raw mode never invokes structured editor | Instrumented TUI receives byte-identical keys with editor counters zero | raw input first inserts into editor buffer |
| T-INPUT-MODE-TRANSITION — Flow/Raw/TUI reuse one execution and route by authority | Real shell enters/exits alternate/raw modes; pid/PTY/IDs remain stable, composer hides, keys/mouse/paste reach child, focus restores | client focus alone selects composer route |
| T-COMPOSER-DRAFT-LOCAL — unsent draft is non-durable and client-local | Type secret canary without submit, restart/attach second client/backup; canary appears in none | draft is written to Session snapshot |
| T-SELECTION-SURFACE-ANCHORS — selection survives surface-specific updates | Terminal, timeline and editor anchors produce ordered copy after unrelated reflow | all anchors use screen row/column |
| T-A11Y-VIRTUALIZED-BOUNDS — accessibility is semantic and bounded | VoiceOver fixture reaches visible terminal/content/actions; offscreen node count stays bounded | full transcript rebuilt each frame |
| T-COMPOSITOR-WARM-PATH-BOUNDARY — rich scene adds no warm control RPC/string cells | NFR trace has only allowed DATA/CREDIT and zero per-cell String allocations with content enabled | key event serializes ContentItem JSON |
| T-LIBRARY-NO-UI-DEPENDENCY — internal core boundaries do not depend on app UI | Resolved dependency graph catches an injected AppKit/host dependency | checker examines filenames only |
| T-LIBRARY-WASM-PROOF-BEFORE-PUBLIC — no public/WASM promise without all gates | Release manifest refuses public stability absent conformance, fuzz, security, WASM and two RILL-client proofs | manifest trusts crate version alone |

### Task and capture evidence corrections

| Gate | Downstream oracle | Required mutation |
|---|---|---|
| T-TASK-SERIALIZE — existing `t_task_persist...` is a library serialization sub-gate | In-process text round-trip preserves current sections, decisions and queue only; it makes no disk/restart claim | `skip_persisting_queue` |
| T-TASK-COMPLETE-OBJECT — Task contains required identity and targets | Durable round-trip preserves status/cwd/host/label and exact domain IDs | target is reconstructed from display label |
| T-TASK-RUNTIME-PERSIST — Task survives GUI and daemon restart | Kill GUI and control daemon; recovered runtime returns same Task sections, decisions and queue from durable store | persistence remains an in-memory String |
| T-TASK-FORK-NAVIGATION — fork stays grouped and hidden by default | Fork creates durable child ID but no Tab/Pane/nav row until open/pin/attention | fork automatically creates visible tab |
| T-TASK-FORK-PROPAGATION — lifecycle propagation is explicit | Cancel/complete parent and child under each policy; journaled outcomes match policy and default changes neither peer | parent cancel always kills child |
| T-TASK-WORKTREE-ISOLATION — concurrent writers never share filesystem authority | Two real tasks resolve distinct worktree roots/inodes and cannot write through the other's grant | second task reuses parent worktree |
| T-TASK-FORK-CONFLICT — conflicts are durable and explicit | Create real divergent edits; conflict event survives restart and no side changes until authorized resolution | adapter silently chooses ours |
| T-ATT-STRUCTURED-IDENTITY — attention deep-links to exact source | Two same-label requests in different panes resolve by stable IDs to their exact source | navigation resolves label or focused pane |
| T-ATT-RESPONSE-AUTH — inline response is authenticated and capability-scoped | Observer and wrong-role fixtures cannot answer; authorized current-generation response changes only named request | rendering card grants approval capability |
| T-ATT-REPLAY-REJECT — stale/duplicate responses have no effect | Replay captured response after completion/expiry/generation change; state/hash remain unchanged and rejection is audited | request ID alone bypasses generation check |
| T-ATT-SECRET-NAVIGATION — secret/TUI prompts never become inline previews | Real no-echo and alternate-screen fixtures create navigation-only items with no canary bytes | notification copies recent terminal cells |
| T-PROTOCOL-SEMANTIC-INDEPENDENCE — semantic channel failure cannot block terminal | Exhaust/fault semantic credit while real PTY emits and accepts input; DATA/CREDIT and paint continue within bounds | terminal and semantic frames share one blocking queue |
| T-PROTOCOL-BYTE-EVENT-ORDER — byte/event correlation survives framing and reconnect | Split/coalesce frames and resume; semantic source offsets match independent byte capture | client arrival time defines event order |
| T-PROTOCOL-SLOW-CLIENT-CHANNEL-ISOLATION — slow/mobile clients resync independently | Throttle one channel/client beyond bounds; controller and other panes continue, slow client receives cursor/snapshot requirement | observer credit gates worker read |
| T-PROTOCOL-VERSION-MISMATCH — incompatible versions fail closed | Matrix mismatches protocol/checkpoint/content versions and observes explicit refusal/discontinuity with no misdecoded frame | unknown content version parsed as current |
| T-TRUST-CAPTURE-POLICY — encryption/redaction never substitutes for capture permission | Capture-disabled policy produces no durable payload even with encryption key and redactor available | encrypted sink bypasses policy denial |

### Shell, unified configuration and privacy

| Gate | Downstream oracle | Required mutation |
|---|---|---|
| T-FID-SHELL-COMPAT — PTY-compatible shells retain native behavior | Packaged zsh/fish/bash plus another available shell match direct-PTY startup argv/env/cwd/TERM, prompt/theme/plugin/ANSI, job-control, completion and signal fixtures | runtime substitutes a RILL wrapper shell |
| T-FID-SHELL-NO-MUTATION — RILL never modifies shell configuration | Hash startup/profile/plugin files before and after local launch/quit/reopen; hashes and contents remain identical and no hidden command reaches PTY | launcher writes a shell-integration line to profile |
| T-CFG-SCHEMA-COVERAGE — one TOML model covers every declared setting family | Round-trip app/terminal theme, fonts/sizes, bindings, rendering, Workspace/Session and privacy settings through public reader; no shadow-store reads occur | Workspace behavior is read from a GUI-only store |
| T-CFG-THEME-CONSISTENCY — one named theme reaches every surface | Packaged terminal, chrome, timeline, editor, diff and controls report the same theme identity and independently expected role tokens | editor falls back to a compiled unrelated palette |
| T-CFG-MIGRATE — validation/migration is atomic and recoverable | Invalid file preserves last valid state; old fixture previews then migrates with verified pre-migration backup; injected failure restores byte-identical prior TOML | migration replaces file before validating output |
| T-CFG-PORTABLE-SECRETS — export/backup/sync contain no credential material | Seed platform credential store and secret-like config inputs; inspect serialized/exported/backed-up/synced payloads and find only opaque references/allowlisted settings | exporter resolves credential reference into value |
| T-PRIVACY-MINIMIZATION — every sink receives only declared minimum scope | Select one ContentItem for clipboard/agent/diagnostic fixtures; sink audit contains only that item and declared metadata | visible Session is attached wholesale |
| T-PRIVACY-BOUNDARY-ISOLATION — data never crosses identity boundaries | Seed unique canaries across OS-user/runtime/host/Workspace/Session/client/agent stores; every unauthorized query/export returns none | cache key omits SessionId |
| T-PRIVACY-DIAGNOSTICS — logs, telemetry and crash reports carry no sensitive payload | Emit terminal/command/clipboard/credential/PII canaries, force diagnostics/crash, and scan independent artifacts; no canary appears | crash reporter includes recent terminal bytes |
| T-PRIVACY-BACKUP-SYNC — backup/sync obey allowlist, encryption and deletion | Inspect encrypted payload envelope and decrypted test fixture: only allowlisted non-secret config exists; deletion removes remote/local copies | sync serializes entire config directory |
| T-PRIVACY-NOTIFICATION-SECRET — secret prompts never leak into previews | Feed unique password/no-echo canaries, trigger attention/OS notification, and independently scan payloads | preview includes recent line/cells |
| T-TRUST-STRUCTURED-REPLAY — approvals bind authenticated request generation | Capture and replay an approval across clients/generations; only the authorized current request changes | handler accepts stale request ID |
| T-TRUST-READONLY-INPUT — observers cannot send terminal or approval input | Observer DATA/approval attempts leave PTY/request hashes unchanged and return explicit denial | UI role only hides controls while protocol accepts input |

The binding execution order is schema/authority plus configuration/privacy →
terminal and PTY compatibility → host state/workers/checkpoints/leases →
semantic transcript/retention → Flow with independent Raw fallback → persistent
topology → Tasks/isolation → structured attention/approvals → artifacts/diffs →
optional activity timeline. Compositor/input/selection and remote/mobile enter
only after their consumed authority exists. Shell compatibility is a foundation
gate throughout. Chip 1 live-swap gates remain parked until checkpoint and
mirror gates have demonstrated red.

## Terminal performance invariant (Red)

Authority: [ADR 0053](adr/0053-runtime-domain-content-and-client-authority.md)
D22 and [SPEC-TERMINAL-PERFORMANCE](spec/SPEC-TERMINAL-PERFORMANCE.md).

These gates **supplement** Proven T-NFR, T-DROP, T-BYTES, T-RESIZE, T-SPAWN,
T-KILL, T-ATTACH and later T-GRAPH / T-CLIENT / T-CONTENT / T-COMPOSITOR /
T-PROTOCOL gates. They do not rename, replace or relax them. Documentation
acceptance is not Proven. A test that would have passed before the protected
asynchronous wiring exists is not evidence.

Packaged macOS application runs are required for every user-visible latency,
isolation and Raw-fallback row. Real PTYs and downstream process or display
oracles are required where mocks cannot observe drain, byte order or present.
Missing instrumentation, display, battery, Accessibility trust or PTY
preconditions fail the gate.

Primary numeric boundary remains T-NFR (ADR 0003): packaged HID, battery,
n ≥ 1000, discards ≤ 2%, p95 below one actual refresh interval, zero
control-plane RPCs. NFR-DROP / T-DROP and NFR-BYTES / T-BYTES remain zero
dropped or reordered bytes. Secondary metrics (p50/p99, drain throughput, frame
time, CPU, memory, queue depth, Raw-fallback time, allocations) are paired
baseline comparisons until one later consolidated product decision authorizes
numeric tolerances. No gate here invents a millisecond budget.

### Named T-PERF gates

| Gate | Downstream oracle | Required mutation |
|---|---|---|
| T-PERF-BASELINE-PARITY — enhancements do not regress T-NFR beyond measurement noise | Same packaged build, machine class, power/display, viewport, HID instrument, n and workload: enhancements-disabled T-NFR remains Proven; each enabled configuration in the matrix reports p50/p95/p99, zero control RPCs, and no defensible key-to-present regression beyond repeated interleaved-run noise | `nfr_with_blocks_uses_easier_budget` (or equivalent: change T-NFR threshold or skip HID when Flow/inspector/attention is on) |
| T-PERF-PTY-DRAIN-INDEPENDENT — PTY drain does not wait for transcript or persistence | Numbered real-PTY output continues within existing T-DROP bounds while a semantic processor and durable sink are stalled; independent byte capture matches child output; worker offset advances | `pty_read_awaits_transcript_ack` |
| T-PERF-PRESENT-INDEPENDENT — present does not wait for persistence or semantic classification | Packaged present timestamps continue while the transcript store is delayed/unavailable; first echoed glyph still uses ADR 0003 `presentedTime`; Raw remains interactive | `present_waits_for_db_commit` |
| T-PERF-SEMANTIC-DEGRADATION — semantic stall/crash cannot block raw | Kill or hang the semantic processor; DATA/CREDIT, input and Metal grid continue; UI shows `Unstructured` or honest Flow degradation, never fabricated commands | `semantic_backpressure_pauses_pty` |
| T-PERF-PANE-ISOLATION — one pane or agent flood cannot stall another | Two real PTYs; flood one; the other exchanges a nonce and meets its drain/present bounds; per-pane queue high-water stays within declared limits | `shared_pump_starves_second_pane` |
| T-PERF-CLIENT-ISOLATION — a slow observer cannot stall the worker or controller | Hold mobile/observer credit at zero during numbered output; host offset and controller stream have no gaps; slow client later resyncs or is required to snapshot | `min_client_credit_gates_pty_read` |
| T-PERF-BOUNDED-RESOURCES — queues, transcript projection, glyphs, images and offscreen layout stay bounded | Prolonged output plus large retained transcript, visible large diff/Markdown, and virtualized scroll keep item/glyph/image/queue counts within declared viewport+overscan and queue capacities; overflow increments a counter rather than growing without bound | `layout_entire_offscreen_timeline` |
| T-PERF-BYTE-FIDELITY — semantic pressure never drops or reorders PTY bytes | Independent capture of child bytes equals host journal and controller DATA under semantic overload, inspector/timeline load and persistence failure; reorder/gap counters stay zero | `drop_pty_bytes_when_semantic_queue_full` |
| T-PERF-RAW-TUI-BYPASS — Vim/Neovim, nested tmux and alternate screen bypass Flow/composer | Real TUI/tmux fixtures: same PTY/execution; composer and Flow counters stay zero on the input path; grid remains Chip 0/Metal | `raw_keys_insert_into_composer_first` |
| T-PERF-RECOVERY-ISOLATION — hash/checkpoint failure stops only the divergent mirror | Corrupt one client hash; that pane stops presenting/accepting input; worker and a second healthy raw client continue; recovery is checkpoint/deltas or explicit discontinuity — no silent sync work on the present path | `divergent_mirror_keeps_presenting` |
| T-CONTENT-SOURCE-AUTHORITY — enhanced fields require named producers | Prompt-shaped text, cursor motion and timing without marks remain `Unstructured`/raw; command, duration, exit, tests, branch, cwd, approval and agent status appear only when the SPEC-TERMINAL-PERFORMANCE source table producers emit them | `scrape_cells_for_command_and_pass_count` |

### Required mutations that every closer must detect

In addition to the per-gate mutations above, a closer for this section MUST
include automated (or recorded manual) reds for:

| Mutation | Must turn red |
|---|---|
| `pty_read_awaits_transcript_ack` | T-PERF-PTY-DRAIN-INDEPENDENT, T-FLOW-RAW-SEMANTIC-FAILURE |
| `present_waits_for_db_commit` | T-PERF-PRESENT-INDEPENDENT |
| `semantic_backpressure_pauses_pty` | T-PERF-SEMANTIC-DEGRADATION, T-PROTOCOL-SEMANTIC-INDEPENDENCE |
| `min_client_credit_gates_pty_read` | T-PERF-CLIENT-ISOLATION, T-CLIENT-CREDIT-ISOLATION |
| `layout_entire_offscreen_timeline` | T-PERF-BOUNDED-RESOURCES, T-COMPOSITOR-VIRTUALIZED-CONTENT |
| `scrape_cells_for_command_and_pass_count` | T-CONTENT-SOURCE-AUTHORITY, T-CONTENT-NO-PROMPT-HEURISTIC |
| `drop_pty_bytes_when_semantic_queue_full` | T-PERF-BYTE-FIDELITY, T-DROP |

### Failure-case coverage

Each row uses real PTY drain plus a second-pane or second-client isolation
check. Honest UI degradation is part of the oracle.

| Failure | Existing overlapping gates | New/required closer |
|---|---|---|
| Semantic processor stalls | T-FLOW-RAW-SEMANTIC-FAILURE | T-PERF-SEMANTIC-DEGRADATION |
| Semantic processor crashes | T-FLOW-RAW-SEMANTIC-FAILURE | T-PERF-SEMANTIC-DEGRADATION |
| Transcript store slow | T-CONTENT-BOUNDED-RECOVERY | T-PERF-PRESENT-INDEPENDENT |
| Transcript store unavailable | T-CONTENT-RETENTION-DISABLED | T-PERF-PRESENT-INDEPENDENT |
| Disk full | T-CONTENT-BOUNDED-RECOVERY | T-PERF-PRESENT-INDEPENDENT (fail closed, no present-path retry) |
| Retention disabled | T-CONTENT-RETENTION-DISABLED | T-PERF-BYTE-FIDELITY |
| Flow projection fails | T-FLOW-RAW-SEMANTIC-FAILURE | T-PERF-SEMANTIC-DEGRADATION |
| Rich-content renderer fails | T-COMPOSITOR-PRESERVES-METAL-GRID | T-PERF-PRESENT-INDEPENDENT |
| Inspector/timeline slow | T-COMPOSITOR-WARM-PATH-BOUNDARY | T-PERF-PRESENT-INDEPENDENT |
| Agent excessive output | T-PERF-PANE-ISOLATION | T-PERF-BYTE-FIDELITY |
| Mobile/observer stops credit | T-CLIENT-CREDIT-ISOLATION, T-PROTOCOL-SLOW-CLIENT-CHANNEL-ISOLATION | T-PERF-CLIENT-ISOLATION |
| One pane unlimited output | T-GRAPH isolation / starve mutations | T-PERF-PANE-ISOLATION |
| State-hash/checkpoint reconcile fails | T-CLIENT-MIRROR-RECONCILE | T-PERF-RECOVERY-ISOLATION |

### 20-workload measurement matrix

Every enabled row records the SPEC-TERMINAL-PERFORMANCE §6 inventory. Disabled
baseline is workload 1. Enabled rows compare to that same-build baseline; they
do not substitute a new T-NFR instrument.

| # | Workload | Existing mandatory gates | Additional Red closer |
|---|---|---|---|
| 1 | Raw terminal, enhancements disabled | T-NFR, T-DROP, T-BYTES, T-SPAWN, T-KILL | T-PERF-BASELINE-PARITY (disabled arm) |
| 2 | Flow enabled, idle | T-NFR (enabled), T-BLOCK-WARM-BOUNDARY, T-COMPOSITOR-WARM-PATH-BOUNDARY | T-PERF-BASELINE-PARITY |
| 3 | Flow, normal shell commands | T-CONTENT-NO-PROMPT-HEURISTIC, T-TRANSCRIPT-BYTE-EVENT-ORDER, T-FID-SHELL-COMPAT | T-PERF-BASELINE-PARITY, T-CONTENT-SOURCE-AUTHORITY |
| 4 | Sustained high-volume output | T-DROP | T-PERF-BYTE-FIDELITY, T-PERF-BOUNDED-RESOURCES |
| 5 | Multiple simultaneously active panes | T-GRAPH starve / NFR-DROP multi-pane | T-PERF-PANE-ISOLATION |
| 6 | Multiple agents concurrent output | T-TASK-WORKTREE-ISOLATION, T-AGENT-COLD | T-PERF-PANE-ISOLATION |
| 7 | Very large retained transcript, virtualized scroll | T-COMPOSITOR-VIRTUALIZED-CONTENT, T-A11Y-VIRTUALIZED-BOUNDS | T-PERF-BOUNDED-RESOURCES |
| 8 | Large visible diff and Markdown | T-COMPOSITOR-VIRTUALIZED-CONTENT | T-PERF-BOUNDED-RESOURCES |
| 9 | Inspector and attention enabled | T-ATT-STRUCTURED-IDENTITY, T-COMPOSITOR-WARM-PATH-BOUNDARY | T-PERF-PRESENT-INDEPENDENT |
| 10 | Optional activity timeline open | T-ACTIVITY-DERIVED-NOT-AUTHORITY | T-PERF-PRESENT-INDEPENDENT |
| 11 | Slow semantic consumer | T-FLOW-RAW-SEMANTIC-FAILURE, T-PROTOCOL-SEMANTIC-INDEPENDENCE | T-PERF-SEMANTIC-DEGRADATION |
| 12 | Slow or failed persistence | T-CONTENT-RETENTION-DISABLED | T-PERF-PRESENT-INDEPENDENT |
| 13 | Slow mobile observer | T-CLIENT-CREDIT-ISOLATION, T-PROTOCOL-SLOW-CLIENT-CHANNEL-ISOLATION, T-MOBILE-BACKGROUND-DETACH | T-PERF-CLIENT-ISOLATION |
| 14 | Alternate-screen TUI (Vim/Neovim) | T-CONTENT-ALT-SAME-PTY, T-EDITOR-RAW-BYPASS, T-INPUT-MODE-TRANSITION | T-PERF-RAW-TUI-BYPASS |
| 15 | Nested tmux | T-FID-SHELL-COMPAT, T-CONTENT-ALT-SAME-PTY | T-PERF-RAW-TUI-BYPASS |
| 16 | Unicode, grapheme shaping and IME | T-BYTES, T-TEXT-CLUSTER-SHAPING; T-NFR is **not** recut for IME | T-PERF-BYTE-FIDELITY (ordering); IME present-path uses existing host input once specified, still under T-NFR when those keys are HID-measurable |
| 17 | Resize storms | T-RESIZE, T-CLIENT-VIEWPORT-AUTHORITY | T-PERF-BYTE-FIDELITY (no drop/reorder across SIGWINCH) |
| 18 | Reconnect / checkpoint reconciliation | T-CLIENT-RING-EVICTION-RESYNC, T-CLIENT-MIRROR-RECONCILE, T-REM-CHECKPOINT-RECONNECT | T-PERF-RECOVERY-ISOLATION |
| 19 | Semantic channel disconnect, terminal channel healthy | T-PROTOCOL-SEMANTIC-INDEPENDENCE | T-PERF-SEMANTIC-DEGRADATION |
| 20 | Memory and queue bounds during prolonged execution | T-CONTENT-BOUNDED-RECOVERY | T-PERF-BOUNDED-RESOURCES |

`make fast`, `make gates`, packaged-app tests, real-PTY tests, mutation jobs
and required CI remain mandatory in addition to these Red closers. Hosted
`macos-14` T-NFR timeout is still not the closer (ADR 0009).
