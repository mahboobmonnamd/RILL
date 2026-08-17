# Contributing

## Sequence

A spike is research. It does not validate production behavior.

After a spike, and for any development, in order:

1. **ADR** (Accepted)
2. **Spec** (GitHub issue: plane, lane, ADR, named test IDs, what we will not do)
3. **Test cases** (red first; user-reported bugs named as the bug)
4. **Implementation** (smallest change)
5. **Integration / e2e** (packaged app for persist, paint, spawn, NFR-KEY)

A PR that implements without 1–3 is rejected.

## Tracker

GitHub Issues and Milestones are the only work tracker. Do not add beads.
Spike 0 is Proven ([ADR 0010](docs/adr/0010-spike-0-closes.md)). Chip 1
handoff: [M4-HANDOFF](docs/M4-HANDOFF.md). An issue is not a license to
hide a later NFR miss with chrome.

Required on every issue:

- Milestone
- `lane:` (`kernel` / `attach` / `chip0-ghostty-vt` / `host` / `chip1-vt-engine`, see [LANES](docs/LANES.md))
- Plane (kernel / attach / display / orchestration)
- ADR (0001 or a new Accepted ADR)
- Named test ID
- What we will **not** do

## Pull requests

- Branch from `main`. Rebase; do not merge foreign history.
- Template below must be filled. Empty “Test plan” is rejected.
- CI must run the named test. Socket-only tests do not close UI gates.
- Two-lane PRs need both lane reviewers.

## TDD

Write a failing test for the intended reason, then the smallest change. Do not patch UI and add a test that would have passed before.

## License

Contributions are MIT OR Apache-2.0, at the recipient’s option, matching this tree.
