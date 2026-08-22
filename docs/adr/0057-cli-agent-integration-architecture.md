# ADR 0057: First-class CLI-agent integration and terminal ownership

- **Status:** Accepted — 2026-08-22
- **Tree:** this repository only
- **Decision approval:** product owner / architecture gate, 2026-08-22
- **Requires:** [ADR 0048](0048-task-is-the-agent-runtime-object.md),
  [ADR 0049](0049-agent-adapters-and-lifecycle-authority.md),
  [ADR 0053](0053-runtime-domain-content-and-client-authority.md),
  [ADR 0056](0056-vertical-slices-backend-and-host.md)
- **Amends:** [ADR 0049](0049-agent-adapters-and-lifecycle-authority.md) and
  [SPEC-AGENT](../spec/SPEC-AGENT.md) by defining the provider-neutral
  integration model for terminal-first CLI agents and the explicit capability
  levels that are legal in the product.
- **Does not authorize:** a public provider SDK, hidden PTYs, prompt scraping as
  a lifecycle source, prompt-text guessing, injecting commands into a PTY,
  an `exec`/direct-write path outside the terminal execution authority, a hosted
  control plane, or product UI that rewrites a CLI TUI as native cards.
- **Research record:** official Claude Code / Codex CLI / Warp terminal-agent
  documentation reviewed on 2026-08-22. Only public docs were used. No provider
  private API was considered.

## Context

The repository already defines the durable runtime objects and the adapter
boundary for generic agents. The missing decision is how RILL interoperates with
real terminal-first coding agents without compromising shell correctness,
privacy, terminal fidelity, worktree isolation, or the domain graph.

Authoritative sources used for this decision:

- Anthropic: Claude Code docs and CLI overview. The product is an agentic coding
  tool that runs in the terminal, reads a repository, edits files, runs commands,
  and exposes hooks, skills, MCP, and task-oriented workflows. The docs describe
  terminal execution and tool use; they do not document a host-owned PTY
  integration contract or a provider-specific RILL protocol.
- OpenAI: Codex CLI docs and product docs. The product is a terminal-first coding
  agent with task execution, command execution, approval semantics, and model
  / credential choices. The docs describe the CLI as a user-run process; they do
  not document a host-side deep integration mechanism that RILL may assume.
- Warp: official terminal agent mode and review workflow docs. They confirm the
  broad pattern: agentic terminal UIs, structured attribution, attention,
  notifications and review around a raw terminal grid. The docs are evidence of
  product behavior and workflow expectations, not an architecture to copy.

The repository's accepted authority still stands: a CLI agent is a process in a
terminal pane, not a product object that owns a PTY or a TUI grid.

## Decision

### D1 — Terminal-first ownership remains canonical

RILL MUST keep CLI coding agents inside the existing terminal ownership model:

```text
Runtime
└── Workspace
    └── Session
        └── Tab
            └── Split tree
                └── TerminalPane
                    └── TerminalExecution (zero or one)
```

- A CLI agent running in a terminal remains a child process inside exactly one
  `TerminalExecution`.
- The PTY, alternate-screen handling, cursor, size, focus, paste, mouse, and raw
  input remain the property of the terminal pane and execution owner.
- RILL does not replace the provider's TUI with native cards or its line editor
  with a fabricating UI.
- RILL does not inject hidden commands, prompt-scraping logic, or provider
  directives via terminal text.
- Exiting the CLI agent returns the same `TerminalExecution` to the appropriate
  shell or Flow presentation. A second PTY is created only if the user creates
  another terminal execution.

### D2 — CLI integrations are capability levels, not a single fake entitlement

A CLI coding agent may integrate at one of three explicit levels, with a
mandatory fallback to the lower one when the provider does not document a
feature:

#### Level 0 — Raw terminal compatibility

For every PTY-compatible CLI agent:

- RILL provides ordinary terminal behaviour.
- No provider-specific integration is required.
- No process probing beyond ordinary execution ownership.
- No semantic claims, approvals, or fabricated status.
- No terminal-cell scraping to infer provider state.

This level MUST remain the default and the safety baseline.

#### Level 1 — Identified CLI-agent session

RILL MAY know which provider is running only via authoritative mechanisms such as:

- the user explicitly selects the executable to launch,
- RILL launches the selected executable itself,
- verified child-process lifecycle information already owned by the runtime,
- a provider-supported integration handshake that the user explicitly enabled.

Identification MUST NOT depend on:

- prompt text,
- window title,
- ANSI colour,
- terminal cell content,
- regex scraping, or
- guessed environment variables.

At this level RILL may show provider identity and may link a Task, but it must
not claim capabilities the provider has not granted.

#### Level 2 — Structured provider adapter

A provider-specific adapter MAY receive explicit structured lifecycle and action
signals only if the official provider docs and integration surface expose them.

Supported capabilities may include:

- task started/completed,
- waiting for input,
- structured question,
- approval request,
- tool invocation,
- command execution,
- file changes,
- diff/artifact production,
- checkpoint,
- usage/model metadata,
- failure,
- resume identifier.

A provider that does not expose a capability remains at the lower level for that
capability.

#### Level 3 — RILL-native agent

A future RILL-native agent may use the complete Task, tool, approval, artifact,
checkpoint and orchestration contracts directly. It still obeys PTY ownership,
input leases, trust policy, worktree isolation, attention rules, privacy,
retention and performance isolation.

### D3 — Provider-specific payloads terminate at the adapter boundary

The repository’s adapter architecture is the correct place to normalize provider
output. This decision makes the boundary explicit:

```text
AgentAdapter
├── identify capabilities
├── start / attach / detach
├── normalize lifecycle events
├── normalize structured requests
├── normalize tool / change / artifact events
├── report resume capability
├── enforce provider-specific bounds
└── expose health / version
```

Requirements:

- Provider-specific payloads are consumed and terminated inside the adapter
  boundary.
- Durable RILL state uses versioned, provider-neutral events and retains
  provenance.
- Unknown provider events are preserved safely when policy allows or reported as
  unsupported; they are never silently misclassified.
- Adapter crashes cannot terminate the PTY, agent process, runtime or another
  adapter.
- Adapter restart / reconnect behaviour is explicit and recoverable.
- Provider version and capability changes are negotiated explicitly.
- Unsupported versions degrade to raw terminal compatibility.
- Provider-specific extensions remain namespaced and versioned.
- No adapter is a public API simply because it is an internal boundary.

### D4 — The product must prefer user-owned task state over prompt scraping

The durable abstraction remains `Task`, not a provider's prompt or terminal cell.

- `Task` is the orchestration object associated with a domain, execution and
  trust context.
- A provider's status, approval or question is only a `Task`-normalized event if
  the provider exposed an authoritative lifecycle or structured API.
- Prompt text, TUI output and window title are evidence for the terminal, never
  authority for a durable task state.
- In the absence of authoritative data, the product must report `unknown`.
- The UI may show native augmentation around the TUI, e.g. provider identity,
  status, approval request, diff review and activity markers, but those are
  projections over authoritative domain objects, not new architectural layers.

### D5 — Native augmentation surrounds the terminal, never replaces it

When authoritative provider data exists, RILL MAY add native UI around the agent's
terminal-owned grid:

- provider identity,
- associated durable Task,
- status,
- notifications,
- structured questions and approvals,
- review changes,
- diffs and artifacts,
- checkpoints,
- attention badge,
- jump to owning pane,
- open activity,
- fork / parallel-task summary,
- trust and permission state,
- session continuity.

These projections are clients of established runtime objects. They are not a
separate protocol or a second terminal engine.

### D6 — Trust, privacy, security, performance and persistence are not optional

The product must preserve and enforce the repository’s existing trust and
execution boundaries:

- PTY ownership remains with `TerminalExecution` and the terminal pane.
- Input leases are enforced. No agent may write into a terminal without the
  explicit capability and ownership path.
- Credentials stay on the machine and are not proxied through a RILL service.
- Secrets are policy-gated, redacted and subject to retention limits.
- Worktrees are explicit, isolated and never shared across concurrent tasks.
- Prompt / transcript / attachment content obeys ContentTimeline retention and
  redaction rules.
- Structured approvals and questions survive GUI or daemon restarts only through
  the durable runtime state and Task contracts.
- Observation must stay cold. No fast terminal-cell scraping or per-frame
  parsing on the warm path.

### D7 — Remote and mobile supervision are same policy, different client surface

Future remote and mobile supervision extends the existing runtime model, not the
terminal ownership model.

- Remote or mobile clients may observe a provider-backed Task or approval queue,
  but they must not gain PTY authority or hidden command access.
- Provider-specific remote supervision is allowed only when the provider's
  official mechanism is explicit and user-granted.
- Approval and trust decisions remain local to the runtime and are not silently
  delegated by a remote client.
- Untrusted network or mobile triggers are never enough to start a capability-
  holding task without a pre-existing grant.

## Consequences

- The product defaults to terminal-first behaviour and raw terminal compatibility.
- Provider-specific integration is additive; capability claims are bounded by
  official provider support.
- The adapter layer is the only place where provider semantics are translated.
- No provider-specific payload becomes a product trust signal without versioned
  normalization.
- The UI may add structured overlays around a CLI TUI, but only over durable
  runtime objects and only when the provider or user has granted the capability.

## Rejected alternatives

- **Prompt scraping as the identity source.** Rejected: brittle, untrusted and
  non-authoritative.
- **Hidden command injection to force provider integration.** Rejected: breaks
  shell correctness and trust boundaries.
- **Rewriting every CLI TUI as cards.** Rejected: breaks PTY correctness and core
  terminal semantics.
- **Inventing a generic adapter API for providers that provide none.** Rejected:
  architecture without evidence.
- **Treating a provider's UI as the source of truth.** Rejected: the terminal TUI
  is a presentation, not the durable runtime state.

## Implementation note

This ADR is architecture and documentation only. It does not authorize production
implementation, launch code or a provider-specific SDK in this patch.
