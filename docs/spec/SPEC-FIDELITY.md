# SPEC-FIDELITY — Chip 0 boundary and terminal fidelity (`lane:chip0-ghostty-vt`)

- **Status:** Accepted — 2026-08-18. Gates below are **Red** until demonstrated
  red-then-green (ADR 0002 D2).
- **Authority:** [ADR 0022](../adr/0022-terminal-fidelity-is-chip0.md)
- **Requires:** [SPEC-CHIP0](SPEC-CHIP0.md), [SPEC-DISPLAY](SPEC-DISPLAY.md),
  [SPEC-KERNEL](SPEC-KERNEL.md), [SPEC-CWD](SPEC-CWD.md)
- **Crates:** `crates/rill-chip0`, `crates/rill-kernel`, `host/macos/`
- **Milestone:** M2 — Chrome

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. Parsing boundary

- Escape sequences MUST be interpreted only inside the Chip 0 adapter.
- The host MUST NOT parse escape sequences.
- The host MUST NOT keep its own copy of mouse mode, alternate-screen state, or
  keyboard-protocol flags. It queries Chip 0.
- Alternate screen, mouse reporting modes, the Kitty keyboard protocol, Unicode
  width and grapheme clustering, and graphics protocols are Chip 0's.

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

- Leaves are spawned by `Kernel::spawn_leaf` with the user's login shell. The
  GUI MUST NOT `posix_spawn` the shell (NFR-SPAWN, link-level gate).
- RILL MUST NOT special-case a shell by name for correctness.
- Shell integration MUST be opt-in and additive. Every behaviour it improves
  MUST degrade correctly without it (SPEC-CWD fail-closed rule).
- Block boundaries MUST NOT require shell integration
  ([SPEC-BLOCKS](SPEC-BLOCKS.md) §2).

## 5. Startup env and cwd

- Working-directory policy (home / previous / custom) and startup shell and env
  MUST be resolved **before** `spawn_leaf`, by the kernel, from config and the
  container node.
- They MUST NOT be applied by writing `cd` or `export` into the PTY after spawn.
- "Previous" reads the cold cwd tap (SPEC-CWD). With no answer, policy falls
  back to home and MUST NOT block the spawn.

## 6. Scrollback ring

- The bound is **bytes per leaf**, configurable, default **8 MiB**. It MUST NOT
  be expressed in lines.
- Hidden panes MUST keep draining ([SPEC-NAV](SPEC-NAV.md) §2). Memory is
  controlled by the ring, never by refusing to read.
- What a resync replays MUST match what the ring holds. Bytes the ring dropped
  MUST NOT be implied to still exist.

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
| T-FID-ENV | Red | §5 |
| T-FID-RING | Red | §6 |
| T-FID-CONTRAST | Red | §8 |

T-NFR MUST NOT be re-cut. T-LOOK-FILE, T-LOOK-ANSI, T-LOOK-UNKNOWN stay green.

## 10. Out of scope

Chip 1 as the live chip, a second VT, Blocks, replacing `libghostty-vt`, GUI
spawn of the user shell.

## 11. What we will not do

- Grow an escape parser in the host.
- Advertise a protocol we cannot render.
- Inject `cd`/`export` into a running shell.
- Bound scrollback by lines.
- Track Ghostty's config as a live compatibility surface.
