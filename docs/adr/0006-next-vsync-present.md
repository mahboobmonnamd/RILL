# ADR 0006: Next-vsync present, no queue-ahead

- **Status:** Accepted — 2026-08-17
- **Tree:** this repository only
- **Issue:** [#11](https://github.com/mahboobmonnamd/RILL/issues/11)
- **Amends:** ADR 0005 D1–D3 (present scheduling). Atlas draw (0003 D1) and
  T-NFR's oracle and budget (0003 D5–D8, 0004 D2, 0005 D4) do not move.
- **Requires:** [ADR 0004](0004-chip0-does-not-close-nfr-key.md),
  [ADR 0005](0005-mtkview-presenter.md) Accepted.
- **Evidence:** echo-only p95 **23.5ms** / ~40 Hz; `MTKView` ASAP every vsync
  p95 **38.2ms** / 120 Hz queued; CA pin (SPIKE-NFR-PIN) p95 **22.4ms** / ~40 Hz;
  this scheduler (2026-08-17 hid) p95 **38.030ms**, cadence 8.33ms / 120 Hz,
  `commit_to_presented` ≈ 24ms, 1000/0, `ax_trusted=1`. `presentAtTime:` and a
  two-drawable cap did not stop WindowServer from holding ~3 frames.

## Context

`CAMetalLayer.maximumDrawableCount` is 2 or 3, never 1. ProMotion on an
`NSWindow` drops to ~40 Hz when Metal presents once per key. Presenting every
vsync with `presentDrawable:` (ASAP) pins 120 Hz and lets WindowServer hold
three–four frames.

The failure is not “we forgot a flag.” It is:

- 1 in flight, present only after `presentedHandler` → cannot ramp above the
  current panel rate (stuck at 40 Hz).
- N in flight, present ASAP every vsync → 120 Hz and a queue (38ms).

ADR 0005's `MTKView` loop took a drawable every vsync without binding it to
**that** vsync. This ADR names the other scheduler: bind each drawable to the
**next** vsync with `presentDrawable:atTime:`, and refuse a third.

## Decision

### D1 — Present at `targetTimestamp`, not ASAP

A 120 Hz `CADisplayLink` supplies `targetTimestamp`. Metal presents with
`presentDrawable:atTime:` that time when it is still in the future. We do not
call `presentDrawable:` (ASAP) on the keep-alive path. ASAP is what stacked
the 38ms queue.

### D2 — At most two outstanding presents

`maximumDrawableCount = 2`. Skip `nextDrawable` when two presents have not
yet called `presentedHandler`. Skip when we already submitted for this
`targetTimestamp`. That is one frame of overlap (enough to request 120 Hz)
without a third frame in WindowServer.

### D3 — Echo publishes; it may late-latch

Socket wake feeds the VT and rebuilds instances. If fewer than two presents
are outstanding, it may present for the current `targetTimestamp` (late
latch). If both slots are full, the next vsync picks up the CPU mirror. Echo
must not take a third drawable.

Keep-alive presents of the last grid are allowed so ProMotion can stay at
120 Hz. They must follow D1–D2. They must not attribute a T-NFR sample
(`_sentinelInMirror` is cleared when a sample is encoded and when a new
sentinel is armed).

### D4 — Oracle and budget do not move

T-NFR remains `NSEvent.timestamp` → `presentedTime`, vsync on, n ≥ 1000,
discards ≤ 2%, p95 < one refresh interval at the display's actual maximum
rate, battery as the closing run.

### D5 — Chip 0's CAMetalLayer schedulers are exhausted

This scheduler missed: packaged hid p95 **38.030ms** vs 8.33ms, 120 Hz cadence,
~24ms `commit_to_presented` (2026-08-17). Stop Chip 0 present-path work. The
remaining options are a presenter that is not `CAMetalLayer` in a normal
`NSWindow` (new ADR) or Spike 0 staying Red. Do not flatten 8.33ms. Do not
repeat echo ASAP, `MTKView` ASAP, CA pin, or next-vsync `atTime`.

## Consequences

- `MTKView` stays the layer host. It stays **paused**. `drawInMTKView:` does
  not take drawables. `CAMetalLayer nextDrawable` is taken only from D1–D3.
- SPEC-DISPLAY present path is this scheduler.
- T-NFR's named test is unchanged.

## Rejected alternatives

- **CA opacity pin.** Measured: still 40 Hz (SPIKE-NFR-PIN).
- **`MTKView` unpaused / ASAP every vsync.** Measured: 38ms queue.
- **Echo-only ASAP.** Measured: 23.5ms / 40 Hz.
- **Off-vsync, measure to `commit`, 16.7ms budget.** Rejected by 0003/0004.
- **Chip 1 / full libghostty exec.** Rejected by 0001; does not move this wait.
