# ADR 0001: Session operating system

- **Status:** Accepted — 2026-08-16
- **Tree:** this repository only
- **Supersedes:** nothing in this tree (first ADR)

## Context

A terminal that persists sessions must not put the display emulator on the far side of a control protocol. A terminal that feels native must not wait on writing a full VT engine before the kernel exists.

## Decision

1. **One kernel, two chips.** Kernel never switches. Chip 0 is `libghostty-vt` in the window plus our Metal (POD cells + damage). Chip 1 is an owned VT library behind the same traits, later. Do not jump to “full libghostty exec.”
2. **Four planes:** orchestration (cold JSON), kernel (PTY + history), attach (framed `SOCK_STREAM`), display (in-process).
3. **No VT on the live display path in the kernel.** Allowed: shallow stream classifier; cold-path resync using the same chip headless, bytes only.
4. **Runtime owns scrollback** as a bounded byte ring. The chip is not the owner.
5. **Sole writer.** No `SCM_RIGHTS` of the master to the GUI. One attach per pane; the second is refused.
6. **Darwin:** `SOCK_SEQPACKET` is unsupported (errno 43). Frame `SOCK_STREAM`.
7. **Persist wedge:** GUI close / `SIGKILL` ≠ kill shell. Daemon crash, logout, and app update are **out of wedge** until a later ADR. Do not “fix” a lost daemon by moving the shell into the GUI.
8. **Enter → PTY** when input exists. No heuristic English/PATH router. Agents and Blocks are not Spike 0.
9. **Proposed ADRs do not authorize code.** An issue without this ADR (or a later Accepted ADR) and a named test is invalid.

## Consequences

- Spike 0 is Chip 0 + attach + persist + resync. If it is not butter, the project stops adding surface area.
- Chip 1 work may exist as an isolated library with no GUI merge until Spike 0 is Proven.
- Monetization, if any, is not specified here and must not paywall the shell.
