# Architecture decision record registry

ADR numbers and filenames are stable authority identifiers. A rename MUST keep
an old-path mapping document and MUST state whether decision text changed.

## 2026-08-21 collision repair

PR #278 accidentally assigned ADR 0020–0034 to a product-architecture series
after ADR 0020–0023 had already been assigned to the Chip 1 series. The earlier
Chip 1 identifiers remain canonical. The later series moved as one contiguous
range so its internal order remains visible. The renumbering changed no
decision text; ADR 0053 makes later substantive amendments explicitly.

| Historical PR #278 identifier | Canonical identifier |
|---|---|
| ADR 0020 — Session graph is the navigation model | [ADR 0038](0038-session-graph-navigation-model.md) |
| ADR 0021 — Inventories are cold readers | [ADR 0039](0039-inventories-are-cold-readers.md) |
| ADR 0022 — Terminal fidelity belongs to Chip 0 | [ADR 0040](0040-terminal-fidelity-is-chip0.md) |
| ADR 0023 — Remote is a second kernel | [ADR 0041](0041-remote-is-a-second-kernel.md) |
| ADR 0024 — Non-terminal panes are cold | [ADR 0042](0042-non-terminal-panes-are-cold.md) |
| ADR 0025 — One look schema | [ADR 0043](0043-one-look-schema-one-config-file.md) |
| ADR 0026 — Trust and automation boundary | [ADR 0044](0044-trust-secrets-and-automation-boundary.md) |
| ADR 0027 — One core, native UI per OS | [ADR 0045](0045-one-core-native-ui-per-os.md) |
| ADR 0028 — Development surfaces are panes | [ADR 0046](0046-development-surfaces-are-panes.md) |
| ADR 0029 — Attention queue | [ADR 0047](0047-attention-is-an-orchestration-queue.md) |
| ADR 0030 — Task runtime object | [ADR 0048](0048-task-is-the-agent-runtime-object.md) |
| ADR 0031 — Agent adapters | [ADR 0049](0049-agent-adapters-and-lifecycle-authority.md) |
| ADR 0032 — Blocks cold overlay | [ADR 0050](0050-blocks-are-a-cold-overlay.md) |
| ADR 0033 — Input editor | [ADR 0051](0051-input-editor-history-and-completion.md) |
| ADR 0034 — Selection and raw-mode arbitration | [ADR 0052](0052-selection-links-and-raw-mode-arbitration.md) |

Old filenames remain as non-authoritative mapping documents so historical
links resolve. Git history and the notes in each canonical ADR preserve the
original merge identity.
