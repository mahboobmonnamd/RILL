# ADR 0007: Spike 0 window is opaque fullscreen

- **Status:** Accepted — 2026-08-17
- **Tree:** this repository only
- **Issue:** [#12](https://github.com/mahboobmonnamd/RILL/issues/12)
- **Amends:** nothing in ADR 0001–0003 except where this fills “one window.”
  Atlas draw (0003 D1) and T-NFR's oracle and budget (0003 D5–D8, 0004 D2)
  do not move. ADR 0006 D5 required a surface that is not `CAMetalLayer` in a
  **normal** titled `NSWindow`. This is that surface.
- **Requires:** [ADR 0006](0006-next-vsync-present.md) Accepted (schedulers
  exhausted).
- **Evidence:** titled-window series — 23.5ms / 40 Hz, 38ms / 120 Hz queued,
  CA pin 22.4ms / 40 Hz, next-vsync `atTime` 38.0ms / 120 Hz queued. Opaque
  fullscreen hid (2026-08-17): p95 **23.249ms**, cadence 25ms / ~40 Hz,
  `commit_to_presented` ≈ 21ms, 1000/1, `ax_trusted=1`. Same class as
  echo-only. The surface did not change when Metal frames appear.

## Context

WindowServer composites a titled, shadowed, desktop-neighbour `NSWindow`.
On ProMotion that path either drops Metal to ~40 Hz or queues ~3 frames at
120 Hz. Chip 0's drawable schedulers cannot fix that (0006 D5).

Spike 0 is still one window, Chip 0, our Metal. The window is the surface
the compositor treats as the display: opaque, borderless, covering the
screen. Titled chrome is Milestone 1 work after Spike 0 is Proven — it is
not how we close NFR-KEY.

The measurement must be this shipped window, not a fullscreen overlay used
only under `--nfr-key`.

## Decision

### D1 — Opaque screen-covering window

`NSWindow` is `NSWindowStyleMaskBorderless`, `opaque`, no shadow, frame
equal to the screen's frame. It can become key (borderless otherwise cannot).
The glyph-atlas view is the content view. Dock/menu hiding is allowed.

### D2 — Echo presents; no Metal keep-alive flood

Socket wake feeds the VT and rebuilds instances, then presents if no
drawable is outstanding. A `CADisplayLink` may supply `targetTimestamp`.
It MUST NOT take a drawable every vsync. That flood is the 38ms queue
(0005, 0006).

At most **one** present outstanding (`presentedHandler` before the next
`nextDrawable`). `presentDrawable:atTime:` when the timestamp is in the
future; otherwise present for the next vsync. Layer `maximumDrawableCount`
stays 2 because the API forbids 1.

### D3 — Oracle and budget do not move

T-NFR remains `NSEvent.timestamp` → `presentedTime`, vsync on, n ≥ 1000,
discards ≤ 2%, p95 < one refresh interval at the display's actual maximum
rate, battery as the closing run.

### D4 — Spike 0 stays Red

Packaged hid on this surface missed: p95 **23.249ms** vs 8.33ms, ~40 Hz
cadence (2026-08-17). Stop. Do not restore titled-window present knobs. Do
not take Chip 1 or full libghostty exec. Spike 0 stays Red until a later ADR
names a presenter that is not `CAMetalLayer` in an `NSWindow` (titled or
fullscreen).

## Consequences

- SPEC-DISPLAY: the Spike 0 window is screen-covering and opaque.
- Interactive `Rill.app` is that window. Cmd-Q quits. T-NFR uses the same
  surface.
- Lane D still must not add tabs, splits, or chrome.

## Rejected alternatives

- **Another `CAMetalLayer` scheduler in a titled window.** Exhausted (0006 D5).
- **Fullscreen only while `--nfr-key` runs.** Measures a different app.
- **Off-vsync / measure to `commit` / 16.7ms budget.** Rejected by 0003/0004.
- **Full libghostty exec.** Rejected by ADR 0001 §1.
