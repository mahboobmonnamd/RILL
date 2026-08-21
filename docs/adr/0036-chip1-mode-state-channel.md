# ADR 0036: Chip 1 mode state reaches the host without a new attach frame

- **Status:** Accepted — 2026-08-20
- **Tree:** this repository only
- **Issue:** T-CHIP1-MODE (M7 precondition 6; tracker slice — not live swap)
- **Requires:** [ADR 0012](0012-chip1-isolated-vt.md) (Chip 1 isolated until
  M7), [ADR 0022](0022-chip1-reply-channel.md) D1/D4 (inherent drain, ordinary
  `DATA`), [ADR 0003](0003-display-pipeline.md) D9 (no control RPC on the
  warm path), [ADR 0035](0035-chip1-character-width.md) (T-CHIP1-WIDTH Proven)
- **Amends:** [SPEC-CHIP1](../spec/SPEC-CHIP1.md) §2 — adds inherent
  `mode_state`. [M4-PLAN](../M4-PLAN.md) precondition 6.
- **Does not authorize:** Chip 1 as the live chip, a `rill-host` / `rilld`
  dependency on `vt-engine`, a new attach frame tag, JSON on the warm path,
  cells over IPC, a second VT in the host, host key/mouse encoder changes,
  sixel / images, Ghostty exec, [#24](https://github.com/mahboobmonnamd/RILL/issues/24)

## Context

[M4-PLAN](../M4-PLAN.md) M7 precondition 6: Chip 1 will know terminal **mode
state**; the host encodes keys and mouse. Without a channel, application
cursor keys, keypad, paste bracketing, and mouse tracking cannot be produced
correctly after a live swap.

Chip 0 already owns this for the live window: the host MUST NOT keep its own
copy of mouse mode or keyboard-protocol flags; it queries Chip 0
([SPEC-FIDELITY](../spec/SPEC-FIDELITY.md) §1, [ADR 0040](0040-terminal-fidelity-is-chip0.md)
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

## Decision

### D1 — Chip 1 tracks; the host encodes

Parser consumption of the sequences above updates Chip 1 state. The host
reads that state and produces key/mouse bytes. Chip 1 MUST NOT write a
socket or a PTY. The host MUST NOT parse the PTY byte stream for these
modes (that would be a second VT).

### D2 — `mode_state()` polled after `feed` (Option A)

An inherent method on `VtEngine` (not `TerminalEmulation`), same shape as
`take_replies` / `color_at`: the host calls it after `feed` and encodes from
the returned `TerminalModeState`. No new attach tag. No JSON.

**Rejected: bytes on `DATA`.** Scanning `DATA` is a second parser in the
host. Emitting a side channel on `DATA` either injects non-PTY bytes toward
the child or breaks T-NFR's received-frame set.

### D3 — Named modes tracked in v0 of this slice

| Field | Sequence |
|---|---|
| `application_cursor_keys` | `CSI ? 1 h/l` (DECCKM) |
| `application_keypad` | `ESC =` / `ESC >` (DECKPAM / DECKPNM) |
| `bracketed_paste` | `CSI ? 2004 h/l` |
| `mouse_x10` | `CSI ? 1000 h/l` |
| `mouse_button` | `CSI ? 1002 h/l` |
| `mouse_any` | `CSI ? 1003 h/l` |
| `mouse_sgr` | `CSI ? 1006 h/l` |
| `focus_events` | `CSI ? 1004 h/l` |
| `alternate_screen` | reflects `?1047` / `?1049` state |
| `cursor_visible` | reflects `?25` (DECTCEM) |

Normative detail: [SPEC-VT-MODE](../spec/SPEC-VT-MODE.md). Gate:
**T-CHIP1-MODE**.

### D4 — This ADR does not authorize `#24` or host wiring

The tracker and `mode_state()` may land on `main` in the isolated crate.
The live-swap ADR MUST cite T-CHIP1-MODE as Proven before the host stops
querying Chip 0 for these flags.

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
  Proposed until Accepted; the tracker slice follows ADR → spec → test → impl.
