# Spike 0 — validation to close

Authority: [ADR 0001](adr/0001-session-operating-system.md),
[ADR 0002](adr/0002-falsifiable-evidence.md),
[ADR 0003](adr/0003-display-pipeline.md).
Gate definitions: [TEST-CASES](TEST-CASES.md).
Findings: [SPIKE-0-AUDIT](SPIKE-0-AUDIT.md).

Run: `sh scripts/validate-spike0.sh` → writes `evidence/spike0-<utc>.json`.

## Evidence classes

| Class | Means |
|---|---|
| **Proven** | Demonstrated red on a build lacking the behaviour, then green, with both recorded in the evidence artifact or CI (ADR 0002 D2) |
| **Green-unproven** | Passes, never demonstrated red. **Not evidence.** |
| **Red** | Fails, or has no test |
| **Manual** | Requires a human step; the recorded artifact, not a pasted transcript, is what a PR cites |

`Partial` is retired. A gate either closes or it does not. "Partial" is what let
`p95=0.032ms` sit in a status table for a day looking like progress.

## Closure matrix

| ID | Command | Closes only with |
|---|---|---|
| T-BYTES | `cargo test -p rill-chip0 t_bytes` + `cargo test -p rill-kernel t_bytes` + ASan run over `fixtures/bytes/` | Proven, ASan clean |
| T-DROP | `cargo test -p rill-kernel t_drop` | Proven, `stalled_reads > 0` recorded |
| T-ATTACH | `cargo test -p rilld t_attach` | Proven, cell-by-cell compare |
| T-RESIZE | `cargo test -p rill-kernel t_resize` | Proven, child-reported winsize |
| T-EXIT | `cargo test -p rill-kernel t_exit` + `cargo test -p rilld t_exit_across_detach` | Proven, including the detached case |
| T-SPAWN | `cargo test -p rill-host --test t_spawn` after packaging | Proven, **and the positive control reports a violation** |
| T-KILL | `cargo test -p rilld --test persist_e2e` against packaged `Rill.app` | Proven on the packaged spawn path |
| T-RESYNC | `cargo test -p rilld t_resync` | Proven, idle shell and alt-screen |
| T-NFR | packaged `Rill --nfr-key=hid` | Proven **on battery**, discards ≤ 2%, p95 < one refresh interval |

Socket-only tests do not close T-KILL, T-SPAWN, or T-NFR. In-process fixtures do
not close anything user-visible.

## Preconditions

A missing precondition is a **failure**, never a skip (ADR 0002 D5). The
`RILL_REQUIRE_*` opt-in flags are deleted.

- `third_party/ghostty` checked out at the SHA in `third_party/ghostty.pin`,
  verified by `build.rs`
- `dist/Rill.app` packaged from the current tree
- A physical display; for T-NFR `hid` mode, Accessibility trust
- For the closing T-NFR run: on battery, lid open, no external power

## Negative controls

`sh scripts/validate-spike0.sh --negative-controls` applies each mutation from
[TEST-CASES](TEST-CASES.md) via `RILL_MUTATE` and asserts the corresponding gate
turns **red**. A gate that stays green under its own mutation is reported as a
**broken instrument** and fails the run — regardless of how the unmutated gate
behaved.

This is the check that would have caught all three S1 findings on the day they
were written.

## Evidence artifact

`evidence/spike0-<utc>.json`:

```json
{
  "utc": "...", "git_sha": "...", "ghostty_sha": "...",
  "host": {"model": "...", "macos": "...", "power": "battery|ac", "refresh_hz": 120},
  "gates": [
    {"id": "T-NFR", "command": "...", "exit": 0, "stdout": "...",
     "class": "Proven", "red_demonstrated_at": "<commit or mutation>",
     "metrics": {"p50_ms": 0, "p95_ms": 0, "p99_ms": 0, "max_ms": 0,
                 "samples": 1000, "discarded": 0, "mode": "hid", "vsync": true}}
  ],
  "negative_controls": [{"gate": "T-NFR", "mutation": "timer_pump", "went_red": true}]
}
```

The human summary is rendered **from** this file. No summary line may be printed
without a corresponding recorded result — the previous script hardcoded `pass`
for three gates, one of which it never ran (audit S4-3).

## Current state — 2026-08-17

**Does not close Spike 0.** No gate is Proven. Do not dispatch `gates.yml`
again: hosted `macos-14` cannot close T-NFR hid, and the library suite already
has a D8 artifact.

CI (ADR 0002 D8, library and packaged gates):
[run 31993832263](https://github.com/mahboobmonnamd/RILL/actions/runs/31993832263)
on `d20568e`, artifact
[spike0-evidence](https://github.com/mahboobmonnamd/RILL/actions/runs/31993832263/artifacts/9276297736)
(`spike0-20260817T041912Z.json`). Host: `VirtualMac2,1`, macOS 14.8.7, AC,
`refresh_hz` null, `nfr_mode=app`.

- Unmutated exit 0: LINT-PLANES, T-BYTES (chip / ASan / kernel), T-DROP,
  T-RESIZE, T-EXIT, T-EXIT-detach, T-ATTACH, T-RESYNC, PACKAGE (`nm -u` none),
  T-SPAWN, T-KILL. Class in the artifact: **Green-unproven**.
- Mutations went red: `drop_high_bytes`, `drop_on_full`, `resize_before_data`,
  `clear_outbound_on_detach`, `accept_replaces_client`, `no_resync`,
  `drop_POSIX_SPAWN_SETSID`, `openpty_in_main_m`.
- T-NFR in that job: **Red**, `timed out after 45s` (`RILL_NFR_OPTIONAL=1` so
  the job still succeeded). `timer_pump` is `went_red: null`. Diagnostic only.
- T-NFR hid stays **Manual** (ADR 0009): laptop battery p95 **7.011ms** vs
  8.33ms, 1000/2, `ax_trusted=1`. `timer_pump` went red (p95 **30.823ms**)
  from Terminal.app. Hosted CI cannot repeat that.
- ASan over `fixtures/bytes/` ran in the suite and again as a separate step
  (`CARGO_TARGET_DIR` isolated).

The withdrawn 2026-08-16 run (`p95=0.032ms control_rpc=0`) must not be cited.
