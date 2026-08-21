# Parallel lanes

Five people can work without sharing a paste of another repository. Contracts
are the files in `docs/`. Spike 0 is Proven
([ADR 0010](adr/0010-spike-0-closes.md)). Milestone 1 first slice is Proven
([ADR 0014](adr/0014-m1-first-slice-closes.md), persist remainder
[ADR 0015](adr/0015-m1-persist-remainder.md)). Chip 1 (`vt-engine`) is the live chip ([ADR 0054](adr/0054-chip0-retired.md)).

Lanes are named after the plane they own. The GitHub label is the first column.

| Label | Owns | May start before Spike 0 Proven? | Must not |
|---|---|---|---|
| **`lane:kernel`** | PTY spawn/reap, sole writer, byte ring, no drop | Yes (Rust library + tests) | Paint; JSON cells |
| **`lane:attach`** | Frame codec, credit, resize/exit/attach-id | Yes (codec + fuzz tests) | Naked `read`/`write`; seqpacket |
| **`lane:host`** | One `NSWindow`, kernel + attach + Chip 1, packaged tests | After kernel / attach have failing named tests | Hide an NFR miss with chrome |
| **`lane:chip1-vt-engine`** | **Chip 1 (live VT):** `vt-engine`, bytes in / snapshots out | Yes | PTY, GUI, Blocks dump |

Chip 0 / `libghostty-vt` is retired ([ADR 0054](adr/0054-chip0-retired.md)). `lane:chip0-ghostty-vt` is a historical label.

The internal ownership boundaries named by ADR 0053 are not new deployment
lanes by default. They begin inside the existing kernel, attach, VT and host
lanes and become crates only after a separate boundary spec proves the need.

Issues must set `lane:` and `milestone:`. A PR that crosses two lanes needs both owners on review.

Retired slugs (GitHub renamed in place): `lane:A` → `lane:kernel`, `lane:B` → `lane:attach`, `lane:C` → `lane:chip0-ghostty-vt`, `lane:D` → `lane:host`, `lane:E` → `lane:chip1-vt-engine`.

## Merge rules

1. Every PR names the plane, the ADR, and the test ID from [SPIKE-0](SPIKE-0.md) or a later spec.
2. Red test first for behavior. A PR that only turns a test green that would have passed before is rejected.
3. `main` stays shippable toward Spike 0. Feature flags that hide a broken data plane are rejected.
4. Chip 1 is the live chip. GitHub Issues are the only tracker.

## Architecture dependency order

Milestone numbers do not authorize work out of dependency order:

1. schema/authority, canonical configuration and privacy;
2. terminal and PTY compatibility;
3. host state, workers, checkpoints and leases;
4. semantic transcript and retention;
5. Flow projection with independent Raw fallback;
6. persistent topology;
7. Tasks and isolation;
8. structured attention and approvals;
9. artifacts and diffs; and
10. optional activity timeline.

Every lane preserves the existing PTY, attach, raw VT, Chip 1 and Metal
foundations while working through this order.

[SPEC-TERMINAL-PERFORMANCE](spec/SPEC-TERMINAL-PERFORMANCE.md) is cross-cutting:
no lane may waive ADR 0053 D22. Feature work that cannot meet T-NFR and the
T-PERF matrix stays disabled, deferred, off-path or redesigned.
