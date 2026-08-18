# ADR 0020: Chip 1 parses in this tree; `vte` is a test oracle only

- **Status:** Accepted — 2026-08-18
- **Tree:** this repository only
- **Issue:** [S-VT #21](https://github.com/mahboobmonnamd/RILL/issues/21),
  epic [#6](https://github.com/mahboobmonnamd/RILL/issues/6)
- **Requires:** [ADR 0012](0012-chip1-isolated-vt.md) (isolation),
  [ADR 0002](0002-falsifiable-evidence.md)
- **Amends:** [ADR 0012](0012-chip1-isolated-vt.md) D6 — that decision left the
  byte parser open and required a spike to record the pick. This is the pick.
  [SPEC-CHIP1](../spec/SPEC-CHIP1.md) §3's closing parser clause is superseded
  by [SPEC-VT-PARSER](../spec/SPEC-VT-PARSER.md).
- **Evidence:** [SPIKE-VT](../SPIKE-VT.md) — research, not a gate
- **Does not authorize:** Chip 1 as the live chip, a `rill-host` / `rilld`
  dependency on `vt-engine`, colour work ([ADR 0021](0021-chip1-colour-identity.md)),
  character width ([ADR 0023](0023-chip1-v0-defers-character-width.md)),
  `libghostty-vt` in Chip 1, full libghostty exec, Blocks

## Context

ADR 0012 D6 required the owned grid and POD types but deliberately left the
byte parser open: in-tree, or the `vte` crate (parser only). It required
[S-VT #21](https://github.com/mahboobmonnamd/RILL/issues/21) to record the pick
before the first CSI parser PR.

S-VT built one throwaway screen and drove it from both candidates, so an
observed difference is a difference in the parser and not in two screens
([SPIKE-VT](../SPIKE-VT.md)). Four results decide this ADR:

- **Neither candidate is viable on its defaults.** `vte` 0.15 dispatches any
  byte in `0x80..=0x9f` to `Perform::execute()` as an 8-bit control, so it never
  becomes a cell. `[0x80, 0x41]` yields `execute=[0x80] row0=[0x41]` — a grid
  indistinguishable from one where the byte was dropped. T-CHIP1-BYTES as
  written fails on `lone_continuation` and `c1_in_utf8`, and its required
  mutation `drop_high_bytes` is **blind** on three of nine fixtures, which
  ADR 0002 D3 does not permit. A remap in our own `Perform` fixes it. Either
  way the C1 policy is ours to state.
- **Throughput is not a differentiator:** 15.9 vs 15.8 MiB/s, 0.6% apart. The
  screen write dominates; the byte state machine is noise.
- **Both are equally bounded:** zero allocation in `feed` over 1 MiB, and
  bounded against 8 MiB unterminated OSC/DCS and a 1M-parameter CSI flood.
- **They agree on the whole v0 subset:** 22 of 22 cases on cells and cursor.

So the pick is not a performance or safety question. It is a question of what
this crate is for, and of where the C1 policy lives.

## Decision

### D1 — The byte parser is written in this tree

`vt-engine` contains its own state machine (ground, ESC, ESC-intermediate, CSI
entry/param/intermediate/ignore, OSC, DCS, SOS/PM/APC, incremental UTF-8).
Normative behaviour: [SPEC-VT-PARSER](../spec/SPEC-VT-PARSER.md).

M4 exists to stop living on an external VT's release cadence (ADR 0012
Context). A runtime parser dependency re-imports that cadence into the one
crate whose purpose is to remove it. The subset costs ~335 lines and agrees
22/22 with `vte`; the expensive half of a VT is the screen, and `vte` supplies
none of it.

### D2 — `vte` is a `dev-dependencies` differential oracle, and never ships

`vte` MAY appear in `crates/vt-engine/Cargo.toml` under `[dev-dependencies]`
only, `default-features = false`. It MUST NOT appear under `[dependencies]` or
`[build-dependencies]`, and MUST NOT be reachable from any shipped artifact.
`scripts/lint-planes.sh` gains `no-vte-at-runtime` (SPEC-VT-CONFORMANCE §5).

The differential is a **secondary** oracle over a named fixture corpus. Rules,
so this does not decay into bug-for-bug matching (which ADR 0012 rejected for
Ghostty):

- The primary oracle stays our own `PodGrid` against values written in the test
  (ADR 0012 D7). A gate MUST NOT be expressed only as "equals `vte`".
- Where the two disagree, **the spec wins** and the divergence is named in
  SPEC-VT-CONFORMANCE §4 with the reason. C1 handling (D3) is the first entry.
- The differential runs on Linux in `fast.yml` with no Zig. This is strictly
  more coverage than the Chip 0 differential, which needs macOS and the pinned
  Zig archive and therefore cannot run there (SPEC-CHIP0 §9).
- `vte` is not pinned by SHA. It is a test-only crate; ADR 0002 D7 pin
  discipline applies to `libghostty-vt`, which ships.

### D3 — C1 policy: invalid bytes become U+FFFD; decoded C1 scalars paint

v0 is a UTF-8 stream. There are no 8-bit control introducers.

- A byte in `0x80..=0x9f` that is not part of a valid UTF-8 sequence is
  **invalid UTF-8** and produces exactly **one** U+FFFD cell. It MUST NOT be
  dispatched as a control.
- A **decoded** scalar in U+0080..=U+009F prints as an ordinary cell.
  `0x9b` MUST NOT open a CSI and `0x9c` MUST NOT terminate a string in v0.
  7-bit `ESC [` and `ESC \` are the only forms honoured.
- Bytes `0xc0`, `0xc1`, `0xf5..=0xff`, overlongs, and surrogates each produce
  exactly one U+FFFD. A truncated sequence produces one U+FFFD and the
  interrupting byte is reprocessed, not swallowed.

This is not unique on the T-BYTES scoreboard. Two measured policies pass all
nine fixtures and keep the mutation detectable on eight: map every C1-range
result to U+FFFD, or paint a decoded C1 scalar as itself (invalid bytes still
U+FFFD). We take the second: it preserves the decoded scalar's identity, and
it matches the Chip 0 inference below. T-CHIP1-C1 is what distinguishes them.
**Inference on the record:** Chip 0's `t_bytes` gate asserts a non-ASCII cell
for `c1_in_utf8` and is green in CI, so
libghostty-vt must also paint rather than execute these; this policy therefore
keeps Chip 1 consistent with the live chip. That inference is **not measured
here** — no Zig in the S-VT environment. A differential against Chip 0 MUST
confirm it before [M7](https://github.com/mahboobmonnamd/RILL/issues/24), and
T-CHIP1-C1 is red until it is asserted in a test.

### D4 — Parsing is bounded and allocation-free

Fixed capacities, no growth on hostile input, no allocation in `feed`
(ADR 0012 D9). Normative values in SPEC-VT-PARSER §6. Overflow **discards and
sets the ignore flag**; it MUST NOT panic, reallocate, or silently truncate a
sequence into a different sequence.

### D5 — DCS, SOS, PM and APC are consumed and ignored

Bounded, no accumulation, no crash. Sixel, ReGIS and image protocols stay out
of v0 (ADR 0012 D5). A DCS body MUST NOT reach the screen as cells.

### D6 — Parser and screen are separate modules behind an action boundary

The state machine emits actions; the screen applies them. Neither names the
other's internals. This is what made S-VT's measurement possible and what keeps
D2's differential honest: both fronts drive the same screen. It also keeps the
pick reversible, as 22/22 agreement showed.

Library paths return `Result`. No `unwrap` / `expect` on reachable
`feed` / `resize` / `snapshot`. Not `Sync`.

### D7 — Mutation detectability is recorded per fixture, not per gate

S-VT found that `csi_high_param` (`ESC [ 0x80 m A`) is blind to
`drop_high_bytes` for **every** candidate: a high byte inside a CSI parameter
changes no cell whether it is parsed or dropped. [TEST-CASES](../TEST-CASES.md)
implied that fixture carries the mutation. It does not.

T-CHIP1-BYTES MUST name the fixtures that carry `drop_high_bytes`, and the
conformance table MUST record the blind ones as blind. A gate whose corpus is
only blind fixtures is not evidence, whatever it prints.

## Consequences

- The first CSI parser PR cites this ADR and S-VT, satisfying ADR 0012 D6.
- `fast.yml` gains `-p rill-vt-types -p vt-engine` (ADR 0012 D8) and runs the
  differential on Linux. `rill-chip0` never joins that job.
- SPEC-CHIP1 becomes an umbrella over SPEC-VT-TYPES, -PARSER, -SCREEN, -COLOR,
  -REPLY and -CONFORMANCE.
- We own parser bugs. The differential plus the fixture corpus is the mitigation.
- Nothing here makes Chip 1 live. Chip 0 remains the live chip until M7.

## Rejected alternatives

- **`vte` as a runtime dependency.** Rejected: it re-adds an external VT to the
  crate meant to remove one, and we would still override its C1 routing — the
  crate would deliberately contradict its own dependency's semantics. It is a
  good crate (Apache-2.0 OR MIT, MSRV 1.62, `arrayvec` + `memchr` only, no
  allocation, bounded); the objection is purpose, not quality.
- **No `vte` anywhere, not even in tests.** Rejected: a dev-dependency ships
  nothing, and refusing it gives up the only independent cross-check that runs
  in `fast.yml`. In a tree this careful about self-referential oracles
  (ADR 0002 D4) that is a real loss for no gain in shipped code.
- **"Matches `vte`" as the gate.** Rejected: same objection ADR 0012 raised to
  matching Ghostty bug-for-bug. Secondary only, named divergences, spec wins.
- **Take `vte`'s C1 behaviour as the spec.** Rejected: it makes T-CHIP1-BYTES
  fail and its required mutation blind on three fixtures.
- **`libghostty-vt` inside Chip 1, or full libghostty exec.** Rejected by
  ADR 0012 D6 and ADR 0001 §1.
