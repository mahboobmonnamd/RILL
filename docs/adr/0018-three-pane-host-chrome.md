# ADR 0018: Three-pane host chrome (one leaf)

- **Status:** Accepted — 2026-08-17
- **Tree:** this repository only
- **Issue:** [#260](https://github.com/mahboobmonnamd/RILL/issues/260)
- **Requires:** [ADR 0009](0009-direct-to-display-echo.md),
  [ADR 0010](0010-spike-0-closes.md),
  [ADR 0011](0011-session-graph.md) (kernel map Proven),
  [ADR 0016](0016-exit-fullscreen-must-not-hang.md)
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
