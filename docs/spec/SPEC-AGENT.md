# SPEC-AGENT — adapters, lifecycle authority, permissions (orchestration plane)

- **Status:** Accepted — 2026-08-18. Gates **Red** until demonstrated
  red-then-green (ADR 0002 D2).
- **Authority:** [ADR 0049](../adr/0049-agent-adapters-and-lifecycle-authority.md),
  constrained by
  [ADR 0053](../adr/0053-runtime-domain-content-and-client-authority.md) D1,
  D7, D10 and D12
- **Requires:** [SPEC-TASK](SPEC-TASK.md), [SPEC-TRUST](SPEC-TRUST.md),
  [SPEC-ATTENTION](SPEC-ATTENTION.md), [SPEC-FIDELITY](SPEC-FIDELITY.md)
- **Milestone:** M3 — Conversations

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. Adapter kinds

Three kinds, one contract: **replay** (reference, ships first), **structured**
(SDK/protocol), **CLI/PTY** (observed).

- Every adapter emits SPEC-TASK §3's closed section set.
- A surface MUST NOT branch on adapter kind. If it must, the section contract is
  incomplete and that is the bug.

## 2. Lifecycle authority

Ranked, highest first:

1. hooks / integrations the agent emits — authoritative
2. structured protocol from an SDK adapter — authoritative
3. process state (exists, exited, reading) — reliable, coarse
4. screen manifest matching — advisory only

- A lower rank MUST NOT override a higher one.
- With only rank 4 available and no match, the state MUST be **`unknown`**, and
  `unknown` MUST be visible as unknown. It MUST NOT be reported as `idle`.
- Screen manifests MUST be versioned, user-overridable config data. They MUST
  NOT be compiled-in regexes.

## 3. Observation cost

- Detection reads process state and Chip 0's existing parse output.
- It MUST NOT add a parser to the byte stream, MUST NOT copy the grid per frame,
  and MUST NOT sample faster than 2 Hz.
- An attached agent leaf MUST still meet NFR-KEY when the user types into it.

## 4. Writing to an agent

There MUST be exactly one code path that writes bytes a human did not type
(direct attach, CLI orchestration, agent-drives-TUI). It MUST:

- require an explicit granted capability (§5),
- hold the TerminalExecution's current input lease or not write,
- be visibly attributed while it holds the write,
- refuse when the target is not the task's declared target `NodeId`.

Agent-drives-TUI MUST additionally be per-Session opt-in, never a default, and
interruptible by explicit human lease takeover. No automated client can retain
or reacquire input merely because it had it before a disconnect.

CLI verbs live on the daemon socket under SPEC-TRUST §7. There is no `exec`
verb.

## 5. Permission profiles

- Allow / deny / ask per host, workspace and task. This is the same model as
  plugin capabilities (SPEC-TRUST §3), not a second one.
- Defaults MUST be **deny** for anything that writes, spawns, reaches the
  network, or leaves the machine.
- "Allow everything" MUST NOT be a preset and MUST NOT be one click away.
- A grant is scoped, revocable, visible in one place, and MUST NOT widen
  implicitly — not on agent update, not for a new task in the same workspace,
  not through remembered choices.
- MCP servers are gated identically; a grant MUST NOT extend to another server
  or workspace. An MCP server is untrusted and MUST NOT reach a leaf outside §4.
- Rules and skills are local user-owned files. They MUST NOT be fetched from a
  service and MUST NOT grant capabilities.

## 6. Credentials

- Keys come from local config or the OS keychain.
- RILL MUST NOT ship provider credentials, MUST NOT proxy requests through a
  RILL-operated service, and MUST NOT log or transmit a key.
- Keys are secrets under SPEC-TRUST §4 at every sink.
- Switching models MUST NOT change a task's permission profile.

## 7. Parallel tasks

- Concurrent agent tasks in one repository MUST use explicit git worktrees, path
  shown and confirmed (SPEC-SURFACES §4).
- Two tasks MUST NOT write the same working tree concurrently. If a worktree
  cannot be created, the second task MUST refuse to start.

## 8. Optional cloud

- With cloud agents, sync and sharing all off, RILL MUST be fully functional.
- Sharing a running session MUST be per-session, explicit, revocable, and MUST
  show what is exposed while exposed.
- Sync MUST apply redaction before anything leaves.
- An external trigger is untrusted and MUST NOT start a capability-holding task
  without a pre-existing grant.
- Nothing ships in M3.

## 9. Input routing and voice

- Raw terminal mode routes input to the leased PTY. Structured submissions
  create explicit Conversation/Task/ContentTimeline events (PRD §6).
- There MUST NOT be a natural-language classifier on the input path — no PATH
  heuristic, no English heuristic, no local model.
- Voice is transcription into the input field; the user still presses Enter or
  ⌘Enter.
- Audio MUST be opt-in, MUST show when the microphone is live, and MUST NOT be
  transmitted without that specific granted capability.

Agent product surfaces land only after domain/lifecycle, runtime workers,
checkpoint reconciliation, ContentTimeline and compositor gates. Milestone M3
does not override ADR 0053 D12's dependency order.

## 10. Computer and browser use

- Agent-driven GUI/browser control is §4's capability plus SPEC-SURFACES §2's
  browser boundary.
- It MUST run in the out-of-process browser, MUST NOT drive the host UI, MUST
  NOT act outside its granted scope, and MUST be interruptible.
- Nothing ships in M3.

## 11. Gates

| ID | Status | Closes |
|---|---|---|
| T-AGENT-AUTHORITY | Red | §2 |
| T-AGENT-COLD | Red | §3 |
| T-AGENT-WRITE | Red | §4 |
| T-AGENT-PERM | Red | §5 |
| T-AGENT-KEY | Red | §6 |
| T-AGENT-WORKTREE | Red | §7 |
| T-AGENT-OFFLINE | Red | §8 |

All MUST be demonstrable against the replay adapter plus a fake CLI agent
fixture. No gate MAY require a live provider.

## 12. What we will not do

- Make screen scraping the primary lifecycle signal.
- Report `idle` when no manifest matches.
- Compile manifests into the binary.
- Branch surfaces per adapter.
- Ship an "allow all" preset.
- Proxy model requests through a RILL service.
- Share a worktree between two agent tasks.
- Classify Enter.
