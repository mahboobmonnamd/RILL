# Spike 0 — attach, persist, Chip 0

**Status: RED. All prior `Proven` marks revoked 2026-08-16 by
[ADR 0002](adr/0002-falsifiable-evidence.md) D1.**

The 2026-08-16 gate run was invalidated by [SPIKE-0-AUDIT](SPIKE-0-AUDIT.md):
three gates were structurally incapable of failing, and five more had names that
asserted behaviour their bodies never exercised. `T-NFR p95=0.032ms` is a **null
result**, not a partial one, and must not be cited.

Stop rule holds. Milestone 1 stays closed.

Authority: [PRD](PRD.md), [ADR 0001](adr/0001-session-operating-system.md),
[ADR 0002](adr/0002-falsifiable-evidence.md),
[ADR 0003](adr/0003-display-pipeline.md),
[ADR 0004](adr/0004-chip0-does-not-close-nfr-key.md) (**Accepted**),
[ADR 0005](adr/0005-mtkview-presenter.md) (**Accepted**),
[ADR 0006](adr/0006-next-vsync-present.md) (**Accepted**),
[ADR 0007](adr/0007-opaque-fullscreen.md) (**Accepted**),
[ADR 0008](adr/0008-cametal-display-link.md) (**Accepted** — exhausted),
[ADR 0009](adr/0009-direct-to-display-echo.md) (**Accepted** — closer).
Gate definitions: [TEST-CASES](TEST-CASES.md).
Closure procedure: [SPIKE-0-VALIDATION](SPIKE-0-VALIDATION.md).

Run: `sh scripts/validate-spike0.sh`

## Goal

One window. Chip 0. Kernel-owned PTY. Framed attach. Quit keeps the shell. A key
paints in one frame. Then stop and measure — with an instrument that can say no.

## Explicitly not in this spike

Sidebar, tabs, Blocks, agents, scheduler, theme store, full Ghostty GPU exec,
Chip 1 as the live chip.

## Named gates

Each gate's oracle, procedure, required mutation, and negative control are
defined in [TEST-CASES](TEST-CASES.md). A gate reaches `Proven` only after being
demonstrated **red** on a build where the behaviour is absent (ADR 0002 D2).

| ID | Test | Fails while… | Status |
|---|---|---|---|
| T-BYTES | invalid UTF-8 through the kernel ring and Chip 0 feed | kernel history drops bytes, or Chip 0 never shows a non-ASCII cell for a high-byte fixture | **Green-unproven** — `drop_high_bytes` went red in CI, then unmutated passed |
| T-DROP | unbounded `yes`, finite credit, `^C`, keep typing | any sequence number missing, or the kernel never stalls its reads | **Green-unproven** — `drop_on_full` went red in CI, then unmutated passed |
| T-ATTACH | attach → detach → attach; cell-by-cell grid compare | grids diverge, or a bare connection displaces the attached client | **Green-unproven** — `accept_replaces_client` went red in CI, then unmutated passed |
| T-RESIZE | child's own `TIOCGWINSZ` after `SIGWINCH`, with pending input | child's size ≠ display geometry, or resize overtakes queued input | **Green-unproven** — `resize_before_data` went red in CI, then unmutated passed |
| T-EXIT | `exit`, including **while detached** | reopened window paints a cursor over a dead process | **Green-unproven** — `clear_outbound_on_detach` went red in CI, then unmutated passed |
| T-SPAWN | `nm -u` + `otool -Iv` on the packaged GUI, plus a positive control | PTY-creation primitives imported, or the check itself is broken | **Green-unproven** — CI packaged GUI has no PTY imports; `openpty_in_main_m` went red |
| T-KILL | packaged `Rill.app`, `SIGKILL` the process group and AppKit Quit | child pid changes, or reattach is blank | **Green-unproven** — CI persist_e2e; `drop_POSIX_SPAWN_SETSID` went red |
| T-RESYNC | reopen idle `zsh` and alt-screen `vim` | blank window over a live process, or resync touches the warm path | **Green-unproven** — `no_resync` went red in CI, then unmutated passed |
| T-NFR | key-down `NSEvent.timestamp` → drawable `presentedTime`, on battery | p95 over one refresh interval, discards > 2%, or any control RPC | **Manual** — battery hid p95 **7.011ms**; `timer_pump` p95 **30.823ms**. Hosted CI timed out |

CI artifact (ADR 0002 D8, library suite): GitHub Actions
[run 31993832263](https://github.com/mahboobmonnamd/RILL/actions/runs/31993832263)
on `d20568e`, evidence `spike0-20260817T041912Z.json`. Do not dispatch
`gates.yml` again; hosted `macos-14` cannot close hid. Battery hid (0009):
`/tmp/rill-nfr-hid.{out,err}` 2026-08-17. `timer_pump` invert:
`/tmp/rill-nfr-timer-pump.{out,err}`. No gate is **Proven**. Spike 0 stays
**RED**. The withdrawn `p95=0.032ms` run must not be cited.

## Blocking defects found by the audit

Independent of the gates, and shipping-blockers on their own:

| ID | Defect |
|---|---|
| S3-1 | **Stack buffer overflow** in the Chip 0 grapheme path, reachable from any process writing to the PTY |
| S3-2 | `EXIT` discarded on detach — FR-EXIT fails on the persist path |
| S3-3 | `Pty::drop` kills the child, so any daemon error path destroys the user's shell |
| S3-4 | PTY master fd exported from the kernel crate, against ADR 0001 §5 |
| S4-1 | Hosted `macos-14` cannot close T-NFR hid. Library suite ran in `gates.yml` 2026-08-17; do not re-dispatch |
| S4-2 | libghostty-vt unpinned against an API upstream calls unstable |

## Stop rule

Milestone 1 does not open until every gate above is `Proven` under ADR 0002
D2–D6, on a packaged build, with T-NFR on battery per ADR 0003.

Present is [ADR 0009](adr/0009-direct-to-display-echo.md): `toggleFullScreen:`
plus opaque echo, one in flight. ADRs 0004–0008 are exhausted. T-NFR hid is
Manual (GitHub-hosted `macos-14` has no panel). Unmutated battery hid passed;
`timer_pump` went red (p95 **30.823ms**). Do not add agents, Blocks, or chrome,
and do not re-cut the instrument (ADR 0002 D11).
