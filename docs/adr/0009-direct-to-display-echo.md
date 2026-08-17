# ADR 0009: Direct-to-display echo presenter

- **Status:** Accepted — 2026-08-17
- **Tree:** this repository only
- **Issue:** [#14](https://github.com/mahboobmonnamd/RILL/issues/14)
- **Amends:** [ADR 0004](0004-chip0-does-not-close-nfr-key.md) D1 and D3 for
  the closer only. 0004 remains the record of the 23.5 ms miss on windowed
  echo. [ADR 0005](0005-mtkview-presenter.md)–[0008](0008-cametal-display-link.md)
  are exhausted presenters, not the closer.
- **Requires:** ADR 0003 D1 atlas and D5–D8 oracle. Full libghostty exec stays
  rejected (ADR 0001 §1).
- **Evidence:** packaged battery hid, `pmset` Battery Power 28% discharging,
  p95 **7.011 ms** vs 8.33 ms (120 Hz), 1000/2 discards (0.20%), vsync on,
  cadence p50=p95=8.33 ms, `ax_trusted=1`. `timer_pump` on this presenter is
  not yet demonstrated red.

## Context

0004–0008 measured Chip 0 present knobs against the same oracle. The miss was
never VT work (`key_to_commit` ≈ 1.4–2.5 ms). It was WindowServer showing a
`CAMetalLayer` drawable:

| Path | Hid p95 | Cadence |
|---|---|---|
| Echo in a titled / borderless cover | ~23 ms | ~40 Hz |
| Heartbeat / MTKView every vsync | ~38 ms | 120 Hz, 3–4 frames queued |
| `presentAtTime:` keep-alive (0006) | 38 ms | 120 Hz queued |
| Borderless screen cover (0007) | 23 ms | 40 Hz |
| `CAMetalDisplayLink` latency 1 (0008) | 47 ms | 120 Hz, ~5 frames |

Apple's direct-to-display path is a **titled** window that calls
`toggleFullScreen:`, an **opaque** `CAMetalLayer`, RGB, Apple silicon — not a
borderless window sized to `screen.frame`. Combined with echo-only present,
one drawable in flight until `presentedTime`, same-stack PTY pump on `keyDown`,
and a `CADisplayLink` that supplies `targetTimestamp` **without** taking a
drawable, packaged hid p95 fell under one 120 Hz tick.

## Decision

### D1 — This is the Spike 0 presenter

- Titled, closable, resizable `NSWindow`. `collectionBehavior` is
  `FullScreenPrimary` only. After `makeKeyAndOrderFront:`, `toggleFullScreen:`.
- `CAMetalLayer.opaque = YES`. Drawable size is the view backing size (1:1).
  `maximumDrawableCount = 2`. `displaySyncEnabled` stays on.
- Echo calls `nextDrawable` only when the VT has damage. One present
  outstanding: the in-flight semaphore releases in `addPresentedHandler:`,
  before the next `nextDrawable`.
- `CADisplayLink` (not `CAMetalDisplayLink`) is pinned to the screen maximum.
  It stores `targetTimestamp`. Echo may `presentDrawable:atTime:` that
  deadline when it is still in the future. The link does **not** take a
  drawable.
- After `send_input`, the host `poll`s the attach socket for ≤2 ms and paints
  on that stack. Socket `dispatch_source` remains for output that is not a
  key echo.
- T-NFR hides the cursor for the measurement window.

### D2 — Rejected on this closer

- `CAMetalDisplayLink` / `preferredFrameLatency` (0008).
- Keep-alive or heartbeat Metal presents (0005, 0006).
- Borderless cover of `screen.frame` as a substitute for `toggleFullScreen:`
  (0007).
- `NSFloatingWindowLevel` during hid.
- Flattening 8.33 ms, measuring to `commit`, or turning vsync off.

### D3 — Oracle and budget do not move

T-NFR remains `NSEvent.timestamp` → `presentedTime`, vsync on, n ≥ 1000,
discards ≤ 2%, p95 < one refresh interval at the display's actual maximum
rate, HID (`CGEventPost`) only, battery as the closing run (ADR 0003 D5–D8).

### D4 — GitHub-hosted `macos-14` is not a self-hosted panel

`.github/workflows/gates.yml` already runs on GitHub-hosted `macos-14`. That
is a cloud VM: no physical display, no battery, no Accessibility identity for
this adhoc bundle. It can run library gates and T-NFR `--nfr-key=app` as a
diagnostic (ADR 0003 D7). It **cannot** close T-NFR hid.

There is no self-hosted Mac runner in this project. T-NFR hid stays **Manual**:
packaged `Rill.app`, real panel, `pmset` Battery Power / discharging, recorded
in `/tmp/rill-nfr-hid.{out,err}` and the `evidence/` artifact. A gate that has
never run in CI is not Proven for the *library* suite (ADR 0002 D8). T-NFR hid
is the exception `gates.yml` already states.

### D5 — `timer_pump` invert is recorded

Unmutated battery hid: p95 **7.011 ms**. `RILL_MUTATE=timer_pump` on the same
presenter (same-stack echo and the NFR 0.5 ms pump disabled): p95 **30.823 ms**,
cadence 33.33 ms (~30 Hz), `timer_pump=1`, 1000/2. The instrument detects a
60 Hz poll. T-NFR is still not **Proven** until library gates have a `gates.yml`
artifact (ADR 0002 D8). Hid stays Manual.

## Consequences

- Spike 0 stays Red until every gate is Proven under ADR 0002, including
  T-NFR with D5's invert. Milestone 1 stays closed.
- ADRs 0004–0008 stay in the tree as the exhausted series. Do not re-run those
  presenters as the closer.
- `docs/spec/SPEC-DISPLAY.md` and T-NFR in `docs/TEST-CASES.md` name this
  presenter.
