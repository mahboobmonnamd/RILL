# ADR 0050: Blocks are a cold overlay; Chip 0 stays the warm path

- **Status:** Accepted — 2026-08-18
- **Superseded in part by:** [ADR 0053](0053-runtime-domain-content-and-client-authority.md)
  D7–D8. A byte range is source evidence, not durable content identity, and
  replay is recovery rather than normal presentation.
- **Historical identifier:** merged as ADR 0032 in PR #278; renumbered to ADR
  0050 on 2026-08-21 with its series. Renumbering changed no decision.
- **Tree:** this repository only
- **Issue:** catalog epic [#33](https://github.com/mahboobmonnamd/RILL/issues/33).
  Rows authorized by this ADR:
  F-069 [#104](https://github.com/mahboobmonnamd/RILL/issues/104), F-070
  [#105](https://github.com/mahboobmonnamd/RILL/issues/105), F-071
  [#106](https://github.com/mahboobmonnamd/RILL/issues/106), F-072
  [#107](https://github.com/mahboobmonnamd/RILL/issues/107), F-073
  [#108](https://github.com/mahboobmonnamd/RILL/issues/108), F-074
  [#109](https://github.com/mahboobmonnamd/RILL/issues/109), F-075
  [#110](https://github.com/mahboobmonnamd/RILL/issues/110), F-076
  [#111](https://github.com/mahboobmonnamd/RILL/issues/111), F-077
  [#112](https://github.com/mahboobmonnamd/RILL/issues/112), F-078
  [#113](https://github.com/mahboobmonnamd/RILL/issues/113), F-079
  [#114](https://github.com/mahboobmonnamd/RILL/issues/114), F-080
  [#115](https://github.com/mahboobmonnamd/RILL/issues/115), F-081
  [#116](https://github.com/mahboobmonnamd/RILL/issues/116), F-082
  [#117](https://github.com/mahboobmonnamd/RILL/issues/117), F-083
  [#118](https://github.com/mahboobmonnamd/RILL/issues/118), F-084
  [#119](https://github.com/mahboobmonnamd/RILL/issues/119), F-085
  [#120](https://github.com/mahboobmonnamd/RILL/issues/120), F-089
  [#124](https://github.com/mahboobmonnamd/RILL/issues/124).
- **Requires:** [ADR 0001](0001-session-operating-system.md),
  [ADR 0003](0003-display-pipeline.md),
  [ADR 0009](0009-direct-to-display-echo.md) (T-NFR closer),
  [ADR 0012](0012-chip1-isolated-vt.md) (Chip 1 isolated until M7),
  [ADR 0040](0040-terminal-fidelity-is-chip0.md) D4 (no shell-integration
  dependency for correctness),
  [ADR 0044](0044-trust-secrets-and-automation-boundary.md) D4 (redaction)
- **Amends:** nothing.
- **Does not authorize:** Chip 1 as the live chip, a second VT, dumping the live
  grid into `Text`, per-cell `String` snapshots, JSON on the warm path, an
  account, a cloud service, Blocks on the `--nfr-key` path.
- **Milestone:** M6 — Blocks

## Context

Eighteen rows: classic/raw input (F-069), Command Blocks (F-070), copy actions
(F-071), pending Block (F-072), prompt drain (F-073), background Blocks (F-074),
sticky header (F-075), find/filter (F-076), rerun (F-077), bookmarks (F-078),
attach as context (F-079), share permalink (F-080), compact/dividers (F-081),
live terminal Block (F-082), raw mode same PTY (F-083), Shift+mouse (F-084),
rich input overlay (F-085), prompt chips (F-089).

This is the feature the whole repository was built to be able to refuse
correctly. PRD §2 names the prototype that died here: cells copied to `String`,
the emulator behind JSON, SwiftUI observing the PTY buffer. AGENTS.md §5 forbids
the shape. ADR 0011 D6 and SPEC-GRAPH §7 both say: do not dump the live grid
into Blocks.

Blocks are still worth having. The resolution is that a Block is **metadata
about a region of the byte stream**, not a copy of it.

## Decision

### 2026-08-21 amendment — D1, D4–D7 and D9 are read through ADR 0053

The historical decisions below remain visible so their evidence is not
silently rewritten. ADR 0053 D7 and D16–D21 supersede the byte-range storage
model: normal primary-screen shell activity defaults to Flow, a compact Block
projection over materialized authoritative transcript/runtime events. Exact
Raw remains user-selectable and is the independent fallback; alternate-screen
and raw-mode applications automatically use the one live terminal grid. Flow,
Raw and TUI reuse the same pane, execution, PTY and canonical terminal state.

Actions operate on stable content/event IDs plus source evidence. They do not
require replaying an arbitrary byte slice through a fresh VT. Styling is a
client decision, and failure of transcript/Block processing cannot block raw
terminal input, output or paint.

### D1 — A Block is a byte range in the ring, plus metadata

A Block records: a `SessionId`, a start and end **offset into the kernel's byte
ring** (FR-HISTORY), a command string, exit status, timing, and cwd.

A Block MUST NOT hold cells, a grid, a `String` of the output, or a second
parse. Rendering a Block replays its byte range through the same Chip 0
implementation the resync path already uses (FR-RESYNC) — cold, on demand.

When the ring has dropped a range (ADR 0040 D6), the Block MUST render as
truncated. It MUST NOT keep a private copy to survive ring eviction; that
private copy is the per-cell snapshot this tree forbids.

Mutation `block_holds_cell_snapshot` MUST turn T-BLOCK-POD red, and MUST also be
detectable as a memory-growth regression — if it is not, the oracle is not
downstream of the mechanism (ADR 0002 D4).

### D2 — Block boundaries come from marks, and degrade honestly

F-070, F-072, F-073, F-074.

Boundaries are set by shell-integration marks (OSC 133) where present. Per
ADR 0040 D4, correctness MUST NOT depend on them: with no marks, output is
**background Blocks** (F-074) — untagged regions — and the UI says so.

RILL MUST NOT infer a boundary by prompt-shape heuristics. A mis-split Block
attributes one command's output to another, which is worse than no Block.

- **Pending Block (F-072):** visible while the command runs, with no exit
  status. It MUST NOT display a fabricated status.
- **Prompt drain (F-073):** bytes emitted before the command starts MUST NOT be
  attached to the new Block. The drain boundary is the mark, not a timer.

Mutation `infer_boundary_by_prompt_regex` MUST turn T-BLOCK-BOUNDARY red.

### D3 — The warm path is unchanged, and `--nfr-key` proves it

Blocks are an **overlay**. Typing, echo and paint travel exactly the path
ADR 0009 measured. Block bookkeeping MUST NOT run on the key path, MUST NOT
allocate per keystroke, and MUST NOT run on the presenter's display-link
callback.

`--nfr-key` MUST run with Blocks **off** and MUST also be demonstrated with
Blocks **on**, meeting the same p95 budget. If Blocks on cannot meet it, Blocks
do not ship — PRD §7: stop that surface, do not re-cut the instrument.

T-NFR MUST NOT be modified by this milestone.

Mutation `block_bookkeeping_on_key_path` MUST turn T-BLOCK-COLD red.

### D4 — Raw mode is the same PTY, and arbitration is explicit

F-069, F-083, F-084.

- **Classic/raw input (F-069):** with Blocks off, keys go straight to the PTY.
  This MUST remain a first-class supported mode, not a degraded fallback.
- **Raw mode same PTY (F-083):** entering the alternate screen selects raw
  presentation of the **same** leaf. It MUST NOT spawn a process, MUST NOT
  create a second leaf, and MUST NOT reparse. Chip 0 already knows the mode
  (ADR 0040 D1); Blocks read that, they do not track it.
- **Shift+mouse (F-084):** while mouse reporting is on, Shift selects in the UI.
  This is ADR 0040 D3's single explicit rule, stated once, no heuristic.

### D5 — Live terminal Block is refused

F-082 asks for a full PTY/TUI rendered as a Block type. **Rejected.**

A live Block would mean a second live surface with its own damage tracking and
its own presenter, inside a scrolling list, alongside the one Chip 0 that T-NFR
measures. That is the second VT this tree has refused since ADR 0012, arriving
as a list item.

The supported answer is D4: alt-screen switches the pane to raw presentation of
the same leaf. One live surface, always.

F-082 closes as `wontfix`.

### D6 — Block actions are local, and rerun never runs by itself

F-071, F-076, F-077, F-078, F-081.

- Copy command / output / both (F-071) copies from retained ContentItems and
  their declared source evidence.
  Redaction (ADR 0044 D4) applies — the clipboard is a sink.
- Find/filter (F-076) searches recorded metadata and replayed bytes. Cold.
- **Rerun / edit-and-run (F-077):** MUST place the command in the input for
  explicit submission. It MUST NOT execute on click. A one-click rerun of
  whatever was in a Block is a footgun the moment a Block holds `rm -rf`.
- Bookmarks (F-078) and compact/dividers (F-081) are view state, no kernel
  effect.

Mutation `rerun_executes_on_click` MUST turn T-BLOCK-RERUN red.

### D7 — Share permalink is optional, off, redacted, and confirmed

F-080. A cloud permalink is an optional layer (ADR 0044 D5). Local copy MUST
always work with sharing off and with no account.

Sharing MUST show exactly what will be uploaded, MUST apply redaction before
upload (ADR 0044 D4), and MUST be a per-Block explicit action. Nothing ships in
M6; the constraint is fixed here.

Attach-as-context (F-079) hands explicitly selected retained content and its
declared source evidence to a task under
ADR 0048 D9's scoping and redaction rules.

### D8 — Prompt chips must not break the user's prompt

F-089. Native cwd/git chips are optional and default **off**. With chips on,
RILL MUST NOT rewrite, suppress, or reposition the shell's own PS1 output. The
user's prompt is the child's bytes (NFR-BYTES).

A chip is chrome beside the region, never an edit of the stream.

### D9 — Rich input overlay for CLI agents

F-085. The mouse-first editor may submit into a CLI agent's PTY. That write is
ADR 0049 D4's single gated path — it MUST require the capability, MUST respect
FR-ONE, and MUST be interruptible by a real keystroke.

The overlay MUST NOT bypass the agent's permission profile because the bytes
originated in a nicer text field.

### D10 — Oracle

| ID | Closes |
|---|---|
| T-BLOCK-POD | Historical D1 memory guard only; ADR 0053 replaces ring-only identity with stable materialized content |
| T-BLOCK-BOUNDARY | D2 — marks or background; no prompt-regex inference |
| T-BLOCK-COLD | D3 — `--nfr-key` meets budget with Blocks on |
| T-BLOCK-RAW | D4 — alt-screen is the same leaf, same pid, no reparse |
| T-BLOCK-RERUN | D6 — rerun fills input, never executes |
| T-BLOCK-CHIP | D8 — PS1 bytes unmodified with chips on |

## Consequences

- [SPEC-BLOCKS](../spec/SPEC-BLOCKS.md) is the Block contract.
- ADR 0040 D6's ring bound becomes user-visible through Block truncation.
- F-082 closes `wontfix`. F-080 records constraints and ships nothing in M6.
- Chip 1 is still not the live chip; M7 and ADR 0012 are unaffected.

## Rejected alternatives

- **Blocks own their own output copy so they survive ring eviction.** Rejected:
  D1, AGENTS.md §5. That is the prototype's per-cell snapshot.
- **A second VT per Block.** Rejected: D5, ADR 0012.
- **Heuristic prompt detection for boundaries.** Rejected: D2.
- **Require shell integration for Blocks to work at all.** Rejected: ADR 0040
  D4. The terminal must be correct without it.
- **One-click rerun.** Rejected: D6.
- **Rewriting PS1 to make chips look native.** Rejected: D8, NFR-BYTES.
- **Measuring NFR-KEY only with Blocks off.** Rejected: D3 — that measures a
  product we would not be shipping.
