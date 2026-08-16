# PRD — RILL

- **Status:** Authority for this repository. Implementation requires an Accepted ADR plus a GitHub issue in an open milestone.
- **Date:** 2026-08-16
- **Audience:** 3–5 engineers in parallel.

## 1. Product

RILL is a native terminal whose **session survives the window**, and whose **typing path is in-process**.

Users: people who live in shells, TUIs, and later coding agents, on macOS first.

Promise:

> Close the app. The shell is still there. Open it again. You see the work. Typing never felt like a remote desktop.

Not a promise (yet): Warp-class Blocks, a live model provider, Linux UI, an in-app editor.

## 2. Why a new tree

A previous prototype put the display emulator behind JSON (`pane_replay` as number arrays, every cell copied to `String`, SwiftUI observing the PTY buffer). Persist worked. Feel did not. Multi-agent on that path would have been undebuggable.

RILL starts from the lock, not from that composition. This repo does not depend on that tree.

## 3. In scope for Spike 0

One window. One runtime. One PTY. Chip 0 display. Framed attach. Quit/SIGKILL keeps the child. Reopen paints via a cold-path resync. Named tests in [SPIKE-0](SPIKE-0.md).

## 4. Out of scope until Spike 0 is Proven

- Sidebar, tabs, splits, Blocks, themes
- Agents, conversations, scheduler, natural-language routing
- Full Ghostty.app embed / Ghostty-owned spawn
- Our own VT engine as the live chip (Chip 1 may be prototyped in isolation; it cannot replace Chip 0 until Spike 0 is butter)
- Accounts, billing, hosted control plane
- Linux / Windows UI

## 5. Requirements

### Functional

| ID | Requirement |
|---|---|
| FR-PTY | Kernel creates, owns, and reaps the session PTY. GUI does not `posix_spawn` the user shell. |
| FR-ATTACH | Live keys and PTY bytes travel on a framed `SOCK_STREAM`. Darwin has no `SOCK_SEQPACKET`. |
| FR-SOLE | Kernel is the only writer on the PTY master. Do not pass the master fd to the GUI. |
| FR-CHIP0 | Window runs `libghostty-vt` + our Metal. `feed` takes bytes. Paint is a flat POD buffer + damage, not per-cell `String`. |
| FR-HISTORY | Kernel owns a bounded byte ring. Window quit does not destroy it. |
| FR-RESYNC | On attach, once, a headless copy of the same chip emits a byte repaint into the splice. Not on the warm path. |
| FR-EXIT | Child exit is an in-band frame. A dead pane does not look alive. |
| FR-RESIZE | Resize is in-band on the splice, ordered with keys. |
| FR-ONE | One leaf pane, one PTY, one attach. A second attach is refused. |
| FR-KILL | GUI `SIGKILL`: same child PID accepts input on reopen. |

### Non-functional

| ID | Requirement |
|---|---|
| NFR-KEY | Key-down `NSEvent.timestamp` → `presentedTime` of the drawable first containing the echoed glyph **at the cell the cursor occupied**. Packaged app. p95 < one display refresh interval over ≥1000 accepted samples, discards ≤ 2%. Warm and under load. **On battery.** Zero control-plane RPCs during the run. Superseded definition and measurement procedure: [ADR 0003](adr/0003-display-pipeline.md) D5–D9. |
| NFR-DROP | `yes` for 10s then `^C`: zero dropped bytes; prompt usable. Per-pane pumps so one flood cannot stall another pane (when panes exist). |
| NFR-BYTES | Invalid UTF-8 from the child reaches the emulator byte-identical. |
| NFR-SPAWN | Shipped GUI binary: no `posix_spawn` / `forkpty` / `openpty` used to start the user shell. Link-level test, not a source grep. |
| NFR-FAIL | Library and daemon paths return `Result`. No `unwrap` on reachable request handling. |

## 6. Input (later — recorded so nobody invents a classifier)

When a conversation object exists: **Enter → PTY. A distinct submit (⌘Enter) → conversation.** No PATH/English heuristic. A scheduler may sit on top later; it does not replace Enter. Not Spike 0.

## 7. Success / stop

Spike 0 is **Proven** only when every named test in [SPIKE-0](SPIKE-0.md) has
been demonstrated **red and then green** on a packaged build
([ADR 0002](adr/0002-falsifiable-evidence.md) D2), including NFR-KEY on battery.

A green test that was never shown to fail is not evidence. The 2026-08-16 run
that reported eight of nine gates Proven is withdrawn; see
[SPIKE-0-AUDIT](SPIKE-0-AUDIT.md).

If it is not butter: **stop.** Do not add agents to hide the miss, and do not
re-cut the instrument to flatter it.
