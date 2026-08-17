# ADR 0019: Dock reopen must show the window

- **Status:** Accepted — 2026-08-17
- **Tree:** this repository only
- **Issue:** [#262](https://github.com/mahboobmonnamd/RILL/issues/262)
- **Requires:** [ADR 0009](0009-direct-to-display-echo.md),
  [ADR 0018](0018-three-pane-host-chrome.md)
- **Amends:** nothing. Leave (ADR 0016) and T-NFR fullscreen (ADR 0009) stand.
- **Does not authorize:** a second window, tab chrome, dropping
  `toggleFullScreen:` on T-NFR, recutting T-NFR.

## Context

`make run` opens a titled window and enters a fullscreen Space (ADR 0009).
Switching to another app leaves that Space. Clicking Rill in the Dock does
not show the window. The process stays in the Dock; quit and `make run`
again is required.

The packaged GUI never sets an `NSApplicationDelegate`. Dock click of a
running app sends `applicationShouldHandleReopen:hasVisibleWindows:`. With
no delegate, that message is dropped. After a Space switch, AppKit reports
no visible windows on the active Space (`hasVisibleWindows=NO`), so the
default is to do nothing.

## Decision

### D1 — Reopen orders the existing window front

`NSApp` MUST have a delegate. `applicationShouldHandleReopen:hasVisibleWindows:`
MUST `unhide`, deminiaturize if needed, and `makeKeyAndOrderFront:` the
existing window, then activate. Ignore `hasVisibleWindows`; the window may
be alive on another Space. Do not create a second window.

`applicationDidBecomeActive:` MUST restore the same way when the window is
not visible or not on the active Space (Cmd-Tab back from another Space).

The window is retained by the delegate. `releasedWhenClosed` is NO so the
red traffic-light does not deallocate it; Dock reopen shows it again.

Mutation `skip_dock_reopen` MUST turn T-DOCK-REOPEN red.

### D2 — Oracle

Named test `t_dock_reopen_makes_the_window_key_and_visible`. Packaged
`Rill.app`. `RILL_TEST_DOCK_REOPEN=1` orders the window out (not visible),
then sends the same reopen selector Dock uses. Heartbeat reports
`visible=1` and `key=1` from `NSWindow`, and `seq` keeps advancing.
Socket-only tests do not close it. T-NFR is not re-cut.

## Consequences

SPEC-DISPLAY §3 names Dock reopen. Packaged e2e is the closer.

## Rejected alternatives

- **Quit when the last window closes.** Rejected: the reported bug is a
  live Dock icon that does not restore. Reopen must show the same window.
- **`NSWindowCollectionBehaviorMoveToActiveSpace` alone.** Rejected: it
  does not substitute for reopen when AppKit reports no visible windows.
- **A second window on reopen.** Rejected: one `NSWindow` (LANES host).
