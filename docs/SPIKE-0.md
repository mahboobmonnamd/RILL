# Spike 0 — attach, persist, Chip 0

**Status: gates run 2026-08-16. Not closed.** T-NFR is Partial (AC, POD snapshot — not key-down → first GPU frame). Stop rule holds: do not open Milestone 1.

Authority: [PRD](PRD.md), [ADR 0001](adr/0001-session-operating-system.md), [SPIKE-0-VALIDATION](SPIKE-0-VALIDATION.md).

Run: `sh scripts/validate-spike0.sh`

## Goal

One window. Chip 0. Kernel-owned PTY. Framed attach. Quit keeps the shell. A key paints in one frame. Then stop and measure.

## Explicitly not in this spike

Sidebar, tabs, Blocks, agents, scheduler, theme store, full Ghostty GPU exec, Chip 1 as the live chip.

## Named gates (all required)

| ID | Test | Fails while… | Evidence 2026-08-16 |
|---|---|---|---|
| T-BYTES | `cat` invalid UTF-8 fixture | emulator sees UTF-8 replacement, not original bytes | **Proven** — `t_bytes_*` in rill-kernel + rill-chip0 |
| T-DROP | `yes` 10s, `^C`, type | dropped chunks or corrupted grid | **Proven** — `t_drop_yes_ten_seconds_ctrl_c_type_does_not_drop` |
| T-ATTACH | attach → detach → attach in one run | grids diverge | **Proven** — `t_attach_detach_attach_grids_do_not_diverge` |
| T-RESIZE | resize while `vim` has pending input | child `TIOCGWINSZ` ≠ display geometry | **Proven** — `t_resize_child_tiocgwinsz_matches_display` |
| T-EXIT | `exit` | next key still accepted as if alive | **Proven** — `t_exit_dead_pane_does_not_accept_keys_as_alive` |
| T-SPAWN | nm/otool on shipped GUI | shell spawn symbols used for the user shell | **Proven** — `t_spawn_gui_binary_has_no_user_shell_pty_symbols` on `dist/Rill.app` |
| T-KILL | GUI `SIGKILL` | child PID changes | **Proven** — `t_quit_app_and_reload_does_not_persist_the_session` against packaged `rilld` |
| T-RESYNC | reopen idle `zsh` and `vim` | blank window over a live process | **Proven** — `t_resync_reopen_idle_shell_is_not_blank` + persist reconnect |
| T-NFR | NFR-KEY as in the PRD | any control RPC on the warm path, or p95 miss on battery | **Partial** — packaged `Rill --nfr-key`: `p95=0.032ms control_rpc=0 battery=0`. Measures key bytes → POD snapshot, not key-down → first Metal present. AC, not battery. Host still CPU-rasters cells into a bitmap then blits. |

## Stop rule

If T-NFR is not Proven on a packaged build, do not open Milestone 1 work into `main`.
