# Parallel lanes

Five people can work without sharing a paste of another repository. Contracts are the files in `docs/`. Merge to `main` is gated by Spike 0 for anything user-visible.

| Lane | Owns | May start before Spike 0 Proven? | Must not |
|---|---|---|---|
| **A — Kernel** | PTY spawn/reap, sole writer, byte ring, no drop | Yes (Rust library + tests) | Paint; JSON cells |
| **B — Attach** | Frame codec, credit, resize/exit/attach-id | Yes (codec + fuzz tests) | Naked `read`/`write`; seqpacket |
| **C — Chip 0** | `libghostty-vt` adapter, POD Metal, `feed(bytes)` | Yes (in-process, fake PTY) | Ghostty FFI in domain types; per-cell `String` |
| **D — Host shell** | One `NSWindow`, wire A+B+C, packaged tests | After A/B/C have failing named tests | Chrome, SwiftUI Observation of PTY bytes |
| **E — Chip 1 crate** | Isolated VT library, bytes in / snapshots out | Yes, **never linked** as live chip until Spike 0 Proven | PTY, GUI, Blocks dump |

Issues must set `lane:` and `milestone:`. A PR that crosses two lanes needs both owners on review.

## Merge rules

1. Every PR names the plane, the ADR, and the test ID from [SPIKE-0](SPIKE-0.md) or a later spec.
2. Red test first for behavior. A PR that only turns a test green that would have passed before is rejected.
3. `main` stays shippable toward Spike 0. Feature flags that hide a broken data plane are rejected.
4. Chip 1 may live under `crates/vt-engine` but `D` must not call it until Milestone 4 opens.
