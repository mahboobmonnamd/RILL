# Spike 0 — attach, persist, Chip 0

**Status: GREEN. Closed 2026-08-17 by
[ADR 0010](adr/0010-spike-0-closes.md).** Every named gate is **Proven** under
[ADR 0002](adr/0002-falsifiable-evidence.md) D2–D6. Library and packaged gates
cite GitHub Actions
[run 31993832263](https://github.com/mahboobmonnamd/RILL/actions/runs/31993832263).
T-NFR cites packaged battery hid (ADR 0009 D4). Milestone 1 is
[ADR 0011](adr/0011-session-graph.md) / [#16](https://github.com/mahboobmonnamd/RILL/issues/16).

The 2026-08-16 marks remain withdrawn ([SPIKE-0-AUDIT](SPIKE-0-AUDIT.md)).
`T-NFR p95=0.032ms` is still a **null result** and must not be cited.

Authority: [PRD](PRD.md), [ADR 0001](adr/0001-session-operating-system.md),
[ADR 0002](adr/0002-falsifiable-evidence.md),
[ADR 0003](adr/0003-display-pipeline.md),
[ADR 0009](adr/0009-direct-to-display-echo.md) (closer),
[ADR 0010](adr/0010-spike-0-closes.md) (close).
ADRs 0004–0008 are exhausted presenters.
Gate definitions: [TEST-CASES](TEST-CASES.md).
Closure procedure: [SPIKE-0-VALIDATION](SPIKE-0-VALIDATION.md).

Run (regression): `sh scripts/validate-spike0.sh`. Do not dispatch `gates.yml`
to reprint hid.

## Goal

One window. Chip 0. Kernel-owned PTY. Framed attach. Quit keeps the shell. A key
paints in one frame. Then stop and measure — with an instrument that can say no.

## Explicitly not in this spike

Sidebar, tabs, Blocks, agents, scheduler, theme store, full Ghostty GPU exec,
Chip 1 as the live chip. Those may start under Milestone 1+; they must not hide
a later NFR miss (ADR 0002 D11).

## Named gates

Each gate's oracle, procedure, required mutation, and negative control are
defined in [TEST-CASES](TEST-CASES.md). A gate reaches `Proven` only after being
demonstrated **red** on a build where the behaviour is absent (ADR 0002 D2).

| ID | Test | Fails while… | Status |
|---|---|---|---|
| T-BYTES | invalid UTF-8 through the kernel ring and Chip 0 feed | kernel history drops bytes, or Chip 0 never shows a non-ASCII cell for a high-byte fixture | **Proven** — `drop_high_bytes` went red in CI, then unmutated passed |
| T-DROP | unbounded `yes`, finite credit, `^C`, keep typing | any sequence number missing, or the kernel never stalls its reads | **Proven** — `drop_on_full` went red in CI, then unmutated passed |
| T-ATTACH | attach → detach → attach; cell-by-cell grid compare | grids diverge, or a bare connection displaces the attached client | **Proven** — `accept_replaces_client` went red in CI, then unmutated passed |
| T-RESIZE | child's own `TIOCGWINSZ` after `SIGWINCH`, with pending input | child's size ≠ display geometry, or resize overtakes queued input | **Proven** — `resize_before_data` went red in CI, then unmutated passed |
| T-EXIT | `exit`, including **while detached** | reopened window paints a cursor over a dead process | **Proven** — `clear_outbound_on_detach` went red in CI, then unmutated passed |
| T-SPAWN | `nm -u` + `otool -Iv` on the packaged GUI, plus a positive control | PTY-creation primitives imported, or the check itself is broken | **Proven** — CI packaged GUI has no PTY imports; `openpty_in_main_m` went red |
| T-KILL | packaged `Rill.app`, `SIGKILL` the process group and AppKit Quit | child pid changes, or reattach is blank | **Proven** — CI persist_e2e; `drop_POSIX_SPAWN_SETSID` went red |
| T-RESYNC | reopen idle `zsh` and alt-screen `vim` | blank window over a live process, or resync touches the warm path | **Proven** — `no_resync` went red in CI, then unmutated passed |
| T-NFR | key-down `NSEvent.timestamp` → drawable `presentedTime`, on battery | p95 over one refresh interval, discards > 2%, or any control RPC | **Proven** — battery hid p95 **7.011ms**; `timer_pump` p95 **30.823ms** (Manual hid, ADR 0009 D4) |

CI artifact (ADR 0002 D8): [run 31993832263](https://github.com/mahboobmonnamd/RILL/actions/runs/31993832263)
on `d20568e`, evidence `spike0-20260817T041912Z.json`. Battery hid:
`/tmp/rill-nfr-hid.{out,err}` 2026-08-17. `timer_pump`:
`/tmp/rill-nfr-timer-pump.{out,err}`. The withdrawn `p95=0.032ms` run must not
be cited.

## Audit defects

Addressed by named tests, lints, or the pin. Not open shipping-blockers:

| ID | Defect | Close |
|---|---|---|
| S3-1 | Chip 0 grapheme stack overflow | T-BYTES ASan + `fixtures/bytes/zwj_emoji.bin` |
| S3-2 | `EXIT` discarded on detach | T-EXIT-detach |
| S3-3 | `Pty::drop` kills the child | T-KILL |
| S3-4 | PTY master fd exported | `lint-planes` no-master-export |
| S4-1 | Hosted `macos-14` cannot close hid | ADR 0009 D4 / 0010 D2 |
| S4-2 | libghostty-vt unpinned | `third_party/ghostty.pin` + `build.rs` |

## Stop rule (satisfied)

Milestone 1 may open ([ADR 0010](adr/0010-spike-0-closes.md) D3). Present is
[ADR 0009](adr/0009-direct-to-display-echo.md). Do not add agents, Blocks, or
chrome to hide a later NFR miss, and do not re-cut the instrument (ADR 0002 D11).
