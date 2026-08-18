# ADR 0021: Chip 1 cells keep colour identity until the snapshot is materialised

- **Status:** Accepted — 2026-08-18
- **Tree:** this repository only
- **Issue:** [#267](https://github.com/mahboobmonnamd/RILL/issues/267)
  (palette identity), [#271](https://github.com/mahboobmonnamd/RILL/issues/271)
  (T-CHIP1-LOOK-ANSI), epic [#6](https://github.com/mahboobmonnamd/RILL/issues/6)
- **Requires:** [ADR 0012](0012-chip1-isolated-vt.md),
  [ADR 0017](0017-ghostty-look-windowed-default.md) D2–D3 (theme file is data),
  [ADR 0020](0020-chip1-parser-in-tree.md)
- **Amends:** [SPEC-CHIP1](../spec/SPEC-CHIP1.md) §2 — `PodCell.fg` / `bg` are
  no longer the *only* colour identity in the crate. `PodCell` itself is
  unchanged: still `#[repr(C)]`, 16 bytes, RGBA8888.
- **Unblocks:** [#267](https://github.com/mahboobmonnamd/RILL/issues/267),
  [#271](https://github.com/mahboobmonnamd/RILL/issues/271). This is the
  "colour ADR" those issues require.
- **Does not authorize:** Chip 1 as the live chip, a host/daemon dependency,
  a compiled-in theme RGB table, applying `background-opacity` to
  `NSWindow.alphaValue` or a non-opaque `CAMetalLayer`, host chrome, Blocks,
  changing the Ghostty pin, extra `attrs` bits

## Context

[#267](https://github.com/mahboobmonnamd/RILL/issues/267) is the defect Chip 0
cannot fix. Chip 0 snapshots are already RGB by the time the host sees them, so
the host can only remap cells whose colour equals the VT **default**. That
cannot paint ANSI 0–15 — Starship and zsh syntax colours — without either a
compiled-in default-RGB-to-theme-RGB map (forbidden: values come from the theme
file, not a Rust catalog) or making libghostty-vt OSC the sole closer (rejected,
ADR 0017 D3). Chip 0's fallback is embedder configuration through
`GHOSTTY_TERMINAL_OPT_COLOR_*` in the adapter, which Chip 1 cannot copy because
it has no Ghostty FFI.

The user-visible bug this protects against is on the record: Catppuccin Latte
was readable in Ghostty and cmux and unreadable in Rill until the adapter loaded
the theme file palette. SGR greens stayed Ghostty's built-in `#b5bd68` and
washed out on Latte's `#eff1f5`.

SPEC-CHIP1 §2 currently describes `PodCell.fg` / `bg` as RGBA8888 with v0
defaults `#cccccc` / `#121212`. If the parser resolves SGR straight to those
RGB values, Chip 1 inherits exactly Chip 0's defect, and
[#272](https://github.com/mahboobmonnamd/RILL/issues/272) — M7 must not regress
packaged T-LOOK-ANSI — becomes unsatisfiable. Colour must therefore be decided
**before** the SGR slice, not retrofitted after it.

## Decision

### D1 — Cells carry colour *identity*, not resolved RGB

Inside the crate, a cell's foreground and background are:

```rust
pub enum Color {
    Default,        // the theme's `foreground` / `background`
    Indexed(u8),    // ANSI 0-15 and xterm-256 cube/grey: theme `palette = N=#hex`
    Rgb(u8, u8, u8) // SGR 38;2 / 48;2 truecolour: already absolute
}
```

SGR 30–37, 40–47, 90–97, 100–107 and `38;5` / `48;5` MUST produce
`Indexed`. `38;2` / `48;2` MUST produce `Rgb`. SGR 0 and 39 / 49 MUST produce
`Default`. The parser MUST NOT collapse an index to RGB.

Normative detail: [SPEC-VT-COLOR](../spec/SPEC-VT-COLOR.md).

### D2 — `snapshot()` materialises against a palette the caller supplied

`PodGrid` and `PodCell` are unchanged: `#[repr(C)]`, 16 bytes, `fg` / `bg`
RGBA8888 with R in the high byte. T-CHIP1-POD stays green. The host ABI does
not move.

The crate holds a `Palette` — 16 ANSI entries, default foreground, default
background, cursor — set by `set_palette` and applied when a snapshot is
materialised. `Indexed(n)` for `n < 16` resolves through the palette;
`16..=255` resolves through the standard xterm-256 cube and greyscale ramp
computed arithmetically, which is not a theme table. `Default` resolves to the
palette's default foreground / background.

Identity is kept until materialisation. That is the whole decision: it lets a
theme file be **data** without a second VT and without the host rewriting
already-RGB cells.

### D3 — The palette is data, never a Rust table

`vt-engine` MUST NOT contain Catppuccin — or any theme's — RGB values, in any
form, including test constants. Tests parse
`fixtures/look/themes/Catppuccin Latte` and `Catppuccin Mocha`, the same files
ADR 0017 D2 resolves, and assert against what the file says. A file named
`Catppuccin Latte` whose `background =` is not official Latte MUST win.

`vt-engine` MUST NOT read `~/.config/rill/config`, discover theme directories,
or parse the Ghostty look grammar. Loading a look file is the host's job
(ADR 0017 D2); the crate receives a `Palette` value. `rill-vt-types` MAY
define `Palette` so both sides name one type.

The **built-in fallback** palette, used when the host sets nothing, is the
existing VT default: fg `#cccccc`, bg `#121212`, and a conventional ANSI 0–15
set. That is a VT default, not a theme (ADR 0017 Context), and it MUST NOT be
described as one.

### D4 — Two named gates, and neither is host-only

- **T-CHIP1-COLOR-IDENTITY** — after `CSI 32 m`, the cell's colour is
  `Indexed(2)` before materialisation, and materialising the *same* grid against
  Latte then Mocha yields the two different `palette = 2=` values from those
  files. Mutation: resolve SGR to RGB at parse time; both materialisations then
  return the same value.
- **T-CHIP1-LOOK-ANSI** ([#271](https://github.com/mahboobmonnamd/RILL/issues/271))
  — feed `CSI 32 m G` with the palette parsed from the Latte file; snapshot
  `fg` equals that file's `palette = 2=`. Unstyled `A` equals the file
  `foreground =`, with WCAG contrast ≥ 4.5 against the file `background =`.
  Repeat for Mocha so a single constant cannot fake it. Mutation: skip applying
  the file palette.

Both are library gates on `vt-engine`, run in `fast.yml`. Neither is T-NFR and
neither closes the packaged host gates: T-LOOK-ANSI, T-LOOK-CELL and
T-SPLIT-LOOK remain Chip 0 / `lane:host`, and M7's obligation to keep them
green is [#272](https://github.com/mahboobmonnamd/RILL/issues/272).

### D5 — Opacity and blur are compositor keys, and not this crate's business

`background-opacity` and `background-blur-radius` are parsed by the host and
not applied (ADR 0017 D2/D3). `vt-engine` MUST NOT carry an alpha channel per
cell, MUST NOT accept an opacity input, and MUST NOT emit a non-opaque
background. `PodCell` colours stay `…|0xff`. When a compositor path is designed
it will be a host ADR, and T-LOOK-GLASS stays its gate.

### D6 — Fail closed

`set_palette` validates its input and returns `Result`. A malformed or partial
palette MUST NOT silently degrade to a built-in theme guess: it is an error, and
the previously valid palette stands. Materialisation MUST NOT panic on any
index; `Indexed(n)` is total over `u8`.

## Consequences

- The SGR slice cannot land before this ADR, and now does not have to.
- [#267](https://github.com/mahboobmonnamd/RILL/issues/267) and
  [#271](https://github.com/mahboobmonnamd/RILL/issues/271) are unblocked; the
  `blocked` label comes off #271.
- `Palette` is a new shared type in `rill-vt-types`. Chip 0 does not gain it;
  Chip 0 keeps configuring libghostty-vt in its adapter (ADR 0017 D3).
- At M7 the host stops remapping default-coloured cells and instead hands
  Chip 1 the palette it already parsed. That deletion is M7's work, not M4's.
- `fixtures/look/themes/*` become test inputs for a Linux library job, so they
  must stay in the repo and stay parseable without AppKit.

## Rejected alternatives

- **Keep v0 resolved-RGB like Chip 0, decide colour later.** Rejected: it
  reproduces the exact defect #267 exists to fix, and retrofitting identity
  after the SGR slice means rewriting the SGR slice.
- **Widen `PodCell` to carry a tag plus a value.** Rejected: T-CHIP1-POD's 16
  bytes and the host ABI are worth more than saving one materialisation pass,
  and the host has no use for an unresolved index today.
- **Materialise in the host instead of the crate.** Rejected: that is Chip 0's
  arrangement, and it is what cannot paint ANSI 0–15 without a compiled table.
- **Feed OSC 4/10/11 into Chip 1 as the mechanism.** Rejected as the closer for
  the same reason ADR 0017 D3 rejected it for Chip 0: the palette is
  configuration, not terminal output. Chip 1 MAY *accept* OSC 4/10/11 later as
  a hint; it is not this decision.
- **A compiled-in Catppuccin table in Rust.** Rejected by ADR 0017 D2 and
  T-LOOK-FILE.
