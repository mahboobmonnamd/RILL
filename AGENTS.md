# Agent contract

This is production software. Fortune-scale teams will run real PTYs and secrets on it.

1. **Sequence.** Spike = research. After a spike, and for any development: **ADR → spec → test cases → implementation → integration/e2e.** Do not implement first. Proposed ADRs do not authorize code.
2. **Accepted ADR + GitHub issue** before behavior. One issue, one lane ([LANES](docs/LANES.md)).
3. **Named test first** for the intended failure. User-reported bugs: the test name states the bug.
4. **Fail closed.** `Result` on library/daemon paths. No secret fail-open.
5. **Planes.** Warm path is attach frames + Chip 0. JSON is orchestration only. No cells over IPC. No per-cell `String` snapshots. No `SOCK_SEQPACKET`. No `SCM_RIGHTS` of the PTY master. No GUI spawn of the user shell.
6. **Spike 0 stop rule.** If [SPIKE-0](docs/SPIKE-0.md) is not butter, do not add agents, Blocks, or chrome to hide it.
7. **Do not reference another product tree.** This repository is the source of truth. If a contract is missing, write an ADR here.
8. **Close** only with named tests, packaged e2e where user-visible, commands, and results in the PR. Socket-only tests do not close persist, paint, spawn, or NFR-KEY.

Handoff: `ADR → spec → test cases → implementation → integration/e2e`
