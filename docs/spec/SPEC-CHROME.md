# SPEC-CHROME — three-pane host chrome (`lane:host`)

- **Status:** Accepted — 2026-08-17
- **Authority:** [ADR 0018](../adr/0018-three-pane-host-chrome.md)
- **Issue:** [#260](https://github.com/mahboobmonnamd/RILL/issues/260)
- **Code:** `host/macos/` (`ChromeHost`, `main.m`, `TerminalView`)
- **Gates:** T-SPLIT (Red until demonstrated)

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. Layout

- Default launch (no `--nfr-key`, no `RILL_MUTATE=no_chrome`) MUST install an
  `NSSplitView` as `contentView` with **three** subviews, left to right:
  navigation, Chip 0, inspector.
- Accessibility identifiers MUST be `chrome-split` on the split,
  `chrome-left`, `chrome-center` (the `TerminalView`), `chrome-right`.
- Left and right MUST have non-zero width after layout. Center MUST be the
  remaining width and MUST be `TerminalView`.
- First responder MUST be `TerminalView`.
- Sidebars MUST NOT accept first responder in this slice.

## 2. Planes

- Center MUST be Chip 0 (`libghostty-vt` + our Metal). It MUST NOT be an
  `NSTextView` / SwiftUI `Text` dump of the live grid.
- Left and right MUST be AppKit chrome. They MUST NOT create a PTY, own
  scrollback, or receive the master fd.
- Chrome MUST NOT add frames on the warm attach path.

## 3. T-NFR

- `--nfr-key` MUST NOT install the split. `TerminalView` remains
  `contentView`. Enter-fullscreen for measurement is unchanged (ADR 0009).

## 4. Placeholders this slice MAY show

- Left: a **Workspaces** heading and one row named from the home directory's
  last path component. Not persisted. Not a second session.
- Right: inert **Changes** and **Files** rows. Clicks MUST NOT spawn, kill,
  or detach.

## 5. Out of scope

Tabs, nested PTY splits, agents, conversations, sidebar hide, workspace
persist, cwd-leave ([#261](https://github.com/mahboobmonnamd/RILL/issues/261)),
Blocks, Chip 1 live, a theme store, `NSVisualEffectView` over Metal.

## 6. What we will not do

- Observe the PTY buffer from SwiftUI.
- Grow a `TerminalGrid` or a second VT for cards.
- JSON, cells, or a `CWD` tag on the typing socket.
- Hide an NFR miss with chrome.
