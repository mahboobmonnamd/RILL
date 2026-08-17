# SPEC-CHIP1 — owned VT crate (`lane:chip1-vt-engine`, Milestone 4)

- **Status:** Accepted for the isolated-crate contract — 2026-08-17
  ([ADR 0012](../adr/0012-chip1-isolated-vt.md)). Named tests are **Red**.
- **Authority:** [ADR 0001](../adr/0001-session-operating-system.md) §1,
  [ADR 0012](../adr/0012-chip1-isolated-vt.md)
- **Issue:** [#6](https://github.com/mahboobmonnamd/RILL/issues/6)
- **Crates (not in tree until tests exist):** `crates/rill-vt-types`,
  `crates/vt-engine`
- **Gates:** T-CHIP1-* in [TEST-CASES](../TEST-CASES.md) — **Red**. Not live.
  Not T-NFR.

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

Handoff for a new team: [M4-HANDOFF](../M4-HANDOFF.md).

## 1. Boundary

- Chip 1 is a **library**. Bytes in, `PodGrid` out. It MUST NOT own a PTY,
  paint, talk AppKit, dump cells into `Text`, or appear in `rill-host` /
  `rilld` `Cargo.toml` until M7.
- Domain sources MUST NOT contain `ghostty_` identifiers.
- `rill-chip0` MUST NOT depend on `vt-engine`. `vt-engine` MUST NOT depend on
  `rill-chip0` except an optional macOS-only *dev* differential.
- Shared types MUST live in `rill-vt-types` so `fast.yml` can test Chip 1 on
  Linux with no Zig (SPEC-CHIP0 §9).

## 2. API

```rust
pub trait TerminalEmulation {
    fn feed(&mut self, bytes: &[u8]) -> Result<(), Error>;
    fn resize(&mut self, cols: u16, rows: u16, cell_w: u32, cell_h: u32) -> Result<(), Error>;
    fn snapshot(&mut self) -> Result<PodGrid, Error>;
}
```

- `feed` MUST pass bytes to the parser unmodified. No UTF-8 validation filter,
  no `from_utf8_lossy`, no dropping `>= 0x80`. Illegal UTF-8 MAY become U+FFFD
  in a cell; that is decoding, not dropping.
- `feed` MUST NOT allocate proportionally to input length.
- `resize` MUST change the visible grid to `cols * rows`. The chip MUST NOT
  keep a scrollback ring.
- `snapshot` MUST return a POD grid. A `String` MUST NOT appear in any type
  reachable from it.
- Inherent methods, matching Chip 0: `reset`, `repaint_bytes`,
  `resync_from_history`. Tests MUST NOT assert on a `\x1b[2J\x1b[H` prefix the
  emit path itself prepends (ADR 0002 D4). Assert a second instance’s grid.
- `snapshot_damaged` is [#18](https://github.com/mahboobmonnamd/RILL/issues/18),
  not this spec.
- Library paths MUST return `Result`. The type is not `Sync`. One thread feeds
  and snapshots.

### PodCell (`repr(C)`, 16 bytes)

| Field | Type | Meaning |
|---|---|---|
| `codepoint` | `u32` | Base scalar. Empty cell is space `32`. |
| `fg` | `u32` | RGBA8888, R in the high byte: `(r<<24)\|(g<<16)\|(b<<8)\|0xff` |
| `bg` | `u32` | Same |
| `attrs` | `u16` | bit0 bold, bit1 underline, bit2 inverse |
| `_pad` | `u16` | zero |

v0 default colours (match Chip 0 adapter until a colour ADR): fg `#cccccc`,
bg `#121212`. Theme / `host-surface.toml` is the host, not this crate.

Italic, strikethrough, wide-lead/tail exist on the presenter (ADR 0003 D1) but
**not** on Chip 0 `attrs` today. v0 MUST NOT add extra attr bits without an ADR.

`RILL_GRAPHEME_MAX` is 32. Longer clusters: render the base codepoint, increment
`grapheme_truncated`, never silent drop, never a fixed stack buffer of 8.

### PodGrid

`cols`, `rows`, `cursor_col`, `cursor_row`, `cursor_visible`, `full_damage`,
`damage_row0`, `damage_row1` (inclusive dirty range), `grapheme_truncated`,
`cells` of length `cols * rows` row-major.

When nothing is dirty, the caller MUST be able to skip the frame:
`full_damage == false` and `damage_row0 > damage_row1`.

## 3. v0 sequences

Goal: T-BYTES fixtures, zsh print, `less`, `vim`, alt-screen TUIs (`htop`).
Not a full xterm.

MUST:

- UTF-8 on the byte stream; illegal sequences → U+FFFD (or C1). MUST NOT drop
  high bytes.
- C0: BEL (MAY be a no-op), BS, HT, LF, VT, FF, CR
- Printable ASCII + Unicode; wrap at `cols`; DEC auto-wrap on by default
- ESC 7/8 (DECSC/DECRC), ESC D/E/M (IND/NEL/RI)
- CSI: CUU CUD CUF CUB CUP HVP CHA VPA CNL CPL
- CSI: ED EL IL DL ICH DCH SU SD
- CSI SGR: 0, 1, 3, 4, 7, 22–24, 27, 30–37, 40–47, 90–97, 100–107,
  38;5 / 48;5, 38;2 / 48;2
- CSI DECSTBM
- Modes: DECTCEM (`?25`), DECAWM (`?7`), alt-screen `?1049` and `?1047`
  (primary buffer preserved)
- Tabs: default 8-col stops
- DA / DSR `6n`: MUST answer. A TUI that hangs on DA is a v0 miss.

MAY consume and ignore OSC 0/1/2/7/8/9/133 (MUST NOT crash). Title and cwd
are attach **classifier** / M6 tap work, not Chip 1 paint.

MUST NOT (v0):

- Sixel, ReGIS, Kitty or iTerm images
- Full ISO charset designation beyond UTF-8
- libghostty key encoder (host)
- Mouse-protocol generation (host); parser MAY ignore mouse reports
- Scrollback inside the chip
- JSON; cells over IPC
- Matching Ghostty without a named fixture

Parser: owned grid is required. Byte parser MAY be in-tree or the `vte` crate
(parser only). MUST NOT use `libghostty-vt`. [S-VT #21](https://github.com/mahboobmonnamd/RILL/issues/21)
records the pick before the first CSI parser PR (ADR 0012 D6).

## 4. Tests

Named gates: [TEST-CASES](../TEST-CASES.md) T-CHIP1-*. All **Red** until
demonstrated. Oracle is `snapshot()` (or cursor), never a copy of the input.
Required mutations are part of each gate. `RILL_MUTATE=drop_high_bytes` for
T-CHIP1-BYTES, `cfg(feature = "mutate")` only.

T-BYTES fixtures are reused (`lone_continuation`, `truncated_3byte`,
`overlong_slash`, `lone_surrogate`, `bom_then_high`, `csi_high_param`,
`c1_in_utf8`, `fixtures/bytes/*.bin`).

## 5. CI

`fast.yml` MUST `clippy` and `test` `-p rill-vt-types -p vt-engine` on Linux.
It MUST NOT gain `rill-chip0`. `lint-planes.sh` MUST cover Chip 1 snapshot
types and unwraps.

## 6. Out of scope

Live swap (M7). Blocks, live TUI-in-block, cwd tap (M6). Chrome, conversations,
Metal, fonts. Changing the Ghostty pin. A second VT in `rilld`.
