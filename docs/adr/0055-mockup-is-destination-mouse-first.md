# ADR 0055: Mockup is the destination; first slice is raw mouse, not Blocks chrome

- **Status:** Accepted — 2026-08-21. **Amended by**
  [ADR 0056](0056-vertical-slices-backend-and-host.md).
- **Tree:** this repository only
- **Issue:** first slice [#344](https://github.com/mahboobmonnamd/RILL/issues/344)
  (T-MOUSE-SGR). Tracker for later mock surfaces remains
  [#338](https://github.com/mahboobmonnamd/RILL/issues/338).
- **Requires:** [ADR 0036](0036-chip1-mode-state-channel.md),
  [ADR 0018](0018-three-pane-host-chrome.md),
  [ADR 0050](0050-blocks-are-a-cold-overlay.md),
  [ADR 0052](0052-selection-links-and-raw-mode-arbitration.md),
  [ADR 0053](0053-runtime-domain-content-and-client-authority.md) D12, D16, D20, D22
- **Amends:** ADR 0036 — the host **may** encode pointer bytes from Chip 1
  `mode_state` (that ADR's original slice tracked modes only).
  ADR 0018 — tabs, nested PTY splits and extra windows stay **later**; this ADR
  does not install them. The mockup does not override D12.
  **Amended by** [ADR 0056](0056-vertical-slices-backend-and-host.md): the next
  vertical slice is in-memory New Tab (#345), not a chrome dump and not a wait
  for D12 persist.
- **Does not authorize:** Flow compositor, ContentTimeline, Command Blocks UI,
  selection/copy, OSC 8 click, Attention/Activity/file-tree chrome, JSON on the
  warm path, a second VT. `create_node` from chrome is ADR 0056 / #345.

## Context

The product mock shows Blocks, mouse, tabs, panes, workspaces and inspectors
together. That is concept evidence (ADR 0053 D16), not a license to paint those
surfaces onto the one-leaf Chip 1 window in one change.

ADR 0053 D12 is binding: terminal/PTY compatibility first; semantic transcript
before Flow Blocks; persistent Tab/pane topology fifth. SPEC-COMPOSITOR stays
Red until ContentTimeline (D12 steps 3–4). Shipping fake Block cards or a tab
bar that does not attach a leaf would be a lie (ADR 0011 D5).

Chip 1 already records mouse modes (T-CHIP1-MODE). The host `mouseDown` still
only focuses the view. TUIs and mouse-aware programs therefore look dead next
to the mock, even though Blocks are not the next legal layer.

## Decision

### D1 — The mockup is ordered work, not a chrome dump

Map mock surfaces to existing authority. Do not invent a second architecture.

| Mock surface | Authority | When it may land |
|---|---|---|
| Raw PTY + Metal grid | ADR 0009, 0054 | Shipped |
| Pointer → child when mouse mode on | this ADR, SPEC-FIDELITY §3, SPEC-HOST-POINTER | First slice (#344) |
| Shift reclaims pointer for UI | ADR 0052 D3 | With selection, not before |
| Selection / copy / ⌘-click paths | ADR 0052 | After pointer reports exist |
| Command Blocks / Flow cards | ADR 0050, SPEC-COMPOSITOR | After ContentTimeline (D12 3–4) |
| Tabs / splits / extra windows | ADR 0018, SPEC-NAV, ADR 0056 | Vertical slice #345 (in-memory kernel + host). Journal persist still D12 step 5 |
| Attention / agents / file tree | ADR 0046–0048, DEFERRED | After Task / inventories |

### D2 — First slice is the host pointer encoder

The host reads Chip 1 `TerminalModeState` after `feed` (already on `Client`)
and encodes ordinary attach `DATA` toward the PTY.

- Reporting is on when any of `mouse_x10`, `mouse_button`, `mouse_any`,
  `mouse_sgr` is true.
- Encoding precedence: SGR (`CSI ? 1006`) if `mouse_sgr`; otherwise X10-style
  `CSI M` when any other mouse flag is set (SPEC-VT-MODE §2).
- Press, release and wheel are in scope. Motion (`mouse_any` / `mouse_button`
  drag) MAY follow in the same encoder without a new ADR.
- Shift held: do **not** encode; do not host-scroll-steal as a heuristic.
  Selection itself remains ADR 0052 (not this slice).
- When reporting is off, wheel keeps host history scroll (existing T-SCROLL).
- When reporting is on, wheel MUST go to the child, not the host history
  (otherwise `less`/`vim` mouse is a lie).
- `--nfr-key` MUST NOT grow a second mouse path. Pointer bytes are warm `DATA`
  like keys.

Mutation `skip_mouse_encode` MUST turn T-MOUSE-SGR red.

### D3 — What this ADR still forbids

Implementing the rest of the mock in the same PR. Growing `TerminalGrid`.
Dumping the live grid into `Text`. GUI `posix_spawn` of the user shell.
`SCM_RIGHTS` of the master. JSON mouse RPC.

## Consequences

- SPEC-HOST-POINTER is the host contract. SPEC-VT-MODE §5's "do not wire the
  encoder in this slice" is superseded for pointer encoding only.
- Tabs/Blocks remain in DEFERRED until their D12 step and named tests exist.
- Users will still lack Block cards and a tab bar after #344; they will be able
  to click in programs that requested mouse tracking.

## Rejected alternatives

- **Paint mock chrome now (tree, Attention, Block spines).** Rejected: D16
  projections without ContentTimeline/Task are invented UI.
- **New Tab that `posix_spawn`s a second shell in the GUI.** Rejected: ADR 0001.
- **Wait for Blocks before any mouse.** Rejected: mouse reporting is PTY
  fidelity (D12 step 1), not a Flow overlay.
