# Spike — Chip 1 parser pick and v0 sequence subset

**Status: research. 2026-08-18.** Records the pick required by
[ADR 0012](adr/0012-chip1-isolated-vt.md) D6 before the first CSI parser PR.
**Research does not authorize `vt-engine` behaviour.** No gate below is
evidence: nothing here has been demonstrated red on a build where the
behaviour is absent, and none of it has run in CI ([ADR 0002](adr/0002-falsifiable-evidence.md)
D2, D8).

**Issue:** [S-VT #21](https://github.com/mahboobmonnamd/RILL/issues/21).
Parent epic [#6](https://github.com/mahboobmonnamd/RILL/issues/6).
Handoff: [M4-HANDOFF](M4-HANDOFF.md). Spec: [SPEC-CHIP1](spec/SPEC-CHIP1.md).

## Question

ADR 0012 D6 requires the owned grid and POD types but leaves the **byte
parser** open: in-tree, or the `vte` crate (parser only)?

Four sub-questions had to be answered with observations rather than taste:

1. Does either candidate satisfy the **v0 fixtures** — specifically, can
   T-CHIP1-BYTES pass, and can its required mutation `drop_high_bytes` turn it
   red (ADR 0002 D3)?
2. Does `feed` allocate proportionally to input length (ADR 0012 D9)?
3. Is either unbounded on hostile output (unterminated OSC/DCS, param floods)?
4. Is parse throughput a differentiator?

## Method

Harness at `.build/svt` (gitignored; research only, never a workspace member).
One throwaway screen implements an `Actions` sink. Two fronts drive that **same**
screen, so an observed difference is a difference in the *parser*, not in two
screens:

- **Front A** — `vte` 0.15.0, `default-features = false` (parser only).
- **Front B** — a hand-written DEC/ECMA-48 byte state machine (ground, ESC,
  ESC-intermediate, CSI entry/param/intermediate/ignore, OSC, DCS, SOS/PM/APC,
  incremental UTF-8), bounded exactly as `vte` bounds itself.

Allocation measured with a counting `GlobalAlloc` around `feed` only.
`rustc 1.97.1`, aarch64-apple-darwin, release.

```
cd .build/svt && cargo run --release
```

Chip 0 was **not** run as a comparison: it needs Zig and libghostty-vt, which
are not installed in this environment. Every claim below about Chip 0 is marked
as inference, not measurement.

## Result 1 — `vte` routes C1 to `execute()`, and that breaks T-CHIP1-BYTES

`vte` 0.15 `src/lib.rs:630-644` and `:725` dispatch a byte in `0x80..=0x9f` —
whether it arrived as invalid UTF-8 or as a decoded scalar — to
`Perform::execute()`, i.e. as an 8-bit control. It never becomes a cell.

Verdict = T-CHIP1-BYTES exactly as [TEST-CASES](TEST-CASES.md) writes it
("high bytes produce a non-ASCII cell except CSI-high-param"):

| fixture | vte (as-is) | vte + C1→U+FFFD | vte + C1→scalar | own (C1 execute) | own (C1 print) |
|---|---|---|---|---|---|
| `lone_continuation` | **FAIL** | PASS | PASS | PASS | PASS |
| `truncated_3byte` | PASS | PASS | PASS | PASS | PASS |
| `overlong_slash` | PASS | PASS | PASS | PASS | PASS |
| `lone_surrogate` | PASS | PASS | PASS | PASS | PASS |
| `bom_then_high` | PASS | PASS | PASS | PASS | PASS |
| `csi_high_param` | PASS | PASS | PASS | PASS | PASS |
| `c1_in_utf8` | **FAIL** | PASS | PASS | **FAIL** | PASS |
| `zwj_emoji.bin` | PASS | PASS | PASS | PASS | PASS |
| `invalid_utf8.bin` | PASS | PASS | PASS | PASS | PASS |

`lone_continuation` = `[0x80, 0x41]` under `vte`: `print=1 execute=1
executed=[0x80] row0=[0x41]`. The high byte reached the parser and was
*consumed as a control*. Nothing distinguishes that grid from the grid where
the byte was dropped.

**Both candidates are viable, but neither is viable on its defaults.** The C1
policy is ours to state, in this tree, either way:

- `vte` needs a remap in our own `Perform::execute` (about six lines).
- The in-tree parser needs the same policy chosen explicitly.

## Result 2 — the required mutation is blind on some fixtures

`drop_high_bytes` (filter `>= 0x80` before the parser) compared live grid vs
mutant grid, per fixture:

| fixture | vte (as-is) | vte + C1→U+FFFD | own (C1 print) |
|---|---|---|---|
| `lone_continuation` | **BLIND** | detected | detected |
| `truncated_3byte` | detected | detected | detected |
| `overlong_slash` | detected | detected | detected |
| `lone_surrogate` | detected | detected | detected |
| `bom_then_high` | detected | detected | detected |
| `csi_high_param` | **BLIND** | **BLIND** | **BLIND** |
| `c1_in_utf8` | **BLIND** | detected | detected |
| `zwj_emoji.bin` | detected | detected | detected |
| `invalid_utf8.bin` | detected | detected | detected |

Two findings for the spec, independent of the pick:

- `csi_high_param` (`ESC [ 0x80 m A`) is blind for **every** candidate: a high
  byte inside a CSI parameter changes no cell whether it is parsed or dropped.
  TEST-CASES must not imply that fixture carries the mutation. The gate stays
  honest because other fixtures detect it, but the claim must be per-fixture.
- Under `vte` defaults, three of nine fixtures are blind. A gate that green on
  those alone would satisfy nobody.

## Result 3 — neither allocates on `feed`; both bounded on hostile input

| measurement | vte | own |
|---|---|---|
| `feed(4 KiB)` | 0 B, 0 allocs | 0 B, 0 allocs |
| `feed(1 MiB)` | 0 B, 0 allocs | 0 B, 0 allocs |
| OSC 8 MiB unterminated | 30720 B / 1 alloc | 30720 B / 1 alloc |
| DCS 8 MiB unterminated | 30720 B / 1 alloc | 30720 B / 1 alloc |
| CSI with 1M params | 30720 B / 1 alloc | 30720 B / 1 alloc |

The single 30720 B allocation is the 80×24 grid at construction, not `feed`.
ADR 0012 D9's no-proportional-allocation clause is satisfiable by both.
`vte` bounds with `MAX_INTERMEDIATES=2`, `MAX_OSC_PARAMS=16`,
`MAX_OSC_RAW=1024`, `partial_utf8: [u8; 4]`; the in-tree front was written to
the same bounds.

## Result 4 — throughput is not a differentiator

8 MiB of mixed text, SGR, CUP and CRLF, parse **and** screen write:

| front | throughput |
|---|---|
| vte | 15.9 MiB/s |
| own | 15.8 MiB/s |

0.6% apart. The screen write dominates; the byte state machine is noise. This
is the throwaway screen, so the absolute number means nothing — the *ratio* is
the finding. **Performance does not choose the parser.**

## Result 5 — the two agree on the whole v0 subset

22 of 22 cases agree on cells **and** cursor: `ascii`, `crlf`, `cup`,
`sgr_bold`, `sgr_256`, `sgr_truecolor`, `sgr_colon_sub`, `ed2`, `el`,
`alt_1049`, `decsc_rc`, `ind_nel_ri`, `tab`, `wrap`, `osc_title`, `osc_st`,
`dcs_skip`, `cursor_hide`, `combining`, `zwj`, `cancel_can`, `csi_ignore`.

Two consequences. The pick is **reversible**: either front can be swapped
behind `Actions` without moving the gates. And `vte` is available as an
independent **differential oracle** in tests without being a runtime
dependency.

## Result 6 — neither candidate supplies character width

`vte` hands us `char`, one `print` per scalar. `'日本X'` produced three prints
and `cursor_col = 3`; a real terminal advances 5 columns. Neither candidate
carries an East Asian Width table, and neither does grapheme clustering.

libghostty-vt did both for Chip 0 (`RILL_GRAPHEME_MAX = 32`,
`grapheme_truncated`). **This is a cost Chip 1 pays no matter which parser it
picks, and SPEC-CHIP1 does not currently specify it.** It is the largest
unpriced item in M4 and needs its own decision and gate; it is not a parser
question.

## Result 7 — SPEC-CHIP1 requires a reply the API cannot deliver

[SPEC-CHIP1](spec/SPEC-CHIP1.md) §3: "DA / DSR `6n`: MUST answer. A TUI that
hangs on DA is a v0 miss." The §2 trait is `feed`, `resize`, `snapshot`, all
returning `Result<(), Error>` or a grid. **There is no channel on which an
answer can leave the crate.** A `vim` that queries DA and waits would hang
against the contract as written. This is an API gap in the spec, found before
any code, and it is independent of the parser pick.

## Dependency facts, if `vte` were taken at runtime

| | |
|---|---|
| Version | 0.15.0, `default-features = false` |
| Transitive | `arrayvec` 0.7.8, `memchr` 2.8.3 (2 crates) |
| With `ansi` feature | additionally `bitflags`, `cursor-icon`, `log` — not needed; parser only |
| License | `Apache-2.0 OR MIT` — compatible with this workspace's `MIT OR Apache-2.0` |
| MSRV | 1.62.1, under this tree's 1.85 |
| Parser size | 607 code lines (`lib.rs` before tests) + 95 (`params.rs`) |
| In-tree equivalent | 335 code lines for the v0 subset, agreeing 22/22 |

Today the whole workspace depends on `libc`, `serde`, `toml`, `cc`. The attach
frame codec is hand-written with zero dependencies.

## Recommendation

**Write the parser in-tree. Take `vte` as a `dev-dependencies` differential
oracle only.**

1. The parser is 335 lines and agrees 22/22 with `vte` on the subset we
   specified. The expensive half of a VT is the screen — wrap, scroll regions,
   alt buffer, damage, clusters, width — and `vte` supplies none of it.
2. M4 exists to stop living on someone else's release cadence
   (ADR 0012 Context). A runtime parser dependency re-imports that cadence
   into the one crate whose purpose is to remove it.
3. We do not inherit "just use the library" semantics anyway: `vte`'s C1
   routing must be overridden for our own fixtures to pass. Owning the policy
   in one state machine is clearer than owning it in a `Perform` shim that
   contradicts the crate underneath.
4. `vte` as a dev-dependency keeps the upside. It is an independent
   implementation, so a differential gate over the named fixture corpus is a
   legitimate secondary oracle — and unlike the Chip 0 differential, it runs
   on Linux in `fast.yml` with no Zig. It must stay secondary: the spec wins
   when they disagree, per ADR 0012's rejection of bug-for-bug matching.
5. Throughput does not argue either way (Result 4), and both are equally
   bounded (Result 3).

**C1 policy:** raw `0x80..=0x9f` bytes are invalid UTF-8 and become **one
U+FFFD**; a *decoded* U+0080–U+009F scalar prints as itself. This is
`own (C1 print)`, the only column that passes all nine fixtures and detects
the mutation on eight. Inference, not measurement: Chip 0's `t_bytes` gate
asserts a non-ASCII cell for `c1_in_utf8` and is green in CI, so libghostty-vt
must also paint rather than execute these — the recommended policy keeps
Chip 1 consistent with the live chip. **Confirm with Zig installed before the
colour/parser PR lands.**

## What this spike did not do

- Run Chip 0, or any differential against libghostty-vt (no Zig here).
- Measure a real screen. The harness screen is throwaway; only ratios are cited.
- Decide width or clustering (Result 6) — that needs its own decision.
- Decide the reply channel (Result 7) — that needs its own decision.
- Write `vt-engine`, or any workspace member. `.build/svt` is gitignored.
- Produce evidence. No gate here has been demonstrated red, and none ran in CI.

## Follow-on

Close #21 with a comment on #6 recording: parser in-tree, `vte` dev-only,
C1 policy, and the three spec defects this spike found (blind
`csi_high_param` mutation, unspecified width/clustering, missing reply
channel).
