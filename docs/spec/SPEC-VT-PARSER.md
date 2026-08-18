# SPEC-VT-PARSER — Chip 1 byte layer (`lane:chip1-vt-engine`, M4)

- **Status:** Accepted for the M4 contract — 2026-08-18. Named tests are **Red**.
- **Authority:** [ADR 0020](../adr/0020-chip1-parser-in-tree.md),
  [ADR 0012](../adr/0012-chip1-isolated-vt.md) D5–D6, D9
- **Issue:** [#6](https://github.com/mahboobmonnamd/RILL/issues/6). Parser pick
  recorded by [S-VT #21](https://github.com/mahboobmonnamd/RILL/issues/21) /
  [SPIKE-VT](../SPIKE-VT.md).
- **Crate:** `crates/vt-engine` (parser module)
- **Gates:** T-CHIP1-BYTES, T-CHIP1-C1, T-CHIP1-CUP, T-CHIP1-BOUNDS,
  T-CHIP1-DIFF — **Red**. Not live. Not T-NFR.
- **Supersedes:** [SPEC-CHIP1](SPEC-CHIP1.md) §3's closing parser clause

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. Boundary

The parser turns bytes into **actions**. It holds no grid, no cursor, no
colours. The screen ([SPEC-VT-SCREEN](SPEC-VT-SCREEN.md)) applies actions.
Neither MUST name the other's internals (ADR 0020 D6).

```rust
trait Actions {
    fn print(&mut self, c: char);
    fn execute(&mut self, byte: u8);
    fn csi(&mut self, params: &Params, intermediates: &[u8], ignore: bool, action: char);
    fn esc(&mut self, intermediates: &[u8], byte: u8);
    fn osc(&mut self, params: &[&[u8]]);
}
```

The split is normative, not stylistic: it is what lets the `vte` differential
drive the same screen (ADR 0020 D2), and it keeps the pick reversible.

`vte` MUST NOT appear in `[dependencies]` or `[build-dependencies]`
(ADR 0020 D2), enforced by `no-vte-at-runtime`.

## 2. UTF-8 and the C1 range

v0 is a UTF-8 stream. There are **no 8-bit control introducers**
(ADR 0020 D3).

- Bytes MUST reach the parser unmodified. No `from_utf8_lossy` over the input,
  no validation filter, no dropping `>= 0x80`.
- A byte in `0x80..=0x9f` that is not part of a valid sequence is invalid UTF-8
  and MUST produce exactly **one** U+FFFD cell. It MUST NOT be dispatched as a
  control.
- A **decoded** scalar in U+0080..=U+009F MUST `print` as an ordinary cell.
  `0x9b` MUST NOT open a CSI; `0x9c` MUST NOT terminate a string. Only 7-bit
  `ESC [` and `ESC \` are honoured.
- `0xc0`, `0xc1`, `0xf5..=0xff`, overlong forms, and surrogate encodings each
  produce exactly one U+FFFD.
- A truncated sequence produces one U+FFFD and the interrupting byte is
  **reprocessed**, not swallowed.
- A sequence split across two `feed` calls MUST complete on the second. Partial
  state is at most 4 bytes and MUST NOT allocate.

This is the only policy S-VT found that passes all nine T-BYTES fixtures and
keeps `drop_high_bytes` detectable on eight (SPIKE-VT Results 1–2).

Gates: **T-CHIP1-BYTES** (§7), **T-CHIP1-C1** — feed `[0xc2, 0x9b, 0x41]`; row 0
is `U+009B` then `A`, and the cursor advanced two columns, proving `0x9b` did not
open a CSI that ate the `A`. Mutation: treat decoded U+0080..=U+009F as a
control.

## 3. C0

`execute` is dispatched for `0x00..=0x17`, `0x19`, `0x1c..=0x1f`. Screen effects
are SPEC-VT-SCREEN §3. BEL MAY be a no-op. `CAN` (`0x18`) and `SUB` (`0x1a`)
MUST abort any sequence in progress and return to ground. `ESC` (`0x1b`) MUST
abort any sequence in progress, including inside OSC and DCS.

## 4. ESC

- `0x20..=0x2f` collect as intermediates; `0x30..=0x7e` dispatches.
- v0 acts on `ESC 7` / `ESC 8` (DECSC/DECRC) and `ESC D` / `ESC E` / `ESC M`
  (IND/NEL/RI). Others are consumed and ignored.
- `ESC ]` enters OSC, `ESC P` enters DCS, `ESC X` / `ESC ^` / `ESC _` enter
  SOS/PM/APC.
- Charset designation beyond UTF-8 is out of scope (ADR 0012 D5): consumed,
  ignored, MUST NOT crash or shift the grid.

## 5. CSI

- Parameters are decimal, separated by `;`. Sub-parameters separated by `:`
  MUST be accepted and, in v0, flattened in order — this is what makes
  `38:2:255:0:255` behave as `38;2;255;0;255`.
- An empty parameter is 0, and the **default** for a given final byte applies
  where the sequence defines one (e.g. `CSI H` is `1;1`).
- Private markers `0x3c..=0x3f` (`<`, `=`, `>`, `?`) collect as intermediates.
  `?` is what distinguishes DEC private modes.
- Parameter accumulation MUST saturate, not wrap or overflow: `CSI
  9223372036854775808 m` MUST NOT panic.
- On overflow of params or intermediates the parser sets `ignore` and the
  sequence MUST be discarded rather than executed with truncated arguments
  (ADR 0020 D4).
- The v0 final bytes acted on are listed in SPEC-VT-SCREEN §4 and
  [SPEC-VT-COLOR](SPEC-VT-COLOR.md) §2. Every other final byte is consumed and
  ignored.

## 6. Bounds

Fixed capacities. `feed` MUST NOT allocate, and MUST NOT allocate
proportionally to input length (ADR 0012 D9).

| Bound | Value |
|---|---|
| CSI / DCS parameters | 32 |
| Intermediates | 2 |
| OSC raw bytes | 1024 |
| OSC parameters | 16 |
| Partial UTF-8 | 4 bytes |

Overflow discards and sets `ignore`. It MUST NOT panic, reallocate, or truncate
a sequence into a *different* valid sequence.

Gate: **T-CHIP1-BOUNDS** — 8 MiB unterminated OSC, 8 MiB unterminated DCS, and a
CSI with 1,000,000 parameters each leave the engine responsive with no
allocation growth attributable to `feed`, and a subsequent `feed(b"A")` still
prints. Oracle is a counting allocator around `feed` plus the resulting grid.
Mutation: accumulate OSC into a `Vec` without a cap. S-VT measured both
candidates as bounded here (SPIKE-VT Result 3), so this gate protects a property
we have, rather than asserting a hope.

## 7. Fixture corpus

T-CHIP1-BYTES reuses the T-BYTES fixtures: `lone_continuation`,
`truncated_3byte`, `overlong_slash`, `lone_surrogate`, `bom_then_high`,
`csi_high_param`, `c1_in_utf8`, plus every `fixtures/bytes/*.bin`.

**Oracle.** `A` is present when the fixture contains `0x41`. Every fixture that
contains a byte `>= 0x80` and does not begin with `0x1b` produces a non-ASCII
cell.

**Required mutation.** `drop_high_bytes` — filter `>= 0x80` before the parser.
The mutation MUST be detected by `lone_continuation`, `truncated_3byte`,
`overlong_slash`, `lone_surrogate`, `bom_then_high`, `c1_in_utf8`,
`zwj_emoji.bin` and `invalid_utf8.bin`.

`csi_high_param` is **blind** to this mutation and MUST NOT be cited as carrying
it: a high byte inside a CSI parameter changes no cell whether parsed or dropped
(ADR 0020 D7, SPIKE-VT Result 2). It stays in the corpus as a no-crash and
no-spurious-cell case.

## 8. OSC, DCS, SOS/PM/APC

- OSC 0/1/2/7/8/9/133 MAY be parsed and are **ignored for paint**. Title and cwd
  are attach-classifier and M6 tap work ([ADR 0013](../adr/0013-cwd-tap.md)).
- OSC terminates on BEL or ST (`ESC \`). An unterminated OSC MUST NOT consume
  unbounded memory (§6) and MUST be abandoned on `ESC`.
- OSC 4/10/11 (palette / default colours) are **consumed and ignored** in v0.
  The palette is configuration, not terminal output (ADR 0021 D3, ADR 0017 D3).
  Honouring them later is its own ADR.
- DCS and SOS/PM/APC bodies are consumed and ignored, bounded. A DCS body MUST
  NOT reach the screen as cells (ADR 0020 D5). Sixel, ReGIS, Kitty and iTerm
  image protocols are out of scope.

## 9. Differential oracle

`vte` (`[dev-dependencies]`, `default-features = false`) drives the same
`Actions` sink as the in-tree parser over the named corpus. Rules and the
divergence register: [SPEC-VT-CONFORMANCE](SPEC-VT-CONFORMANCE.md) §4.

Gate: **T-CHIP1-DIFF**. It is a **secondary** oracle. A gate MUST NOT be
expressed only as "equals `vte`", and where they disagree the spec wins.

## 10. Out of scope

Width and full grapheme segmentation
([ADR 0023](../adr/0023-chip1-v0-defers-character-width.md)), key encoding,
mouse report generation (the parser MAY ignore mouse reports), sixel/images,
8-bit C1 introducers, charset designation, scrollback, the live swap (M7).
