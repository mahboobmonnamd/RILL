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
| Workspace / durable Session / Tab / split tree / TerminalPane | Kernel nodes at bind; chrome projects Workspace id. **In-memory New Tab** is #345 (ADR 0056). Journal persist remains D12 step 5 | SPEC-DOMAIN-LIFECYCLE, SPEC-GRAPH, #345 |
| `rill-orchestrate` Task / Attention / layered config | Library Proven; chrome agent inventory is empty; HITL UI after durable Task | SPEC-TASK, SPEC-CONFIG |
| One TOML authority; Ghostty grammar as import-only | Live overlay still wins after `host-surface.toml` | ADR 0043, ADR 0053 D14 |
| T-PERF matrix / hid T-NFR in GitHub-hosted CI | SPEC-TERMINAL-PERFORMANCE Red; ADR 0009 D4 forbids hid close on `macos-14` | ADR 0009 |
| Scrollback-as-lines / reflow | Host POD viewport over `take_scrolled_off`; chip still has no ring; reflow later | SPEC-VT-SCREEN §5, §8 |
| Compositor / Flow / Raw / TUI switch | SPEC-COMPOSITOR is specification only until ContentTimeline (D12 steps 3–4). Named tests already in TEST-CASES (`T-INPUT-MODE-TRANSITION`, `T-COMPOSITOR-PRESERVES-METAL-GRID`, `T-PERF-RAW-TUI-BYPASS`). Implementation is not authorized in this slice. | ADR 0053 D7–D9 |
| Mock tabs / Block cards / file tree / Attention | Concept evidence only (ADR 0053 D16). First legal mock-adjacent slice is raw mouse (#344, ADR 0055), not chrome. | ADR 0055, SPEC-HOST-POINTER |

Do not delete the kernel container tree or `rill-orchestrate` as dead code.
They are staged, not failed features.
