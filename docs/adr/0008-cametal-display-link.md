# ADR 0008: CAMetalDisplayLink at latency 1

- **Status:** Accepted — 2026-08-17
- **Tree:** this repository only
- **Issue:** [#13](https://github.com/mahboobmonnamd/RILL/issues/13)
- **Amends:** ADR 0006–0007 present scheduling only. Atlas (0003 D1) and
  T-NFR's oracle and budget (0003 D5–D8) do not move. The 0007 fullscreen
  surface stays (Apple: windowed mode may increase `preferredFrameLatency`).
- **Requires:** [ADR 0007](0007-opaque-fullscreen.md) Accepted.
- **Evidence:** `nextDrawable` series 23–38ms. This API (2026-08-17 hid):
  p95 **46.901ms**, cadence 8.33ms / 120 Hz n=5999, `commit_to_presented`
  **41.56ms** (~5 frames), 1000/2, `ax_trusted=1`. `preferredFrameLatency = 1`
  is GPU slack, not input-to-photon. macOS still queued extra frames. Worse
  than echo-only 23ms.

## Context

Every Chip 0 path that called `nextDrawable` / `presentDrawable:` itself
missed: 23ms at ~40 Hz or 38ms queued at 120 Hz, including opaque fullscreen
(0007). That is WindowServer plus a drawable we took.

Apple’s ProMotion Metal presenter is `CAMetalDisplayLink` bound to the
`CAMetalLayer`. The system calls `metalDisplayLink:needsUpdate:` with a
drawable already tied to a vsync, a CPU deadline (`targetTimestamp`), and a
presentation time (`targetPresentationTimestamp`). `preferredFrameLatency = 1`
requests one frame of GPU slack. We never used this API.

Apple Developer Forums (MTKView fullscreen / `presentAfterMinimumDuration`
~25ms on ProMotion): hide the cursor in exclusive fullscreen or present
pacing breaks. T-NFR hides the cursor for the measurement window.

## Decision

### D1 — The system owns the drawable

`CAMetalDisplayLink` is created with the view’s `CAMetalLayer`.
`preferredFrameLatency = 1.0`. `preferredFrameRateRange` is 120–120–120 (or
the screen maximum). Echo MUST NOT call `nextDrawable`. Present happens only
in `metalDisplayLink:needsUpdate:`, using `update.drawable`.

### D2 — Present before `targetTimestamp`

Encode the latest CPU instance mirror onto that drawable and
`presentDrawable:` **without** `atTime` before `update.targetTimestamp`.
`presentAtTime:` throws `CAMetalDrawableInvalidOperation` on a display-link
drawable.

### D3 — Oracle and budget do not move

T-NFR remains `NSEvent.timestamp` → `presentedTime`, vsync on, n ≥ 1000,
discards ≤ 2%, p95 < one refresh interval at the display's actual maximum
rate, battery as the closing run.

### D4 — This API missed, worse than echo-only

Packaged hid p95 **46.901ms** vs 8.33ms (2026-08-17). Stop. Do not set
`preferredFrameLatency = 2` (more frames). Do not return to `nextDrawable`
schedulers. Do not flatten 8.33ms. Spike 0 stays Red.

## Consequences

- SPEC-DISPLAY present path is `CAMetalDisplayLink`.
- T-NFR hides `NSCursor` for the run (0008 context).
- `MTKView` stays paused; it is only the layer host.

## Rejected alternatives

- **`nextDrawable` + CADisplayLink / MTKView / CA pin / fullscreen echo.**
  Measured misses (0004–0007).
- **`preferredFrameLatency = 2`. ** Rejected for NFR-KEY; that is two frames.
- **Off-vsync / measure to `commit` / 16.7ms.** Rejected by 0003/0004.
- **Full libghostty exec.** Rejected by ADR 0001 §1.
