# Spike 0 — validation to close

Authority: [ADR 0001](adr/0001-session-operating-system.md), [SPIKE-0](SPIKE-0.md).
Evidence class: Proven / Partial / Manual / External. Packaged-app gates are not proven by in-process fixtures.

Run: `sh scripts/validate-spike0.sh`

| ID | Spec | Command | Close requires |
|---|---|---|---|
| T-BYTES | invalid UTF-8 reaches emulator byte-identical | `cargo test -p rill-chip0 t_bytes` and `cargo test -p rill-kernel t_bytes` | Proven |
| T-DROP | `yes` 10s, `^C`, type; no dropped chunks | `cargo test -p rill-kernel t_drop` | Proven |
| T-ATTACH | attach → detach → attach; grids do not diverge | `cargo test -p rilld t_attach` | Proven |
| T-RESIZE | resize with pending input; `TIOCGWINSZ` matches | `cargo test -p rill-kernel t_resize` | Proven |
| T-EXIT | `exit`; next key not accepted as alive | `cargo test -p rill-kernel t_exit` | Proven |
| T-SPAWN | shipped GUI has no user-shell PTY symbols | `cargo test -p rill-host t_spawn` after package | Proven |
| T-KILL | GUI `SIGKILL` / quit; same child PID | `cargo test -p rilld t_quit_app_and_reload_does_not_persist_the_session` | Proven on packaged spawn path |
| T-RESYNC | reopen is not a blank window over a live process | `cargo test -p rilld t_resync` plus T-KILL reconnect | Proven |
| T-NFR | key → first POD glyph; no control RPC; **battery** | packaged `Rill --nfr-key` | Proven only on battery; AC is Partial |

Socket-only tests do not close T-KILL, T-SPAWN, or T-NFR.

User-reported: quit app and reload does not persist the session. The T-KILL test name states that bug.

## Last run

`sh scripts/validate-spike0.sh` on 2026-08-16. Workspace tests passed. Packaged T-SPAWN passed. Packaged `rilld` persist e2e passed. Packaged `--nfr-key`: `T-NFR p95=0.032ms control_rpc=0 battery=0 rc=0`.

Spike 0 stays open until T-NFR is Proven on battery for key-down → first GPU frame.
