# SPEC-VT-COLOR — Chip 1 colour identity (`lane:chip1-vt-engine`, M4)

- **Status:** Accepted for the M4 contract — 2026-08-18. Named tests are **Red**.
- **Authority:** [ADR 0021](../adr/0021-chip1-colour-identity.md),
  [ADR 0017](../adr/0017-ghostty-look-windowed-default.md) D2–D3
- **Issue:** [#267](https://github.com/mahboobmonnamd/RILL/issues/267),
  [#271](https://github.com/mahboobmonnamd/RILL/issues/271), epic
  [#6](https://github.com/mahboobmonnamd/RILL/issues/6)
- **Crate:** `crates/vt-engine`, types in `crates/rill-vt-types`
- **Gates:** T-CHIP1-COLOR-IDENTITY, T-CHIP1-LOOK-ANSI, T-CHIP1-SGR — **Red**.
  Library only. Not the packaged host gates T-LOOK-ANSI / T-LOOK-CELL /
  T-SPLIT-LOOK, which stay Chip 0 / `lane:host`.

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. The rule

**Cells keep colour identity until the snapshot is materialised.** SGR MUST NOT
be resolved to RGB at parse time (ADR 0021 D1).

This is the difference between a theme file being data and a theme file being a
suggestion. Chip 0 resolves to RGB inside libghostty-vt, so the host can only
remap cells that still equal the VT default — which is why ANSI 0–15 (Starship,
zsh syntax) could not be themed without either a compiled RGB catalog or making
OSC the sole closer, both rejected (ADR 0017 D3).

## 2. SGR → identity

| SGR | Result |
|---|---|
| `0` | Reset attrs; fg and bg → `Color::Default` |
| `1` / `22` | bold on / off (`attrs` bit0) |
| `3` | italic — **consumed, no attr bit in v0** (SPEC-VT-TYPES §2) |
| `4` / `24` | underline on / off (bit1) |
| `7` / `27` | inverse on / off (bit2) |
| `30`–`37` | fg `Indexed(0..=7)` |
| `90`–`97` | fg `Indexed(8..=15)` |
| `40`–`47` | bg `Indexed(0..=7)` |
| `100`–`107` | bg `Indexed(8..=15)` |
| `38;5;n` / `48;5;n` | `Indexed(n)` |
| `38;2;r;g;b` / `48;2;r;g;b` | `Rgb(r, g, b)` |
| `39` / `49` | fg / bg → `Color::Default` |

- Sub-parameter form (`38:2:r:g:b`, `38:5:n`) MUST behave identically
  (SPEC-VT-PARSER §5).
- `CSI m` with no parameters is `CSI 0 m`.
- Unknown SGR parameters are consumed and ignored; they MUST NOT abort the rest
  of the sequence. `ESC[1;99;4m` MUST still apply bold and underline.
- A missing colour argument (`ESC[38m`, `ESC[38;5m`) MUST NOT panic and MUST NOT
  consume following parameters as colour.
- `Indexed` MUST be preserved as the index. Collapsing `Indexed(2)` to RGB at
  parse time is exactly the mutation T-CHIP1-COLOR-IDENTITY detects.

## 3. Materialisation

`snapshot()` resolves identity to `PodCell.fg` / `bg` as RGBA8888, R in the high
byte, alpha always `0xff`:

- `Default` → the palette's `foreground` / `background`.
- `Indexed(n)` for `n <= 15` → `palette.ansi[n]`.
- `Indexed(n)` for `16..=231` → the xterm 6×6×6 cube, computed arithmetically:
  channel levels `0, 95, 135, 175, 215, 255`.
- `Indexed(n)` for `232..=255` → the greyscale ramp, `8 + 10 * (n - 232)`.
- `Rgb(r, g, b)` → itself.

The cube and ramp are **arithmetic**, not a theme table, so they are permitted
in Rust. `Indexed` MUST be total over `u8`: no index may panic.

Inverse (`attrs` bit2) is applied by the **presenter**, not by swapping `fg` and
`bg` during materialisation, so the snapshot keeps what the program asked for.

## 4. Palette input

```rust
pub fn set_palette(&mut self, palette: Palette) -> Result<(), Error>;
```

- The host loads the look file and passes a `Palette` (ADR 0017 D2 owns
  discovery and grammar). `vt-engine` MUST NOT read `~/.config/rill/config`,
  search theme directories, parse the Ghostty line grammar, or touch the
  filesystem at all.
- `vt-engine` and `rill-vt-types` MUST NOT contain Catppuccin — or any theme's —
  RGB values, including in test constants (ADR 0021 D3).
- Before any `set_palette`, `Palette::vt_default()` applies. That is a VT
  default and MUST NOT be called a theme. Values match Chip 0 before
  `apply_look`: fg `#cccccc`, bg `#121212`, cursor `#cccccc` (adapter
  fallbacks in `rill_chip0_vt.c`), and ANSI 0–15 equal to Chip 0's un-looked
  adapter palette at the pin in `third_party/ghostty.pin` (what the adapter
  loads before `apply_look`). Index 2 is `#b5bd68`, the Chip 0 default green
  T-LOOK-ANSI already names.

  | Index | RGB | Index | RGB |
  |---|---|---|---|
  | 0 | `#1d1f21` | 8 | `#666666` |
  | 1 | `#cc6666` | 9 | `#d54e53` |
  | 2 | `#b5bd68` | 10 | `#b9ca4a` |
  | 3 | `#f0c674` | 11 | `#e7c547` |
  | 4 | `#81a2be` | 12 | `#7aa6da` |
  | 5 | `#b294bb` | 13 | `#c397d8` |
  | 6 | `#8abeb7` | 14 | `#70c0b1` |
  | 7 | `#c5c8c6` | 15 | `#eaeaea` |

  These sixteen values are listed in `Palette::vt_default()` as hex. They MUST
  NOT be produced at runtime by calling Ghostty. They are the
  `no-theme-rgb-in-rust` exemption (SPEC-VT-CONFORMANCE §5).
- `set_palette` validates and returns `Result`. A malformed palette MUST NOT
  degrade to a built-in guess; it is an error and the previous valid palette
  stands (ADR 0021 D6).
- Changing the palette sets `full_damage`: every materialised colour may move.
- OSC 4/10/11 are consumed and ignored in v0 (SPEC-VT-PARSER §8). The palette is
  configuration, not terminal output.

## 5. Alpha, opacity, blur

`PodCell` colours always end `0xff`. The crate MUST NOT carry per-cell alpha,
accept an opacity input, or emit a non-opaque background.
`background-opacity` and `background-blur-radius` are compositor keys parsed and
not applied by the host (ADR 0017 D2/D3, ADR 0021 D5). T-LOOK-GLASS stays the
host's gate.

## 6. Gates

**T-CHIP1-COLOR-IDENTITY.** Feed `CSI 32 m G`. Before materialisation the cell's
fg is `Indexed(2)`. Materialise the **same** engine state against a palette
parsed from `fixtures/look/themes/Catppuccin Latte`, then against
`Catppuccin Mocha`: the two `fg` values differ and each equals that file's
`palette = 2=`.

*Required mutation.* Resolve SGR to RGB at parse time. Both materialisations then
return the same value and the gate goes red.

**T-CHIP1-LOOK-ANSI** ([#271](https://github.com/mahboobmonnamd/RILL/issues/271)).
With the Latte palette applied, `CSI 32 m G` gives `fg` equal to the file's
`palette = 2=`; an unstyled `A` gives `fg` equal to the file's `foreground =`,
with WCAG contrast ≥ 4.5 against the file's `background =`. Repeat for Mocha so
one cream constant cannot fake it.

*Required mutation.* Skip applying the file palette; SGR 32 stays the built-in
default and contrast against the Latte background fails.

Both parse the fixture files at test time. Neither is T-NFR. Neither closes the
packaged host gates; M7's obligation to keep those green is
[#272](https://github.com/mahboobmonnamd/RILL/issues/272).

## 7. Out of scope

Theme discovery and look-file grammar (host, ADR 0017), compositor opacity and
blur, `selection-background` / `selection-foreground` (no selection in v0),
cursor colour rendering (presenter), OSC 4/10/11 as a live mechanism, host
chrome ([#270](https://github.com/mahboobmonnamd/RILL/issues/270)), the live
swap (M7).
