# Spike — pin 120 Hz without extra Metal presents

**Status: research. Closed by [ADR 0009](adr/0009-direct-to-display-echo.md) /
[ADR 0010](adr/0010-spike-0-closes.md). Do not merge this pin as a presenter.**
**Issue:** [#10](https://github.com/mahboobmonnamd/RILL/issues/10) (closed)

This file is the 2026-08-17 experiment that **did not** pin 120 Hz. Spike 0 is
Proven on ADR 0009, not on this pin. T-NFR's oracle and budget do not move.

## Why

Chip 0 has two honest misses:

| Present | Cadence | p95 | What happened |
|---|---|---|---|
| Echo-only `presentDrawable:` | ~40 Hz | **23.5ms** | ProMotion dropped; one frame at 40 Hz |
| `MTKView` every vsync | 120 Hz | **38.2ms** | Three Metal frames queued |

Empty `CADisplayLink` requesting 120 Hz did **not** pin Metal presents. Extra
Metal presents did pin 120 Hz and queued the swapchain. Native VT / Chip 1
does not change the compositor.

## Question

Can a **non-Metal** 120 Hz request hold the panel at 120 Hz while Metal
presents **once per echo**, one drawable in flight, `presentDrawable:atTime:`
the next vsync?

If yes, `commit_to_presented` should be ~one 8.33ms tick, not ~25ms.

## What this spike tries

1. `MTKView` **paused**. Draw only when the grid is dirty (`[self draw]` from
   socket wake). No per-vsync `nextDrawable`.
2. A 1×1 Core Animation opacity animation with
   `preferredFrameRateRange` 120–120–120. It must not take a Metal drawable.
3. A `CADisplayLink` that **only** stores `targetTimestamp`. It must not
   present.
4. At most one Metal drawable in flight. Skip, then present when the previous
   `presentedHandler` fires.
5. `presentDrawable:atTime:targetTimestamp` when that time is still in the
   future; otherwise present on the next vsync.

## What this spike will not do

- Flatten 8.33ms, measure to `commit`, or turn vsync off.
- Heartbeat Metal presents.
- Take Chip 1 or full libghostty exec.
- Merge. A miss stays Red (ADR 0005 D5). A hit still needs a new ADR.

## How we will know

Packaged hid, `ax_trusted=1`. Look at `present_cadence` and the first eight
`T-NFR seg` lines.

| Cadence | `commit_to_presented` | Meaning |
|---|---|---|
| ~25ms (~40 Hz) | ~20ms | Pin failed. Same as echo-only. |
| ~8.3ms (120 Hz) | ~24ms | Pin worked, Metal still queued. Same as MTKView. |
| ~8.3ms **or** one present per key at 120 Hz ticks | ~8ms | Question answered yes. Still not Proven until ADR → spec → T-NFR → battery hid. |

## Result — 2026-08-17 hid

**No.** Packaged hid, `ax_trusted=1`, 1000/0 discarded:

```
SPIKE-NFR-PIN: CA pin + one-in-flight echo draw
p50=21.824ms p95=22.421ms p99=46.755ms max=78.119ms
present_cadence p50=25.00ms (~40Hz) n=1021
```

After warmup, `key_to_commit` ≈ 1.5–4ms, `commit_to_presented` ≈ 17–19ms,
`present_delta` = 25ms. Same class as echo-only 23.5ms / 40 Hz. The CA
opacity pin plus a DisplayLink that does not present did **not** hold Metal
onto 120 Hz ticks.

Must not merge this pin. Spike 0 closed on ADR 0009, not here. Do not repeat
this experiment as a closer.
