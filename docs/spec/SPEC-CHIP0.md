# SPEC-CHIP0 — retired

- **Status:** **Retired 2026-08-21** ([ADR 0054](../adr/0054-chip0-retired.md)).
  The crate `crates/rill-chip0` and `libghostty-vt` are gone. Live emulator:
  [SPEC-CHIP1](SPEC-CHIP1.md) / [SPEC-VT-LIVE-SWAP](SPEC-VT-LIVE-SWAP.md).
  This page is historical.
- **Authority:** [ADR 0001](../adr/0001-session-operating-system.md) §1, §3,
  [ADR 0003](../adr/0003-display-pipeline.md)
- **Crate:** `crates/rill-chip0`
- **Gates:** T-BYTES, T-RESYNC — **Proven**. T-NFR is the host (SPEC-DISPLAY).

## 1. Boundary

- libghostty-vt types MUST appear only in
  `crates/rill-chip0/src/adapter/rill_chip0_vt.{c,h}`. No `ghostty_` identifier
  elsewhere; enforced by `scripts/lint-planes.sh`.
- `Chip0` implements `TerminalEmulation` and is swappable with a future Chip 1
  behind the same traits. Domain code MUST NOT name either.

## 2. Pinned dependency

- libghostty-vt is pinned to
  `ghostty-org/ghostty@26df373ec83fb1cebb4fee0a8394144ae984a9b8`
  (`third_party/ghostty.pin`).
- `build.rs` MUST verify the checked-out SHA against the pin and fail closed on
  mismatch or on an archive of unknown provenance (ADR 0002 D7).
- Upstream declares this API unstable. Moving the pin is its own PR with the
  full gate suite re-run.

## 3. Feed

```rust
fn feed(&mut self, bytes: &[u8]) -> Result<(), Error>;
```

- Bytes MUST reach `ghostty_terminal_vt_write` unmodified. No UTF-8 validation,
  no `from_utf8_lossy`, no re-encoding.
- `Chip0.fed` — the retained copy of every byte ever fed — is **deleted**. It
  existed only to satisfy a tautological test and is an unbounded leak on the
  warm path (audit S2-2, S3-8a).
- `feed` MUST NOT allocate proportionally to input length.

## 4. Snapshot

```rust
fn snapshot(&mut self) -> Result<PodGrid, Error>;
fn snapshot_damaged(&mut self, out: &mut PodBuffer) -> Result<Damage, Error>;
```

- `PodCell` is `#[repr(C)]`, 16 bytes: `codepoint u32, fg u32, bg u32, attrs
  u16, _pad u16`. A `String` MUST NOT appear in any type reachable from a
  snapshot.
- **Proven today:** `snapshot()` returns a POD grid; the host rewrites instance
  rows using `damage_row0..=damage_row1` / `full_damage`.
- **Later:** [#18](https://github.com/mahboobmonnamd/RILL/issues/18) —
  `snapshot_damaged` as a trait method that writes **only damaged rows** into a
  caller-owned buffer and MUST NOT allocate (ADR 0003 D3).
  `TerminalEmulation` does not yet declare it. Not a Spike 0 reopen.
- `full_damage` is set when libghostty-vt reports
  `GHOSTTY_RENDER_STATE_DIRTY_FULL`. `damage_row0..=damage_row1` is the
  inclusive dirty range otherwise. When dirty is `FALSE` the caller MUST be able
  to skip the frame entirely.
- `ghostty_render_state_clean` is called once per consumed frame, and only
  after the caller has taken the data.

## 5. Grapheme handling — memory safety

Audit S3-1. **Fixed in the closer:** query length, heap buffer, count truncations.
The old adapter passed a fixed `uint32_t buf[8]` and discarded the clamp. Keep
the fixture and ASan job; do not reintroduce a stack buffer.

- Query `GRAPHEMES_LEN` first.
- Either allocate to fit, or use
  `GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8`, which takes a
  `GhosttyBuffer` with explicit capacity and returns `GHOSTTY_OUT_OF_SPACE`
  rather than writing past the end.
- A cluster that exceeds a reasonable bound is truncated to its base codepoint
  and **counted**, not silently dropped.
- `fixtures/bytes/zwj_emoji.bin` (a ≥9-codepoint ZWJ sequence) is a permanent
  regression fixture, run under ASan in `gates.yml` (`RILL_ASAN=1`, isolated
  target dir). `fast.yml` must not grow a Chip 0 / Zig dependency.

## 6. Resize

```rust
fn resize(&mut self, cols: u16, rows: u16, cell_w: u32, cell_h: u32) -> Result<(), Error>;
```

Delegates to `ghostty_terminal_resize`. The chip MUST NOT own scrollback —
history lives in the kernel's byte ring (ADR 0001 §4).

## 7. Resync

```rust
pub fn resync_from_history(&mut self, history: &[u8]) -> Result<Vec<u8>, Error>;
```

- Cold path only, once per attach. After a reattach, `Session::resync_count()`
  MUST NOT increase during 100 subsequent keystrokes (TEST-CASES T-RESYNC).
  Attach→detach→attach is two resyncs; the absolute count is not 1.
- Resets, feeds history, and emits VT bytes via
  `ghostty_formatter_terminal_new` + `GHOSTTY_FORMATTER_FORMAT_VT`.
- The window MUST NOT be able to distinguish resync bytes from live bytes: they
  arrive as ordinary `DATA` frames with no marker.
- The headless resync chip is the **same implementation** as the live chip. A
  second VT implementation in the kernel is forbidden (ADR 0001 §3).
- Tests MUST NOT assert on the `\x1b[2J\x1b[H` prefix this function itself
  prepends (audit S2, ADR 0002 D4). Assert on the resulting grid.

## 8. Error handling

- `Vt::new` failure paths in C MUST free every partially constructed handle.
  The current cascade does; keep it.
- Every FFI return is checked. `Vt` methods returning `void` in C are only
  acceptable where upstream cannot fail.
- `unsafe impl Send for Vt` is retained but the chip is **not** `Sync` and MUST
  NOT be shared across threads (ADR 0003 D4 keeps feed and render on one
  thread).

## 9. Test tier

`rill-chip0` requires the Zig-built archive, so it cannot run in `fast.yml`'s
Linux job. `rill-attach` and the pure-Rust parts of `rill-kernel` MUST NOT
depend on `rill-chip0`, so kernel and attach can be developed and tested
without Chip 0's toolchain (audit S4-4).
