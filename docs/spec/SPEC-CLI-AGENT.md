# SPEC-CLI-AGENT — terminal-first CLI coding agent integration

- **Status:** Accepted — 2026-08-22. This is specification-only guidance for
  the architecture gate; implementation is not authorized in this patch.
- **Authority:** [ADR 0057](../adr/0057-cli-agent-integration-architecture.md)
- **Requires:** [SPEC-AGENT](SPEC-AGENT.md), [SPEC-TASK](SPEC-TASK.md),
  [SPEC-TRUST](SPEC-TRUST.md), [SPEC-ATTENTION](SPEC-ATTENTION.md),
  [SPEC-CONTENT](SPEC-CONTENT.md)
- **Milestone:** M3 — Conversations

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. Scope

This specification defines the contract for integrating RILL with terminal-first
CLI coding agents such as Claude Code, Codex CLI and future compatible agents.
It is provider-neutral at the product boundary and intentionally does not require
that every provider expose the same structured events.

## 2. Terminal ownership is not negotiable

- A CLI agent runs in the same `TerminalExecution` as its terminal pane.
- The PTY, child process group, focus, raw mode, alternate screen and TUI remain
  terminal-owned state.
- RILL MUST NOT create a second PTY for a provider's TUI unless the user
  explicitly creates another terminal execution.
- RILL MUST NOT convert the TUI into native cards or rewrite the provider's
  rendering model in place.
- Exiting a provider-backed task returns the pane to the correct shell or Flow
  presentation without abandoning the terminal execution.

## 3. Allowed identification mechanisms

The identity of a CLI agent MAY be known only through one of the following
authoritative sources:

1. an explicit user launch of a known executable,
2. verified runtime child-process ownership,
3. a provider-supported integration handshake explicitly enabled by the user,
4. a direct structured protocol event from a provider adapter.

The following are forbidden as primary authority:

- prompt text,
- window title,
- ANSI colour or style,
- terminal-cell content,
- regex over transcripts,
- guessed environment variables,
- shell history,
- provider-lurking heuristics.

When no authoritative mechanism is present, the state MUST be `unknown`.

## 4. Capability levels

### 4.1 Level 0 — raw terminal compatibility

- Normal terminal semantics remain active.
- No agent-specific status is shown.
- No claims are made about a provider's internal lifecycle.
- The product behaves as an ordinary terminal pane.

### 4.2 Level 1 — identified CLI-agent session

- RILL may show provider identity and attach a Task to the terminal pane.
- The provider is known by an explicit, verified mechanism.
- No provider-specific capabilities are assumed beyond identity and runtime
  ownership.

### 4.3 Level 2 — structured provider adapter

A provider adapter MAY normalize official structured events into the Task and
attention model. The normalization boundary is provider-neutral and versioned.

Allowed events include:

- task started / completed,
- waiting for input,
- approval request,
- question,
- tool or command execution,
- file change / diff,
- artifact checkpoint,
- failure,
- resume identifier.

The adapter MUST preserve provenance and MUST NOT silently claim unsupported
capabilities. Unknown events are either preserved or marked unsupported.

### 4.4 Level 3 — native RILL agent

This is the future native orchestration model. It is distinct from external CLI
provider integrations and does not change the PTY ownership contract.

## 5. Adapter boundary and normalized events

- Provider-specific payloads terminate inside the `AgentAdapter` boundary.
- The adapter exposes provider-neutral events to the runtime.
- The runtime stores only versioned normalized events with provenance.
- Unknown or unsupported provider events remain visible as data or explicit
  unsupported states; they are never misclassified as a known RILL event.
- Adapter recovery is explicit and cannot crash the PTY or another adapter.
- Unsupported provider versions degrade to raw terminal compatibility.

## 6. Structured requests, approvals, and questions

- Approvals and questions remain durable `Task` events, not terminal prompts.
- The product may surface them as native UI around the grid, but must not inject
  them into the PTY.
- A question answered by the user becomes a new `Task` section, not a synthetic
  keystroke.
- There is no auto-approval on timeout.
- A provider approval or tool request is only a product signal if it is observed
  via a documented provider mechanism or a verified adapter event.

## 7. Trust, privacy and performance

- No provider-specific adapter may widen permissions or leak credentials.
- Secrets remain local to the machine and obey the repository's trust boundary.
- Data retention and redaction use the ContentTimeline and trust policy.
- Process observation must remain cold and low-frequency; it must not add a
  parser to the warm PTY path or sample per-frame.
- Concurrent tasks MUST use distinct worktrees and explicit path ownership.

## 8. Remote and mobile supervision

- Remote or mobile clients MAY surface task/approval state but MUST NOT gain
  terminal ownership or hidden command access.
- Any remote supervision path is explicit, user-granted and revocable.
- Remote clients never become the terminal authority.

## 9. Gates

| ID | Status | Closes |
|---|---|---|
| T-CLI-AGENT-LEVELS | Red | §4 |
| T-CLI-AGENT-IDENTITY | Red | §3 |
| T-CLI-AGENT-TERMINAL-OWNERSHIP | Red | §2 |
| T-CLI-AGENT-APPROVALS | Red | §6 |
| T-CLI-AGENT-ADAPTER-BOUNDARY | Red | §5 |
| T-CLI-AGENT-WORKTREE | Red | §7 |
| T-CLI-AGENT-REMOTE | Red | §8 |

All gates MUST be demonstrated with replay fixtures or a fake CLI-agent fixture.
No gate may require a live provider unless the provider's documented mechanism is
explicitly under test.

## 10. What we will not do

- infer provider identity from prompt output,
- rewrite a TUI into cards,
- scrape terminal grids for a status oracle,
- invent a provider API that does not exist,
- create second PTYs without an explicit user terminal execution,
- silently widen permissions,
- share one worktree across two concurrent tasks,
- proxy secrets through a hosted service.
