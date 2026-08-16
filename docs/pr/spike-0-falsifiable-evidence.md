# Spike 0: revoke unearned evidence, make every gate falsifiable

Follows `.github/PULL_REQUEST_TEMPLATE.md`. Paste this as the PR body when
pushing `spike-0/falsifiable-evidence`.

## Summary

- **Plane:** all four
- **ADR:** [0002](../adr/0002-falsifiable-evidence.md) (new, Accepted),
  [0003](../adr/0003-display-pipeline.md) (new, Accepted). ADR 0001 unchanged.
- **Lane:** A, B, C, D
- **Test ID:** all nine, plus the new negative controls

Spike 0 was recorded as eight-of-nine `Proven`. Three of those gates were
structurally incapable of failing, and five more had names asserting behaviour
their bodies never exercised. This PR revokes every `Proven` mark, adds the
rules that would have caught it, and rewrites the gates so they can say no.

Full findings: [SPIKE-0-AUDIT](../SPIKE-0-AUDIT.md).

### The three that could not fail

| Gate | What it did |
|---|---|
| T-SPAWN | `nm -U` lists **defined** symbols. `_forkpty` and friends can only appear as **undefined** imports. The command excluded exactly the set the assertion inspected. |
| T-NFR | Searched the whole grid for `'a' + i%26`, which the shell had echoed there 26 keys earlier. Exited before any PTY round trip. `p95=0.032ms` was `Instant::now()` overhead. |
| control_rpc | `is_control_rpc()` was `{ false }` and never called. The reported value grepped a binary tag+length stream for `pane_replay`. |

### What is not changing

ADR 0001 stands. The four-plane split, sole-writer PTY, framed `SOCK_STREAM`,
and kernel-owned byte ring are correct. The Chip 0 C adapter is real — verified
against upstream headers at `26df373`, parameter for parameter. `persist_e2e.rs`
is the one test in the tree that earned its name and is now the model for the
rest.

## Sequence

- [x] ADR Accepted — 0002 and 0003, both in this PR
- [x] Spec — [KERNEL](../spec/SPEC-KERNEL.md), [ATTACH](../spec/SPEC-ATTACH.md), [CHIP0](../spec/SPEC-CHIP0.md), [DISPLAY](../spec/SPEC-DISPLAY.md)
- [x] Test cases — [TEST-CASES](../TEST-CASES.md), each with an oracle and a required mutation
- [x] Implementation — four blocking defects, rewritten gates, new renderer
- [ ] **Integration / e2e — NOT RUN. See "What this PR does not claim."**

## Test plan

| Check | Status |
|---|---|
| `sh scripts/lint-planes.sh` | **Run, passes.** Also verified to fail under four injected violations (qualified `RawFd` export, bare `RawFd` export, `SOCK_SEQPACKET`, `expect` on a daemon path) |
| Shell syntax, workflow YAML, internal doc links | Run, all pass |
| `cargo fmt` / `clippy` / `cargo test` | **Not run** — no Rust toolchain in the authoring environment |
| `sh scripts/validate-spike0.sh` | **Not run** — needs macOS, Zig, a display |
| `Rill --nfr-key=hid` | **Not run** — needs a Mac on battery with Accessibility trust |

## What this PR does not claim

Per [ADR 0002 D2](../adr/0002-falsifiable-evidence.md) and AGENTS.md §8, this PR
**closes no gate**. Spike 0 remains **Red**. No `Proven` mark is added anywhere.

The Rust and Objective-C in this branch has not been compiled. `TerminalView.m`
is a new Metal renderer written without a compiler and should be treated as a
first draft: expect errors in the shader string, the `simd` struct layout
matching the Metal `Instance` struct, and the atlas bearing math.

Merging this is a decision to adopt the **rules and the corrected gates**, not
an assertion that anything passes. The first honest gate run happens after this
lands.

## Blocking defects fixed here

| ID | Defect |
|---|---|
| S3-1 | **Stack buffer overflow** in the Chip 0 grapheme path, reachable from any process writing to the PTY |
| S3-2 | `EXIT` destroyed on detach — FR-EXIT failing on the persist path Spike 0 exists to prove |
| S3-3 | `Pty::drop` killing the child, so any daemon error path destroys the user's shell |
| S3-4 | PTY master fd exported from the kernel crate, against ADR 0001 §5 |
| S4-1 | No CI at all |
| S4-2 | libghostty-vt unpinned against an API upstream calls unstable |

## Expected first results

Two tests should fail immediately with **no mutation applied**:

- `t_exit_across_detach_is_delivered_to_the_reattaching_client`
- `t_attach_a_bare_connection_cannot_displace_the_attached_client`

Their red is the first real evidence this repository will have produced. Capture
it before applying the fixes in commit 3.

## Review order

1. [SPIKE-0-AUDIT](../SPIKE-0-AUDIT.md) — the findings
2. [ADR 0002](../adr/0002-falsifiable-evidence.md) — is "demonstrated red" a cost the team will actually pay?
3. [ADR 0003](../adr/0003-display-pipeline.md) — is a full renderer the right scope for Spike 0, or should paint move to Spike 1?
4. [TEST-CASES](../TEST-CASES.md) — does each required mutation actually correspond to a way the system breaks?
5. The code

## Explicitly not in this PR

- Any gate closure, any `Proven` mark, any evidence artifact
- A macOS CI runner (required before `gates.yml` can enforce anything)
- Colour emoji, full IME, mouse, selection, scrollback UI
- Agents, Blocks, chrome, Chip 1 as the live chip
