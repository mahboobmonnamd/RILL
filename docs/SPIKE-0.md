# Spike 0 — attach, persist, Chip 0

**Status: not started.** No application target exists in this repository yet.

Authority: [PRD](PRD.md), [ADR 0001](adr/0001-session-operating-system.md).

## Goal

One window. Chip 0. Kernel-owned PTY. Framed attach. Quit keeps the shell. A key paints in one frame. Then stop and measure.

## Explicitly not in this spike

Sidebar, tabs, Blocks, agents, scheduler, theme store, full Ghostty GPU exec, Chip 1 as the live chip.

## Named gates (all required)

| ID | Test | Fails while… |
|---|---|---|
| T-BYTES | `cat` invalid UTF-8 fixture | emulator sees UTF-8 replacement, not original bytes |
| T-DROP | `yes` 10s, `^C`, type | dropped chunks or corrupted grid |
| T-ATTACH | attach → detach → attach in one run | grids diverge |
| T-RESIZE | resize while `vim` has pending input | child `TIOCGWINSZ` ≠ display geometry |
| T-EXIT | `exit` | next key still accepted as if alive |
| T-SPAWN | nm/otool on shipped GUI | shell spawn symbols used for the user shell |
| T-KILL | GUI `SIGKILL` | child PID changes |
| T-RESYNC | reopen idle `zsh` and `vim` | blank window over a live process |
| T-NFR | NFR-KEY as in the PRD | any control RPC on the warm path, or p95 miss on battery |

## Stop rule

If T-NFR is not Proven on a packaged build, do not open Milestone 1 work into `main`.
