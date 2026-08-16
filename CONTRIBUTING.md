# Contributing

## Tracker

GitHub Issues and Milestones are the only work tracker. An issue is not a license to expand Spike 0.

Required on every issue:

- Milestone
- `lane:` (A–E, see [LANES](docs/LANES.md))
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
