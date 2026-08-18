# RILL

A session operating system for terminals. macOS first. MIT OR Apache-2.0.

**Spike 0 is GREEN** ([ADR 0010](docs/adr/0010-spike-0-closes.md)). Kernel,
attach, Chip 0, and a packaged `Rill.app` exist. Every named gate is Proven:
library suite in [run 31993832263](https://github.com/mahboobmonnamd/RILL/actions/runs/31993832263),
T-NFR on packaged battery hid. The 2026-08-16 marks remain withdrawn; the
[audit](docs/SPIKE-0-AUDIT.md) is that day's defect list, closed by ADR 0010.

Milestone 1 first slice is **Proven** ([ADR 0014](docs/adr/0014-m1-first-slice-closes.md)).
Persist remainder is [ADR 0015](docs/adr/0015-m1-persist-remainder.md).
Do not add agents, Blocks, or chrome to hide a later NFR miss. Run `make gates`
for regression. Launch the window with `make run`.

## In one page

| Word | Meaning |
|---|---|
| **Kernel** | The session OS. Owns shells, IDs, journal, who may type where. Lives after you quit the window. |
| **PTY** | The Unix pipe to `zsh` / `vim` / `claude`. The kernel owns the master end. Nothing else writes it. |
| **Attach plane** | Framed bytes between the window and the kernel (keys in, output out). Not JSON. Not cells. |
| **UI / display chip** | What you see and type into. Lives in the window process. Dies on quit. Chip 0 = `libghostty-vt` + our GPU. Chip 1 = our emulator later, same traits. |
| **Scrollback** | Byte history in the **kernel**, not in the window. Quit does not destroy it. |
| **Orchestration** | Later: conversations, agents, scheduler. JSON and cold. Never on the typing path. |

```mermaid
flowchart TB
  subgraph window["Window process — dies on quit"]
    UI[Chrome: nav | Chip 0 | inspector]
    CHIP[Display chip: VT + GPU + keys]
  end
  subgraph attach["Attach plane"]
    FRAMES["SOCK_STREAM + tagged frames<br/>bytes, credit, resize, exit, attach-id"]
  end
  subgraph kernel["Kernel process — survives quit"]
    PTY[PTY master — sole writer]
    RING[Bounded byte history]
    GRAPH[Pane / conversation graph]
  end
  CHIP -->|keys| FRAMES
  FRAMES -->|output bytes| CHIP
  FRAMES --> PTY
  PTY --> RING
  GRAPH -.->|cold JSON| FRAMES
```

Warm typing never touches JSON, cell dumps, or the graph.

## Setup

```sh
make setup
make run
```

Installs or checks Rust (`rustc` ≥ 1.85), Zig (≥ 0.16, for Chip 0 / `libghostty-vt` only), and Xcode Command Line Tools. Then fetches Ghostty source as a build-time dep and builds the VT library — not Ghostty.app. `third_party/ghostty` is gitignored.

## Start here

1. [ADR 0010](docs/adr/0010-spike-0-closes.md) — Spike 0 is Proven
2. [PRD](docs/PRD.md) — what we ship and what we refuse
3. [Architecture](docs/ARCHITECTURE.md) — planes, contracts, diagrams
4. [ADR 0001](docs/adr/0001-session-operating-system.md) — the lock
5. [ADR 0002](docs/adr/0002-falsifiable-evidence.md) — what counts as evidence
6. [ADR 0003](docs/adr/0003-display-pipeline.md) — renderer and key→present
7. [Spike 0](docs/SPIKE-0.md) — named gates ([validation](docs/SPIKE-0-VALIDATION.md), [test cases](docs/TEST-CASES.md))
8. [SPIKE-0-AUDIT](docs/SPIKE-0-AUDIT.md) — historical: why 2026-08-16 was Red
9. [Specs](docs/spec/) — [kernel](docs/spec/SPEC-KERNEL.md), [attach](docs/spec/SPEC-ATTACH.md), [chip 0](docs/spec/SPEC-CHIP0.md), [display](docs/spec/SPEC-DISPLAY.md), [graph](docs/spec/SPEC-GRAPH.md), [chrome](docs/spec/SPEC-CHROME.md), [chip 1](docs/spec/SPEC-CHIP1.md) (umbrella over [types](docs/spec/SPEC-VT-TYPES.md), [parser](docs/spec/SPEC-VT-PARSER.md), [screen](docs/spec/SPEC-VT-SCREEN.md), [colour](docs/spec/SPEC-VT-COLOR.md), [reply](docs/spec/SPEC-VT-REPLY.md), [conformance](docs/spec/SPEC-VT-CONFORMANCE.md))
10. [ADR 0011](docs/adr/0011-session-graph.md) / [ADR 0014](docs/adr/0014-m1-first-slice-closes.md) / [ADR 0015](docs/adr/0015-m1-persist-remainder.md) — Milestone 1
11. [ADR 0018](docs/adr/0018-three-pane-host-chrome.md) — M2 three-pane chrome (one leaf)
12. [ADR 0012](docs/adr/0012-chip1-isolated-vt.md) / [M4-HANDOFF](docs/M4-HANDOFF.md) — isolated Chip 1 (not live)
13. [M4-PLAN](docs/M4-PLAN.md) — Chip 1 slice plan. [ADR 0020](docs/adr/0020-chip1-parser-in-tree.md) parser (in-tree; `vte` dev-only), [ADR 0021](docs/adr/0021-chip1-colour-identity.md) colour identity, [ADR 0022](docs/adr/0022-chip1-reply-channel.md) DA/DSR replies, [ADR 0023](docs/adr/0023-chip1-v0-defers-character-width.md) width deferred (blocks M7). Spike: [SPIKE-VT](docs/SPIKE-VT.md)
14. [Lanes](docs/LANES.md) — how 3–5 people work in parallel
15. [CONTRIBUTING](CONTRIBUTING.md) — issues, PRs, TDD

## License

[MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE-2.0), at your option.
The app does not require an account to type. Monetization is not specified in this tree.
