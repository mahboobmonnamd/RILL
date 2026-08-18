# SPEC-BLOCKS — Blocks as a cold overlay (`lane:host`)

- **Status:** Accepted — 2026-08-18. Gates **Red** until demonstrated
  red-then-green (ADR 0002 D2).
- **Authority:** [ADR 0032](../adr/0032-blocks-are-a-cold-overlay.md)
- **Requires:** [SPEC-CHIP0](SPEC-CHIP0.md), [SPEC-KERNEL](SPEC-KERNEL.md),
  [SPEC-DISPLAY](SPEC-DISPLAY.md), [SPEC-FIDELITY](SPEC-FIDELITY.md),
  [SPEC-TRUST](SPEC-TRUST.md)
- **Milestone:** M6 — Blocks

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. What a Block is

- A Block records: `SessionId`, start and end **offsets into the kernel byte
  ring**, command string, exit status, timing, cwd.
- A Block MUST NOT hold cells, a grid, a `String` of output, or a second parse.
- Rendering replays the byte range through the same Chip 0 implementation the
  resync path uses — cold, on demand.
- When the ring has dropped a range, the Block MUST render as truncated. It MUST
  NOT keep a private copy to survive eviction.

## 2. Boundaries

- Boundaries come from shell-integration marks (OSC 133) where present.
- Correctness MUST NOT depend on marks. With none, output is **background
  Blocks** — untagged regions — and the UI MUST say so.
- RILL MUST NOT infer boundaries by prompt-shape heuristics.
- A pending Block is visible while the command runs, with no exit status. It
  MUST NOT display a fabricated status.
- Pre-command bytes MUST NOT attach to the new Block. The drain boundary is the
  mark, not a timer.

## 3. The warm path

- Blocks are an overlay. Typing, echo and paint travel the path ADR 0009
  measured.
- Block bookkeeping MUST NOT run on the key path, MUST NOT allocate per
  keystroke, and MUST NOT run on the display-link callback.
- `--nfr-key` MUST be demonstrated with Blocks **off** and with Blocks **on**,
  meeting the same p95 budget. If Blocks on cannot meet it, Blocks do not ship.
- T-NFR MUST NOT be modified by this milestone.

## 4. Raw mode

- With Blocks off, keys go straight to the PTY. This is a first-class supported
  mode.
- Entering the alternate screen selects raw presentation of the **same** leaf.
  It MUST NOT spawn, create a second leaf, or reparse.
- Chip 0 owns the mode; Blocks read it (SPEC-FIDELITY §1).
- While mouse reporting is on, Shift selects in the UI — the single rule from
  SPEC-FIDELITY §3.

## 5. Actions

- Copy command / output / both copies from the replayed byte range. Redaction
  applies; the clipboard is a sink.
- Find/filter searches recorded metadata and replayed bytes, cold.
- **Rerun MUST place the command in the input for explicit submission. It MUST
  NOT execute on click.**
- Bookmarks and compact/divider density are view state with no kernel effect.

## 6. Sharing and context

- A cloud permalink is optional and additive; local copy MUST work with sharing
  off and no account.
- Sharing MUST show exactly what will be uploaded, MUST redact before upload,
  and MUST be a per-Block explicit action. Nothing ships in M6.
- Attach-as-context hands a byte range to a task under SPEC-TASK §9.

## 7. Prompt chips

- Chips are optional and default off.
- With chips on, RILL MUST NOT rewrite, suppress, or reposition the shell's own
  PS1 output (NFR-BYTES). A chip is chrome beside the region.

## 8. Rich input overlay

- Submitting into a CLI agent's PTY from the overlay is SPEC-AGENT §4's single
  gated write. It MUST require the capability, respect FR-ONE, and be
  interruptible.
- The overlay MUST NOT bypass a permission profile.

## 9. Refused

- A live PTY/TUI rendered as a Block type. That is a second live surface with
  its own presenter inside a scrolling list — the second VT this tree refuses.
  §4 is the supported answer.

## 10. Gates

| ID | Status | Closes |
|---|---|---|
| T-BLOCK-POD | Red | §1 |
| T-BLOCK-BOUNDARY | Red | §2 |
| T-BLOCK-COLD | Red | §3 |
| T-BLOCK-RAW | Red | §4 |
| T-BLOCK-RERUN | Red | §5 |
| T-BLOCK-CHIP | Red | §7 |

T-BLOCK-POD's mutation MUST also be detectable as memory growth; otherwise the
oracle is not downstream of the mechanism (ADR 0002 D4).

## 11. What we will not do

- Let Blocks own an output copy.
- Give a Block its own VT.
- Infer boundaries by regex.
- Require shell integration for correctness.
- Execute on a rerun click.
- Rewrite PS1.
- Measure NFR-KEY only with Blocks off.
