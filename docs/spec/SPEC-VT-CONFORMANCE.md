# SPEC-VT-CONFORMANCE — Chip 1 oracles, fixtures, CI (`lane:chip1-vt-engine`, M4)

- **Status:** Accepted for the M4 contract — 2026-08-18. Named tests are **Red**.
- **Authority:** [ADR 0002](../adr/0002-falsifiable-evidence.md) D2–D6, D9,
  [ADR 0012](../adr/0012-chip1-isolated-vt.md) D7–D8,
  [ADR 0020](../adr/0020-chip1-parser-in-tree.md) D2, D7
- **Issue:** epic [#6](https://github.com/mahboobmonnamd/RILL/issues/6)
- **Crates:** `crates/vt-engine`, `crates/rill-vt-types`
- **Gates:** every T-CHIP1-* in [TEST-CASES](../TEST-CASES.md) — **Red**

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. Oracle rules

The primary oracle for every gate is our own `snapshot()` — codepoint, cursor,
`attrs`, materialised colour, `cells.len()`, damage — or drained replies
(SPEC-VT-REPLY §6), compared against values written in the test.

Banned (ADR 0002 D4, ADR 0012 D7):

- A `Chip1.fed` field, or any retained copy of the input. Chip 0 deleted its
  `fed` field for exactly this reason (SPEC-CHIP0 §3); Chip 1 MUST NOT add one.
  A byte counter is fine; a byte log is not.
- Asserting on the `\x1b[2J\x1b[H` prefix that `resync_from_history` prepends
  itself. Assert a second instance's grid.
- A predicate hardcoded to the passing value.
- "Equals this `vte` version" or "equals this Ghostty pin" as the whole gate.
  **Named exception:** T-CHIP1-DIFF (this spec §4) compares grids from the
  in-tree parser and `vte` driving the same `Actions` sink. It remains
  **secondary**: it MUST still declare a required mutation, MUST apply the
  divergence register, and MUST NOT become the sole oracle for any other gate.
- Grepping a byte stream for a string the format cannot contain.

Every behavioural gate MUST declare a **required mutation** (ADR 0002 D3) and
MUST have been observed failing on a build where the behaviour is absent
(D2). A skip is a failure (D5): if a fixture file or the `mutate` feature is
absent, the test fails rather than returning green.

## 2. Fixture corpus

| Fixture | Source |
|---|---|
| `lone_continuation` `[80 41]` | inline |
| `truncated_3byte` `[e2 82 41]` | inline |
| `overlong_slash` `[c0 af]` | inline |
| `lone_surrogate` `[ed a0 80]` | inline |
| `bom_then_high` `[ff fe 80 41]` | inline |
| `csi_high_param` `[1b 5b 80 6d 41]` | inline |
| `c1_in_utf8` `[c2 9b 41]` | inline |
| `fixtures/bytes/*.bin` | every `.bin`, discovered not enumerated |
| `fixtures/invalid_utf8.bin` | on disk |
| `fixtures/look/themes/Catppuccin Latte`, `Catppuccin Mocha` | palette input (SPEC-VT-COLOR §6) |

The `.bin` sweep MUST read the directory rather than listing filenames, so a new
fixture is covered by adding a file. An empty or missing directory is a failure,
not a skip.

## 3. Mutation detectability is per fixture

S-VT measured which fixtures can actually detect `drop_high_bytes`
([SPIKE-VT](../SPIKE-VT.md) Result 2). This table is normative
(ADR 0020 D7):

| Fixture | Detects `drop_high_bytes` |
|---|---|
| `lone_continuation` | yes |
| `truncated_3byte` | yes |
| `overlong_slash` | yes |
| `lone_surrogate` | yes |
| `bom_then_high` | yes |
| `c1_in_utf8` | yes |
| `zwj_emoji.bin` | yes |
| `invalid_utf8.bin` | yes |
| `csi_high_param` | **no — blind** |

`csi_high_param` MUST NOT be cited as carrying the mutation: a high byte inside
a CSI parameter changes no cell whether it is parsed or dropped. It stays in the
corpus as a no-crash, no-spurious-cell case.

A gate whose corpus contains only blind fixtures is not evidence, whatever it
prints.

## 4. The `vte` differential

`vte` MAY appear **only** in `[dev-dependencies]`, `default-features = false`
(ADR 0020 D2). It MUST NOT appear in `[dependencies]` or
`[build-dependencies]`, and MUST NOT be reachable from a shipped artifact.

Both parsers drive the same screen through `Actions` (SPEC-VT-PARSER §1) and the
resulting grids are compared over the corpus in §2 plus the v0 sequence cases.

Mutations are injected into the **in-tree parser front only**, downstream of
the harness's byte source. `vte` MUST see the original bytes. Mutating the
shared source, or both fronts, would keep the differential green under
mutation and is forbidden.

- The differential is **secondary**. A gate MUST NOT be expressed only as
  "equals `vte`".
- Where they disagree, **the spec wins**, and the divergence MUST be registered
  below with a reason. An unregistered disagreement fails the gate.
- `vte` is not SHA-pinned: it ships nothing. ADR 0002 D7 pin discipline applies
  to `libghostty-vt`, which does ship.

### Divergence register

| # | Divergence | Reason |
|---|---|---|
| 1 | `vte` dispatches `0x80..=0x9f` to `execute()` as an 8-bit control; we produce one U+FFFD for an invalid byte and paint a decoded U+0080..=U+009F scalar | ADR 0020 D3. v0 is a UTF-8 stream with no 8-bit introducers. `vte`'s behaviour makes T-CHIP1-BYTES fail and its mutation blind on three fixtures. |

The differential harness MUST apply divergence 1 as an explicit remap when
comparing, rather than silently excluding the fixtures that expose it.

### Chip 0 differential

A Chip 0 differential MAY exist as a **macOS-only, dev-only** secondary check
(ADR 0012 D2). It MUST NOT be the primary oracle, MUST NOT run in `fast.yml`
(SPEC-CHIP0 §9), and MUST NOT make `vt-engine` depend on `rill-chip0` outside
`[dev-dependencies]`.

It is the only way to confirm ADR 0020 D3's **inference** that libghostty-vt
paints rather than executes decoded C1 scalars. That inference is unmeasured and
MUST be confirmed before M7.

## 5. Lints

`scripts/lint-planes.sh` MUST cover Chip 1:

| Lint | Rule |
|---|---|
| `no-cell-strings` | No `String` reachable from a snapshot type in `vt-engine` / `rill-vt-types` |
| `no-unwrap` | No `unwrap` / `expect` on reachable library paths in either crate |
| `no-ghostty-in-domain` | Already scans all crates; `ghostty_` MUST NOT appear in Chip 1 |
| `no-vte-at-runtime` | `vte` MUST NOT appear outside `[dev-dependencies]` |
| `no-theme-rgb-in-rust` | No theme's hex values in `vt-engine` / `rill-vt-types`, including test constants (ADR 0021 D3). Derivation and exemption below. |
| `no-host-dep-on-vt-engine` | `rill-host` / `rilld` MUST NOT depend on `vt-engine` until M7 (ADR 0012 D1) |

The last two are new. `no-host-dep-on-vt-engine` is the executable form of the
isolation promise: an accidental dependency is exactly the mistake ADR 0012 D1
forbids, and a lint catches it before review does.

`no-theme-rgb-in-rust` derives its forbidden set **at lint time** from
`fixtures/look/themes/*`: every RGB in Ghostty-grammar keys (`palette = N=`,
`foreground =`, `background =`, `cursor-color =`, `selection-background =`,
`selection-foreground =`). A hit is a Rust integer or hex literal whose 24-bit
RGB equals a forbidden value (`#rrggbb`, `0xrrggbb`, or `0xrrggbbff` with
alpha `ff`). **Exempt:** the finite set `Palette::vt_default()` defines
(SPEC-VT-COLOR §4: `#cccccc`, `#121212`, and the sixteen ANSI values). Those
literals MAY appear in `vt_default()` itself. A test constant that copies a
theme-file value is still a hit, even if it collides with a default slot.

## 6. CI

- `fast.yml` (Linux, no Zig) MUST add `-p rill-vt-types -p vt-engine` to the
  clippy and test list (ADR 0012 D8), and MUST run the `vte` differential and the
  negative controls that are wired as env mutations.
- `fast.yml` MUST NOT gain `rill-chip0` (SPEC-CHIP0 §9).
- Mutations are compiled only under `feature = "mutate"`, which no shipping build
  enables, and selected by `RILL_MUTATE=<name>`, matching Chip 0's arrangement.
  Named mutations: `drop_high_bytes`, `c1_as_control`, `drop_print`,
  `ignore_crlf`, `ignore_csi`, `noop_ed`, `ignore_sgr`, `unbounded_history`,
  `empty_resync`, `sgr_rgb_at_parse`,
  `skip_file_palette`, `eager_wrap`, `ignore_decstbm`, `always_full_damage`,
  `no_reply`, `unbounded_replies`, `unbounded_osc`, `single_buffer`,
  `fixed_grapheme_buf`, `wide_advances_two`.
- A gate that has never run in CI is not evidence, whatever a laptop printed
  (ADR 0002 D8). The S-VT numbers in [SPIKE-VT](../SPIKE-VT.md) are research and
  MUST NOT be cited as gate evidence.
- Evidence artifacts follow ADR 0002 D10. M4 gates are library gates; they do
  **not** close persist, paint, spawn or NFR-KEY, and no T-CHIP1 gate may be
  offered as closing a packaged host gate.

## 7. Fuzzing

`vt-engine`'s `feed` SHOULD gain a `cargo-fuzz` target mirroring the attach
decoder's arrangement (`crates/rill-attach/fuzz`), bounded in CI. Invariants: no
panic, no allocation growth attributable to `feed`, `cells.len()` stays
`cols * rows`, and the reply buffer stays within its cap for any input.

## 8. Out of scope

Packaged gates, T-NFR, the live swap (M7), Chip 0's own gates, host chrome
gates, width gates beyond the deferral marker
([ADR 0023](../adr/0023-chip1-v0-defers-character-width.md) D3).
