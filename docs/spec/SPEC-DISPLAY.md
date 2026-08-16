# SPEC-DISPLAY — host window and renderer (Lane D)

- **Status:** Draft for Spike 0 remediation — 2026-08-16
- **Authority:** [ADR 0003](../adr/0003-display-pipeline.md)
- **Code:** `host/macos/`, `crates/rill-host`
- **Gates:** T-NFR, T-SPAWN, T-KILL, T-RESIZE

## 1. Prohibitions

- The GUI MUST NOT create a PTY. No `forkpty`, `openpty`, `posix_openpt`,
  `grantpt`, `unlockpt`, `ptsname`, `login_tty` — as **imports**, verified by
  `nm -u` and `otool -Iv` (T-SPAWN).
- `posix_spawn` of `rilld` is permitted and required. The gate distinguishes
  the two by checking PTY-creation primitives, plus a runtime parent check.
- The GUI MUST NOT own scrollback, receive the master fd, or consume cells over
  IPC.

## 2. Daemon launch

- `main.m` spawns `rilld` with `POSIX_SPAWN_SETSID` so the GUI's process group
  death cannot take it.
- The return of `posix_spawn` MUST be checked. Readiness is a **bounded poll
  for the socket**, not a fixed `usleep` (audit S3-8f): retry connect every
  20ms for up to 3s, then fail with a diagnostic.
- If `rilld` is already running, the GUI attaches to the existing socket. It
  MUST NOT unlink a live socket.

## 3. Warm path

- Event-driven. A `DISPATCH_SOURCE_TYPE_READ` on the attach socket drives
  read → feed → damaged-instance rebuild → present.
- No `NSTimer`. No polling interval anywhere on the keystroke path
  (ADR 0003 D2).
- `Client::send` MUST NOT toggle socket blocking mode per call (audit S3-8d).
  The socket stays non-blocking; a partial write queues the remainder and
  completes on writability.
- Keystroke writes MUST NOT block the UI thread.

## 4. Renderer

Per ADR 0003 D1:

- `MTLPixelFormatR8Unorm` glyph atlas, 2048×2048, shelf-packed, keyed by
  `(codepoint, bold, italic)`, rasterised with CoreText at the backing scale
  factor.
- One instance per visible cell, 16 bytes:
  `{ atlas_uv ushort4, cell ushort2, fg uchar4, bg uchar4, flags ushort }`.
- A single instanced draw. Fragment shader returns
  `mix(bg, fg, atlas.r)`; underline and strikethrough derive from cell-local UV
  and `flags`. Cursor is one extra instance.
- Triple-buffered instance buffers behind a semaphore of 3. **No allocation on
  the render path** — no per-frame `MTLTexture`, no per-frame `CGBitmapContext`
  (audit S3-8h).
- Only damaged rows are rewritten (ADR 0003 D3).
- `CAMetalLayer.maximumDrawableCount = 3`; `displaySyncEnabled` on for gate
  runs.
- Colour emoji render as an explicit tofu box and are **counted and reported**.
  Silent mis-rendering is not acceptable; a BGRA atlas is Milestone 1.

## 5. Geometry

- Cell size derives from the font's advance and line metrics via CoreText, not
  the hardcoded 8×16 currently in `TerminalView` and `Client::resize`.
- On resize: recompute cols/rows from the real cell box, resize the chip, and
  send `RESIZE` with both cell and pixel dimensions.
- Backing scale factor changes (moving between displays) rebuild the atlas.

## 6. Input

Audit S3-8g — currently `^C` cannot be typed into the shipped app, which makes
T-DROP unreachable through the GUI.

- Control characters: `Ctrl` + letter → `0x01..0x1a`; `Ctrl+[` → `0x1b`, and the
  rest of the C0 set.
- `Option` per configuration: ESC-prefix (default) or composed character.
- Full `NSTextInputClient` conformance for IME (marked text, candidate windows).
  Marked text renders in the chip's cells, not in an overlay.
- Special keys: arrows, Home/End, PgUp/PgDn, Fn keys, Delete — DECCKM-aware,
  respecting application cursor key mode.
- Bracketed paste when the child enables it.
- Enter → PTY. No English/PATH heuristic router (ADR 0001 §8).

Key encoding SHOULD use libghostty-vt's key encoder rather than a hand-rolled
table; it is already linked and it is where this correctness lives upstream.

## 7. Dead pane

- On `EXIT` the view stops accepting input, renders the cursor as hollow, and
  shows the exit status. It MUST NOT continue to look alive (FR-EXIT).
- This applies to an `EXIT` retained across detach and replayed on reattach
  (SPEC-KERNEL §6).

## 8. Measurement hooks

For T-NFR (ADR 0003 D5–D8), the view exposes:

- the `NSEvent.timestamp` of the key that produced the current pending sentinel;
- a per-drawable `addPresentedHandler:` that records `presentedTime` against the
  sentinel that frame carried;
- accepted/discarded sample counts, refresh rate, vsync state, power source.

These hooks are compiled into the shipped binary. A measurement path that only
exists in a test build is not measuring the shipped app.

## 9. Out of scope for Spike 0

Tabs, splits, sidebar, Blocks, themes, scrollback UI, mouse reporting, selection,
search, ligatures, colour emoji, and any second window.
