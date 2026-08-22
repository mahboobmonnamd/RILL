# ADR 0057: Typed TOML configuration with an optional Lua automation layer

- **Status:** Accepted — 2026-08-22
- **Issue:** configuration and automation architecture for RILL
- **Tree:** this repository only

## Context

RILL is a native macOS terminal with a Rust runtime and Swift host. The product has a cold orchestration layer, a PTY/VT hot path, and a native UI shell. This separation is already reflected in the repository's plan: config resolution lives in the orchestration plane, while PTY and rendering stay outside it.

The system needs a configuration model that is versioned, typed, deterministic, and safe to reload, while also allowing a small automation layer for user-driven workflows. The config and automation responsibilities must not blur: the terminal hot path must remain fast, isolated, and independent from user scripts.

## Problem

A flat configuration model is not scalable, and a scripting language that directly mutates runtime objects would break the hot-path safety boundary. We need a configuration model that persists user intent in a canonical file while exposing a separate automation layer that is asynchronous, validated, and bounded.

## Considered alternatives

### TOML-only

Advantages:

- simple
- typed and serializable
- easy to validate
- works well as canonical persisted config

Disadvantages:

- too limited for user automation
- no event-driven behavior
- no workflow customization without broadening the config schema itself

### Lua-only

Advantages:

- expressive
- flexible automation

Disadvantages:

- conflicts with the principle that config and automation are distinct
- risky to place beside terminal hot-path logic
- poor trust boundary and safety controls
- not a good persisted config format

### TOML + Lua hybrid

Advantages:

- keeps persisted config canonical and typed
- allows automation behind a narrow API
- preserves a clean trust boundary
- supports a gradual rollout

Disadvantages:

- requires explicit architectural boundaries and validation
- needs rate-limiting and failure isolation

## Decision

RILL uses:

> TOML is the canonical persisted configuration format.
>
> Lua is an optional automation and extension runtime.
>
> Lua does not replace TOML.
>
> Lua cannot participate in PTY, VT, terminal-state, shaping, or rendering hot paths.
>
> Lua interacts with RILL through typed events and actions.

### D1 — Canonical config format

The persisted configuration is TOML, versioned and validated before activation. It is the single source of truth for user preferences and runtime settings. The schema can grow by versioned evolution, and each release can provide migration logic if a field changes meaning.

### D2 — Automation is separate

Lua is optional and treated as executable user code. It may subscribe to semantic runtime events, but it may not directly mutate or query internal Rust objects or PTY buffers. All Lua output becomes validated, typed actions.

### D3 — Runtime boundary

The hot path remains fully cold from the perspective of Lua. Lua may not synchronously observe PTY bytes, VT parsing, screen mutation, damage tracking, text shaping, glyph atlas management, GPU rendering, frame scheduling, or presentation. It cannot execute in any of those code paths.

### D4 — Typed events and actions

The runtime emits semantic events such as workspace-opened and command-completed. Lua receives those events through a stable typed API. Subsequent behavior is expressed as validated actions, such as create-tab or show-notification, which are dispatched through the same general action system used by native features.

### D5 — Failure isolation

Lua failures are contained. A slow or crashed script must not stop the terminal, stall PTY work, or drop sessions. The system uses bounded queues, execution timeouts where practical, safe-mode startup, and fail-closed policy.

## Rationale

This design puts the product on a durable architectural foundation:

- TOML remains easy to read, diff, and version.
- Lua is kept out of the hot path.
- The trust boundary is explicit and auditable.
- Actions remain typed and therefore reviewable and debuggable.
- The runtime can safely disable, reload, or fail isolated automation without harming terminal core correctness.

## Architectural boundaries

The boundaries are intentionally narrow:

- Config layer: parse, validate, load, version, reload, precedence.
- Automation layer: subscribe to semantic events and emit typed actions.
- Action layer: validate and dispatch actions through a bounded queue.
- Terminal hot path: PTY, VT, state, damage, shaping, rendering, frame scheduling.

Lua may not cross these boundaries directly.

## Security implications

Lua is executable user code. It should run with a small, RILL-owned API surface, limited file/network/process access, and explicit trust boundaries. Sensitive material such as PTY data, terminal scrollback, secrets, or agent context must not be exposed unless explicitly designed and authorized.

## Performance implications

The hot path is performance-critical. Lua is intentionally not synchronous and not inserted into terminal rendering, parsing, or PTY operations. The design isolates cost to the orchestration plane and applies queue limits and timeouts so a misbehaving script cannot degrade terminal throughput.

## Consequences

### Positive

- deterministic config
- safer automation model
- explicit trust boundaries
- clear future extension surface
- no terminal hot-path regressions

### Negative

- Lua cannot directly operate on every internal object
- users need to think in event/action terms instead of direct mutation
- more design discipline is required at the API boundary

## Future evolution

This ADR intentionally chooses a small v1 API. Future work may add additional v1 event kinds or actions only when the runtime can support them safely. Anything that requires hot-path access is not allowed and must remain outside the automation API surface.
