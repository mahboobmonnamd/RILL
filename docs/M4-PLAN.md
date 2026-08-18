# M4 plan — Chip 1 owned VT, slice by slice

**Status: plan. 2026-08-18.** `lane:chip1-vt-engine` / Milestone 4.
Epic [#6](https://github.com/mahboobmonnamd/RILL/issues/6). Handoff context:
[M4-HANDOFF](M4-HANDOFF.md).

This page is the *order of work*. It authorizes nothing on its own: each slice
still needs its named tests written first and observed red
([ADR 0002](adr/0002-falsifiable-evidence.md) D2). Chip 0 stays the live chip
until **M7** ([ADR 0012](adr/0012-chip1-isolated-vt.md) D1).

## Where we are

[S-VT #21](https://github.com/mahboobmonnamd/RILL/issues/21) is **closed**
([SPIKE-VT](SPIKE-VT.md)). It recorded the parser pick required by ADR 0012 D6
and found three defects in the contract before any code was written: a blind
mutation fixture, unspecified character width, and a mandated reply the API could
not deliver. Four ADRs answer those:

| ADR | Decides |
|---|---|
| [0020](adr/0020-chip1-parser-in-tree.md) | Parser in-tree; `vte` dev-only differential; C1 policy; per-fixture mutations |
| [0021](adr/0021-chip1-colour-identity.md) | Cells keep colour identity; `snapshot()` materialises against the theme palette |
| [0022](adr/0022-chip1-reply-channel.md) | `take_replies()` for DA / DSR, bounded and counted |
| [0023](adr/0023-chip1-v0-defers-character-width.md) | v0 defers width; **M7 is blocked on it** |

Six slice specs carry the normative detail:
[SPEC-VT-TYPES](spec/SPEC-VT-TYPES.md),
[SPEC-VT-PARSER](spec/SPEC-VT-PARSER.md),
[SPEC-VT-SCREEN](spec/SPEC-VT-SCREEN.md),
[SPEC-VT-COLOR](spec/SPEC-VT-COLOR.md),
[SPEC-VT-REPLY](spec/SPEC-VT-REPLY.md),
[SPEC-VT-CONFORMANCE](spec/SPEC-VT-CONFORMANCE.md).
[SPEC-CHIP1](spec/SPEC-CHIP1.md) is now the umbrella over them.

**In the tree:** nothing. No `crates/vt-engine`, no `crates/rill-vt-types`.
That is correct — the specs and red tests come first.

## Crate layout

```
crates/rill-vt-types/     no deps beyond core/std; Linux-clean
  src/lib.rs              PodCell, PodGrid, Color, Palette, TerminalEmulation, Error

crates/vt-engine/         [dev-dependencies] vte only
  src/lib.rs              VtEngine: feed / resize / snapshot / set_palette / take_replies
  src/parser.rs           bytes -> Actions
  src/screen.rs           Actions -> grid
  src/color.rs            SGR identity + materialisation
  src/reply.rs            DA / DSR buffer
  tests/                  T-CHIP1-* gates
  fuzz/                   feed target, mirroring crates/rill-attach/fuzz
```

`rill-host` and `rilld` MUST NOT appear in either manifest until M7.

## Slices

Each row is one GitHub issue under [#6](https://github.com/mahboobmonnamd/RILL/issues/6),
`lane:chip1-vt-engine`, milestone M4. Each PR: named tests first, observed red
with the output in the PR, then the smallest change that turns them green.

### Slice 1 — scaffolding and the gate harness

**Spec:** SPEC-VT-TYPES, SPEC-VT-CONFORMANCE §5–§6.
**Gates:** T-CHIP1-POD.
**Work:** `rill-vt-types` with the POD types, `Color`, `Palette`,
`Palette::vt_default()`, the relocated trait, `Error`. Empty `vt-engine` that
compiles. `fast.yml` gains `-p rill-vt-types -p vt-engine`.
`lint-planes.sh` gains `no-vte-at-runtime`, `no-theme-rgb-in-rust`,
`no-host-dep-on-vt-engine`, and extends `no-cell-strings` / `no-unwrap` to both
crates.
**Done when:** the gate harness runs on Linux with no Zig and the new lints pass.

Relocating `TerminalEmulation` out of `rill-chip0` is a **separate PR** in this
slice, and Chip 0's gates MUST stay green across it (SPEC-VT-TYPES §5). It is a
relocation, not a redefinition.

Scaffolding first so every later slice lands already gated. A slice that adds
behaviour before CI can see it is how a green-unproven test gets in.

### Slice 2 — parser core and screen core

**Spec:** SPEC-VT-PARSER §1–§4, §6–§7; SPEC-VT-SCREEN §1–§3, §6–§8.
**Gates:** T-CHIP1-ASCII, T-CHIP1-BYTES, T-CHIP1-C1, T-CHIP1-CRLF,
T-CHIP1-WRAP, T-CHIP1-SIZE, T-CHIP1-DAMAGE.
**Work:** UTF-8 decoding with the C1 policy, C0, ESC, the `Actions` boundary,
grid, cursor, deferred wrap, erasure with current background, damage tracking,
resize.
**Mutations:** `drop_high_bytes`, `c1_as_control`, `eager_wrap`,
`always_full_damage`.
**Done when:** the fixture corpus passes and each mutation is red on the
fixtures SPEC-VT-CONFORMANCE §3 says can detect it.

This slice is the one that proves the C1 decision. The first PR touching CSI
MUST cite ADR 0020 and S-VT (ADR 0012 D6).

### Slice 3 — CSI cursor moves and erase

**Spec:** SPEC-VT-SCREEN §4.
**Gates:** T-CHIP1-CUP, T-CHIP1-ED.
**Work:** CUU/CUD/CUF/CUB, CUP/HVP, CHA/VPA, CNL/CPL, ED, EL, IL/DL, ICH/DCH,
parameter defaults, saturating accumulation, `ignore` on overflow.
**Mutations:** ignore CSI; ED as a no-op.

### Slice 4 — scroll region and alt screen

**Spec:** SPEC-VT-SCREEN §5.
**Gates:** T-CHIP1-SCROLL, T-CHIP1-ALT.
**Work:** DECSTBM, SU/SD, LF scrolling inside the region, `?1049` / `?1047`,
DECSC/DECRC, DECTCEM, DECAWM.
**Mutations:** `ignore_decstbm`, `single_buffer`.

`less`, `vim` and `htop` become usable at the end of this slice — that is the
ADR 0012 D5 goal, minus width.

### Slice 5 — colour

**Spec:** SPEC-VT-COLOR. **Blocked on nothing now** (ADR 0021 is Accepted).
**Gates:** T-CHIP1-SGR, T-CHIP1-COLOR-IDENTITY, T-CHIP1-LOOK-ANSI.
**Issues:** closes the library half of
[#267](https://github.com/mahboobmonnamd/RILL/issues/267) and
[#271](https://github.com/mahboobmonnamd/RILL/issues/271).
**Work:** SGR to `Color`, `set_palette`, materialisation, the xterm-256 cube and
ramp arithmetically, `full_damage` on palette change.
**Mutations:** `sgr_rgb_at_parse`, `skip_file_palette`.

Colour comes before replies because it changes the snapshot path, and after the
screen because it needs cells to colour. It MUST NOT come after a slice that
resolved SGR to RGB — that is why ADR 0021 was written before any of this.

### Slice 6 — device replies

**Spec:** SPEC-VT-REPLY.
**Gates:** T-CHIP1-REPLY.
**Work:** reply buffer with the 1024-byte cap, primary/secondary DA, DSR `6n`
and `5n`, `replies_dropped`, discard on resync.
**Mutations:** `no_reply`, `unbounded_replies`.

### Slice 7 — clusters and the width marker

**Spec:** SPEC-VT-SCREEN §9, ADR 0023.
**Gates:** T-CHIP1-GRAPHEME, T-CHIP1-WIDTH-DEFERRED.
**Work:** bounded cluster append, `RILL_GRAPHEME_MAX = 32`,
`grapheme_truncated`, and the gate that documents the one-column-per-scalar miss.
**Mutations:** `fixed_grapheme_buf`, `wide_advances_two`.

### Slice 8 — bounds, differential, fuzz

**Spec:** SPEC-VT-PARSER §6, §9; SPEC-VT-CONFORMANCE §4, §7.
**Gates:** T-CHIP1-BOUNDS, T-CHIP1-DIFF.
**Work:** `vte` under `[dev-dependencies]`, the shared-sink differential with
divergence 1 applied as an explicit remap, the hostile-input gate with a
counting allocator, and a `cargo-fuzz` target for `feed`.
**Mutations:** `unbounded_osc`; plus every parser mutation must also turn
T-CHIP1-DIFF red — if one does not, the corpus is too small.

### Slice 9 — resync emit

**Spec:** SPEC-CHIP1 §2, SPEC-VT-CONFORMANCE §1.
**Gates:** T-CHIP1-RESYNC.
**Work:** `reset`, `repaint_bytes`, `resync_from_history` emitting VT bytes a
second instance can replay onto a matching grid (ADR 0012 D4). Replies produced
during replay are discarded and counted.
**Mutation:** emit empty, or emit only the `\x1b[2J\x1b[H` prefix.

The oracle is the second instance's grid, never the prefix this function
prepends itself — the tautology Chip 0's audit S2 removed.

## Ordering, in one line

Scaffolding → parser + screen → CSI → scroll/alt → colour → replies → clusters →
hardening → resync. Colour before replies because it moves the snapshot path;
scaffolding first so nothing lands ungated; resync last because it needs a
correct screen to emit from.

## CI

- `fast.yml` (Linux, no Zig): clippy and test `-p rill-vt-types -p vt-engine`,
  the `vte` differential, the wired negative controls, and `lint-planes.sh`.
- `fast.yml` MUST NOT gain `rill-chip0` (SPEC-CHIP0 §9).
- `gates.yml` is unchanged by M4. No T-CHIP1 gate closes a packaged gate, and
  none is T-NFR.
- Mutations compile only under `feature = "mutate"` and select via
  `RILL_MUTATE=<name>`, matching Chip 0.
- A gate that has never run in CI is not evidence (ADR 0002 D8). The S-VT
  numbers are research.

## Done

**M4 is done when:** `rill-vt-types` and `vt-engine` are on `main`; every
T-CHIP1 gate has been demonstrated red then green in `fast.yml`; the new lints
pass; and `rill-host` / `rilld` still do not depend on `vt-engine`.

**M4 does not require** character width. **M7 does.**

## M7 preconditions (not this milestone)

[#24](https://github.com/mahboobmonnamd/RILL/issues/24) MUST NOT start until all
of:

1. Every T-CHIP1 gate Proven.
2. **Character width implemented**, T-CHIP1-WIDTH Proven, ADR 0023 amended
   (ADR 0023 D4). Otherwise the swap regresses CJK against a live chip that
   handles it today.
3. **T-CHIP1-LOOK-ANSI Proven**, and packaged T-LOOK-ANSI / T-LOOK-CELL /
   T-SPLIT-LOOK still green after the swap
   ([#272](https://github.com/mahboobmonnamd/RILL/issues/272)).
4. The Chip 0 differential run on macOS with Zig, **confirming ADR 0020 D3's
   inference** that libghostty-vt paints rather than executes decoded C1
   scalars. That inference is currently unmeasured.
5. An Accepted live-swap ADR saying how the host drains `take_replies` onto
   ordinary `DATA` frames, and packaged T-NFR hid re-proven on battery without
   recutting the instrument.

## Risks

| Risk | Mitigation |
|---|---|
| We own parser bugs now | `vte` differential over the corpus, plus fuzzing `feed` |
| Width deferral quietly becomes permanent | T-CHIP1-WIDTH-DEFERRED plus ADR 0023 D4 blocking M7 |
| ADR 0020 D3's C1 inference is wrong | M7 precondition 4; T-CHIP1-C1 pins our own behaviour meanwhile |
| The differential drifts into bug-for-bug matching | Secondary-only rule, divergence register, spec wins (ADR 0020 D2) |
| A theme constant creeps into Rust | `no-theme-rgb-in-rust` lint, and gates parse the fixture files |
| Accidental host dependency before M7 | `no-host-dep-on-vt-engine` lint |

## Issues to file

Nine slice issues under [#6](https://github.com/mahboobmonnamd/RILL/issues/6),
one per slice above, each naming its spec, gates, mutations and non-goals per
[CONTRIBUTING](../CONTRIBUTING.md). Plus:

- Close [#21](https://github.com/mahboobmonnamd/RILL/issues/21) with a comment on
  #6 recording the pick, the C1 policy, and the three spec defects found.
- Drop `blocked` from [#271](https://github.com/mahboobmonnamd/RILL/issues/271);
  ADR 0021 unblocks it.
- Update [#267](https://github.com/mahboobmonnamd/RILL/issues/267): the colour
  ADR it required is ADR 0021.
- New: the **width slice** (M7 precondition) with its own spike for the width
  data source, which ADR 0023 D5 deliberately left open.
- Add width and the Chip 0 C1 differential to
  [#24](https://github.com/mahboobmonnamd/RILL/issues/24)'s blocked-on list.
