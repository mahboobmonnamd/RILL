# ADR 0023: Chip 1 v0 defers character width; M7 is blocked on it

- **Status:** Accepted — 2026-08-18. **Amended 2026-08-19** by
  [ADR 0035](0035-chip1-character-width.md): D1, D3, D4 and D5 are
  **superseded**. D2's combining-range restriction (`U+0300..=U+036F`) is
  **replaced** by 0035 (generated Mn/Me), not merely extended. D2's bound,
  counted truncation, and "not full UAX #29" still hold. This file is kept
  so the deferral's history stays on the record; do not delete it.
- **Tree:** this repository only
- **Issue:** epic [#6](https://github.com/mahboobmonnamd/RILL/issues/6)
  (child issues to be filed: T-CHIP1-WIDTH-DEFERRED, and the width slice itself)
- **Requires:** [ADR 0012](0012-chip1-isolated-vt.md),
  [ADR 0020](0020-chip1-parser-in-tree.md)
- **Amends:** [SPEC-CHIP1](../spec/SPEC-CHIP1.md) §3 — adds an explicit v0
  non-goal the spec did not name
- **Blocks:** [M7 #24](https://github.com/mahboobmonnamd/RILL/issues/24),
  [#272](https://github.com/mahboobmonnamd/RILL/issues/272)
- **Does not authorize:** shipping Chip 1 as the live chip with this gap, adding
  a width dependency, extra `attrs` bits, or describing the deferral as done
  (the width-source pick and the T-CHIP1-WIDTH replacement are ADR 0035)

## Context

S-VT ([SPIKE-VT](../SPIKE-VT.md) Result 6) found that **neither** parser
candidate supplies character width, and neither does grapheme clustering.
`'日本X'` produced three `print` calls and advanced the cursor 3 columns; a
conforming terminal advances 5, because each CJK ideograph occupies two cells.

libghostty-vt did this for Chip 0, including cluster handling with
`RILL_GRAPHEME_MAX = 32` and a `grapheme_truncated` counter (SPEC-CHIP0 §5).
Chip 1 inherits none of it. SPEC-CHIP1 never specified width at all, so it was
invisible in the M4 estimate — the largest unpriced item in the milestone, and
not a parser question.

Correct width needs an East Asian Width table from the Unicode Character
Database, plus rules for combining marks, ZWJ sequences, variation selectors and
regional indicators. That is a body of data and a body of edge cases. Doing it
badly is worse than not doing it: a wrong width corrupts every subsequent cell
on the row, and `vim` and `htop` redraw against a cursor position they believe
they control.

## Decision

### D1 — v0 advances one column per scalar, and says so

**Superseded by [ADR 0035](0035-chip1-character-width.md) D1/D5.** Chip 1 now
clusters, then widths; `attrs` bits 3 and 4 are wide-lead / wide-tail;
`PodCell` stays 16 bytes. The v0 miss this paragraph named is closed by
T-CHIP1-WIDTH, not by deleting this ADR.

Every printable scalar occupies exactly one cell in v0. `attrs` gains no
wide-lead or wide-tail bit (SPEC-CHIP1 §2 keeps three bits and 16-byte
`PodCell`). CJK, emoji and other wide characters render **narrow and
misaligned**. This is a known, named miss, not a bug to be filed later.

### D2 — Clustering is bounded, simplified, and counted

**Amended by [ADR 0035](0035-chip1-character-width.md) D1.** The
combining-range restriction (`U+0300..=U+036F` only) is **replaced**, not
merely extended: clustering uses generated Mn/Me (and ZWJ, variation
selectors, printable after ZWJ, regional-indicator pairs).
`RILL_GRAPHEME_MAX = 32`, counted truncation, no fixed stack buffer, and
"not full UAX #29" still hold. Indic conjuncts remain a named miss.

Combining marks in `U+0300..=U+036F`, ZWJ (`U+200D`) and variation selectors
append to the preceding cell's cluster rather than consuming a new cell.
A printable scalar immediately following ZWJ MUST also append to that cluster.
The combining-mark range is restricted: marks outside `U+0300..=U+036F`
consume a cell in v0. That restriction is a named miss; full clustering
arrives with width.
`RILL_GRAPHEME_MAX` is 32, matching Chip 0. Beyond the bound: keep the base
codepoint, increment `grapheme_truncated`, never silently drop, never a fixed
stack buffer (the audit S3-1 defect).

This is deliberately **not** UAX #29. It is the subset that keeps the invariants
SPEC-CHIP1 §2 already requires — no overrun, visible base, counted truncation —
and it satisfies T-CHIP1-GRAPHEME. Full segmentation arrives with width in the
later slice.

### D3 — The deferral carries its own gate

**Superseded by [ADR 0035](0035-chip1-character-width.md) D7.**
T-CHIP1-WIDTH-DEFERRED is **replaced** by T-CHIP1-WIDTH (`cursor_col == 5`,
lead/tail cells), not deleted quietly. Mutation inverts to `narrow_cjk`.

**T-CHIP1-WIDTH-DEFERRED** — feed `日本X` into an 80×24 Chip 1; assert the
cursor is at column 3 and that this **documents the v0 miss**, citing this ADR
in the test's doc comment.

The gate is not a boast. It exists so the deferral is falsifiable and so that
the day someone implements width, a red test tells them to update this ADR
instead of leaving a silent behaviour change. When width lands, this gate is
**replaced** by T-CHIP1-WIDTH (cursor at column 5), not deleted quietly.

**Required mutation.** Advance two columns for a scalar with East Asian Width
Wide. Under v0 that must turn this gate red, proving the gate observes the
cursor and not a constant.

### D4 — M7 MUST NOT swap the live chip while this gap is open

**Superseded in part by [ADR 0035](0035-chip1-character-width.md) D7:** the
v0 gap this paragraph described is closed in `vt-engine` once T-CHIP1-WIDTH
is Proven. The **precondition on M7** survives: the live-swap ADR MUST still
cite T-CHIP1-WIDTH as Proven. ADR 0035 does not authorize the swap.

Chip 0 today lays out CJK and emoji correctly through libghostty-vt. Making
Chip 1 live with D1 in force would visibly regress any user typing or displaying
CJK, and would regress it in exactly the way
[#272](https://github.com/mahboobmonnamd/RILL/issues/272) exists to prevent for
colour.

Therefore: **width is a precondition of M7**, alongside T-CHIP1-LOOK-ANSI. The
M7 live-swap ADR MUST cite T-CHIP1-WIDTH as Proven. M4 is complete without it;
M7 is not.

### D5 — The width source is undecided, and that is deliberate

**Superseded by [ADR 0035](0035-chip1-character-width.md) D2/D3** after
[SPIKE-WIDTH](../SPIKE-WIDTH.md): generated in-tree UCD, Unicode version
pinned; `unicode-width` is `[dev-dependencies]` only. The two constraints
below still hold and 0035 repeats them.

This ADR does **not** pick between a generated in-tree UCD table and a crate
such as `unicode-width`. That decision needs its own spike, because the tradeoff
is data provenance and update cadence — the same class of question ADR 0020
answered for the parser, and it deserves the same evidence.

Whichever wins, two constraints already hold: the table is **data, not a `match`
arm hand-written from memory**, and it MUST NOT pull a transitive dependency
into a shipped artifact without an ADR. A width crate at runtime is a
`[dependencies]` change and therefore needs the ADR 0020 D2 argument made again
on its own terms.

## Consequences

- M4 ships a VT that is correct for ASCII and Latin text, which is what the
  T-BYTES fixtures, zsh, `less` and `vim` in a Latin locale exercise.
- `fixtures/bytes/zwj_emoji.bin` still must not overrun (T-CHIP1-GRAPHEME); it
  will simply render in the wrong number of columns.
- M7 gains a named precondition. Better a blocked milestone than a silent
  regression against a live chip that already handles this.
- The width slice is a normal ADR → spec → red test → implementation sequence
  later, not a patch bolted onto the screen.

## Rejected alternatives

- **Implement width in v0 with a UCD table.** Rejected for M4 scope: it is a
  separable body of work with its own data-provenance decision, and blocking the
  parser and screen slices behind it buys nothing. Not rejected on merit — it is
  required before M7.
- **Take `unicode-width` now.** Rejected as a v0 default: it is a runtime
  dependency decision that deserves the argument ADR 0020 D2 made, not a
  side effect of the screen slice.
- **Hand-write a "good enough" wide-char range check.** Rejected: a wrong width
  corrupts the rest of the row, and a hand-typed range list is precisely the
  compiled-from-memory catalog this tree keeps refusing (ADR 0017 D2 for
  colour, same principle).
- **Say nothing and let width appear later.** Rejected: an unnamed gap becomes
  an M7 surprise, and ADR 0002's whole posture is that a known miss is recorded
  as a miss.
- **Full UAX #29 clustering in v0.** Rejected: same scope argument as width, and
  the bounded rule already satisfies the memory-safety invariants.
