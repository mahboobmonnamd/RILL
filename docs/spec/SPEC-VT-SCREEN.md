# SPEC-VT-SCREEN — Chip 1 grid and cursor (`lane:chip1-vt-engine`, M4)

- **Status:** Accepted for the M4 contract — 2026-08-18. Named tests are **Red**.
- **Authority:** [ADR 0012](../adr/0012-chip1-isolated-vt.md) D3, D5,
  [ADR 0020](../adr/0020-chip1-parser-in-tree.md) D6,
  [ADR 0023](../adr/0023-chip1-v0-defers-character-width.md)
- **Issue:** [#6](https://github.com/mahboobmonnamd/RILL/issues/6)
- **Crate:** `crates/vt-engine` (screen module)
- **Gates:** T-CHIP1-ASCII, T-CHIP1-CRLF, T-CHIP1-CUP, T-CHIP1-ED,
  T-CHIP1-ALT, T-CHIP1-SIZE, T-CHIP1-GRAPHEME, T-CHIP1-WRAP, T-CHIP1-SCROLL,
  T-CHIP1-DAMAGE, T-CHIP1-WIDTH-DEFERRED — **Red**. Not live. Not T-NFR.

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. Boundary

The screen consumes actions from [SPEC-VT-PARSER](SPEC-VT-PARSER.md) and owns
the visible grid, cursor and pending attributes. It MUST NOT parse bytes, own a
PTY, paint, or keep scrollback.

Visible grid is exactly `cols * rows` (ADR 0012 D3). History is the kernel byte
ring (ADR 0001 §4). A snapshot MUST NOT grow with fed bytes.

## 2. Cursor and wrap

- Cursor position is 0-based internally; VT sequences are 1-based.
- Cursor moves MUST clamp to the grid. A move MUST NOT scroll.
- **Deferred wrap** is required: after printing in the last column the cursor
  stays on that column with a pending-wrap flag. The next printable resets the
  flag, moves to column 0 and advances a row. `CR`, `CUP`, and any explicit
  cursor move clear the flag.

  Without deferred wrap, printing exactly `cols` characters scrolls one row too
  early and every full-width line in `vim` is wrong.
- DECAWM (`?7`) is **on** by default. With it off, printing in the last column
  overwrites that cell and does not wrap.
- DECSC / DECRC (`ESC 7` / `ESC 8`) save and restore cursor position, pending
  attributes and pending-wrap.

Gate: **T-CHIP1-WRAP** — on a 10-column grid, feed 10 printables then one more;
the 11th lands at row 1 column 0 and rows 0 and 1 hold the expected text.
Mutation: wrap eagerly on reaching the last column. That must turn it red.

## 3. C0 effects

| Byte | Effect |
|---|---|
| `BS` `0x08` | Cursor left one, clamped at column 0. Clears pending wrap. |
| `HT` `0x09` | Next tab stop; default stops every 8 columns. Clamped to last column. |
| `LF` `0x0a`, `VT` `0x0b`, `FF` `0x0c` | Cursor down one row within the scroll region, scrolling the region if at the bottom. Column unchanged. Clears pending wrap. |
| `CR` `0x0d` | Column 0. Clears pending wrap. |
| `BEL` `0x07` | MAY be a no-op. |

Gate: **T-CHIP1-CRLF** — `A\r\nB` puts `B` at row 1 column 0.
Mutation: ignore CR/LF.

## 4. CSI acted on in v0

| Sequence | Behaviour |
|---|---|
| CUU/CUD/CUF/CUB `A B C D` | Relative cursor move, default 1, clamped. |
| CUP `H`, HVP `f` | Absolute, 1-based `row;col`, default `1;1`, clamped. |
| CHA `G`, VPA `d` | Absolute column / row. |
| CNL `E`, CPL `F` | Down/up n rows, column 0. |
| ED `J` | `0` cursor to end, `1` start to cursor, `2`/`3` whole display. Cleared cells take the **current** background per §6. |
| EL `K` | `0` cursor to end of line, `1` start to cursor, `2` whole line. |
| IL `L`, DL `M` | Insert / delete n lines at the cursor row, within the scroll region. Lines shift; the region's other rows are untouched. |
| ICH `@`, DCH `P` | Insert / delete n cells on the cursor row; the row shifts, the rest of the grid does not. |
| SU `S`, SD `T` | Scroll the region up / down n lines. |
| DECSTBM `r` | Set top and bottom margins, 1-based inclusive. `CSI r` with no params resets to the full grid. Setting the region moves the cursor to its home. |
| DECTCEM `?25 h/l` | Cursor visible / hidden → `PodGrid.cursor_visible`. |
| DECAWM `?7 h/l` | Auto-wrap, §2. |
| `?1049 h/l`, `?1047 h/l` | Alt screen, §5. |
| SGR `m` | [SPEC-VT-COLOR](SPEC-VT-COLOR.md) §2. |
| DA `c`, DSR `n` | [SPEC-VT-REPLY](SPEC-VT-REPLY.md) §3. |

Every other final byte is consumed and ignored (SPEC-VT-PARSER §5).

Gates: **T-CHIP1-CUP** — `ESC[5;10H` puts the cursor at 0-based row 4, column 9,
documented in the test. Mutation: ignore CSI. **T-CHIP1-ED** — feed text, then
`ESC[2J`; every cell codepoint is space `32`. Mutation: ED is a no-op.

## 5. Scroll region and alt screen

- Scrolling MUST respect DECSTBM. Rows outside the region MUST NOT move. This is
  what `less` and `vim` status lines depend on; a full-grid scroll destroys them.
- Content scrolled off the top is **discarded**. The chip keeps no scrollback
  (ADR 0012 D3).
- `?1049h` switches to a **cleared** alt screen, saving the primary buffer and
  the cursor. `?1049l` restores both. `?1047h/l` switches buffers preserving the
  primary but does not save the cursor.
- Entering the alt screen twice MUST NOT overwrite the saved primary. Leaving
  when not in the alt screen MUST be a no-op, not a clear.

Gate: **T-CHIP1-ALT** — feed `A`, `?1049h`, feed `B`, `?1049l`; `A` is visible
and `B` is gone. Mutation: single buffer.

Gate: **T-CHIP1-SCROLL** — set `DECSTBM 2;4` on a 6-row grid, fill rows, force a
scroll; rows 0 and 5 are unchanged and rows 1..3 shifted. Mutation: ignore
DECSTBM and scroll the whole grid.

## 6. Cells, attributes and erasure

- A printable writes `codepoint`, the current colours and `attrs` into the cell.
- The empty cell is space `32` with the current default colours.
- Erasure (ED, EL, IL/DL vacated rows, alt-screen clear) fills with space and
  the **current background**, not the default background. A program that sets a
  background and clears expects the painted colour; using the default is the
  classic "black bar" bug.
- `attrs` is bit0 bold, bit1 underline, bit2 inverse. v0 MUST NOT add bits
  (SPEC-VT-TYPES §2).

Gate: **T-CHIP1-SGR** — `ESC[1mX` leaves that cell `attrs & 1 != 0`.
Mutation: ignore SGR.

## 7. Damage

- `full_damage` on reset, resize, alt-screen switch and whole-display erase.
- Otherwise `damage_row0..=damage_row1` is the inclusive range of rows touched
  since the last snapshot.
- When nothing changed, `full_damage == false` and `damage_row0 > damage_row1`
  so the caller can skip the frame (SPEC-VT-TYPES §3).
- `snapshot()` clears damage **after** the caller has the data.

Gate: **T-CHIP1-DAMAGE** — feed one character on row 3, snapshot: damage covers
row 3 and not row 0. Snapshot again with no feed: the skip condition holds.
Mutation: always report `full_damage`. That must turn the skip assertion red.

## 8. Resize

```rust
fn resize(&mut self, cols: u16, rows: u16, cell_w: u32, cell_h: u32) -> Result<(), Error>;
```

- The grid becomes exactly `cols * rows`; `cells.len()` follows (T-CHIP1-SIZE).
- Cursor is clamped into the new bounds. The scroll region is reset when it no
  longer fits.
- `cell_w` / `cell_h` are accepted for signature parity with Chip 0 and are not
  used by v0 layout. They MUST NOT change cell contents.
- `cols == 0` or `rows == 0` is an error, not a panic and not a silent clamp to
  1: a zero-area grid would make every index calculation a special case.
- Reflow is **not** required in v0. Rows are truncated or padded. A reflowing
  resize is a later ADR.

## 9. Clusters and width

- Combining marks (`U+0300..=U+036F`), ZWJ (`U+200D`) and variation selectors
  append to the preceding cell's cluster instead of consuming a cell.
- `RILL_GRAPHEME_MAX` is 32, matching Chip 0. Beyond it: keep the base
  codepoint, increment `grapheme_truncated`, never silently drop, and never use
  a fixed stack buffer (audit S3-1).
- **Every printable scalar advances exactly one column in v0.** CJK and emoji
  render narrow and misaligned. This is a named miss
  ([ADR 0023](../adr/0023-chip1-v0-defers-character-width.md) D1), a
  precondition of M7, and not a defect to be filed.

Gates: **T-CHIP1-GRAPHEME** — `e` + 40× U+0301 survives; `grapheme_truncated >=
1` or the base is still visible; no panic. Mutation: fixed 8-slot stack buffer
or silent drop. **T-CHIP1-WIDTH-DEFERRED** — `日本X` leaves the cursor at column
3, documenting the v0 miss. Mutation: advance two columns for East Asian Wide.

## 10. Out of scope

Width tables and UAX #29 (ADR 0023), reflow on resize, scrollback, sixel and
images, mouse and key encoding, `snapshot_damaged`
([#18](https://github.com/mahboobmonnamd/RILL/issues/18)), paint, Blocks, the
live swap (M7).
