# Agent contract

This is production software. Fortune-scale teams will run real PTYs and secrets on it.

1. **Sequence.** Spike = research. After a spike, and for any development: **ADR → spec → test cases → implementation → integration/e2e.** Do not implement first. Proposed ADRs do not authorize code.
2. **Accepted ADR + GitHub issue** before behavior. One issue, one lane ([LANES](docs/LANES.md)).
3. **Named test first** for the intended failure. The test name states the
   **requirement**; the doc comment states the bug it was born from
   ([ADR 0002](docs/adr/0002-falsifiable-evidence.md) D6).
3a. **Demonstrated red.** No test counts as evidence until it has been observed
   failing on a build where the behaviour is absent, with the failure output in
   the PR (ADR 0002 D2). Every behavioural test declares a **required mutation**
   it must detect (D3).
3b. **No self-referential oracles.** Do not assert on a buffer the code under
   test copied your input into, on a constant it prepended, on a predicate
   hardcoded to the passing value, or on a grep the format cannot satisfy
   (D4). The oracle must be downstream of the mechanism.
3c. **A skip is a failure.** A test whose preconditions are absent fails; it
   does not return green (D5).
4. **Fail closed.** `Result` on library/daemon paths. No secret fail-open.
5. **Planes.** Warm path is attach frames + Chip 0. JSON is orchestration only. No cells over IPC. No per-cell `String` snapshots. No `SOCK_SEQPACKET`. No `SCM_RIGHTS` of the PTY master. No GUI spawn of the user shell.
6. **Spike 0.** [SPIKE-0](docs/SPIKE-0.md) is Proven ([ADR 0010](docs/adr/0010-spike-0-closes.md)). M1 first slice is Proven ([ADR 0014](docs/adr/0014-m1-first-slice-closes.md)). Do not add agents, Blocks, or chrome to hide a later NFR miss. Chip 1 stays isolated until **M7** ([ADR 0012](docs/adr/0012-chip1-isolated-vt.md), [M4-HANDOFF](docs/M4-HANDOFF.md)).
7. **Do not reference another product tree.** This repository is the source of truth. If a contract is missing, write an ADR here.
8. **Close** only with named tests, packaged e2e where user-visible, commands, and results in the PR. Socket-only tests do not close persist, paint, spawn, or NFR-KEY. A gate that has never run in CI is not evidence, whatever a laptop printed (ADR 0002 D8).
9. **Pins are deliberate.** libghostty-vt moves only in its own PR with the full
   gate suite re-run (ADR 0002 D7). Never as a side effect of a clean checkout.

Handoff: `ADR → spec → test cases → implementation → integration/e2e`
