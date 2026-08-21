# SPEC-CHROME — three-pane host chrome (`lane:host`)

- **Status:** Accepted — 2026-08-17 for the three-pane evidence slice;
  product visibility amended 2026-08-21.
- **Authority:** [ADR 0018](../adr/0018-three-pane-host-chrome.md), amended by
  [ADR 0053](../adr/0053-runtime-domain-content-and-client-authority.md) D2
- **Issue:** [#260](https://github.com/mahboobmonnamd/RILL/issues/260),
  look-file chrome: [#269](https://github.com/mahboobmonnamd/RILL/issues/269),
  inset / type / surface: [#270](https://github.com/mahboobmonnamd/RILL/issues/270)
- **Code:** `host/macos/` (`ChromeHost`, `main.m`, `TerminalView`)
- **Gates:** T-SPLIT **Proven** on the split. T-SPLIT-LOOK, T-CHROME-INSET,
  T-CHROME-FONT demonstrated green on packaged `Rill.app`; CI `gates.yml`
  is the D8 closer.

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. Layout

- The proven M2 slice's default launch (no `--nfr-key`, no
  `RILL_MUTATE=no_chrome`) installs an
  `NSSplitView` as `contentView` with **three** subviews, left to right:
  navigation, Chip 1, inspector.
- Accessibility identifiers MUST be `chrome-split` on the split,
  `chrome-left`, `chrome-center` (the `TerminalView`), `chrome-right`.
- Left and right MUST have non-zero width after layout. Center MUST be the
  remaining width and MUST be `TerminalView`.
- First responder MUST be `TerminalView`.
- Sidebars MUST NOT accept first responder in this slice.

### 1a. Product visibility contract

The fixed three-pane default is historical evidence for chrome layout, not the
final product default. Workspace UI, Session UI, sidebars, inspectors and agent
surfaces are independently hideable per client. With them hidden, the terminal
uses the available content area and the same stable implicit/named domain
objects remain underneath.

Toggling chrome MUST NOT create, delete, migrate, detach, terminate or resize a
domain object except for an explicit user-requested terminal geometry change by
the current lease owner. Re-enabling chrome projects the same IDs. Users may
keep every product-management surface hidden indefinitely.

## 2. Planes

- Center MUST be Chip 1 (`vt-engine` + our Metal). It MUST NOT be an
  `NSTextView` / SwiftUI `Text` dump of the live grid.
- Left and right MUST be AppKit chrome. They MUST NOT create a PTY, own
  scrollback, or receive the master fd.
- Chrome MUST NOT add frames on the warm attach path.

## 3. T-NFR

- `--nfr-key` MUST NOT install the split. `TerminalView` remains
  `contentView`. Enter-fullscreen for measurement is unchanged (ADR 0009).

## 4. Placeholders this slice MAY show

- Left: Workspaces heading and one row whose label is the kernel `Workspace`
  `NodeId` from the cold nav socket (T-NAV-WORKSPACE-PROJECTION). Mutation
  `chrome_invents_workspace_row` restores a host-local directory name.
- Right: inert **Changes** and **Files** rows, plus an **Agents** heading with
  no rows until Task exists (SPEC-NAV §7). Clicks MUST NOT spawn, kill,
  or detach.

## 4a. Look

- Left and right MUST paint a **chrome surface**: look `background` with each
  8-bit RGB channel saturating-minus 9. Center Chip 1 keeps the file
  `background`. Labels use look `foreground`.
- They MUST NOT use a hardcoded gray independent of the theme file, and MUST
  NOT match Chip 1's file background (T-SPLIT-LOOK).
- Latte and Mocha MUST both paint; a cream constant that matches only one
  file is not this gate.
- `background-opacity` MUST NOT make chrome or the window translucent.

## 4b. Inset and type

- Section labels MUST sit `padding-y` points from the top of their pane
  (same look value Chip 1 uses as `window-padding-y` / host-surface
  `padding-y`). Layout MUST use the live pane bounds. A 680pt template is
  not this gate (T-CHROME-INSET).
- Section labels MUST use `NSFont.systemFontSize` (13pt control size), not
  11pt caption size (T-CHROME-FONT). Terminal glyphs stay the look
  `font-family` / `font-size`.

## 5. Out of scope

Command Blocks, agents, conversations, product visibility wiring,
Workspace/Session **journal** persist, cwd-leave ([#261](https://github.com/mahboobmonnamd/RILL/issues/261)),
a theme store, `NSVisualEffectView` over Metal. In-memory New Tab is
[ADR 0056](../adr/0056-vertical-slices-backend-and-host.md) / SPEC-NAV, not this
layout spec.

## 6. What we will not do

- Observe the PTY buffer from SwiftUI.
- Grow a `TerminalGrid` or a second VT for cards.
- JSON, cells, or a `CWD` tag on the typing socket.
- Hide an NFR miss with chrome.
