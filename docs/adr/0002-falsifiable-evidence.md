# ADR 0002: Falsifiable evidence

- **Status:** Accepted — 2026-08-16
- **Tree:** this repository only
- **Supersedes:** nothing. **Amends:** ADR 0001 §9 (adds evidence rules to the
  authorization sequence). ADR 0001's decisions 1–8 stand unchanged.
  **Amended by:** [ADR 0010](0010-spike-0-closes.md) (D11 satisfied 2026-08-17).
- **Evidence:** [SPIKE-0-AUDIT](../SPIKE-0-AUDIT.md)

## Context

Spike 0 was marked eight-ninths `Proven`. The audit found three of those gates
structurally incapable of failing, and five more whose names assert behaviour
their bodies never exercise.

The failure was not carelessness. Every one of those tests passes, is named
after a real requirement, and is wired into a validation script. The tree had no
mechanism to distinguish a test that passed because the system works from a test
that passed because it asks nothing. `T-NFR p95=0.032ms` was reported, written
into two documents, and used to argue Spike 0 was nearly closed — a number three
orders of magnitude below a PTY round trip, which should have been read as an
instrument fault and instead was read as a result.

The previous prototype died of a data-plane mistake that was visible in the
architecture. This tree very nearly died of an epistemics mistake that was
invisible in the architecture. ADR 0001 governs what we build. This ADR governs
what we are allowed to believe about it.

## Decision

### D1 — All prior `Proven` marks are revoked

Every gate in [SPIKE-0](../SPIKE-0.md) returns to **Red**. Spike 0 is Red. The
recorded `p95=0.032ms control_rpc=0` is a **null result**, not a partial one,
and must not be cited.

A gate returns to `Proven` only under D2–D6. Gates whose tests the audit did not
impeach still return to Red: their tests were never demonstrated red, so their
green is unevidenced rather than disproven.

### D2 — A gate requires a demonstrated red

No test may be marked as evidence for a gate until it has been observed
**failing on a build where the behaviour is absent**, and the failure output is
recorded in the PR.

`git stash` the fix and paste the failure. Ship the test before the
implementation. Either is acceptable; an unfalsified test is not.

### D3 — Every gate declares a mutation that must turn it red

Each gate in [TEST-CASES](../TEST-CASES.md) names a **required mutation**: a
specific, minimal change to the production code that the gate must detect. The
mutation is part of the gate's definition, reviewed like the assertion.

Where the mutation can be expressed as a build flag or environment variable, it
is wired as a **negative control** the harness executes automatically, asserting
the gate goes red. A gate with an automated negative control is worth more than
one without; gates that can carry one, must.

### D4 — Self-referential oracles are rejected

A test must not assert on a value the code under test produced solely for the
test's benefit. Specifically banned:

- Asserting on a buffer the tested function copied the input into
  (`Chip0.fed` — this is why that field exists, and it is deleted).
- Asserting on a constant the tested function prepended (`\x1b[2J`).
- Any predicate hardcoded to the passing value (`is_control_rpc() -> false`).
- Grepping a byte stream for a string the stream's format cannot contain.

The oracle must be **downstream of the mechanism** and independently
observable: the child process's own view, the packaged binary's link table, the
compositor's presentation timestamp, a second process's `kill(pid, 0)`.

### D5 — A skip is a failure

No test may return green when its preconditions are absent. If a gate needs a
packaged app, a battery, a TTY, or a pinned library and does not have it, it
**fails**. `RILL_REQUIRE_*` opt-in flags are deleted; the requirement is
unconditional and the harness supplies the precondition.

### D6 — Named tests state the fixed behaviour

A test name describes what the system does when correct. The bug it was born
from goes in the doc comment with its report, not in the name.

`t_quit_app_and_reload_does_not_persist_the_session` is renamed
`t_kill_gui_process_group_child_pid_survives_and_reattach_shows_prior_output`.
AGENTS.md §3 is amended accordingly: *the test name states the requirement; the
doc comment states the bug.*

### D7 — The emulator dependency is pinned

libghostty-vt is pinned to
`ghostty-org/ghostty@26df373ec83fb1cebb4fee0a8394144ae984a9b8` in
`third_party/ghostty.pin`. `scripts/fetch-libghostty-vt.sh` fetches that commit
and **verifies the checked-out SHA before building**, failing closed on
mismatch or on a pre-existing archive built from an unknown revision.

Upstream states the API "is definitely going to change." Moving the pin is a
deliberate act: its own PR, the full gate suite re-run, and the results recorded.
It is never a side effect of a clean checkout.

### D8 — CI enforces the gates

`.github/workflows/` gains:

- **`fast.yml`** — Linux, no Zig: `rill-attach` codec tests, `cargo fmt
  --check`, `clippy -D warnings`, and the repository invariant lints (D9). Runs
  on every push. Lane B and Lane A logic must not require Lane C's toolchain.
- **`gates.yml`** — self-hosted macOS: pinned libghostty-vt, packaged
  `Rill.app`, the full gate suite, negative controls, and evidence upload.

`main` is protected on `fast.yml`. `gates.yml` is required for any PR touching
`crates/` or `host/`. A gate that has never run in CI is not evidence, whatever
a laptop printed.

Until a macOS runner exists, `gates.yml` runs on `workflow_dispatch` and its
recorded evidence artifact — not a pasted terminal transcript — is what a PR
cites.

### D9 — Plane violations are lints, not review conventions

The prohibitions in AGENTS.md §5 become executable checks in
`scripts/lint-planes.sh`, run by `fast.yml`:

| Check | Rejects |
|---|---|
| no-master-export | any `pub` item in `rill-kernel` returning `RawFd`/`OwnedFd` |
| no-scm-rights | `SCM_RIGHTS` anywhere |
| no-seqpacket | `SOCK_SEQPACKET` anywhere |
| no-ghostty-in-domain | `ghostty_` outside `crates/rill-chip0/src/adapter/` |
| no-cell-strings | `String` in any type reachable from a POD snapshot |
| no-unwrap-in-daemon | `unwrap`/`expect`/`panic!` outside `#[cfg(test)]` in `rill-kernel`, `rill-attach`, `rilld` |
| no-json-on-warm-path | `serde_json` in the kernel, attach, or display dependency graph |

A lint that cannot be written yet is recorded as a TODO **in the lint script**,
where it is visible, not in a document.

### D10 — Evidence is a machine-readable artifact

`scripts/validate-spike0.sh` emits `evidence/spike0-<utc>.json`: per gate, the
command, exit status, captured stdout, the libghostty-vt SHA, the git SHA, the
host model, the macOS version, and power source. The human summary is rendered
**from** that file. No line in the summary may be printed without a
corresponding recorded result — the current script hardcodes `pass` for three
gates, one of which it never runs.

### D11 — The stop rule survives contact

ADR 0001's stop rule holds and is now measurable: Milestone 1 does not open
until every gate in [SPIKE-0](../SPIKE-0.md) is `Proven` under D2–D6, on a
packaged build, with T-NFR on battery per [ADR 0003](0003-display-pipeline.md).
**Record:** satisfied 2026-08-17 ([ADR 0010](0010-spike-0-closes.md)).

If the honest number misses, that is the spike succeeding. The prohibition on
adding agents, Blocks, or chrome to hide a miss extends explicitly to **adding
instrumentation that flatters the miss**.

## Consequences

- Spike 0's completion date moves right. The previous date was measuring nothing.
- Every gate is rewritten before any is re-marked. The audit's S1/S2 lists are
  the work queue.
- CI becomes a dependency of the project, not a nicety. A macOS runner is
  required for Spike 0 closure.
- Contributors carry a real cost: a demonstrated red per behavioural test. This
  is the cheapest defence against the failure that produced this ADR.
- `Chip0.fed`, `Frame::is_control_rpc`, `pty::leak_master_forbidden`, and the
  `RILL_REQUIRE_*` skip flags are deleted. They exist only to make tests pass.

## Rejected alternatives

- **Fix the three tests and keep the other marks.** Rejected: the other gates
  were never demonstrated red either. Their green is unevidenced, and the same
  process produced all nine.
- **Start the tree over.** Rejected: ADR 0001 is sound and the Chip 0 adapter is
  correct against the pinned upstream headers. A rewrite would re-derive them
  and would not address the epistemics failure, which is not in the code.
- **Mutation-testing framework (`cargo-mutants`) instead of D3.** Deferred, not
  rejected. Valuable for the pure-Rust crates, useless for the gates that matter
  most — packaged spawn, process-group kill, GPU presentation. D3's named
  mutation covers those; a framework can be added under Lane A later.
