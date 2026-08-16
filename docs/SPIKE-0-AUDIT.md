# Spike 0 — validation audit

- **Date:** 2026-08-16
- **Scope:** every file in this tree at `8960da7`.
- **Verdict:** **Spike 0 is Red.** Every `Proven` mark in [SPIKE-0](SPIKE-0.md) and
  [SPIKE-0-VALIDATION](SPIKE-0-VALIDATION.md) is revoked by
  [ADR 0002](adr/0002-falsifiable-evidence.md).
- **Authority for the fix:** [ADR 0002](adr/0002-falsifiable-evidence.md),
  [ADR 0003](adr/0003-display-pipeline.md).

The architecture is not the problem. The evidence is. Three gates are written so
that they cannot fail, and the tree has been reading their green as proof.

Severity: **S1** gate cannot fail · **S2** test name does not state what the test
does · **S3** production defect · **S4** process / reproducibility.

---

## What is right, and stays

Recorded first, because the remediation must not throw it away.

- **ADR 0001 stands.** The four-plane split, sole-writer PTY, framed
  `SOCK_STREAM`, bounded byte ring in the runtime, and "no VT on the kernel's
  live display path" are correct. Nothing below asks to change them.
- **Chip 0 is not a stub.** `crates/rill-chip0/src/adapter/rill_chip0_vt.c`
  uses the real libghostty-vt C API. Verified against upstream headers at
  `ghostty-org/ghostty@26df373ec83fb1cebb4fee0a8394144ae984a9b8`:
  `ghostty_terminal_new/free/reset/resize/vt_write`, the whole
  `ghostty_render_state_*` row/cell iterator surface, and the
  `GHOSTTY_INIT_SIZED` sized-struct ABI pattern all match, parameter for
  parameter. Whoever wrote that adapter read the headers.
- **`crates/rilld/tests/persist_e2e.rs` earns its name.** It launches `rilld`
  in a real process group, `SIGKILL`s the group, then asserts pid survival and
  reattach content. It is the only test in the tree that could have failed for
  the reason it claims. It is the model for every gate below.
- **The attach `Decoder` handles partial reads.** That is the actual hazard
  `SOCK_STREAM` introduces over `SOCK_SEQPACKET`, and it is covered.
- **`rilld/src/main.rs`** does `setsid` + `SIG_IGN(SIGHUP)` before binding.
  Correct shape for the persist wedge.

---

## S1 — Gates that cannot fail

### S1-1 · T-SPAWN asserts on a symbol class the command excludes

`crates/rill-host/tests/t_spawn.rs` runs `nm -U <binary>` and asserts that
`_forkpty`, `_openpty`, `_posix_openpt`, `_grantpt`, `_unlockpt` do not appear.

`-U` restricts the listing to **defined** symbols (llvm-nm: `-U,
--defined-only`; Apple's legacy nm: "Don't display undefined symbols"). Those
five symbols live in libSystem. In `Rill` they could only ever appear as
**undefined** symbols — imports. The command excludes precisely the set the
assertion inspects. The test passes on a binary that calls `forkpty` on every
keystroke.

Confirm on the Mac in one line — the second command prints imports, the first
prints nothing:

```sh
nm -U dist/Rill.app/Contents/MacOS/Rill | grep -c posix_spawn   # expect 0
nm -u dist/Rill.app/Contents/MacOS/Rill | grep -c posix_spawn   # expect >0
```

A second, deeper problem: `host/macos/main.m` **does** call `posix_spawn`, to
launch `rilld`. That is correct and intended. So the oracle "no spawn symbols
in the GUI" is not expressible as a symbol list — the GUI legitimately spawns
*something*. The gate needs two assertions: no PTY-creation imports at all, and
a runtime check that the user shell's parent is `rilld`.

### S1-2 · T-NFR re-detects glyphs that are already on screen

`rill_host::nfr_key` types `b'a' + (i % 26)` and then polls
`grid.cells.iter().any(|c| c.codepoint == ch)` — the whole grid, for a single
letter, with no position constraint.

The shell echoes each key onto the prompt line, where it stays. The alphabet
repeats every 26 keys. From roughly the 27th sample onward the target glyph is
already present from a previous cycle, so the wait loop exits on its first
`snapshot()` call without any PTY round trip having occurred.

That is what `p95=0.032ms` means. A PTY write, a scheduler hop, a shell echo, a
read, and a VT feed cannot complete in 32 microseconds. The gate is measuring
the cost of `Instant::now()` and a grid scan.

The recorded evidence `p95=0.032ms control_rpc=0 battery=0` should be treated as
a null result, not a partial one.

### S1-3 · The control-RPC check certifies itself

Two layers, both vacuous:

- `Frame::is_control_rpc()` in `crates/rill-attach/src/lib.rs` is
  `fn is_control_rpc(&self) -> bool { false }`. It is never called.
- The value actually reported comes from `looks_like_json`, which greps the
  **attach byte stream** for `pane_replay` or `"cells"`. The attach stream is a
  private tag+length binary framing that by construction never carries those
  strings. `control_rpc=0` is guaranteed by the format, not by the absence of
  RPCs.

`validate-spike0.sh` then does `grep -q 'control_rpc=0'` and treats the match as
a passed gate. If the warm path made a hundred control RPCs over a second
socket, this reports `control_rpc=0`.

---

## S2 — Test names that do not state what the test does

AGENTS.md §3: *"Named test first for the intended failure."* These names assert
behaviour the bodies never exercise. A reader trusting `cargo test` output is
misled by design, not by accident.

| Test | Name claims | Body does |
|---|---|---|
| `t_drop_yes_ten_seconds_ctrl_c_type_does_not_drop` | 10s `yes`, `^C`, then type | `yes \| head -n 20000` (self-terminating, bounded); no `0x03` is ever sent; nothing is typed afterward |
| `t_kill_gui_sigkill_does_not_change_child_pid` (kernel) | GUI `SIGKILL` | calls `session.detach()` in-process; no signal is sent to anything |
| `t_resize_child_tiocgwinsz_matches_display` | child's `TIOCGWINSZ` matches | `TIOCGWINSZ` on the **master fd it just `TIOCSWINSZ`'d**; the child is `sleep 8` and is never asked |
| `t_bytes_invalid_utf8_reaches_emulator_byte_identical` (chip0) | emulator sees the bytes | asserts `chip.bytes_fed() == fixture`, where `fed` is a `Vec` filled by `extend_from_slice` *before* the VT is called |
| `t_resync_headless_emits_bytes_not_cells` | resync carries screen bytes | asserts output starts with `\x1b[2J`, which `resync_from_history` prepends two lines earlier |

Detail on the two worst:

**S2-1 · T-DROP never exercises backpressure.** The loop re-grants
`Credit(u32::MAX)` on every iteration. `Session::on_pty_readable` returns early
only when `credit == 0`, which now never happens. The stop-reading-the-master
path — the entire mechanism NFR-DROP exists to verify — is dead code during the
test. The assertion `y_lines >= 20000` counts `b'y'` bytes from a stream where
`head -n 20000` guarantees 20000 lines of `y\n`. It asserts that `head` works.

**S2-2 · T-BYTES tests `Vec::extend_from_slice`.** `Chip0::feed` appends to
`self.fed` and *then* calls the VT. Comparing `fed` to the fixture cannot
detect lossy conversion inside the VT, which is where the risk is. The fixture
is also 4 bytes (`ff fe 80 41`) — no overlong encodings, lone surrogates,
truncated multi-byte sequences, or high bytes inside CSI parameters. The
kernel-side sibling is worse: it writes the fixture as PTY *input* with the line
discipline in canonical mode with `ECHO`, so what reaches the ring is the tty
echo, not child output. It passes for the wrong reason and would break outright
on any fixture containing `0x03`, `0x04`, `0x15`, or `0x7f`.

---

## S3 — Production defects

### S3-1 · Stack buffer overflow reachable from untrusted terminal output — **critical**

`crates/rill-chip0/src/adapter/rill_chip0_vt.c`, `rill_vt_snapshot`:

```c
uint32_t buf[8];
uint32_t take = glen < 8 ? glen : 8;
ghostty_render_state_row_cells_get(
    vt->cells, GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_BUF, buf);
cp = buf[0];
(void)take;
```

The upstream contract for `GRAPHEMES_BUF` is: *"The buffer must be at least
graphemes_len elements."* The clamp is computed, then discarded. There is no
API to pass a capacity. Any grapheme cluster with more than 8 codepoints — a ZWJ
emoji sequence, stacked combining marks — writes past an 8-element stack array.

Any process that can write to the PTY controls that byte count. That includes
`cat` on a hostile file, a compromised dependency's build output, and an SSH
session to a machine you do not own. This is the highest-severity item in the
tree and it is on the display path of every frame.

Fix: query `GRAPHEMES_LEN` first, and either allocate to fit or use
`GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8`, which takes a
`GhosttyBuffer` with an explicit capacity and returns `GHOSTTY_OUT_OF_SPACE`
rather than overrunning.

### S3-2 · `EXIT` is destroyed on detach — FR-EXIT fails on the persist path

`Session::detach()` ends with `self.outbound.clear()`. `Daemon::flush_outbound`
drains and discards outbound whenever `client` is `None`.

So: child exits while the window is closed → `poll_child` queues
`Frame::Exit` → no client → frame dropped. On reattach nothing re-emits it, and
`Client::alive` initialises to `true`. The reopened window shows a live cursor
over a dead process, which is the exact failure FR-EXIT names ("a dead pane does
not look alive"), on the exact path Spike 0 exists to prove.

`Session` already retains `child_exit`. The reattach handler must replay it.

### S3-3 · `Pty::drop` kills the child

```rust
impl Drop for Pty {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
```

`Daemon::run` propagates any error out of `step`, which drops `Daemon` →
`Session` → `Pty` → `kill`. A transient `poll` error, an unwind, or any future
`?` on a path that owns the session destroys the user's shell — the one thing
this product promises never to do. Persistence must not depend on nobody ever
returning `Err`.

Fix: `Pty` must detach the child by default and require an explicit
`Pty::terminate()` for intentional teardown. Tests that want cleanup call it.

### S3-4 · The master fd is exported

`Session::master_fd()` is `pub`, and `pty::leak_master_forbidden` is a `pub fn`
returning the raw master. ADR 0001 D5 forbids the master leaving the kernel; the
comment above `leak_master_forbidden` says *"reviews can grep that we never
export the master"* while being the export.

`rilld` genuinely needs readiness, not the fd. Replace both with a poll-only
capability on `Session` (`fn pollfd(&self) -> BorrowedFd<'_>` at minimum, ideally
`fn wait_readable(&mut self, timeout)`), and delete `leak_master_forbidden`.

### S3-5 · Backpressure is nominal

`Client::connect` opens with `Credit(u32::MAX)` and `Client::pump` adds
`Credit(65536)` per pump regardless of consumption, against a kernel that does
`credit.saturating_add`. Credit monotonically outruns delivery, so the GUI never
applies backpressure in the shipped path. The mechanism exists and is
unreachable — which is why S2-1's test could grant infinite credit without
anyone noticing.

Credit must be a window the client *replenishes as it consumes*, and a gate must
observe the kernel stopping its reads.

### S3-6 · Second-attach refusal has a hole

`Daemon::accept_client` refuses only when `self.client.is_some() &&
self.session.attached()`. A client that connects and never sends `ATTACH` leaves
`attached()` false, so the next connection overwrites `self.client`, silently
dropping the first stream. FR-ONE ("a second attach is refused") is bypassable
by not attaching.

### S3-7 · `Daemon::bind` liveness check races

`exists()` → `connect()` → `remove_file()` → `bind()`. Two daemons racing can
both observe a dead socket and the second unlinks the first's. Needs an
exclusive `flock` on a sibling lock file, or bind-to-temp-and-rename.

### S3-8 · Remaining defects

| ID | File | Defect |
|---|---|---|
| S3-8a | `rill-chip0/src/lib.rs` | `Chip0.fed` retains **every byte ever fed**, forever, solely to satisfy the tautological test in S2-2. Unbounded leak on the GUI warm path, proportional to total terminal output. |
| S3-8b | `rill_chip0_vt.c` + `adapter.rs` | Full grid `calloc` in C, then a full `collect()` copy in Rust, every snapshot. Damage rows are computed and then ignored by `TerminalView.paintGrid`, which always redraws all rows. FR-CHIP0's "damage, not full copy" is not implemented end to end. |
| S3-8c | `rill-kernel/src/pty.rs` | `select()` / `fd_set` is undefined behaviour for fd ≥ `FD_SETSIZE` (1024), reachable in a long-lived daemon. `rilld` already uses `poll`; the kernel should too. |
| S3-8d | `rill-host/src/lib.rs` | `Client::send` flips the socket to blocking, `write_all`s, flips back — two `fcntl`s per keystroke on the warm path, plus a blocking write that can stall the UI thread. |
| S3-8e | `rill-host/src/lib.rs` | p95 index is `(n * 0.95).ceil()` on a 0-indexed vec — overshoots by one. Minor, but the number is quoted as evidence. |
| S3-8f | `host/macos/main.m` | `posix_spawn`'s return is ignored and readiness is a flat `usleep(150ms)`. On a loaded machine the app opens dead with "connect failed". Poll for the socket instead. |
| S3-8g | `host/macos/TerminalView.m` | `keyDown:` reads only `charactersIgnoringModifiers` plus two keycodes. No Ctrl, no Alt/ESC-prefix, no `NSTextInputClient`/IME, no mouse. **`^C` cannot be typed into the shipped app** — which also makes T-DROP's `^C` clause untestable through the GUI. |
| S3-8h | `host/macos/TerminalView.m` | A new `MTLTexture` is allocated every frame at 60 Hz and the atlas-free CoreText raster runs on the UI thread. See [ADR 0003](adr/0003-display-pipeline.md). |

---

## S4 — Process and reproducibility

### S4-1 · There is no CI

`.github/` contains issue templates and a PR template. There are **no
workflows**. Every gate in this repository is enforced by a human remembering to
run a shell script on a laptop. AGENTS.md §8 and LANES §1–4 are aspirational
documents, not controls. With 3–5 engineers in parallel this is the single
highest-leverage gap after S1.

### S4-2 · The emulator dependency is unpinned

`scripts/fetch-libghostty-vt.sh` does `git clone --depth 1` of ghostty **main**,
with no commit pin, and skips the fetch entirely if `libghostty-vt.a` already
exists. Upstream's own header states:

> WARNING: This is an incomplete, work-in-progress API. It is not yet stable and
> is definitely going to change.

So a green run today and a red run tomorrow share no referent, and neither does
a green run on two engineers' machines. Current `main` is
`26df373ec83fb1cebb4fee0a8394144ae984a9b8`; that is the pin ADR 0002 adopts.

### S4-3 · `validate-spike0.sh` prints results it did not measure

The summary block hardcodes `pass` for T-SPAWN, T-KILL, and T-RESYNC. T-RESYNC
has **no corresponding command anywhere in the script** — the line is printed
unconditionally. The script also emits no machine-readable evidence, so nothing
downstream can verify what actually ran.

### S4-4 · No fast test tier

`crates/rill-chip0/build.rs` panics when `libghostty-vt.a` is missing, and
`rill-attach`'s pure codec tests sit in the same workspace. `cargo test
--workspace` therefore requires Zig and a full Ghostty build to run a
frame-decoding unit test. Lane B cannot work without Lane C's toolchain, which
contradicts LANES.

### S4-5 · The status docs contradict themselves

[SPIKE-0](SPIKE-0.md) marks T-KILL **Proven** via
`t_quit_app_and_reload_does_not_persist_the_session` — a name that asserts the
bug is *present*. Per AGENTS.md §3 that is the correct name for a red test.
Marking it Proven while it passes means the name is now false. Rename to state
the fixed behaviour and record the bug in the test's doc comment.

---

## Remediation order

Numbered because the dependencies are real.

1. **ADR 0002** — revoke all `Proven` marks; adopt falsifiability rules,
   negative controls, the dependency pin, and CI enforcement. *No code lands
   before this is Accepted.*
2. **ADR 0003** — display pipeline and the corrected T-NFR definition.
3. **Specs and test cases** for every gate, each with an oracle and a required
   mutation that must turn it red.
4. **Harness** — pin libghostty-vt, add CI, split the fast test tier, emit
   machine-readable evidence.
5. **S3-1** (buffer overflow), **S3-2** (EXIT loss), **S3-3** (drop kills child),
   **S3-4** (fd export) — these are shipping-blockers independent of Spike 0.
6. **Rewrite the S1/S2 tests** against the new specs. Each must be demonstrated
   red before it is demonstrated green.
7. **Metal renderer and true key→present instrumentation** per ADR 0003.

Spike 0 remains **Red** and Milestone 1 remains closed until step 7 produces a
p95 on battery from an oracle that can fail.
