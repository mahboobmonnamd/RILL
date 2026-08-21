# ADR 0040: Terminal fidelity belongs to Chip 0

- **Status:** Accepted — 2026-08-18
- **Amended by:** [ADR 0037](0037-chip1-live-swap.md) for the eventual live VT
  authority and [ADR 0053](0053-runtime-domain-content-and-client-authority.md)
  D5–D8 and D13 for host checkpoints, ContentTimeline, retention policy and
  explicit shell compatibility.
- **Historical identifier:** merged as ADR 0022 in PR #278; renumbered to ADR
  0040 on 2026-08-21 to resolve a collision. Renumbering changed no decision.
- **Tree:** this repository only
- **Issue:** catalog epic [#33](https://github.com/mahboobmonnamd/RILL/issues/33).
  Rows authorized by this ADR:
  F-090 [#129](https://github.com/mahboobmonnamd/RILL/issues/129), F-091
  [#130](https://github.com/mahboobmonnamd/RILL/issues/130), F-092
  [#131](https://github.com/mahboobmonnamd/RILL/issues/131), F-093
  [#132](https://github.com/mahboobmonnamd/RILL/issues/132), F-094
  [#133](https://github.com/mahboobmonnamd/RILL/issues/133), F-095
  [#134](https://github.com/mahboobmonnamd/RILL/issues/134), F-096
  [#135](https://github.com/mahboobmonnamd/RILL/issues/135), F-097
  [#136](https://github.com/mahboobmonnamd/RILL/issues/136), F-098
  [#137](https://github.com/mahboobmonnamd/RILL/issues/137), F-099
  [#138](https://github.com/mahboobmonnamd/RILL/issues/138), F-100
  [#139](https://github.com/mahboobmonnamd/RILL/issues/139), F-101
  [#140](https://github.com/mahboobmonnamd/RILL/issues/140), F-102
  [#141](https://github.com/mahboobmonnamd/RILL/issues/141), F-103
  [#142](https://github.com/mahboobmonnamd/RILL/issues/142), F-104
  [#143](https://github.com/mahboobmonnamd/RILL/issues/143).
- **Requires:** [ADR 0003](0003-display-pipeline.md),
  [ADR 0009](0009-direct-to-display-echo.md),
  [ADR 0012](0012-chip1-isolated-vt.md) (Chip 1 stays isolated until M7),
  [ADR 0013](0013-cwd-tap.md), [ADR 0017](0017-ghostty-look-windowed-default.md)
- **Amends:** nothing.
- **Does not authorize:** Chip 1 as the live chip, a second VT on the warm path,
  Blocks (ADR 0050), shell-integration-driven Block boundaries, replacing
  `libghostty-vt`, GUI spawn of the user shell.

## Context

Fifteen catalog rows describe what a terminal must get right: the engine
(F-090), shells (F-091), alternate screen (F-092), mouse reporting (F-093),
the Kitty keyboard protocol (F-094), Unicode/IME/clipboard (F-095), graphics
protocols (F-096), shell integration (F-097), Ghostty config import (F-098),
scrollback bound (F-099), cwd policy (F-100), startup env (F-101), nested TUIs
(F-102), bell (F-103), contrast (F-104).

Almost all of it already has an owner. Chip 0 is `libghostty-vt` behind our
adapter with a POD Metal surface (FR-CHIP0, `crates/rill-chip0`). ADR 0017
already resolves the look from a theme file. ADR 0013 already taps cwd.

So this ADR is mostly a **boundary**, not new machinery: it says which of these
rows are Chip 0's to answer, which are the kernel's, and which are ours to
refuse. The failure it prevents is fidelity work leaking into the host — a
host-side mouse-mode tracker, a host-side alt-screen flag, a second parser for
OSC — which is how a display pipeline acquires a second VT by accident.

## Decision

### D1 — Sequence interpretation is Chip 0's, entirely

Alternate screen (F-092), mouse reporting modes (F-093), Kitty keyboard
protocol (F-094), Unicode width and grapheme clustering (F-095), and graphics
protocols (F-096) are interpreted **only** inside the Chip 0 adapter.

The host MUST NOT parse escape sequences. The host MUST NOT keep its own copy of
mouse mode, alt-screen state, or keyboard-protocol flags. It queries Chip 0.

A host that grows a `switch` on `\x1b[` bytes has grown the second VT that
ADR 0012 exists to prevent. Mutation `host_parses_escapes` MUST turn
T-FID-BOUNDARY red.

### D2 — Capability is reported, never assumed

Where `libghostty-vt` does not implement a protocol (F-096 graphics is the live
example), Chip 0 MUST report the capability as absent and the terminal MUST NOT
advertise it to the child. Advertising a protocol we then mis-render is worse
than not advertising it.

Capability reporting is cold, at attach. It MUST NOT be queried per frame.

### D3 — Input arbitration has one rule and one owner

When an app enables mouse reporting (F-093), mouse events go to the child.
The UI reclaims **only** with an explicit modifier, and that modifier is
Shift (F-084, ADR 0052 D3). There is no heuristic, no "if it looks like a
selection". Same shape as PRD §6: an explicit key, not a classifier.

IME composition (F-095) MUST commit through the same path as ordinary key
input, in order. A composition that bypasses the attach splice would reorder
against `RESIZE` (FR-RESIZE). Preedit MAY render as an overlay; the committed
text MUST travel as bytes on the splice.

Clipboard: OSC 52 read MUST be refused by default and MUST require explicit
opt-in. Write MAY be allowed with the same trust prompt ADR 0044 D2 defines.
A remote-readable clipboard is an exfiltration path; fail closed.

### D4 — Shells and nested tools are the kernel's, and are not special-cased

zsh / bash / fish (F-091) and nested TUIs (F-102) are `Kernel::spawn_leaf` with
the user's login shell. The GUI MUST NOT `posix_spawn` the shell — NFR-SPAWN is
a link-level gate and stands.

RILL MUST NOT special-case a shell by name for correctness. Shell **integration**
(F-097) is opt-in and additive: it improves cwd, exit code and duration when the
user installs it. Every one of those must still degrade to a correct terminal
without it (ADR 0013 D4 already requires cwd to fail closed with no OSC 7).

ADR 0053 D13 strengthens this contract: zsh, fish, bash and every other
PTY-compatible shell retain normal argv/environment/cwd, startup files,
prompts, themes, plugins, line editing, completion, ANSI colour, signals and
job control. RILL MUST NOT require a wrapper shell, inject hidden commands,
rewrite a prompt or modify shell/profile/plugin files for correctness.

Nothing in this milestone may make Block boundaries depend on shell
integration. That coupling is ADR 0050 D2's problem and is decided there.

### D5 — Startup env and cwd policy are cold kernel inputs

Working directory policy (F-100: home / previous / custom) and startup shell and
env (F-101) are resolved **before** `spawn_leaf`, by the kernel, from config
(ADR 0043 D3) and the container node (ADR 0038 D1).

They MUST NOT be applied by writing `cd` or `export` into the PTY after spawn.
Writing setup commands into a user's shell corrupts history and races the
prompt. Mutation `env_via_pty_write` MUST turn T-FID-ENV red.

"Previous" cwd reads ADR 0013's cold tap. If the tap has no answer, policy falls
back to home — it MUST NOT block the spawn.

### D6 — Scrollback is bounded in bytes, by the kernel, and the bound is honest

F-099: the ring is already bounded (FR-HISTORY). This ADR fixes the units and
the policy.

The bound is **bytes per leaf**, configurable, default 8 MiB. It is not lines —
a line is unbounded. Hidden panes keep draining (ADR 0038 D2); memory is
controlled by the ring, never by refusing to read.

"Compression" MUST NOT change what a resync replays. If a byte was dropped by
the ring, the UI MUST NOT imply it still has it. Named test
`t_ring_bound_is_bytes_and_resync_matches_ring`.

### D7 — Ghostty config import is a one-time adapter into our schema

F-098 imports fonts, colors and cursor from a Ghostty file. ADR 0017 already
reads look files; this row is the **import** step into the canonical schema
(ADR 0043 D1).

Import MUST be explicit and one-time. RILL MUST NOT read a live Ghostty config
as its running configuration, and MUST NOT follow Ghostty's config semantics as
a compatibility contract. Unknown keys are dropped with a report, not guessed
(ADR 0017 D-unknown / T-LOOK-UNKNOWN already sets this behaviour).

### D8 — Bell and contrast

Audible bell (F-103) is opt-in, default off, rate-limited to one per second per
leaf. BEL MUST also raise the attention path once that exists (ADR 0047 D3).

Contrast (F-104): the resolved look MUST NOT paint foreground equal to
background for any of the 16 ANSI slots or for the default pair. This is
checkable arithmetic on the theme file, and it is a **gate**, not a guideline:
named test `t_resolved_look_has_no_invisible_pair`, run over both shipped
themes. Mutation `allow_invisible_pair` MUST turn T-FID-CONTRAST red.

### D9 — Oracle

| ID | Closes |
|---|---|
| T-FID-BOUNDARY | D1 — no escape parsing outside Chip 0 |
| T-FID-CAP | D2 — unimplemented protocols are not advertised |
| T-FID-INPUT | D3 — mouse arbitration, IME ordering, OSC 52 closed |
| T-FID-ENV | D5 — env/cwd before spawn, never via PTY write |
| T-FID-RING | D6 — byte bound; resync matches the ring |
| T-FID-CONTRAST | D8 — no invisible pair in either theme |

T-NFR is not re-cut. None of these may add work to the key path.

## Consequences

- [SPEC-FIDELITY](../spec/SPEC-FIDELITY.md) is the Chip 0 boundary contract.
- SPEC-CHIP0 gains the capability-report call in D2.
- ADR 0050 (Blocks) inherits D4: Blocks may not require shell integration.

## Rejected alternatives

- **Host-side mouse/alt-screen tracking so chrome can react faster.** Rejected:
  D1. That is a second VT with extra steps.
- **Advertise graphics protocols and degrade gracefully.** Rejected: D2. A child
  that believes it can draw an image and cannot is a corrupted screen.
- **Heuristic UI-vs-TUI mouse arbitration.** Rejected: PRD §6's rule. Explicit
  modifier or nothing.
- **`cd`/`export` injected into the shell at startup.** Rejected: D5.
- **Line-bounded scrollback.** Rejected: D6, unbounded worst case.
- **Track Ghostty's config as a live compatibility surface.** Rejected: D7 and
  AGENTS.md §7 — this tree is the source of truth.
