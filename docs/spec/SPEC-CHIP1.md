# SPEC-CHIP1 — owned VT crate (`lane:chip1-vt-engine`, Milestone 4)

- **Status:** Accepted for the isolated-crate contract — 2026-08-17
  ([ADR 0012](../adr/0012-chip1-isolated-vt.md)). **Amended 2026-08-18** by
  ADRs [0020](../adr/0020-chip1-parser-in-tree.md),
  [0021](../adr/0021-chip1-colour-identity.md),
  [0022](../adr/0022-chip1-reply-channel.md),
  [0023](../adr/0023-chip1-v0-defers-character-width.md) after
  [S-VT #21](https://github.com/mahboobmonnamd/RILL/issues/21) closed.
  **Amended 2026-08-19** by [ADR 0035](../adr/0035-chip1-character-width.md)
  (width; amends 0023 D1/D3/D4/D5). Named tests are **Red**.
- **Authority:** [ADR 0001](../adr/0001-session-operating-system.md) §1,
  [ADR 0012](../adr/0012-chip1-isolated-vt.md)
- **Issue:** [#6](https://github.com/mahboobmonnamd/RILL/issues/6)
- **Crates (not in tree until tests exist):** `crates/rill-vt-types`,
  `crates/vt-engine`
- **Gates:** T-CHIP1-* in [TEST-CASES](../TEST-CASES.md) — **Red**. Not live.
  Not T-NFR.

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

Handoff for a new team: [M4-HANDOFF](../M4-HANDOFF.md). Slice plan and
sequencing: [M4-PLAN](../M4-PLAN.md).

## 0. This page is the umbrella

This spec states the **boundary** and the shape of the crate. The normative
detail lives in six specs, one per slice:

| Spec | Owns |
|---|---|
| [SPEC-VT-TYPES](SPEC-VT-TYPES.md) | `rill-vt-types`: `PodCell`, `PodGrid`, `Color`, `Palette`, the trait, errors |
| [SPEC-VT-PARSER](SPEC-VT-PARSER.md) | Bytes → actions: UTF-8, C1 policy, C0, ESC, CSI, OSC, DCS, bounds |
| [SPEC-VT-SCREEN](SPEC-VT-SCREEN.md) | Grid, cursor, wrap, scroll region, alt screen, erasure, damage, resize, clusters |
| [SPEC-VT-COLOR](SPEC-VT-COLOR.md) | SGR → colour identity, materialisation against the theme palette |
| [SPEC-VT-REPLY](SPEC-VT-REPLY.md) | DA / DSR answers and the bounded reply buffer |
| [SPEC-VT-CONFORMANCE](SPEC-VT-CONFORMANCE.md) | Oracles, fixtures, per-fixture mutations, the `vte` differential, lints, CI |

Where this page and a slice spec disagree, the slice spec is newer and wins.
Three clauses below are superseded outright and say so in place.

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
- Inherent, **Chip 1 only**: `take_replies` / `has_replies`
  ([ADR 0022](../adr/0022-chip1-reply-channel.md),
  [SPEC-VT-REPLY](SPEC-VT-REPLY.md)) and `set_palette`
  ([ADR 0021](../adr/0021-chip1-colour-identity.md),
  [SPEC-VT-COLOR](SPEC-VT-COLOR.md)), and `mode_state`
  ([ADR 0036](../adr/0036-chip1-mode-state-channel.md),
  [SPEC-VT-MODE](SPEC-VT-MODE.md)). The **trait** is unchanged, so Chip 0 is
  untouched and ADR 0012 D2 still holds.
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
| `attrs` | `u16` | bit0 bold, bit1 underline, bit2 inverse, bit3 wide-lead, bit4 wide-tail |
| `_pad` | `u16` | zero |

**Superseded — the colour ADR now exists.** The v0 default is still fg
`#cccccc` / bg `#121212`, but that is `Palette::vt_default()`: a VT default, not
a theme. Cells carry `Color::Default | Indexed(u8) | Rgb` **inside** the engine
and are materialised into `PodCell.fg` / `bg` at `snapshot()` against a
`Palette` the host supplies from the theme **file**. `PodCell` itself does not
move: still `#[repr(C)]`, 16 bytes, RGBA8888, so T-CHIP1-POD and the host ABI
are unaffected. Chip 1 still MUST NOT compile a theme RGB table, and MUST NOT
treat `PodCell.fg` / `bg` as the only colour identity.

Normative: [ADR 0021](../adr/0021-chip1-colour-identity.md),
[SPEC-VT-COLOR](SPEC-VT-COLOR.md). This is the colour ADR that
[#267](https://github.com/mahboobmonnamd/RILL/issues/267) and
[#271](https://github.com/mahboobmonnamd/RILL/issues/271) were blocked on;
T-CHIP1-LOOK-ANSI is now specified (SPEC-VT-COLOR §6). Compositor opacity and
blur remain out (ADR 0021 D5).

Italic and strikethrough exist on the presenter (ADR 0003 D1) but **not** on
Chip 0 `attrs` today. Wide-lead/tail bits 3–4 are authorized for Chip 1 by
[ADR 0035](../adr/0035-chip1-character-width.md) D5; `PodCell` stays 16 bytes.
Adding further bits needs an ADR.

`RILL_GRAPHEME_MAX` is 32. Longer clusters: render the base codepoint, increment
`grapheme_truncated`, never silent drop, never a fixed stack buffer of 8.

**Character width.** Cluster, then width: East Asian Width W/F → 2 columns,
Ambiguous → 1. Generated in-tree table; `unicode-width` is a
`[dev-dependencies]` secondary oracle only. T-CHIP1-WIDTH (`日本X` →
`cursor_col == 5`) replaces T-CHIP1-WIDTH-DEFERRED. M7 must still cite
T-CHIP1-WIDTH as Proven before a live swap
([ADR 0035](../adr/0035-chip1-character-width.md),
[SPEC-VT-SCREEN](SPEC-VT-SCREEN.md) §9). This spec does **not** link Chip 1
as the live chip.

### PodGrid

`cols`, `rows`, `cursor_col`, `cursor_row`, `cursor_visible`, `full_damage`,
`damage_row0`, `damage_row1` (inclusive dirty range), `default_fg`,
`default_bg`, `grapheme_truncated`, `replies_dropped`, `cells` of length
`cols * rows` row-major.

Chip 1 fills `default_fg` / `default_bg` from `Palette`; Chip 0 fills them
from the adapter. `replies_dropped` is 0 on Chip 0 (SPEC-VT-TYPES §3).

When nothing is dirty, the caller MUST be able to skip the frame:
`full_damage == false` and `damage_row0 > damage_row1`.

## 3. v0 sequences

Goal: T-BYTES fixtures, zsh print, `less`, `vim`, alt-screen TUIs (`htop`).
Not a full xterm.

MUST:

- UTF-8 on the byte stream. **Superseded — "or C1" is resolved:** an invalid
  `0x80..=0x9f` byte is one U+FFFD and MUST NOT be dispatched as a control; a
  decoded U+0080..=U+009F scalar paints. `0x9b` does not open a CSI in v0
  ([ADR 0020](../adr/0020-chip1-parser-in-tree.md) D3,
  [SPEC-VT-PARSER](SPEC-VT-PARSER.md) §2). MUST NOT drop high bytes.
- C0: BEL (MAY be a no-op), BS, HT, LF, VT, FF, CR
- Printable ASCII + Unicode; wrap at `cols`; DEC auto-wrap on by default
- ESC 7/8 (DECSC/DECRC), ESC D/E/M (IND/NEL/RI)
- CSI: CUU CUD CUF CUB CUP HVP CHA VPA CNL CPL
- CSI: ED EL ECH (`CSI X`) IL DL ICH DCH SU SD.
  REP (`CSI b`) is a named v0 miss: `infocmp xterm-256color` lists `ech=` and
  does not list `rep`. Consumed and ignored until a later slice
  ([SPEC-VT-SCREEN](SPEC-VT-SCREEN.md) §4).
- CSI SGR: 0, 1, 3, 4, 7, 22–24, 27, 30–37, 40–47, 90–97, 100–107,
  38;5 / 48;5, 38;2 / 48;2
- CSI DECSTBM
- Modes: DECTCEM (`?25`), DECAWM (`?7`), alt-screen `?1049` and `?1047`
  (primary buffer preserved)
- Tabs: default 8-col stops
- DA / DSR `6n`: MUST answer. A TUI that hangs on DA is a v0 miss. **The §2 API
  could not deliver that answer** (SPIKE-VT Result 7); the channel is now
  `take_replies` ([ADR 0022](../adr/0022-chip1-reply-channel.md),
  [SPEC-VT-REPLY](SPEC-VT-REPLY.md)).

MAY consume and ignore OSC 0/1/2/7/8/9/133 (MUST NOT crash). Title and cwd
are attach **classifier** / M6 tap work ([ADR 0013](../adr/0013-cwd-tap.md)),
not Chip 1 paint.

MUST NOT (v0):

- Sixel, ReGIS, Kitty or iTerm images
- Full ISO charset designation beyond UTF-8
- libghostty key encoder (host)
- Mouse-protocol generation (host); parser MAY ignore mouse reports
- Scrollback inside the chip
- JSON; cells over IPC
- Matching Ghostty without a named fixture

**Superseded — the pick is recorded.** [S-VT #21](https://github.com/mahboobmonnamd/RILL/issues/21)
closed ([SPIKE-VT](../SPIKE-VT.md)): the byte parser is written **in this tree**
and `vte` is a `[dev-dependencies]` differential oracle only. Owned grid and POD
are still required, and `libghostty-vt` still MUST NOT appear in Chip 1
([ADR 0020](../adr/0020-chip1-parser-in-tree.md) D1–D2,
[SPEC-VT-PARSER](SPEC-VT-PARSER.md)).

## 4. Tests

Named gates: [TEST-CASES](../TEST-CASES.md) T-CHIP1-*. All **Red** until
demonstrated. Oracle is `snapshot()` (or cursor, or drained replies), never a
copy of the input. Required mutations are part of each gate.
`cfg(feature = "mutate")` only.

T-BYTES fixtures are reused. **Mutation detectability is per fixture:**
`csi_high_param` is blind to `drop_high_bytes` for every candidate S-VT
measured, and MUST NOT be cited as carrying it (ADR 0020 D7). The normative
corpus and table are [SPEC-VT-CONFORMANCE](SPEC-VT-CONFORMANCE.md) §2–§3.

## 5. CI

`fast.yml` MUST `clippy` and `test` `-p rill-vt-types -p vt-engine` on Linux,
and MUST run the `vte` differential there. It MUST NOT gain `rill-chip0`.
`lint-planes.sh` MUST cover Chip 1 snapshot types and unwraps, plus
`no-vte-at-runtime`, `no-theme-rgb-in-rust` and `no-host-dep-on-vt-engine`
([SPEC-VT-CONFORMANCE](SPEC-VT-CONFORMANCE.md) §5–§6).

## 6. Out of scope (isolated crate)

Live swap **wiring** ([#24](https://github.com/mahboobmonnamd/RILL/issues/24)) —
[ADR 0037](../adr/0037-chip1-live-swap.md) is Accepted; host/`rilld` dependency
lifts only in the swap PR after spec and named tests. Blocks, live TUI-in-block,
cwd tap (M6). Chrome, conversations, Metal, fonts. Changing the Ghostty pin. A
second VT in `rilld`.
