# Parallel lanes

Five people can work without sharing a paste of another repository. Contracts
are the files in `docs/`. Spike 0 is Proven
([ADR 0010](adr/0010-spike-0-closes.md)). Milestone 1 first slice is Proven
([ADR 0014](adr/0014-m1-first-slice-closes.md), persist remainder
[ADR 0015](adr/0015-m1-persist-remainder.md)). Chip 1 isolated crate is M4
([ADR 0012](adr/0012-chip1-isolated-vt.md), [M4-HANDOFF](M4-HANDOFF.md),
[M4-PLAN](M4-PLAN.md), [#6](https://github.com/mahboobmonnamd/RILL/issues/6));
its parser, colour, reply and width decisions are ADRs
[0020](adr/0020-chip1-parser-in-tree.md)–[0023](adr/0023-chip1-v0-defers-character-width.md)
after [SPIKE-VT](SPIKE-VT.md), width source [ADR 0035](adr/0035-chip1-character-width.md)
after [SPIKE-WIDTH](SPIKE-WIDTH.md). Merge of chrome must
not hide a later NFR miss. Chrome is M2. Chip 1 is not the live chip until M7,
and width (ADR 0035 D7) is one of M7's preconditions. ADR 0053 adds a binding
hold: host checkpoint compatibility and disposable-client reconciliation must
be specified and demonstrated red before live-swap implementation resumes.

Lanes are named after the plane they own. The GitHub label is the first column.

| Label | Owns | May start before Spike 0 Proven? | Must not |
|---|---|---|---|
| **`lane:kernel`** | PTY spawn/reap, sole writer, byte ring, no drop | Yes (Rust library + tests) | Paint; JSON cells |
| **`lane:attach`** | Frame codec, credit, resize/exit/attach-id | Yes (codec + fuzz tests) | Naked `read`/`write`; seqpacket |
| **`lane:chip0-ghostty-vt`** | **Chip 0 (live emulator):** `libghostty-vt` adapter + our POD Metal, `feed(bytes)` | Yes (in-process, fake PTY) | Ghostty FFI in domain types; per-cell `String` |
| **`lane:host`** | One `NSWindow`, wire kernel + attach + Chip 0, packaged tests | After kernel / attach / Chip 0 have failing named tests | Hide an NFR miss with chrome |
| **`lane:chip1-vt-engine`** | **Chip 1 (isolated VT engine):** own crate, bytes in / snapshots out. Not the live chip. | Yes, **never linked** as live chip until M7 | PTY, GUI, Blocks dump |

Chip 0 is what the window paints today. Chip 1 is a later in-tree VT and must not replace Chip 0 until **M7** (Accepted live-swap ADR + packaged T-NFR). M4 is the isolated crate.

The internal ownership boundaries named by ADR 0053 are not new deployment
lanes by default. They begin inside the existing kernel, attach, VT and host
lanes and become crates only after a separate boundary spec proves the need.

Issues must set `lane:` and `milestone:`. A PR that crosses two lanes needs both owners on review.

Retired slugs (GitHub renamed in place): `lane:A` → `lane:kernel`, `lane:B` → `lane:attach`, `lane:C` → `lane:chip0-ghostty-vt`, `lane:D` → `lane:host`, `lane:E` → `lane:chip1-vt-engine`.

## Merge rules

1. Every PR names the plane, the ADR, and the test ID from [SPIKE-0](SPIKE-0.md) or a later spec.
2. Red test first for behavior. A PR that only turns a test green that would have passed before is rejected.
3. `main` stays shippable toward Spike 0. Feature flags that hide a broken data plane are rejected.
4. Chip 1 may live under `crates/vt-engine` but the host must not call it until **M7**. GitHub Issues are the only tracker.

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

Every lane preserves the existing PTY, attach, raw VT, Chip 0/Chip 1 and Metal
foundations while working through this order.
