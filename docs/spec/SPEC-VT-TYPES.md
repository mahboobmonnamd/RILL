# SPEC-VT-TYPES — shared POD and trait crate (`lane:chip1-vt-engine`, M4)

- **Status:** Accepted for the M4 contract — 2026-08-18. Named tests are **Red**.
- **Authority:** [ADR 0012](../adr/0012-chip1-isolated-vt.md) D2,
  [ADR 0021](../adr/0021-chip1-colour-identity.md),
  [ADR 0022](../adr/0022-chip1-reply-channel.md),
  [ADR 0035](../adr/0035-chip1-character-width.md) D5
- **Issue:** [#6](https://github.com/mahboobmonnamd/RILL/issues/6)
- **Crate:** `crates/rill-vt-types` (not in tree until its tests exist)
- **Gates:** T-CHIP1-POD, T-CHIP1-SIZE — **Red**. Not live. Not T-NFR.

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

Umbrella: [SPEC-CHIP1](SPEC-CHIP1.md).

## 1. Why this crate exists

`rill-chip0` cannot build without Zig and the pinned `libghostty-vt` archive, so
it cannot run in `fast.yml`'s Linux job (SPEC-CHIP0 §9). The POD types and the
trait therefore MUST live in a crate that depends on neither, so Chip 1 is
testable on Linux and Chip 0 keeps implementing the same trait
(ADR 0012 D2).

`rill-vt-types` MUST have no dependencies beyond `core` / `std`. No `libc`, no
`serde`, no Zig, no `ghostty_` identifier, no AppKit. It MUST NOT depend on
`rill-chip0`, `vt-engine`, `rill-attach`, `rill-kernel`, or `rill-host`.

## 2. `PodCell`

`#[repr(C)]`, exactly **16 bytes**, alignment 4.

| Field | Type | Meaning |
|---|---|---|
| `codepoint` | `u32` | Base scalar. Empty cell is space `32`. |
| `fg` | `u32` | RGBA8888, R in the high byte: `(r<<24)\|(g<<16)\|(b<<8)\|0xff` |
| `bg` | `u32` | Same |
| `attrs` | `u16` | bit0 bold, bit1 underline, bit2 inverse, bit3 wide-lead, bit4 wide-tail |
| `_pad` | `u16` | zero |

- A `String` — or any type transitively containing one, or any pointer, `Vec`,
  or reference — MUST NOT be reachable from `PodCell`.
- Bits 3 and 4 mark a wide cluster's lead and tail
  ([ADR 0035](../adr/0035-chip1-character-width.md) D5). Empty cell is space
  `32` with those bits clear. Tail `codepoint` is the lead's base scalar and
  MUST NOT be `0` (the host paints 0 as space). Italic and strikethrough
  exist on the presenter (ADR 0003 D1) but not here. Adding further bits
  needs an ADR. `_pad` stays zero.
- Alpha is always `0xff`. Per-cell alpha is forbidden (ADR 0021 D5).
- `PodCell` values in a snapshot are already materialised RGB. Colour
  **identity** does not appear in this type; it lives inside the engine
  (ADR 0021 D1, [SPEC-VT-COLOR](SPEC-VT-COLOR.md)).

Gate: **T-CHIP1-POD** asserts `size_of::<PodCell>() == 16` and
`align_of::<PodCell>() == 4`. Mutation: add a `String` field.

## 3. `PodGrid`

| Field | Type | Meaning |
|---|---|---|
| `cols`, `rows` | `u16` | Visible grid. |
| `cursor_col`, `cursor_row` | `u16` | 0-based. |
| `cursor_visible` | `bool` | DECTCEM. |
| `full_damage` | `bool` | Whole grid dirty. |
| `damage_row0`, `damage_row1` | `u16` | Inclusive dirty row range. |
| `default_fg`, `default_bg` | `u32` | VT default colours, RGBA8888 as `PodCell.fg` / `bg`. |
| `grapheme_truncated` | `u32` | Clusters truncated to base (ADR 0023 D2). |
| `replies_dropped` | `u32` | Replies lost to a full buffer (ADR 0022 D3). |
| `cells` | `Vec<PodCell>` | Length exactly `cols * rows`, row-major. |

- `cells.len()` MUST equal `cols as usize * rows as usize`. It MUST NOT grow
  with fed bytes: history is the kernel byte ring (ADR 0012 D3).
- When nothing is dirty the caller MUST be able to skip the frame:
  `full_damage == false` and `damage_row0 > damage_row1`.
- `cell(col, row)` returns `Option<&PodCell>` and MUST NOT panic on any input.
- `default_fg` / `default_bg` are the VT defaults the host remaps against
  (ADR 0017 D3). Chip 1 MUST fill them from the current `Palette`
  (`foreground` / `background` at materialisation). Chip 0 fills them from the
  adapter snapshot header. They are not a theme.
- `replies_dropped` is Chip 1's overflow counter (ADR 0022 D3). Chip 0 MUST
  report `0`: it has no reply buffer; libghostty-vt answers queries internally.

Gate: **T-CHIP1-SIZE** asserts `cells.len() == 200` after `resize(40, 5, …)`.
Mutation: unbounded history in the snapshot.

## 4. `Color` and `Palette`

Defined here so the host and the engine name one type (ADR 0021 D3).

```rust
pub struct Rgb { pub r: u8, pub g: u8, pub b: u8 }

pub enum Color { Default, Indexed(u8), Rgb(u8, u8, u8) }

pub struct Palette {
    pub ansi: [Rgb; 16],
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
}
```

- `Rgb` is a 24-bit triplet with **no alpha**. Alpha is always `0xff` at
  materialisation (ADR 0021 D5). `Color::Rgb(r, g, b)` is the same three
  channels, not a second type.
- `Palette` is **data**. This crate MUST NOT contain a named theme's values, and
  MUST NOT parse a look file, read `~/.config/rill/config`, or search theme
  directories — that is the host (ADR 0017 D2).
- `Palette::vt_default()` is the VT default, not a theme: fg `#cccccc`,
  bg `#121212`, cursor `#cccccc`, and the sixteen ANSI values in
  [SPEC-VT-COLOR](SPEC-VT-COLOR.md) §4. It MUST NOT be described or named as a
  theme.
- Resolution rules are [SPEC-VT-COLOR](SPEC-VT-COLOR.md) §3.

## 5. `TerminalEmulation`

```rust
pub trait TerminalEmulation {
    fn feed(&mut self, bytes: &[u8]) -> Result<(), Error>;
    fn resize(&mut self, cols: u16, rows: u16, cell_w: u32, cell_h: u32) -> Result<(), Error>;
    fn snapshot(&mut self) -> Result<PodGrid, Error>;
}
```

Unchanged from what Chip 0 implements today (ADR 0012 D2). Moving it into this
crate is a **relocation**, not a redefinition: `PodCell`, `PodGrid`, `Error`,
and the trait move together. `rill-chip0` re-exports the relocated items and
implements the trait; its behaviour does not change. Chip 0's gates MUST stay
green across that move, and the move MUST be its own PR.

Chip 0's `Error::Config` (host-surface / look parse) and `Error::Io`
(`From<std::io::Error>`) **survive as variants on the relocated `Error`**.
Chip 0 continues to construct them from look and host-surface paths. Chip 1
MUST NOT construct `Config` (it does not parse look files or host-surface).
Chip 1 v0 `feed` / `resize` / `snapshot` perform no I/O and MUST NOT construct
`Io`.

`snapshot_damaged` is [#18](https://github.com/mahboobmonnamd/RILL/issues/18)
(Lane C) and is not declared here.

Inherent on each chip, not on the trait: `reset`, `repaint_bytes`,
`resync_from_history`, and — Chip 1 only — `take_replies` / `has_replies`
(ADR 0022 D1) and `set_palette` (ADR 0021 D2).

## 6. Errors

```rust
pub enum Error {
    Vt(&'static str),
    Config(String),
    Io(std::io::Error),
    /* non-exhaustive */
}
```

- Library paths MUST return `Result`. No `unwrap` / `expect` / `panic!` /
  indexing that can panic on a reachable `feed` / `resize` / `snapshot` /
  `set_palette` / `take_replies` path. Enforced by `lint-planes.sh`
  (SPEC-VT-CONFORMANCE §5).
- `Error` MUST implement `Display` and `std::error::Error`.
- `Error::Io(std::io::Error)` is permitted. The type MUST NOT invent a numeric
  errno it did not receive from `std::io::Error`. Wrapping `std::io::Error` is
  not a "raw OS error code" in that sense.

## 7. Threading

Neither the trait nor the engine is `Sync`. One thread feeds and snapshots
(ADR 0003 D4, ADR 0012 D9). This crate MUST NOT add a lock to work around that.

## 8. Out of scope

PTY, sockets, paint, GPU, AppKit, fonts, Blocks, scrollback, JSON, cells over
IPC, `snapshot_damaged` (#18), theme-file parsing, live swap (M7). Width
tables live in `vt-engine` ([ADR 0035](../adr/0035-chip1-character-width.md)),
not in this crate.
