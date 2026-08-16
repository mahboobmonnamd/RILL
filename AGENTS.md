# Agent contract

This is production software. Fortune-scale teams will run real PTYs and secrets on it.

1. **Accepted ADR + GitHub issue** before behavior. Proposed docs do not authorize code.
2. **Named test first** for the intended failure. User-reported bugs: the test name states the bug.
3. **Fail closed.** `Result` on library/daemon paths. No secret fail-open.
4. **Planes.** Warm path is attach frames + Chip 0. JSON is orchestration only. No cells over IPC. No per-cell `String` snapshots. No `SOCK_SEQPACKET`. No `SCM_RIGHTS` of the PTY master. No GUI spawn of the user shell.
5. **Spike 0 stop rule.** If [SPIKE-0](docs/SPIKE-0.md) is not butter, do not add agents, Blocks, or chrome to hide it.
6. **Do not reference another product tree.** This repository is the source of truth. If a contract is missing, write an ADR here.
7. **Issues.** GitHub Issues + milestones. One issue, one lane ([LANES](docs/LANES.md)). Close only with named tests and command results in the PR.

Handoff: `ADR + FR → files → named tests → commands/results → open gates`
