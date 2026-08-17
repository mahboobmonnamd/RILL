# ADR 0010: Spike 0 closes

- **Status:** Accepted — 2026-08-17
- **Tree:** this repository only
- **Issue:** [#14](https://github.com/mahboobmonnamd/RILL/issues/14)
- **Amends:** [ADR 0002](0002-falsifiable-evidence.md) D11 (now satisfied);
  [ADR 0009](0009-direct-to-display-echo.md) consequences. D2–D7 and D9–D10
  stand. D8 stands for the library suite; T-NFR hid remains the D4 exception
  already named in 0009.
- **Requires:** [ADR 0009](0009-direct-to-display-echo.md) presenter. Oracle
  and budget do not move (ADR 0003 D5–D8).
- **Evidence:** GitHub Actions
  [run 31993832263](https://github.com/mahboobmonnamd/RILL/actions/runs/31993832263)
  (`d20568e`, `spike0-20260817T041912Z.json`); packaged battery hid p95
  **7.011 ms**; `timer_pump` p95 **30.823 ms**.
- **Amended by:** [ADR 0012](0012-chip1-isolated-vt.md) — Chip 1 stays isolated
  until **M7** (live swap). M4 is the crate.

## Context

The 2026-08-16 `Proven` marks were revoked (ADR 0002 D1, [SPIKE-0-AUDIT](../SPIKE-0-AUDIT.md)).
The gates were rewritten so they can fail. On 2026-08-17:

- Hosted `macos-14` ran `gates.yml` with negative controls. Every library and
  packaged gate except T-NFR was green unmutated; each named mutation went
  red. T-NFR `--nfr-key=app` timed out at 45 s (no panel). That job is the
  D8 artifact. It cannot close hid (0009 D4).
- Packaged `--nfr-key=hid` on battery, from Terminal.app, met the budget
  (p95 7.011 ms vs 8.33 ms at 120 Hz, 1000/2, vsync on, `ax_trusted=1`).
  `RILL_MUTATE=timer_pump` missed (p95 30.823 ms). That is D2 for T-NFR.

Waiting for a self-hosted Mac to reprint hid would spend Actions minutes on a
runner that still has no panel. ADR 0004 already rejected closing Spike 0
*except* T-NFR. This ADR closes with T-NFR, using the Manual hid 0009 already
named as the closer.

## Decision

### D1 — Library and packaged gates are Proven from the CI artifact

T-BYTES, T-DROP, T-ATTACH, T-RESIZE, T-EXIT, T-SPAWN, T-KILL, and T-RESYNC are
**Proven** under ADR 0002 D2–D6 and D8. Cite
[run 31993832263](https://github.com/mahboobmonnamd/RILL/actions/runs/31993832263),
not a laptop transcript.

### D2 — T-NFR is Proven from packaged battery hid

T-NFR is **Proven** under D2–D6 on a packaged build, on battery, per ADR 0003
D5–D8 and ADR 0002 D11. The demonstrated red is `timer_pump` (p95 30.823 ms).
The green is unmutated hid (p95 7.011 ms). Hosted `macos-14` is not that
measurement (0009 D4). Do not flatten 8.33 ms, measure to `commit`, or turn
vsync off.

### D3 — Milestone 1 may open

[SPIKE-0](../SPIKE-0.md) is closed. The stop rule in ADR 0002 D11 is satisfied.
M1 (session graph) may take issues. Do not add agents, Blocks, or chrome to
**hide a later NFR miss** (D11's second paragraph still applies). Chip 1 stays
isolated until M4.

### D4 — Do not re-dispatch hosted `gates.yml` to chase hid

Another GitHub-hosted macOS job cannot close hid and is not required to keep
D1. `fast.yml` remains the cheap push gate.

## Consequences

- Status docs mark every Spike 0 gate **Proven**. The `fast.yml` step that
  forbade that string is deleted in the closer commit.
- M0 GitHub issues close against this evidence. M1 is unblocked.
- The withdrawn `p95=0.032ms` run remains unciteable.

## Rejected alternatives

- **Wait for a self-hosted Mac.** Rejected: 0009 D4 already recorded that
  hosted `macos-14` cannot close hid. The closer already ran on a real panel.
- **Proven except T-NFR.** Rejected by ADR 0004. This close includes T-NFR.
- **Re-run `gates.yml` until app-mode p95 is under 8.33 ms.** Rejected: app
  mode is diagnostic (ADR 0003 D7). Hid is the closer.
