# SPEC-FIDELITY — Chip 0 boundary and terminal fidelity (`lane:chip0-ghostty-vt`)

- **Status:** Accepted — 2026-08-18. Gates below are **Red** until demonstrated
  red-then-green (ADR 0002 D2).
- **Authority:** [ADR 0040](../adr/0040-terminal-fidelity-is-chip0.md), amended
  by [ADR 0037](../adr/0037-chip1-live-swap.md) and
  [ADR 0053](../adr/0053-runtime-domain-content-and-client-authority.md) D5–D8
  and D13/D22
- **Requires:** [SPEC-CHIP0](SPEC-CHIP0.md), [SPEC-DISPLAY](SPEC-DISPLAY.md),
  [SPEC-KERNEL](SPEC-KERNEL.md), [SPEC-CWD](SPEC-CWD.md), and
  [SPEC-TERMINAL-PERFORMANCE](SPEC-TERMINAL-PERFORMANCE.md)
- **Crates:** `crates/rill-chip0`, `crates/rill-kernel`, `host/macos/`
- **Milestone:** M2 — Chrome

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. Parsing boundary

- Escape sequences MUST be interpreted only inside the selected terminal-core
  implementation: Chip 0 now and Chip 1 only after its live gates.
- Host application/UI code MUST NOT grow an escape parser. The authoritative
  host terminal core owns screen, modes, cursor and canonical geometry; a
  disposable client mirror maintains the same state for warm rendering and
  reconciles with host checkpoints/hashes.
- Alternate screen, mouse reporting modes, keyboard protocols, Unicode width,
  grapheme clustering and graphics protocols belong to terminal core, not the
  compositor or ContentTimeline.

## 2. Capability reporting

- Where `libghostty-vt` does not implement a protocol, Chip 0 MUST report it
  absent and the terminal MUST NOT advertise it to the child.
- Capability reporting MUST be cold, at attach. It MUST NOT be queried per
  frame.

## 3. Input

- With mouse reporting on, events go to the child. The UI reclaims **only** with
  Shift. There MUST be no heuristic arbitration.
- IME committed text MUST travel as bytes on the attach splice, ordered with
  keys and `RESIZE` (FR-RESIZE). Preedit MAY render as an overlay.
- OSC 52 clipboard **read** MUST be refused by default and MUST require explicit
  opt-in. Write MAY be allowed under the trust prompt (SPEC-TRUST §2).

## 4. Shells and nested tools

- TerminalExecutions are spawned by the runtime worker with the user's selected
  zsh, fish, bash or other PTY-compatible shell. The GUI MUST NOT
  `posix_spawn` the shell (NFR-SPAWN, link-level gate).
- Spawn MUST preserve normal PTY, argv, environment, cwd, signal, job-control,
  TERM/capability and login/non-login semantics. Existing startup files,
  prompts, themes, plugins, line editors, completions, ANSI colours and
  interactive programs MUST work without RILL-specific changes.
- RILL MUST NOT special-case a shell by name for correctness.
- Shell integration MUST be opt-in and additive. Every behaviour it improves
  MUST degrade correctly without it (SPEC-CWD fail-closed rule).
- RILL MUST NOT inject hidden commands, rewrite a prompt, replace a shell
  theme/plugin, edit startup files or require a wrapper shell for correctness.
- Block boundaries MUST NOT require shell integration
  ([SPEC-BLOCKS](SPEC-BLOCKS.md) §2).

## 5. Startup env and cwd

- Working-directory policy (home / previous / custom) and startup shell and env
  MUST be resolved before spawning TerminalExecution, by the runtime, from
  config and the owning Session/terminal-pane context.
- They MUST NOT be applied by writing `cd` or `export` into the PTY after spawn.
- "Previous" reads the cold cwd tap (SPEC-CWD). With no answer, policy falls
  back to home and MUST NOT block the spawn.

## 6. Hot recovery ring and retained content

- The hot bound is **bytes per TerminalExecution**, configurable and expressed
  in bytes, not lines. The Accepted product default remains 8 MiB until a later
  ADR changes it; the current 4 MiB code constant is therefore a Red mismatch,
  not authority.
- Hidden panes MUST keep draining ([SPEC-NAV](SPEC-NAV.md) §2). Memory is
  controlled by the ring, never by refusing to read.
- Ring offsets are monotonic and eviction advances an explicit retained base.
  Reconnect uses a host checkpoint plus retained deltas. Bytes or durable
  content that policy deleted MUST NOT be implied to exist. Durable capture is
  separately policy-controlled by SPEC-CONTENT and may be disabled.

## 7. Config import

- Ghostty import MUST be explicit and one-time, producing values in the
  canonical schema ([SPEC-CONFIG](SPEC-CONFIG.md) §1).
- RILL MUST NOT read a live Ghostty config as its running configuration.
- Unknown keys are reported and dropped, never guessed (T-LOOK-UNKNOWN).

## 8. Bell and contrast

- The audible bell MUST be opt-in, default off, and rate-limited to one per
  second per leaf. BEL MUST also raise an attention entry
  ([SPEC-ATTENTION](SPEC-ATTENTION.md) §3).
- The resolved look MUST NOT paint foreground equal to background for any of the
  16 ANSI slots or the default pair, in either shipped theme.

## 9. Gates

| ID | Status | Closes |
|---|---|---|
| T-FID-BOUNDARY | Red | §1 |
| T-FID-CAP | Red | §2 |
| T-FID-INPUT | Red | §3 |
| T-FID-SHELL-COMPAT | Red | §4 |
| T-FID-SHELL-NO-MUTATION | Red | §4 |
| T-FID-ENV | Red | §5 |
| T-FID-RING | Red | §6 |
| T-FID-CONTRAST | Red | §8 |

T-NFR MUST NOT be re-cut. T-LOOK-FILE, T-LOOK-ANSI, T-LOOK-UNKNOWN stay green.
Every later fidelity or product surface also runs the applicable paired
disabled/enabled matrix in SPEC-TERMINAL-PERFORMANCE. Missing semantic metadata
may remove enhancement but never change raw bytes, terminal modes, grapheme
fidelity, input ordering or grid presentation.

## 10. Out of scope

Chip 1 as the live chip, structured content, replacing `libghostty-vt`, and GUI
spawn of the user shell remain outside the currently proven fidelity slice.
One authoritative host VT plus disposable client mirrors is not permission for
competing authoritative parsers.

## 11. What we will not do

- Grow an escape parser in the host.
- Advertise a protocol we cannot render.
- Inject `cd`/`export` into a running shell.
- Bound scrollback by lines.
- Track Ghostty's config as a live compatibility surface.
