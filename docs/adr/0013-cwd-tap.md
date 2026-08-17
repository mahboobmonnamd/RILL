# ADR 0013: Cwd tap (kernel fg process, not OSC 7)

- **Status:** Accepted — 2026-08-17
- **Tree:** this repository only
- **Issue:** [#23](https://github.com/mahboobmonnamd/RILL/issues/23)
- **Requires:** [ADR 0001](0001-session-operating-system.md),
  [ADR 0010](0010-spike-0-closes.md), [ADR 0011](0011-session-graph.md)
- **Authorizes:** kernel `Session::cwd()` after named T-CWD tests (M6)
- **Does not authorize:** Block path-header chrome ([#22](https://github.com/mahboobmonnamd/RILL/issues/22)),
  prompt parsing, JSON on the warm path, a `CWD` tag on the attach typing
  socket, Chip 1 as the live chip, cwd logic in `vt-engine`

Spike evidence (Darwin 25.5, 2026-08-17): `/tmp/rill-cwd-spike*.c` against a
real PTY. Accepted so the kernel tap may be implemented. The live TUI in a
Block, and a path fix at the top of that Block, stay [#22](https://github.com/mahboobmonnamd/RILL/issues/22):
same chip paints the TUI; the header **binds** to this tap. Chip 1 is unchanged.

## Context

[#23](https://github.com/mahboobmonnamd/RILL/issues/23) asked which source
updates when a TUI is on the alt-screen, and at what latency. The Block
header must be a **tap**, not a snapshot at command start. Prompt-parsing is
forbidden.

Two candidates were named: kernel child cwd (macOS `proc_pidinfo` /
`PROC_PIDVNODEPATHINFO`) and OSC 7 on the byte stream.

## Evidence

| Setup | Session-leader cwd | `tcgetpgrp(master)` cwd | OSC 7 in PTY bytes |
|---|---|---|---|
| Leaf **is** the TUI (`python3` `os.chdir("/private/tmp")`) | `/private/tmp` | same pid, `/private/tmp` | **no** |
| Interactive `zsh -f -i`, fg job `python3 /tmp/tui_chdir.py` | **unchanged** (zsh still in the start dir) | **python's pgrp**, `/private/tmp` | **no** |
| Non-interactive `sh -c python…` (no job control) | session leader == python's pgrp; OSC 7 still **no** | same | **no** |

`cd` inside a typical alt-screen TUI (`vim` `:cd`, file managers, etc.) is
the second row: the shell is stopped, the TUI is the foreground process
group, and the TUI does not emit OSC 7.

Latency:

- **Kernel tap:** one `tcgetpgrp` + one `proc_pidinfo` on the fg pgrp.
  Milliseconds, whenever the kernel **samples**. Sampling is not on the
  key-down path.
- **OSC 7:** only if some process writes `ESC ] 7 ; …` into the PTY. Then it
  is in the next `DATA` bytes. During alt-screen TUI use it is **absent**.

## Decision

### D1 — Kernel owns the tap, per `SessionId`

`Session` (the leaf) is the only place that may read the master for cwd.
Attach may classify OSC 7 into a journal. It MUST NOT paint. Chip 0 / Chip 1
MUST NOT own cwd. `vt-engine` MUST NOT.

### D2 — Source of truth is the foreground process group

On Darwin: `tcgetpgrp` on the kernel-owned master, then
`proc_pidinfo(fg, PROC_PIDVNODEPATHINFO, …)` and `pvi_cdir.vip_path`.

The posix_spawn child pid (session leader) is **not** sufficient. A TUI
started from an interactive shell keeps the shell's cwd while the TUI
`chdir`s.

Linux is a later ADR (`UnsupportedPlatform` until then).

### D3 — OSC 7 is a hint, never the tap

When OSC 7 is present, attach MAY journal it. A later Block header MUST still
agree with D2. A TUI that never emits OSC 7 MUST still update.

### D4 — Cold API, not an attach frame

Host and daemon are separate processes. Cwd MUST NOT ride the warm attach
stream (`DATA` / `CREDIT`). A `CWD` tag would fail T-NFR (received frames
during a measurement window are `DATA` only).

Until an orchestration socket exists, the contract is an in-process
`Session::cwd() -> Result<PathBuf, Error>`. Library tests call that.
Packaged Block chrome is [#22](https://github.com/mahboobmonnamd/RILL/issues/22)
and stays unauthorized here.

Sample off the key path (after a PTY read, or a bounded idle timer). Never
per keystroke.

### D5 — Fail closed

If `tcgetpgrp` / `proc_pidinfo` fails: keep last known and return `Error`.
Do not invent a path from the prompt, from OSC 7 alone, or from an empty
string presented as success.

Do not persist cwd into logs or the byte ring.

### D6 — Native VT and Block path header are not this slice

Live TUI-in-block is still the **same chip** in a Block (Chip 0 today, Chip 1
only at M7). The path at the top is host chrome bound to `Session::cwd()`,
not a second VT and not a dump of the live grid into `Text`. This ADR does
not change that plan and does not implement the header.

## Consequences

- Named tests in [TEST-CASES](../TEST-CASES.md) (T-CWD-*) start **Red**.
- Kernel tap implementation is authorized after those tests are demonstrated
  red.
- [#23](https://github.com/mahboobmonnamd/RILL/issues/23) lives on **M6**, not
  M1. Block path chrome is [#22](https://github.com/mahboobmonnamd/RILL/issues/22).

## Rejected alternatives

- **OSC 7 only.** Rejected: silent on alt-screen TUI `chdir`.
- **Session-leader `getcwd` only.** Rejected: interactive zsh + fg python left
  zsh cwd unchanged while python was in `/private/tmp`.
- **Parse the prompt.** Rejected by the issue and by ADR 0001.
- **`CWD` attach frame.** Rejected: T-NFR received-frame set is `DATA` only.
- **Put cwd in Chip 1 / `vt-engine`.** Rejected: ADR 0012, SPEC-CHIP1 §3.
