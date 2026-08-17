# ADR 0018: Three-pane host chrome (one leaf)

- **Status:** Accepted — 2026-08-17
- **Tree:** this repository only
- **Issue:** [#260](https://github.com/mahboobmonnamd/RILL/issues/260),
  look-file chrome: [#269](https://github.com/mahboobmonnamd/RILL/issues/269),
  inset / type / surface: [#270](https://github.com/mahboobmonnamd/RILL/issues/270)
- **Requires:** [ADR 0009](0009-direct-to-display-echo.md),
  [ADR 0010](0010-spike-0-closes.md),
  [ADR 0011](0011-session-graph.md) (kernel map Proven),
  [ADR 0016](0016-exit-fullscreen-must-not-hang.md),
  [ADR 0017](0017-ghostty-look-windowed-default.md) (look files; D3 allows
  chrome to use the theme background)
- **Amends:** ADR 0011 D5 — the shipped window may wrap the **one** attached
  leaf in M2 chrome. Tabs, nested PTY splits, and a second window stay later.
- **Does not authorize:** tabs, nested pane splits, agents, conversations,
  workspace persist, cwd-leave ([#261](https://github.com/mahboobmonnamd/RILL/issues/261)),
  JSON on the warm path, a second VT, dumping the live grid into `Text`,
  chrome on `--nfr-key`, `NSVisualEffectView` over Metal, inventory F-004 /
  F-008 / F-020 / F-024.

## Context

Spike 0 and M1 first slice are Proven. The window is still one `TerminalView`
as `contentView`. That is the measurement closer (ADR 0009), not the product
shell.

M2 starts as layout: left navigation, center Chip 0, right inspector. The
kernel already has N leaves. This slice still attaches **one**. Chrome that
hid a one-`Session` kernel would be a lie (ADR 0011 D5); chrome around a map
that already exists is not.

Cwd-leave (create or switch a workspace when the fg process leaves the
workspace root) is [#261](https://github.com/mahboobmonnamd/RILL/issues/261).
It needs a cold host read of `Session::cwd()` (ADR 0013 D4). Not this PR.

## Decision

### D1 — Default launch is a three-column split

`NSWindow.contentView` is an `NSSplitView` (`isVertical = YES`) with three
subviews:

| Column | Owns | Must not |
|---|---|---|
| Left | Workspace list chrome | Paint PTY bytes; a second VT |
| Center | Chip 0 `TerminalView` (`MTKView`) | Become `NSTextView` of the grid |
| Right | Inspector chrome (inert stubs) | Own a PTY; JSON on the key path |

Center is the only live emulator. Sidebars are AppKit. First responder is
the terminal. Sidebars MUST NOT become first responder on click in this
slice.

Left MAY show one placeholder workspace row (directory name). Right MAY show
inert **Changes** and **Files** rows. Neither is wired.

### D2 — T-NFR has no chrome

`--nfr-key` keeps `TerminalView` as `contentView` and still enters a
fullscreen Space (ADR 0009). Chrome MUST NOT recut the closer. Mutation
`no_chrome` restores that path for T-SPLIT's negative control.

### D3 — Chrome is cold

No extra attach tags. No cells over IPC. No JSON on the typing path.
Installing the split MUST NOT `posix_spawn` a user shell and MUST NOT kill
the leaf.

### D4 — Oracle

Named test `t_window_is_three_pane_split_around_chip0`. Packaged `Rill.app`
heartbeat reports three columns with non-zero left/right widths and
`first=terminal`. Socket-only tests do not close this. Mutation `no_chrome`
MUST turn it red.

### D5 — Sidebars use a derived chrome surface from the look file

Left and right MUST NOT paint a hardcoded gray. They MUST NOT paint the same
pixels as Chip 0's look `background`. Chrome surface is the resolved look
`background` with each 8-bit RGB channel saturating-minus **9**. That is a
formula, not a compiled Catppuccin mantle table (ADR 0017 D2). Center Chip 0
keeps the file `background`. Label `foreground` is the look `foreground`.

Oracle: left pane `CALayer.backgroundColor` equals that formula applied to
`background =` from the theme **file** (Latte and Mocha both required). It
MUST NOT equal the file background. Mutation `hardcoded_chrome_gray` MUST
turn T-SPLIT-LOOK red.

Do not compile Catppuccin RGB into `ChromeHost`. Do not set
`NSWindow.alphaValue` from `background-opacity`.

### D6 — Chrome top inset and type follow the live pane

Section labels MUST sit `padding-y` (host-surface / look `window-padding-y`)
from the top of the chrome pane, using **live bounds**. MUST NOT position
from a hardcoded 680pt content height. Mutation `hardcoded_chrome_y` MUST
turn T-CHROME-INSET red.

Section labels MUST use `NSFont.systemFontSize` (macOS control size), not
11pt caption size. Mutation `tiny_chrome_font` MUST turn T-CHROME-FONT red.
Terminal faces stay the look `font-family` / `font-size` (not this decision).

## Consequences

- [SPEC-CHROME](../spec/SPEC-CHROME.md) is the host contract for this slice.
- [SPEC-DISPLAY](../spec/SPEC-DISPLAY.md) §9 stays Spike 0; M2 chrome is not a
  reopen of T-NFR.
- [#261](https://github.com/mahboobmonnamd/RILL/issues/261) may take workspace
  identity and cwd-leave after its own ADR.

## Rejected alternatives

- **SwiftUI `NavigationSplitView` observing the PTY buffer.** Rejected: ADR
  0001; the previous prototype died there.
- **Center as chat / `Text` dump of the grid.** Rejected: Chip 0 stays the
  live surface.
- **Chrome on the T-NFR path.** Rejected: ADR 0009 closer.
- **Tabs and nested splits in the same PR.** Rejected: this issue is the
  split around one leaf.
- **Hardcoded dark sidebars around a Latte terminal.** Rejected: [#269](https://github.com/mahboobmonnamd/RILL/issues/269).
- **Compile Catppuccin mantle `#e6e9ef`.** Rejected: ADR 0017 D2; the
  saturating-minus-9 formula is the chrome surface.
