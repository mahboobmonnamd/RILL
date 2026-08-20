# ADR 0036: Chip 1 mode state reaches the host without a new attach frame

- **Status:** Proposed — 2026-08-19. **Does not authorize implementation.**
- **Tree:** this repository only
- **Issue:** not claimed. This ADR does **not** authorize
  [M7 live swap #24](https://github.com/mahboobmonnamd/RILL/issues/24).
- **Requires:** [ADR 0012](0012-chip1-isolated-vt.md) (Chip 1 isolated until
  M7), [ADR 0022](0022-chip1-reply-channel.md) D1/D4 (inherent drain, ordinary
  `DATA`), [ADR 0003](0003-display-pipeline.md) D9 (no control RPC on the
  warm path)
- **Amends:** nothing until Accepted.
- **Does not authorize:** Chip 1 as the live chip, a `rill-host` / `rilld`
  dependency on `vt-engine`, a new attach frame tag, JSON on the warm path,
  cells over IPC, a second VT in the host, sixel / images, Ghostty exec,
  `#24`

## Context

[M4-PLAN](../M4-PLAN.md) M7 precondition 6: Chip 1 will know terminal **mode
state**; the host encodes keys and mouse. Without a channel, application
cursor keys, keypad, paste bracketing, and mouse tracking cannot be produced
correctly after a live swap.

Chip 0 already owns this for the live window: the host MUST NOT keep its own
copy of mouse mode or keyboard-protocol flags; it queries Chip 0
([SPEC-FIDELITY](../spec/SPEC-FIDELITY.md) §1, [ADR 0022](0022-terminal-fidelity-is-chip0.md)
D3). Chip 1 has no equivalent channel yet. `take_replies` (ADR 0022) is the
wrong pipe: those bytes are owed to the **child**. Mode state is owed to the
**host encoder**.

Named modes this channel MUST cover when an Accepted follow-up implements it:

| Mode | Sequence | Why the host needs it |
|---|---|---|
| DECCKM | `CSI ? 1 h/l` | Application vs normal cursor keys |
| DECKPAM / DECKPNM | `ESC =` / `ESC >` | Application vs numeric keypad |
| Bracketed paste | `CSI ? 2004 h/l` | Wrap paste in `200~` / `201~` |
| Mouse X10 / button / any / SGR | `CSI ? 1000/1002/1003/1006 h/l` | Whether and how to encode pointer events |
| Focus events | `CSI ? 1004 h/l` | Encode focus in / focus out |

The host encodes; Chip 1 tracks. Transport is **not** a new attach frame tag.
JSON is forbidden on the warm path. Encoded key and mouse **payloads** still
travel as ordinary `DATA` toward the PTY (same as today).

## Decision (proposed — not a license to land)

### D1 — Chip 1 tracks; the host encodes

Parser consumption of the sequences above updates Chip 1 state. The host
reads that state and produces key/mouse bytes. Chip 1 MUST NOT write a
socket or a PTY. The host MUST NOT parse the PTY byte stream for these
modes (that would be a second VT).

### D2 — Two options for how the host learns the flags

**Option A — inherent Chip 1 mode snapshot, polled after `feed`.** An
inherent method on `VtEngine` (not `TerminalEmulation`), same shape as
`take_replies` / `color_at`: the host calls it after `feed` and encodes
from the returned flags. No new attach tag. No JSON. The snapshot is a
fixed POD of booleans/enums, not a cell dump.

**Option B — bytes on `DATA`.** Chip 1 would emit mode-change as extra
bytes on the attach `DATA` stream (or the host would infer modes by
scanning `DATA`). Scanning `DATA` is a second parser in the host. Emitting
a side channel on `DATA` either injects non-PTY bytes toward the child or
breaks T-NFR's received-frame set (`DATA` only, and those `DATA` are PTY
bytes).

**Recommend A.** It matches ADR 0022 D1 (inherent, polled, no fd) and
SPEC-FIDELITY's "query the chip" rule. Encoded keys and mouse still go out
as ordinary `DATA`. Option B either splits VT across the host or overloads
the PTY stream.

### D3 — This ADR does not authorize `#24` or any code

Accepting this ADR (later) still does not swap the live chip. The live-swap
ADR MUST cite T-CHIP1-WIDTH as Proven and MUST name this channel. Proposed
docs do not authorize implementation.

## Consequences (when Accepted)

- M7 precondition 6 has a named shape: poll Chip 1, encode in the host,
  `DATA` to the PTY.
- Chip 0 stays live until that swap ADR.

## Rejected alternatives

- **New attach frame tag (`MODE`, `MOUSE`, …).** Rejected: T-NFR
  received-frame set is `DATA` only (ADR 0003 D9, SPEC-ATTACH). Same
  rejection as a `CWD` tag (ADR 0013 D4).
- **JSON / control RPC on the warm path.** Rejected: AGENTS.md §5.
- **Host-side escape parser.** Rejected: second VT (ADR 0012, SPEC-FIDELITY §1).
- **Putting mode bits on `PodGrid` / growing `PodCell`.** Rejected: the grid
  is paint; mode state is input encoding. `PodCell` stays 16 bytes (ADR 0035
  D5).
- **Implementing the tracker in this PR.** Rejected: this document is
  Proposed.
