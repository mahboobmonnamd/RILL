# SPEC-AUTOMATION — configuration, events, actions, and Lua isolation

- **Status:** Draft — 2026-08-22
- **Authority:** [ADR 0057](../adr/0057-toml-config-plus-lua-automation.md)
- **Applies to:** config parsing, automation lifecycle, action dispatch, terminal hot path isolation

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. Canonical configuration

- AUT-001: The canonical persisted configuration format MUST be TOML with an explicit schema version.
- AUT-002: The configuration schema MUST be versioned and MUST support deterministic effective configuration.
- AUT-003: The runtime MUST validate config before activation and MUST keep the last known good configuration if validation fails.
- AUT-004: Unknown keys MUST be rejected or reported rather than silently accepted.
- AUT-005: Defaults MUST be explicit and MUST be layered deterministically.
- AUT-006: User overrides MUST be applied after system/admin policy and before CLI/runtime overrides.
- AUT-007: The runtime MUST support an environment-selected config path (for example `RILL_CONFIG` and `~/.config/rill/config.toml`).

## 2. Precedence and reload

- AUT-010: The precedence order MUST be: compiled defaults, system/admin policy, user config, profile/workspace override when allowed, CLI/runtime override, effective typed config.
- AUT-011: Resolution MUST be cold and atomic. Configuration loading MUST not re-consult sources per query or per frame.
- AUT-012: Reload MUST be transactional: parse, validate, build candidate config, then atomically accept.
- AUT-013: A failed reload MUST preserve the current configuration and MUST NOT drop active sessions.
- AUT-014: The runtime SHOULD support a config-file watcher and SHOULD revalidate on change.

## 3. Lua lifecycle and safety

- AUT-020: Lua MUST be optional and MUST be disabled by default unless the user has enabled it.
- AUT-021: Lua MUST be loaded behind a stable typed API and MUST NOT directly mutate Rust objects.
- AUT-022: Lua MUST NOT be invoked from PTY read/write, VT parsing, terminal state mutation, damage tracking, text shaping, glyph atlas management, rendering, or frame scheduling.
- AUT-023: Lua execution MUST be isolated behind bounded queues and MUST fail closed.
- AUT-024: A slow, hung, or failed Lua runtime MUST NOT stall `TerminalExecution`.
- AUT-025: The runtime SHOULD expose a safe-mode startup flag for recovery.
- AUT-026: If Lua fails to load or reload, the runtime MUST continue operating with the last known good automation state.

## 4. Events and actions

- AUT-030: The runtime MUST expose typed semantic events for supported lifecycle and UI state changes.
- AUT-031: Supported v1 events SHOULD be limited to the current architecture's reliable surface, such as workspace-opened, tab-created, pane-focused, command-completed, config-reloaded, and bell.
- AUT-032: Events that require a not-yet-supported contract MUST be classified as future or not allowed instead of being exposed prematurely.
- AUT-033: Lua actions MUST be validated before dispatch.
- AUT-034: Lua MAY request runtime-only ephemeral overrides, but it MUST NOT silently mutate the persisted TOML file.
- AUT-035: The action surface MUST be typed and stable, with explicit IDs rather than pointers or UI references.

## 5. Failure isolation and performance

- AUT-040: The automation queue MUST have a bounded size and MUST reject or coalesce excess work.
- AUT-041: The runtime SHOULD enforce per-event or per-script execution budgets and timeouts where practical.
- AUT-042: A Lua crash, hang, or error MUST be isolated from terminal correctness and performance.
- AUT-043: The automation system MUST NOT block the PTY hot path or the renderer.
- AUT-044: The default configuration SHOULD allow disabling automation entirely without affecting terminal operation.

## 6. Security

- AUT-050: Lua MUST be treated as untrusted executable user code.
- AUT-051: The runtime MUST expose a small, RILL-owned API surface and MUST NOT provide unrestricted access to internal state.
- AUT-052: Secrets, terminal scrollback, raw PTY byte streams, and sensitive agent context MUST NOT be exposed without explicit design and consent.
- AUT-053: The runtime MUST define which filesystem, network, and process APIs are available and whether arbitrary Lua libraries are permitted.
- AUT-054: Enterprise policy or admin policy MUST be able to disable the automation runtime without requiring a schema redesign.

## 7. Logging and compatibility

- AUT-060: Automation failures MUST be logged in a structured, non-secret way.
- AUT-061: Log messages MUST distinguish configuration failure, Lua failure, script timeout, and action rejection.
- AUT-062: The runtime SHOULD maintain compatibility with older config versions via explicit, reviewable migration logic.
- AUT-063: The automation layer MUST remain compatible with the system's no-account, local-first design.

## 8. Implementation phases

### Phase 1 — Typed TOML

- configure versioned schema
- load and validate config
- apply precedence and defaults
- add migration hooks
- add tests

### Phase 2 — Action/Event Foundation

- define `RillEvent`
- define `RillAction`
- define `ActionDispatcher`
- validate typed input

### Phase 3 — Lua Host

- embed or host the chosen Lua runtime
- bind a small API
- isolate events/actions
- enforce queue limits and safe mode

### Phase 4 — Initial automations

- add only the v1 proven API
- add a small set of notifications and workspace actions
- expand only when actual use cases require it

## 9. Requirement summary

- TOML config is canonical.
- Lua is an optional extension layer.
- Lua is asynchronous and bounded.
- The terminal hot path remains untouched.
- The automation surface is typed and stable.
- Failures do not impact terminal correctness.
