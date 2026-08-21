# SPEC-MOUSE — selection, hyperlinks, copy mode, key navigation (`lane:host`)

- **Status:** Accepted — 2026-08-18. Gates **Red** until demonstrated
  red-then-green (ADR 0002 D2).
- **Authority:** [ADR 0052](../adr/0052-selection-links-and-raw-mode-arbitration.md),
  amended by
  [ADR 0053](../adr/0053-runtime-domain-content-and-client-authority.md) D9
- **Requires:** [SPEC-FIDELITY](SPEC-FIDELITY.md), [SPEC-BLOCKS](SPEC-BLOCKS.md),
  [SPEC-SURFACES](SPEC-SURFACES.md) §10, [SPEC-TRUST](SPEC-TRUST.md),
  [SPEC-CWD](SPEC-CWD.md), [SPEC-CONTENT](SPEC-CONTENT.md),
  [SPEC-COMPOSITOR](SPEC-COMPOSITOR.md)
- **Milestone:** after content/compositor foundations; historical M6 numbering
  does not override ADR 0053 D12.

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. Selection

- Live terminal selection resolves against terminal grid position plus
  TerminalExecution/checkpoint identity. Structured-content and editor
  selection use ContentItem/grapheme and document anchors respectively.
- It MUST NOT build a per-cell `String` mirror, MUST NOT run on the display-link
  callback, and MUST NOT modify the grid.
- Copy-on-select MUST be opt-in, default off, and MUST fire on release only —
  never during a drag.
- Smart select expands within the owning surface. Rectangular terminal select
  is a selection mode, not a second buffer. Cross-surface copy orders derived
  fragments without mutating sources.
- The clipboard is a derived sink; redaction policy applies but does not claim
  complete secret removal or authorize capture.

## 2. Copy mode

- vi/tmux motions over policy-retained primary content MUST work **while the
  process stays live**.
- Entering copy mode MUST NOT stop the PTY reader. The child keeps running and
  the ring keeps filling (PRD NFR-DROP).
- Leaving returns to the live tail. Missing/deleted history is shown as a
  discontinuity, not reconstructed from unrelated ring contents.

## 3. Opening paths and links

- ⌘-click opens a path, URL, or OSC 8 hyperlink. Plain click MUST NOT.
- Screen content is untrusted. Therefore:
  - A path MUST be resolved against the pane's cwd (cold tap) and MUST be
    verified to exist before an editor is invoked.
  - The editor MUST be invoked as an **argv vector** through SPEC-SURFACES §10's
    resolver. RILL MUST NOT build a shell string from screen text.
  - `file:line:col` parses to structured fields; line and column MUST be
    validated as integers, never passed through as text.
  - URL schemes MUST be on an allowlist (`http`, `https`, plus user additions).
    A RILL-handled scheme gets the deep-link confirmation (SPEC-NAV §9).
    `file:` URLs from screen content MUST NOT auto-open.
  - An OSC 8 hyperlink whose visible text differs from its target MUST show the
    **target** before opening.

## 4. Mouse arbitration

- While mouse reporting is on, events go to the child; Shift reclaims for the
  UI. This is SPEC-FIDELITY §3's single rule and is not re-decided here.
- Right-click follows the same arbitration: plain right-click reaches the child,
  Shift+right-click opens the menu.
- Menu items are the same actions available elsewhere, each with its own
  confirmation. A menu MUST NOT expose an otherwise-gated action.

## 5. Highlighting

- Syntax highlight and error underline are visual only. They MUST NOT alter
  bytes sent to the PTY and MUST NOT alter the grid.
- "Unknown binary" MUST be determined by a cached, cold `PATH` lookup. It MUST
  NOT execute the candidate and MUST NOT block a keystroke on a stat storm.
- When the check is unavailable or times out, nothing is underlined.

## 6. Keyboard navigation

- ⌘⇧[ / ⌘⇧], Ctrl+Tab / Ctrl+⇧Tab and tab-strip arrows navigate the container
  tree (SPEC-NAV §1).
- Ctrl+Tab MUST be resolvable through config, and SPEC-CONFIG §5's load-time
  report MUST name a binding that keeps it from a child that wants it.
- Tab-strip arrow keys apply only when the strip has focus. They MUST NOT be
  intercepted while the terminal is first responder.
- Every navigation action MUST be reachable without a mouse (SPEC-TRUST §8) and
  MUST NOT write to any leaf.

## 7. Gates

| ID | Status | Closes |
|---|---|---|
| T-SEL-LIVE | Red | §2 |
| T-SEL-POD | Red | §1 |
| T-SEL-OPEN | Red | §3 |
| T-SEL-LINK | Red | §3 |
| T-SEL-HIGHLIGHT | Red | §5 |
| T-SEL-KEYNAV | Red | §6 |

NFR-KEY MUST hold with selection and highlighting active (SPEC-BLOCKS §3).

## 8. What we will not do

- Open paths or URLs on a plain click.
- Build an editor command as a shell string.
- Trust OSC 8 display text over its target.
- Execute a candidate to decide whether to underline it.
- Pause the PTY reader during copy mode.
- Mirror the grid into per-cell strings.
