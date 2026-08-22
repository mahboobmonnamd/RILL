# Implementation plan: typed TOML + Lua automation foundation

## Phase 1 — typed TOML

1. Define the canonical config schema in a single typed Rust model.
2. Add parsing, validation, and default resolution.
3. Implement precedence order and config path selection.
4. Add migration support with atomic reload semantics.
5. Add unit tests for malformed TOML, unknown keys, defaults, precedence, and reload failure.

## Phase 2 — action/event foundation

1. Define the semantic event set (`RillEventKind`) and classify v1/future/not allowed.
2. Define typed action values (`RillAction`) and validation logic.
3. Add a bounded `ActionDispatcher` queue and typed validation errors.
4. Keep the action layer out of PTY and render code paths.

## Phase 3 — Lua host

1. Choose a safe embedded Lua runtime. `mlua` is the most idiomatic choice for Rust because it keeps ownership and lifecycle explicit, offers a controllable API surface, and integrates well with macOS packaging.
2. Add the host behind a small `rill` Lua object with `on(...)`, `action(...)`, `notify(...)`, and `log(...)` APIs only.
3. Add timeouts, queue limits, and a safe-mode startup path.
4. Ensure reload is transactional and cannot destructively replace a functioning runtime.

## Phase 4 — initial automations

1. Expose only the minimally proven events/actions.
2. Add notification-driven and workspace-driven actions first.
3. Expand the API only when a real product need is demonstrated.
4. Treat all additions as versioned API changes.

## Guardrails

- No Lua callback may run in PTY or renderer code.
- No direct mutation of internal terminal state from Lua.
- No silent writes to `config.toml` from scripts.
- No automation API that requires a hot-path code change to support.
