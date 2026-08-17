# ADR 0017: Ghostty look overlay; windowed default

- **Status:** Accepted — 2026-08-17
- **Tree:** this repository only
- **Issue:** [#259](https://github.com/mahboobmonnamd/RILL/issues/259)
- **Requires:** [ADR 0009](0009-direct-to-display-echo.md),
  [ADR 0010](0010-spike-0-closes.md),
  [ADR 0016](0016-exit-fullscreen-must-not-hang.md)
- **Amends:** ADR 0009 D1 — `toggleFullScreen:` is the T-NFR closer, not the
  default `make run` path. [ADR 0016](0016-exit-fullscreen-must-not-hang.md)
  still owns leave.
- **Does not authorize:** a theme store, git-clone of themes as a product
  feature, tabs/splits/sidebar/Blocks, Herdr or `cmux.json` import, dropping
  `toggleFullScreen:` on T-NFR hid, a non-opaque `CAMetalLayer`,
  `NSWindow.alphaValue` as Ghostty `background-opacity`, a compiled-in
  Catppuccin (or any theme) RGB table, `NSVisualEffectView` backdrop blur,
  inventory F-210 / F-104, Chip 1 palette-index cells
  ([#267](https://github.com/mahboobmonnamd/RILL/issues/267); that is a Chip 1
  colour ADR, not this host overlay).

## Context

Spike 0 proved attach, persist, and NFR-KEY. The closer (ADR 0009) is a titled
window that enters a fullscreen Space. That is a measurement path, not a
product default. `make run` must open a normal window.

Look is a RILL file, same Ghostty line grammar, copied from the user's
system-setup Ghostty look keys. Path: `~/.config/rill/config` (override
`RILL_CONFIG`). zsh / Starship stay the user's shell config; this ADR does
not touch them.

Chip 0's empty cell is still `#121212` / `#cccccc`. That is the VT default,
not the host look. Remap is host-side, from the loaded theme **file**.
ANSI 0–15 and compositor opacity need Chip 1 palette-index cells; this ADR
does not invent a Chip 0 RGB catalog to fake them.

## Decision

### D1 — Default launch is windowed

After `makeKeyAndOrderFront:`, do **not** call `toggleFullScreen:`.
`collectionBehavior` stays `FullScreenPrimary`. The green button and a second
`toggleFullScreen:` still enter and leave a Space (ADR 0016).

`--nfr-key` (T-NFR) still enters fullscreen before measuring. That is the
0009 closer. Do not recut the oracle.

`RILL_TEST_EXIT_FULLSCREEN=1` enters, then leaves. T-FS-EXIT does not depend
on default launch being fullscreen.

Mutation `always_toggle_fullscreen` MUST turn T-WINDOWED red.

### D2 — Overlay `~/.config/rill/config`; not a theme store

Load `host-surface.toml` (bundled / `RILL_HOST_SURFACE`), then overlay the
first file that exists:

1. `RILL_CONFIG`
2. `~/.config/rill/config`

Parser is Ghostty's line grammar, not TOML. Unquoted `theme = Catppuccin Latte`
and unquoted `#hex` are values. Unknown keys are ignored. An unknown `theme =`
name does not replace already-resolved host-surface colors.

Do **not** live-read `~/.config/ghostty/config` or cmux files. Those are
another product. Copy look keys into the RILL file once.

`theme =` resolves a Ghostty-grammar **file**, not a Rust `match` on the
name. Search, in order, the first directory that contains that file:

1. `themes/` next to the look file (`~/.config/rill/themes/`)
2. `themes/` next to `host-surface.toml` (packaged `Resources/themes/`)
3. `~/.config/rill/themes/`
4. in-tree `fixtures/look/themes/` when that directory exists (tests)

We ship files under `fixtures/look/themes/` and copy them into
`Resources/themes/` at package time. We do not clone a theme git repo.
We do **not** compile Catppuccin (or any theme) RGB into Rust. A file
named `Catppuccin Latte` whose `background =` is not official Latte MUST
win over any previously hardcoded table.

Look keys this ADR applies: `theme`, `font-family`, `font-size`,
`font-family-fallback`, `background` / `foreground` / `cursor` / `palette`,
`window-padding-x` / `window-padding-y`, `background-opacity`,
`split-divider-color`, `macos-option-as-alt`.

Explicit hex overrides the theme file. `background-opacity` and
`background-blur-radius` are parsed and **not** applied
(`NSWindow.alphaValue` washes the surface; `NSVisualEffectView` blanks
it). Opacity is a compositor key for Chip 1.

Mutation `skip_ghostty_overlay` MUST turn T-LOOK-OVERLAY red.
Mutation `invent_theme_rgb` MUST turn T-LOOK-FILE red.

### D3 — Theme is host remap, not Chip 0

After Chip 0 snapshot, cells whose fg/bg equal the VT default are rewritten
to the **file-resolved** default foreground / background. Truecolor / other
RGB pass through. Chip 0 domain types still do not name Ghostty FFI. Theme
is not a second VT. The host MUST NOT RGB-rewrite ANSI 0–15 from a
compiled-in Chip 0 default map; Chip 0 snapshots have already lost the
palette index.

The Metal layer stays **opaque** (ADR 0009). `NSWindow` stays opaque.
`window.alphaValue` stays `1`. Window chrome MAY use the theme background
at alpha 1. `background-opacity` MUST NOT set `NSWindow.alphaValue` in
windowed or fullscreen (that was the washed-out glass bug).

Mutation `skip_theme_apply` MUST turn T-LOOK-CELL red.
Mutation `window_alpha_from_opacity` MUST turn T-LOOK-GLASS red.

### D4 — Shell is `$SHELL`

This ADR does not add `command =` / `shell =` to `host-surface.toml`. zshrc,
Starship, and Atuin stay in the user's tree. `rilld` already execs `$SHELL`.

## Consequences

- SPEC-DISPLAY §3 names windowed default. §10 names the look overlay.
- T-NFR hid evidence is unchanged (still fullscreen).
- `host-surface.toml` remains the bundled fallback when `~/.config/rill/config`
  is absent.

## Rejected alternatives

- **Stay fullscreen forever / hide the green button.** Rejected: ADR 0016.
- **Borderless `screen.frame` cover.** Rejected: ADR 0009 D2.
- **A RILL-only theme store / git clone.** Rejected: look keys live in one
  user file, `~/.config/rill/config`.
- **Live-read `~/.config/ghostty/config`.** Rejected: that file belongs to
  Ghostty. Copy the look subset into the RILL path.
- **Import `cmux.json` or Herdr `config.toml`.** Rejected: chrome and a
  multiplexer, not look.
- **Feed OSC 10/11 as the only path.** Rejected as sole mechanism: remap is
  host-side and does not depend on libghostty-vt OSC support. OSC MAY be used
  later; it is not this closer.
- **Compiled-in Catppuccin RGB tables.** Rejected: `theme =` loads a file.
  T-LOOK-FILE is the gate.
- **`NSWindow.alphaValue = background-opacity`.** Rejected: Ghostty opacity
  is compositor; this made the window glass and hid the theme. T-LOOK-GLASS
  is the gate.
