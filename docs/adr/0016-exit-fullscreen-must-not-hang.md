# ADR 0016: Leaving fullscreen must not hang

- **Status:** Accepted — 2026-08-17
- **Tree:** this repository only
- **Issue:** [#257](https://github.com/mahboobmonnamd/RILL/issues/257)
- **Requires:** [ADR 0009](0009-direct-to-display-echo.md)
- **Amends:** ADR 0009 D1 — `toggleFullScreen:` is the enter path. Leave must
  complete. Present MUST NOT wait forever on the main thread.
- **Does not authorize:** a borderless `screen.frame` cover, dropping
  `toggleFullScreen:`, a second T-NFR instrument, tab chrome.

## Context

`make run` opens a titled window and enters a fullscreen Space (ADR 0009).
Clicking the green button (or `toggleFullScreen:` again) to return to a
normal window hung the process. Force quit was required.

`presentEcho` waits `DISPATCH_TIME_FOREVER` on the in-flight semaphore, then
calls `nextDrawable`, from `setFrameSize:` on the main thread. Fullscreen
exit resizes the layer while a drawable is outstanding. The presented
handler never runs, the semaphore never signals, and the next resize blocks
the run loop forever.

## Decision

### D1 — Present must not own the main thread

`presentEcho` MUST bound the in-flight wait. A command-buffer completed
handler MUST release the semaphore if `presentedTime` never arrives.
Skipping every frame during the Space transition stalls leave (the
compositor still needs drawables). Live windowed resize MAY skip.

### D2 — Leave is the same API as enter

The green button and a second `toggleFullScreen:` are the same path. After
leave, the window is titled and resizable again. Do not substitute a
borderless cover (0009 D2).

### D3 — Oracle

Named test `t_exit_fullscreen_does_not_hang_the_window`. A main-thread
heartbeat file must keep advancing after leave. Mutation
`wait_forever_on_inflight` MUST hang that gate. Socket-only tests do not
close it. T-NFR is not re-cut.

## Consequences

SPEC-DISPLAY §3 names leave. Packaged e2e is the closer.

## Rejected alternatives

- **Disable the green button / stay fullscreen forever.** Rejected: the
  window is titled and resizable.
- **`nextDrawable` from a background queue.** Rejected: Metal drawables are
  taken on the presenting thread; skip the frame instead.
