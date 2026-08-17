# SPEC-CWD — live cwd tap (`lane:kernel`)

- **Status:** Proposed — 2026-08-17 (does not authorize code)
- **Authority:** [ADR 0013](../adr/0013-cwd-tap.md)
- **Issue:** [#23](https://github.com/mahboobmonnamd/RILL/issues/23)
- **Crate:** `crates/rill-kernel` (tap). Attach MAY journal OSC 7.
  `vt-engine` MUST NOT. Host Block header is [#22](https://github.com/mahboobmonnamd/RILL/issues/22).
- **Gates:** T-CWD-FG, T-CWD-NO-OSC7, T-CWD-FAIL-CLOSED (Red until ADR 0013
  is Accepted and the tests exist)

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. Tap

- The kernel MUST expose cwd per `SessionId`.
- Darwin: `tcgetpgrp` on the kernel-owned PTY master, then
  `proc_pidinfo(PROC_PIDVNODEPATHINFO)` on that foreground pgrp, using
  `pvi_cdir.vip_path`.
- The posix_spawn child pid MUST NOT be the only pid sampled.
- Linux: `UnsupportedPlatform` until a later ADR.
- Sampling MUST NOT run on the key-down path.

## 2. OSC 7

- Attach MAY classify `OSC 7` into a journal.
- OSC 7 MUST NOT be the source of truth.
- A TUI that never emits OSC 7 MUST still report cwd after it `chdir`s.

## 3. Transport

- Cwd MUST NOT be an attach warm-path frame (`DATA` / `CREDIT` / a new `CWD`
  tag on the typing socket).
- In-process tests call `Session::cwd()` (name may differ). Packaged Block
  chrome is out of this spec.

## 4. Fail closed

- Unreadable cwd MUST return `Err` and MUST keep last known.
- MUST NOT parse the prompt.
- MUST NOT write cwd into the byte ring or into logs.

## 5. Out of scope

Block path headers, JSON on the warm path, Chip 1, `vt-engine`, prompt
snapshots at command start, Linux.
