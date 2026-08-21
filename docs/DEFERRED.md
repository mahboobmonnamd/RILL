# Deferred — specified, not shipped

This page is the map for work that **must not** be treated as cleanup of the
current binary. Authority: [ADR 0053](adr/0053-runtime-domain-content-and-client-authority.md)
D12. Tracker: [#338](https://github.com/mahboobmonnamd/RILL/issues/338).

Shipped-path repairs live on [#334](https://github.com/mahboobmonnamd/RILL/issues/334)
(proxy splice), [#335](https://github.com/mahboobmonnamd/RILL/issues/335)
(worker credit / idle poll), [#336](https://github.com/mahboobmonnamd/RILL/issues/336)
(alt-screen resize), [#337](https://github.com/mahboobmonnamd/RILL/issues/337)
(host getenv, KeepAlive, Chip 0 wording).

| Item | Where it sits today | Issue / spec |
|---|---|---|
| GUI protocol 2, Checkpoint / Delta / ResyncRequest | `Client::connect` sends `attach(1)`; other frames count as warm-path violations | [#338](https://github.com/mahboobmonnamd/RILL/issues/338), SPEC-CLIENT-AUTHORITY |
| Per-leaf canonical VT on the worker | One scratch `Daemon.chip`; live feed is `rill-host::Client` | ADR 0053, [#338](https://github.com/mahboobmonnamd/RILL/issues/338) |
| OSC dispatch (title, OSC 7, 133 marks) | Parser consumes OSC; screen has no OSC actions | SPEC-CONTENT, SPEC-CWD |
| Workspace / durable Session / Tab / split tree / TerminalPane | `NodeKind` tests only; GUI is one leaf | SPEC-DOMAIN-LIFECYCLE, SPEC-GRAPH |
| `rill-orchestrate` Task / Attention / layered config | No crate depends on it | SPEC-TASK, SPEC-CONFIG |
| One TOML authority; Ghostty grammar as import-only | Live overlay still wins after `host-surface.toml` | ADR 0043, ADR 0053 D14 |
| T-PERF matrix / hid T-NFR in GitHub-hosted CI | SPEC-TERMINAL-PERFORMANCE Red; ADR 0009 D4 forbids hid close on `macos-14` | ADR 0009 |
| Scrollback-as-lines / reflow | Byte ring is history; chip has no scrollback | SPEC-VT-SCREEN §5, §8 |
| **See text that left the grid** (tree/`ls` cut; no wheel) | `TerminalView` has no `scrollWheel:`; live grid only | **[#339](https://github.com/mahboobmonnamd/RILL/issues/339)** |
| **App shortcut keys** | Keys go to the PTY; no host keybinding engine | **[#340](https://github.com/mahboobmonnamd/RILL/issues/340)** (catalog [#229](https://github.com/mahboobmonnamd/RILL/issues/229)) |
| **Workspaces** | Left chrome is a placeholder row, not graph identity | **[#341](https://github.com/mahboobmonnamd/RILL/issues/341)** (catalog [#41](https://github.com/mahboobmonnamd/RILL/issues/41)) |
| Compositor, Flow, agents | SPEC-COMPOSITOR / SPEC-CONTENT Red | ADR 0053 D7–D9 |

Do not delete the kernel container tree or `rill-orchestrate` as dead code.
They are staged, not failed features.
