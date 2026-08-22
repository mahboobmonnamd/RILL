# Stage 3 development plan

## Status
- Tracker: #331
- Slice issues: #349–#361
- Lane: `lane:kernel`
- Milestone: M6 — Blocks
- SPEC-CONTENT accepted for ADR 0053 D12 item 3 on 2026-08-22
- Implementation: all 13 Stage 3 gates green

## Goal
Establish the Stage 3 contract without breaking the warm path or live-swap work already guarded by M7 rules.

## Scope
Stage 3 is intentionally limited to D12 item 3:

- preserve source authority for terminal data
- keep offsets monotonic across ring eviction
- ensure checkpoint + delta recovery remains explicit and fail-closed
- keep the live PTY/VT path unchanged while the ledger model is defined
- keep Flow/Block projection and host/compositor work in Stage 4

## Acceptance criteria
- named T-CONTENT-* tests are red before implementation
- retained offsets remain monotonic after eviction
- `recovery()` and checkpoint flows remain honest even after ring compaction
- other warm-path invariants remain unchanged
- no Stage 3 work leaks into the M7 swap PR path

## Gate map

The test symbol is also the exact Cargo filter used by the CI negative control.

| Issue | Gate | Required mutation | Test symbol |
|---|---|---|---|
| #349 | T-CONTENT-MONOTONIC-OFFSETS | `snapshot_relative_offsets` | `t_content_monotonic_offsets_survive_ring_eviction` |
| #350 | T-CONTENT-RANGE-REQUIRES-STATE | `allow_range_without_state` | `t_content_range_requires_compatible_state` |
| #351 | T-CONTENT-SURVIVES-RING-EVICTION | `timeline_reads_hot_ring` | `t_content_materialized_output_survives_ring_eviction` |
| #352 | T-CONTENT-NO-PROMPT-HEURISTIC | `prompt_regex_creates_command` | `t_content_prompt_shape_does_not_create_command_boundary` |
| #353 | T-CONTENT-ALT-SAME-PTY | `alt_grid_becomes_timeline_item` | `t_content_alternate_screen_creates_no_timeline_item` |
| #354 | T-TRANSCRIPT-EVENT-IDEMPOTENCY | `duplicate_event_id_appends` | `t_transcript_event_append_is_idempotent_and_conflicts_fail_closed` |
| #355 | T-TRANSCRIPT-BYTE-EVENT-ORDER | `semantic_before_source_offset` | `t_transcript_terminal_ranges_follow_byte_order_and_generation` |
| #356 | T-CONTENT-RETENTION-DISABLED | `disabled_policy_writes_transcript` | `t_content_retention_disabled_writes_nothing_durable` |
| #357 | T-CONTENT-RETENTION-RESTRICTIVE-WINS | `closest_workspace_setting_wins` | `t_content_retention_policy_restrictive_wins` |
| #358 | T-CONTENT-REDACTION-DERIVED | `redactor_mutates_canonical_record` | `t_content_redaction_is_a_derived_sink` |
| #359 | T-CONTENT-TRUNCATION-VISIBLE | `omit_truncation_marker` | `t_content_timeline_marks_truncation_explicitly` |
| #360 | T-CONTENT-BOUNDED-RECOVERY | `unbounded_transcript` | `t_content_transcript_bound_fails_closed_with_cursor` |
| #361 | T-CONTENT-SOURCE-AUTHORITY | `scrape_cells_for_command_and_pass_count` | `t_content_source_authority_rejects_command_claim_from_pty` |

## Implemented boundaries

- absolute hot-ring offsets and fail-closed checkpoint recovery
- replay-state validation against stream origin or compatible checkpoint
- materialized terminal content independent of hot-ring eviction
- explicit primary/alternate screen materialization policy over the same PTY
- stable, idempotent semantic events with ordered generation/offset correlation
- separate bounded live timeline and policy-gated durable capture sink
- restrictive retention resolution, bounded ledger capacity and recovery cursor
- derived redaction, explicit discontinuity and visible truncation
- source provenance checks that reject PTY-inferred command claims

## Relevant references
- [docs/spec/SPEC-CONTENT.md](spec/SPEC-CONTENT.md)
- [docs/adr/0053-runtime-domain-content-and-client-authority.md](adr/0053-runtime-domain-content-and-client-authority.md)
- [docs/ARCHITECTURE.md](ARCHITECTURE.md)
- [docs/DEFERRED.md](DEFERRED.md)
- [crates/rill-kernel/src/checkpoint.rs](../crates/rill-kernel/src/checkpoint.rs)
- [crates/rill-kernel/src/session.rs](../crates/rill-kernel/src/session.rs)

## Local evidence

- all 13 required mutations were detected by their named tests
- all 13 negative controls are wired into `.github/workflows/fast.yml`
- `cargo clippy -p rill-attach -p rill-kernel --all-targets -- -D warnings`: green
- `cargo test -p rill-attach`: 16 passed
- `cargo test -p rill-kernel -- --test-threads=1`: 55 passed
- `cargo test -p rill-kernel --test gates`: 24 passed at default parallelism
- `scripts/lint-planes.sh`: green
- Stage 3 file formatting and `git diff --check`: green
- `make fast`: green

ADR 0002 D8 closure evidence is the merged PR #365 CI run, not the local results
above.

## Closure

PR #365 must merge with a green `fast` workflow that runs every row above both
normally and under its required mutation. That merge closes #349–#361 and
tracker #331. Stage 4 host/Flow work remains in #348.
