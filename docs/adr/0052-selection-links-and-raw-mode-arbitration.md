# ADR 0052: Selection, hyperlinks and raw-mode arbitration

- **Status:** Accepted — 2026-08-18
- **Amended by:** [ADR 0053](0053-runtime-domain-content-and-client-authority.md)
  D9. Selection uses surface-specific anchors across terminal and structured
  content while raw-mode arbitration remains explicit.
- **Historical identifier:** merged as ADR 0034 in PR #278; renumbered to ADR
  0052 on 2026-08-21 with its series. Renumbering changed no decision.
- **Tree:** this repository only
- **Issue:** catalog epic [#33](https://github.com/mahboobmonnamd/RILL/issues/33).
  Rows authorized by this ADR:
  F-050 [#85](https://github.com/mahboobmonnamd/RILL/issues/85), F-053
  [#88](https://github.com/mahboobmonnamd/RILL/issues/88), F-054
  [#89](https://github.com/mahboobmonnamd/RILL/issues/89), F-055
  [#90](https://github.com/mahboobmonnamd/RILL/issues/90), F-056
  [#91](https://github.com/mahboobmonnamd/RILL/issues/91), F-057
  [#92](https://github.com/mahboobmonnamd/RILL/issues/92), F-086
  [#121](https://github.com/mahboobmonnamd/RILL/issues/121), F-087
  [#122](https://github.com/mahboobmonnamd/RILL/issues/122), F-256
  [#128](https://github.com/mahboobmonnamd/RILL/issues/128).
- **Requires:** [ADR 0009](0009-direct-to-display-echo.md),
  [ADR 0040](0040-terminal-fidelity-is-chip0.md) D1/D3 (Chip 0 owns parsing and
  arbitration), [ADR 0046](0046-development-surfaces-are-panes.md) D4 (editor
  resolver, argv), [ADR 0050](0050-blocks-are-a-cold-overlay.md),
  [ADR 0051](0051-input-editor-history-and-completion.md)
- **Amends:** nothing.
- **Does not authorize:** a host-side escape parser, a second VT, modifying the
  grid, opening a URL without confirmation, a shell-string editor invocation,
  Chip 1 live.
- **Milestone:** M6 — Blocks

## Context

Nine rows cover the output region's pointer and keyboard surface: mouse-first
chrome (F-050), copy on select (F-053), smart/rectangular select (F-054),
clickable files and links (F-055), path with line/column (F-056), syntax
highlight and error underline (F-057), right-click menus (F-086), keyboard copy
mode (F-087), keyboard tab and pane cycle (F-256).

ADR 0040 D3 already set the one arbitration rule (Shift reclaims while mouse
reporting is on). This ADR is about what the pointer and the keyboard may do
with what is on screen — and the recurring hazard is that **screen content is
untrusted**. A path, a URL, an OSC 8 target: all of it was written by a process,
possibly remote, possibly hostile. Clicking must never be a shortcut to
executing what a byte stream asked for.

## Decision

### D1 — Selection reads the POD buffer, cold, and never mutates the grid

F-050, F-053, F-054, F-087.

Selection resolves against Chip 0's flat POD buffer (FR-CHIP0). It MUST NOT
build a per-cell `String` mirror, MUST NOT run on the display-link callback, and
MUST NOT modify the grid.

- **Copy on select (F-053)** is opt-in, default off, and MUST NOT fire during a
  drag in progress — only on release.
- **Smart select (F-054)** double-click expands to a word, path, or URL by
  reading the buffer. Rectangular/column select is a selection mode, not a
  second buffer.
- **Keyboard copy mode (F-087)** gives vi/tmux motions over scrollback **while
  the process stays live**. Entering it MUST NOT stop the PTY reader — the child
  keeps running and the ring keeps filling (NFR-DROP). Leaving returns to the
  live tail.

The clipboard is a sink: redaction (ADR 0044 D4) applies to copy.

Mutation `copy_mode_stops_reader` MUST turn T-SEL-LIVE red.

### D2 — Clicking a path or URL requires an explicit modifier and a resolved target

F-055, F-056. ⌘-click on a path, URL, or OSC 8 hyperlink opens it. Plain click
MUST NOT.

The target is **untrusted** (ADR 0044 D1). Therefore:

- A path MUST be resolved against the pane's cwd (ADR 0013's cold tap) and MUST
  be verified to exist before an editor is invoked.
- The editor is invoked as an **argv vector** through ADR 0046 D4's resolver.
  RILL MUST NOT build a shell string from screen text — a filename is attacker
  input, and this is the injection path.
- `file:line:col` (F-056) parses to structured fields; the line and column MUST
  be validated as integers, never passed through as text.
- A URL scheme MUST be on an allowlist (`http`, `https`, plus schemes the user
  added). A scheme RILL itself handles (deep links, ADR 0039 D5) gets that
  ADR's confirmation. `file:` URLs from screen content MUST NOT auto-open.
- An OSC 8 hyperlink whose visible text differs from its target MUST show the
  **target** before opening. Displayed text is not the destination, and a
  terminal that hides that is a phishing surface.

Mutation `open_path_via_shell_string` MUST turn T-SEL-OPEN red; mutation
`osc8_opens_without_target_shown` MUST turn T-SEL-LINK red.

### D3 — Mouse arbitration is stated once and is not re-decided here

While an app has mouse reporting on, events go to the child; Shift reclaims for
the UI (ADR 0040 D3, F-084). This ADR adds no second rule and no heuristic.

Right-click menus (F-086) follow the same arbitration: with reporting on, a
plain right-click reaches the child and Shift+right-click opens the menu. Menu
items are the same actions available elsewhere, each with its own confirmation.
A menu MUST NOT expose an action that is otherwise gated.

### D4 — Highlighting is presentation only, and underlining is not a claim of truth

F-057. Syntax highlight in the input field, and error underlining of unknown
binaries, are visual only. They MUST NOT alter bytes sent to the PTY
(ADR 0051 D2) and MUST NOT alter the grid (D1).

"Unknown binary" MUST be determined from `PATH` lookup and cached, cold. It MUST
NOT execute the binary to find out, and MUST NOT block a keystroke on a
filesystem stat storm. When the check is unavailable or times out, nothing is
underlined — a false "this does not exist" trains users to ignore the signal.

Mutation `underline_execs_candidate` MUST turn T-SEL-HIGHLIGHT red.

### D5 — Keyboard navigation is complete and does not steal from the child

F-256. ⌘⇧[ / ⌘⇧], Ctrl+Tab / Ctrl+⇧Tab, and arrow keys on the tab strip navigate
tabs and panes (ADR 0038's tree).

Ctrl+Tab is the collision to respect: it MUST be resolvable through config
(ADR 0043 D5) and its load-time report MUST name it if a user binding would keep
it from a child that wants it. Arrow keys on the tab strip apply only when the
strip has focus — they MUST NOT be intercepted while the terminal is first
responder.

Every navigation action here MUST also be reachable without a mouse
(ADR 0044 D8), and MUST NOT write to any leaf (ADR 0039 D2).

### D6 — Oracle

| ID | Closes |
|---|---|
| T-SEL-LIVE | D1 — copy mode keeps the child running and the ring filling |
| T-SEL-POD | D1 — no per-cell `String` mirror; grid unmodified |
| T-SEL-OPEN | D2 — argv invocation; path verified; no shell string |
| T-SEL-LINK | D2 — OSC 8 target shown; scheme allowlist; no auto `file:` |
| T-SEL-HIGHLIGHT | D4 — no exec to classify; silent when unavailable |
| T-SEL-KEYNAV | D5 — keyboard-complete; no interception while terminal focused |

NFR-KEY MUST hold with selection and highlighting active (ADR 0050 D3).

## Consequences

- [SPEC-MOUSE](../spec/SPEC-MOUSE.md) is the pointer, link and copy-mode
  contract.
- ADR 0046 D4's editor resolver is shared, not duplicated.
- ADR 0040 D3 remains the single arbitration statement.

## Rejected alternatives

- **Plain click opens paths and URLs.** Rejected: D2. Every accidental click
  becomes an action on attacker-chosen text.
- **Build the editor command as a shell string.** Rejected: D2, ADR 0046 D4.
- **Trust OSC 8 display text.** Rejected: D2, phishing.
- **Execute a candidate to decide whether to underline it.** Rejected: D4.
- **Pause the PTY reader during copy mode to freeze scrollback.** Rejected: D1.
  Stalling the child to make scrolling easier breaks NFR-DROP.
- **A per-cell string mirror to make selection simpler.** Rejected: D1,
  AGENTS.md §5.
