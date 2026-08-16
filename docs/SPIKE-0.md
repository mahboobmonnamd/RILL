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
[ADR 0003](adr/0003-display-pipeline.md).
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
| T-BYTES | invalid UTF-8 corpus through the VT's own re-emission | emulator sees `U+FFFD` where the fixture did not encode it | **Red** — old test compared the input to our own copy of the input |
| T-DROP | unbounded `yes`, finite credit, `^C`, keep typing | any sequence number missing, or the kernel never stalls its reads | **Red** — old test granted infinite credit and asserted `head` works |
| T-ATTACH | attach → detach → attach; cell-by-cell grid compare | grids diverge, or a bare connection displaces the attached client | **Red** — connect-without-attach hole fails today |
| T-RESIZE | child's own `TIOCGWINSZ` after `SIGWINCH`, with pending input | child's size ≠ display geometry, or resize overtakes queued input | **Red** — old test round-tripped our own ioctl |
| T-EXIT | `exit`, including **while detached** | reopened window paints a cursor over a dead process | **Red** — fails today: `detach()` discards the `EXIT` frame |
| T-SPAWN | `nm -u` + `otool -Iv` on the packaged GUI, plus a positive control | PTY-creation primitives imported, or the check itself is broken | **Red** — old test used `nm -U`, which excludes the symbols it asserted on |
| T-KILL | packaged `Rill.app`, `SIGKILL` the process group and AppKit Quit | child pid changes, or reattach is blank | **Red** — procedure is sound, never demonstrated red |
| T-RESYNC | reopen idle `zsh` and alt-screen `vim` | blank window over a live process, or resync touches the warm path | **Red** — old test asserted on a prefix it prepended itself |
| T-NFR | key-down `NSEvent.timestamp` → drawable `presentedTime`, on battery | p95 over one refresh interval, discards > 2%, or any control RPC | **Red** — old test found glyphs the shell had already echoed, and stopped at the POD snapshot |

Two gates fail against `main` with no mutation applied — T-EXIT's detach case
and T-ATTACH's connect-without-attaching case. They are the first tests to
write.

## Blocking defects found by the audit

Independent of the gates, and shipping-blockers on their own:

| ID | Defect |
|---|---|
| S3-1 | **Stack buffer overflow** in the Chip 0 grapheme path, reachable from any process writing to the PTY |
| S3-2 | `EXIT` discarded on detach — FR-EXIT fails on the persist path |
| S3-3 | `Pty::drop` kills the child, so any daemon error path destroys the user's shell |
| S3-4 | PTY master fd exported from the kernel crate, against ADR 0001 §5 |
| S4-1 | No CI. Every gate is enforced by a human remembering to run a script |
| S4-2 | libghostty-vt unpinned against an API upstream calls unstable |

## Stop rule

Milestone 1 does not open until every gate above is `Proven` under ADR 0002
D2–D6, on a packaged build, with T-NFR on battery per ADR 0003.

If the honest number misses, that is the spike succeeding. Do not add agents,
Blocks, or chrome to hide it — and do not re-cut the instrument to flatter it
(ADR 0002 D11).
