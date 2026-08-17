# ADR 0005: Chip 0 presents via MTKView

- **Status:** Accepted — 2026-08-17
- **Tree:** this repository only
- **Issue:** [#9](https://github.com/mahboobmonnamd/RILL/issues/9)
- **Amends:** ADR 0003 D2 (present ownership only). Atlas draw (0003 D1) and
  T-NFR's oracle and budget (0003 D5–D8, 0004 D2) do not move.
- **Requires:** [ADR 0004](0004-chip0-does-not-close-nfr-key.md) Accepted.
- **Evidence:** the 2026-08-16 TerminalView series (echo-only ~40 Hz / 23.5ms;
  heartbeat+queue ~120 Hz / 38ms; one-in-flight during sample ~40 Hz again).
  That series is the research spike 0004 D4 required. It must not be repeated.

## Context

[ADR 0004](0004-chip0-does-not-close-nfr-key.md) D3 records that Chip 0's
glyph-atlas draw is not the miss: `key_to_commit` ≈ 1.4ms. The miss is
`commit` → `presentedTime` (~20–22ms) at a ~40 Hz present cadence.

The failed series stacked `nextDrawable` from echo **and** from a
`CADisplayLink` heartbeat. Extra presents pinned ProMotion at 120 Hz and queued
three–four vsyncs. Skipping them returned 40 Hz. 0004 forbids more knobs on
that path.

A vsync-owned draw loop that takes **one** drawable per callback, with echo
never calling `nextDrawable`, is a different presenter. `MTKView` is that loop.
It is still our Metal inside an `NSWindow`. It is not Ghostty's GPU exec
(ADR 0001 §1).

This may still miss one 120 Hz frame: even a correct 120 Hz swapchain has a
phase wait. If packaged battery hid still misses, Spike 0 stays Red (0004 D1).
We do not flatten the instrument.

## Decision

### D1 — `TerminalView` is an `MTKView`

`TerminalView` subclasses `MTKView` and is its own `MTKViewDelegate`.
`preferredFramesPerSecond` is the screen's `maximumFramesPerSecond`.
`enableSetNeedsDisplay` is off: the view draws on vsync, not on
`setNeedsDisplay:`. `paused` is off while the view is in a window.

### D2 — Echo does not present

Socket wake (ADR 0003 D2) still feeds the VT and rebuilds damaged instance
rows. It MUST NOT call `nextDrawable` or `presentDrawable:`. Present happens
only in `drawInMTKView:`, from `currentDrawable` / `currentRenderPassDescriptor`.

### D3 — Two drawables, not a stacked heartbeat

`CAMetalLayer.maximumDrawableCount = 2`. `displaySyncEnabled` stays on for the
recorded number. We do not add a second `CADisplayLink` that takes drawables.
We do not present from both echo and the vsync callback.

Instance buffers stay triple-buffered (0003 D1). Drawable count is 2 so the
compositor cannot hold three–four unpresented frames the way the heartbeat
series did.

### D4 — Oracle and budget do not move

T-NFR remains `NSEvent.timestamp` → `presentedTime`, vsync on, n ≥ 1000,
discards ≤ 2%, p95 < one refresh interval at the display's actual maximum
rate, battery as the closing run. A miss is a miss.

### D5 — A miss does not authorize the next knob

If this presenter still misses on packaged battery hid, stop. Do not restore
echo `presentDrawable:`. Do not add heartbeat presents. Do not take Chip 1 or
Ghostty GPU exec. A later presenter needs a later ADR.

## Consequences

- SPEC-DISPLAY §3: feed is event-driven; present is `MTKView`'s vsync callback.
- SPEC-DISPLAY §4: `maximumDrawableCount = 2`.
- T-NFR's named test is unchanged. It still fails for p95 23.5ms vs 8.33ms on
  the Chip 0 echo present path until this presenter is measured.
- Package links MetalKit.

## Rejected alternatives

- **More `TerminalView` echo/heartbeat knobs.** Rejected by ADR 0004 D3.
- **`CADisplayLink` that presents.** That was the 38ms series.
- **Off-vsync / measure to `commit` / 16.7ms budget.** Rejected by 0003 and 0004.
- **Full libghostty exec.** Rejected by ADR 0001 §1.
- **Native VT / Chip 1 as the live chip.** Does not change the compositor; Spike 0
  stop rule still holds.
