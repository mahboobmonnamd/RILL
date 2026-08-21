# Spike — Chip 1 character width and clustering source

**Status: research. 2026-08-19.** Records the measurements [ADR 0023](adr/0023-chip1-v0-defers-character-width.md) D5 required before a width-source ADR.

**Research does not authorize `vt-engine` behaviour, a `[dependencies]` crate, extra `PodCell` attr bits, or a live swap.** No gate below is evidence: nothing here has been demonstrated red on a build where the behaviour is absent, and none of it has run in CI ([ADR 0002](adr/0002-falsifiable-evidence.md) D2, D8).

**Issue:** [S-WIDTH #302](https://github.com/mahboobmonnamd/RILL/issues/302). Child implementation gate: [T-CHIP1-WIDTH #303](https://github.com/mahboobmonnamd/RILL/issues/303) (blocked on the width ADR). Live swap remains [#24](https://github.com/mahboobmonnamd/RILL/issues/24) / [#305](https://github.com/mahboobmonnamd/RILL/issues/305).

Parent: [SPIKE-VT](SPIKE-VT.md) Result 6 (neither parser supplies width).

## Question

ADR 0023 D5 left open: generated in-tree UCD East Asian Width, or a crate such as `unicode-width`? Two constraints already hold: the table is **data**, not a `match` from memory; a runtime crate is a `[dependencies]` change and needs the ADR 0020 D2 argument on its own terms.

Sub-questions that need observations, not taste:

1. Does `unicode-width` on `'日本X'` report **5** columns (the T-CHIP1-WIDTH oracle)?
2. Is scalar width enough, or must we width a **cluster** (ZWJ emoji, combining marks)?
3. Can Chip 1 keep a 16-byte `PodCell` with three attr bits, or does layout require wide-lead/tail bits the presenter already named (ADR 0003 D1)?
4. What is the crate's size, Unicode cadence, and default `"cjk"` behaviour for Ambiguous?

## Method

No workspace member. Throwaway `/tmp/rill-swidth` (not committed):

- `unicode-width` **0.2.2** (crates.io, default features including `"cjk"`).
- `unicode-segmentation` 1.x for UAX #29 graphemes (comparison only).
- Python 3 `unicodedata.east_asian_width` (UCD property, not a terminal).

`rustc` aarch64-apple-darwin. Chip 0 / libghostty-vt was **not** run. Claims about Chip 0 layout are from the adapter source in this tree, marked as such.

## Result 1 — T-CHIP1-WIDTH's oracle is 5, and both tables agree on CJK

`'日本X'`: U+65E5 W, U+672C W, U+0058 Na.

| Source | Columns |
|---|---|
| UAX #11 naive (`W`/`F` → 2, else 1) | **5** |
| `UnicodeWidthStr::width("日本X")` 0.2.2 | **5** |
| Chip 1 v0 today (T-CHIP1-WIDTH-DEFERRED) | **3** |

Hangul `가`, fullwidth `Ａ`, emoji `😊` are width 2 in both. That is enough to replace T-CHIP1-WIDTH-DEFERRED with T-CHIP1-WIDTH for the named CJK fixture. It is **not** enough for ZWJ.

## Result 2 — summing scalar widths is wrong for ZWJ; cluster then width

Family emoji `👨‍👩‍👧‍👦`:

| Method | Columns |
|---|---|
| `s.width()` (`UnicodeWidthStr`) | **2** |
| sum of `char.width()` | **8** (`[2,0,2,0,2,0,2]`) |
| UAX #29 grapheme, then `g.width()` | **2** |
| Chip 1 v0 (append to cluster, one column) | **1** |

Combining `e` + U+0301: `str.width() == 1`, mark width 0.

**Verdict.** Width must be a property of the **cluster**, not of each `print` scalar independently. ADR 0023 D2 already said full clustering arrives with width; this measurement is why. A screen that calls `char.width()` per `print` and advances will pass `日本X` and fail every ZWJ TUI chrome.

`unicode-width` 0.2's `tables.rs` is not a plain East Asian Width array. It encodes emoji presentation, ZWJ, regional indicators, keycaps, tags. That is why the file is **~1.56 MiB**. Taking the crate for `UnicodeWidthChar` only would still need a second clustering pass, and summing those widths would still be wrong.

## Result 3 — Ambiguous is a product choice, not a table bug

| Scalar | EAW | `width()` 0.2.2 (defaults) |
|---|---|---|
| `α` U+03B1 | A | 1 |
| `■` U+25A0 | A | 1 |
| `₩` U+20A9 | H | 1 |

The crate's default `"cjk"` feature **adds** `width_cjk()` (Ambiguous → 2 in CJK contexts). It does not change `width()` for these samples. Shipping `unicode-width` with default features still pulls that extra table. `default-features = false` drops `"cjk"`.

**Recommended default for RILL:** Ambiguous → **1**, matching a western UTF-8 `wcwidth` and `unicode-width::width`. Do not silently enable `width_cjk`. A locale-switch is a later ADR, not M7.

## Result 4 — Chip 0 already expands to one POD cell per column; the presenter ignores wide flags

`crates/rill-chip0/src/adapter/rill_chip0_vt.c` walks libghostty's cell iterator and writes **one `PodCell` per column**. The second column of a wide glyph is whatever Ghostty emits (typically a continuation). Snapshot `attrs` are only bold/underline/inverse. Extra grapheme codepoints are discarded; only the base scalar is stored (SPEC-CHIP0 §5).

`host/macos/TerminalView.m` builds one Metal instance per cell. `codepoint == 0` is drawn as space (`cell.codepoint ? cell.codepoint : 32`). Instance `flags` currently set **underline only**. ADR 0003 D1 named wide-lead/tail on the instance, but they are not wired.

**Verdict for the cell model (recommendation, not an ADR):**

- Keep `PodCell` **16 bytes**. Do not grow the ABI.
- A wide cluster occupies **two columns**: lead holds the base scalar (same as Chip 0 today); tail is a continuation the presenter must **not** atlas as its own glyph.
- Continuation MUST NOT be `codepoint == 0` unless the host stops treating 0 as space, or the tail will paint a blank **and** the lead glyph stays one cell wide — cursor would be right, pixels wrong.
- Wide-lead / wide-tail belong in `attrs` bits 3 and 4 (13 bits still free). SPEC-VT-TYPES §2 forbids that in **v0**; the width ADR must authorize it. `_pad` stays zero. T-CHIP1-POD (`size_of == 16`) stays green.
- Host instance flags (ADR 0003) should be set from those bits at swap time so CJK is two cells on GPU. That is display work on the M7 PR, not a second VT.

## Result 5 — `unicode-width` at runtime fails the ADR 0020 D2 test

| | `unicode-width` 0.2.2 | In-tree generated UCD + small machine |
|---|---|---|
| Provenance | crates.io / unicode-rs | Unicode EAW + emoji files pinned in this tree |
| Cadence | crate bump (Unicode + their emoji state) | we bump the pin, regenerate, review the diff |
| Size | `tables.rs` ~1.56 MiB (includes ZWJ/RI/keycap) | EAW text is small; emoji state is the cost either way |
| Transitive | **none** | none |
| License | MIT OR Apache-2.0 | Unicode data license + our generator |
| `fast.yml` | registry fetch | no network if generated Rust is committed |
| Matches T-CHIP1-WIDTH | yes (Result 1) | yes if W/F → 2 |
| Cluster-aware `str.width` | yes | we must write it (Result 2) |

Chip 1 exists so this tree does not live on an external VT's release cadence (ADR 0012 Context, ADR 0020 D1). Importing `unicode-width` as `[dependencies]` re-imports Unicode/emoji cadence into the crate whose job is to own it. Throughput is irrelevant: width is a table lookup on `print`, not the NFR path (ADR 0004: libghostty-vt is not the slow part).

`unicode-width` as **`[dev-dependencies]` only**, like `vte`, is a good **secondary** oracle. A gate MUST NOT be “equals `unicode-width`”. Primary oracle: cursor column and lead/tail cells against values written in the test (ADR 0012 D7). Divergence (Ambiguous, Brahmic, `width_cjk`) is named in the spec.

Hand-written CJK `match` ranges remain forbidden (ADR 0023 D5).

## Recommended pick (for the width ADR — Proposed until Accepted)

1. **Cluster, then width.** Extend Chip 1 clustering from ADR 0023 D2's subset to the sequences `unicode-width` 0.2 actually special-cases for terminals: combining marks, ZWJ, variation selectors, regional-indicator pairs. Cite UAX #11 + the emoji ZWJ rule. Full generic UAX #29 (Indic conjuncts, etc.) may stay a named miss if the ADR says so; Devanagari `क्ष` is width 2 in `unicode-width` and 1+1 if we only EAW the scalars.
2. **Ship generated data in this tree**, committed (no `fast.yml` Unicode download). Pin the Unicode version next to `third_party/ghostty.pin` discipline: moving it is its own PR. Generator is a script; humans do not edit the table.
3. **Do not add `unicode-width` under `[dependencies]`.** MAY add it under `[dev-dependencies]` as a differential, `default-features = false` unless a test explicitly wants `width_cjk`.
4. **Ambiguous → 1.**
5. **Two cells + attrs bits 3/4** (lead/tail). `PodCell` stays 16 bytes. Host presenter honors the bits on the live-swap PR so paint matches cursor. Chip 0 adapter may leave those bits 0 until it is removed from the warm path.
6. **Wrap:** if remaining columns `<` cluster width, pending-wrap / DECAWM the same as ASCII (SPEC-VT-SCREEN §2), then place the wide cluster. A wide glyph MUST NOT split across rows.
7. **T-CHIP1-WIDTH** replaces T-CHIP1-WIDTH-DEFERRED: `日本X` → `cursor_col == 5`, cells 0–1 lead/tail for `日`, 2–3 for `本`, 4 = `X`. Mutation: force one column per scalar (today's v0). Additional named fixtures: ZWJ family width 2, combining mark width 0 extra columns. Do not delete T-CHIP1-GRAPHEME.

## What this spike does not decide

- Live swap ([#305](https://github.com/mahboobmonnamd/RILL/issues/305), [#24](https://github.com/mahboobmonnamd/RILL/issues/24)).
- Mode channel, `take_replies` drain, T-NFR recut (forbidden).
- Blocks / live TUI-in-block ([ADR 0050](adr/0050-blocks-are-a-cold-overlay.md) D5 still refuses a second VT).
- Colour emoji atlas (ADR 0003: tofu until a BGRA atlas).
- Chip 0 C1 measurement ([#304](https://github.com/mahboobmonnamd/RILL/issues/304)).

## What we will not do from these notes

- Implement width in `vt-engine` before an Accepted width ADR.
- Link Chip 1 as the live chip.
- Hand-write ranges.
- Use `unicode-width` as the primary test oracle.
- Grow `PodCell` past 16 bytes.
- Cite another application tree.

## Next

1. Width-source ADR (Proposed → Accepted) citing this spike. **Does not authorize #24.**
2. Spec delta: SPEC-VT-SCREEN §9, SPEC-VT-TYPES §2 attr bits, SPEC-VT-CONFORMANCE mutation list (`wide_advances_two` already exists for the deferred gate; invert it).
3. [#303](https://github.com/mahboobmonnamd/RILL/issues/303) tests red, then the smallest screen change.
4. Packaged CJK look stays [#272](https://github.com/mahboobmonnamd/RILL/issues/272) after the live swap.
