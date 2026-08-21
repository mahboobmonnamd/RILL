# SPEC-DISPLAY — host window and renderer (`lane:host`)

- **Status:** Accepted for Spike 0 Proven clauses — 2026-08-17
  ([ADR 0010](../adr/0010-spike-0-closes.md)). Presenter: [ADR 0009](../adr/0009-direct-to-display-echo.md).
- **Authority:** [ADR 0003](../adr/0003-display-pipeline.md),
  [ADR 0009](../adr/0009-direct-to-display-echo.md),
  [ADR 0016](../adr/0016-exit-fullscreen-must-not-hang.md),
  [ADR 0017](../adr/0017-ghostty-look-windowed-default.md),
  [ADR 0018](../adr/0018-three-pane-host-chrome.md) (M2 chrome; not a Spike 0 reopen),
  [ADR 0019](../adr/0019-dock-reopen-shows-window.md), amended for the future
  product surface by
  [ADR 0053](../adr/0053-runtime-domain-content-and-client-authority.md) D4–D6
  and D9/D22
- **Future contract:** [SPEC-RUNTIME-SUPERVISION](SPEC-RUNTIME-SUPERVISION.md),
  [SPEC-CLIENT-AUTHORITY](SPEC-CLIENT-AUTHORITY.md), and
  [SPEC-COMPOSITOR](SPEC-COMPOSITOR.md), governed by
  [SPEC-TERMINAL-PERFORMANCE](SPEC-TERMINAL-PERFORMANCE.md). Existing Spike 0
  clauses remain evidence for that slice only.
- **Code:** `host/macos/`, `crates/rill-host`
- **Gates:** T-NFR, T-SPAWN, T-KILL, T-RESIZE — **Proven**. T-FS-EXIT
  ([#257](https://github.com/mahboobmonnamd/RILL/issues/257)). T-LOOK /
  T-WINDOWED / T-LOOK-GLASS ([#259](https://github.com/mahboobmonnamd/RILL/issues/259)).
  Three-pane chrome is [SPEC-CHROME](SPEC-CHROME.md) / T-SPLIT
  ([#260](https://github.com/mahboobmonnamd/RILL/issues/260)). T-DOCK-REOPEN
  ([#262](https://github.com/mahboobmonnamd/RILL/issues/262)). §6 IME and §7
  window paint of EXIT are **later**, not a reopen of those gates.

## 1. Prohibitions

- The GUI MUST NOT create a PTY. No `forkpty`, `openpty`, `posix_openpt`,
  `grantpt`, `unlockpt`, `ptsname`, `login_tty` — as **imports**, verified by
  `nm -u` and `otool -Iv` (T-SPAWN).
- `posix_spawn` of `rilld` is permitted for the proven Spike 0 development
  lifecycle. Production uses the per-user supervised service. The gate still
  distinguishes daemon launch from forbidden GUI PTY creation.
- The GUI MUST NOT own scrollback, receive the master fd, or consume cells over
  IPC.

## 2. Historical Spike 0 daemon launch and production successor

- `main.m` spawns `rilld` with `POSIX_SPAWN_SETSID` so the GUI's process group
  death cannot take it.
- The return of `posix_spawn` MUST be checked. Readiness is a **bounded poll
  for the socket**, not a fixed `usleep` (audit S3-8f): retry connect every
  20ms for up to 3s, then fail with a diagnostic.
- If `rilld` is already running, the GUI attaches to the existing socket. It
  MUST NOT unlink a live socket.

These clauses prove GUI/process separation, not the production service or
worker-survival contract. Production registration, protected endpoint,
daemon restart and update compatibility are SPEC-RUNTIME-SUPERVISION and must
not be claimed from `POSIX_SPAWN_SETSID`.

## 3. Warm path

- Event-driven feed. A `DISPATCH_SOURCE_TYPE_READ` on the attach socket drives
  read → feed → damaged-instance rebuild (ADR 0003 D2).
- This is the presentation end of the protected terminal path. Transcript,
  Flow, persistence, inspector, attention, artifact, rich-layout, observer and
  diagnostic work MUST NOT run synchronously before feed, damage or present.
- The window is titled, closable, resizable. `collectionBehavior` is
  `FullScreenPrimary`. Default `make run` is **windowed** (ADR 0017 D1). It
  MUST NOT call `toggleFullScreen:` on launch. `--nfr-key` still enters a
  fullscreen Space before measuring (ADR 0009). A borderless cover of
  `screen.frame` is not this surface. The layer is opaque. `NSWindow` is
  opaque. `window.alphaValue` stays `1` even when `background-opacity` is
  set (ADR 0017 D3).
- Leaving a Space (green traffic-light / a second `toggleFullScreen:`)
  MUST complete. The main thread MUST NOT wait forever on `nextDrawable` or
  the in-flight present semaphore (ADR 0016). The in-flight wait is bounded;
  a completed handler releases the semaphore if `presentedTime` never arrives.
  T-FS-EXIT enters then leaves; it does not require default launch to be
  fullscreen.
- Dock click of a running app MUST show the existing window (ADR 0019).
  `NSApp` has a delegate. `applicationShouldHandleReopen:hasVisibleWindows:`
  unhides, deminiaturizes if needed, and `makeKeyAndOrderFront:`s that
  window. `hasVisibleWindows=NO` is not a reason to do nothing: after a
  Space switch the window is often not on the active Space. Become-active
  restores the same way when the window is not visible or not on the
  active Space. One window; do not allocate a second. T-NFR is unchanged.
- Present is echo-only `nextDrawable`, one in flight until `presentedTime`.
  A `CADisplayLink` supplies `targetTimestamp` and does not take a drawable.
  After `send_input`, the host may `poll` the attach socket for ≤2 ms and
  paint on that stack (ADR 0009 D1).
- No `NSTimer`. No 60 Hz polling interval on the keystroke path.
- `Client::send` MUST NOT toggle socket blocking mode per call (audit S3-8d).
  The socket stays non-blocking; a partial write queues the remainder and
  completes on writability.
- Keystroke writes MUST NOT block the UI thread. The ≤2 ms attach-socket
  `poll` after send (ADR 0009 D1) waits for echo, not for the write.

## 4. Terminal-grid renderer

Per ADR 0003 D1:

- `MTLPixelFormatR8Unorm` glyph atlas, 2048×2048, shelf-packed, keyed by
  `(codepoint, bold, italic)`, rasterised with CoreText at the backing scale
  factor (T-GLYPH-SCALE, [#275](https://github.com/mahboobmonnamd/RILL/issues/275)).
  Point-sized atlas entries on a pixel `cellPx`
  (Retina) MUST NOT ship: glyphs occupy ~¼ of the cursor cell.
- One instance per visible cell, 16 bytes:
  `{ atlas_uv ushort4, cell ushort2, fg uchar4, bg uchar4, flags ushort }`.
- A single instanced draw. Fragment shader returns
  `mix(bg, fg, atlas.r)`; underline and strikethrough derive from cell-local UV
  and `flags`. Cursor is one extra instance.
- One instance buffer in flight behind a semaphore of 1 (ADR 0009). **No
  allocation on the render path** — no per-frame `MTLTexture`, no per-frame
  `CGBitmapContext` (audit S3-8h).
- Only damaged rows are rewritten (ADR 0003 D3).
- `CAMetalLayer.maximumDrawableCount = 2`; `displaySyncEnabled` on for gate
  runs. At most one present outstanding (ADR 0009). Do not keep-alive present.
- Colour emoji render as an explicit tofu box and are **counted and reported**.
  Silent mis-rendering is not acceptable; a BGRA atlas is later display work,
  not the M1 session-graph slice ([ADR 0014](../adr/0014-m1-first-slice-closes.md) D4).

This renderer remains the specialized terminal-grid primitive inside the RILL
compositor. It is not replaced and is not the renderer for all structured
content. Rich scene, virtualization, shared shaping, editor, selection and
accessibility requirements are SPEC-COMPOSITOR and remain Red.

## 5. Geometry

- Cell size derives from the font's advance and line metrics via CoreText.
  `TerminalView` does this. Kernel `Winsize::default` still uses 80×8 / 24×16
  px until the first `RESIZE` — leftover, not a font-family bug.
- On resize by the input/resize lease owner: recompute cols/rows from the real
  cell box and request canonical RESIZE with cell and pixel dimensions.
  Observers do not resize the PTY; they crop, pan or letterbox the live grid.
- Backing scale factor changes (moving between displays) rebuild the atlas.

## 6. Input

**Proven for Spike 0:** Ctrl+letter including `^C` (T-DROP / hid). Arrows and
Enter are a hand table.

**Later — not Spike 0 Proven.** Full `NSTextInputClient` (marked text in-cell),
DECCKM, libghostty-vt key encoder, Option policy, Fn/Home/End, bracketed paste.
The ObjC comment already records IME as unimplemented; do not read the protocol
conformance as done.

Audit S3-8g (`^C` untypeable) was the 2026-08-16 defect; Ctrl handling landed
with the closer. Enter → PTY; no English/PATH heuristic (ADR 0001 §8).

Key encoding SHOULD later use libghostty-vt's encoder rather than the hand
table.

## 7. Dead pane

**Proven (kernel / attach):** retained `EXIT` on reattach; `Client` stops
`send_input` when `alive` is false (T-EXIT).

**Later (window):** [#17](https://github.com/mahboobmonnamd/RILL/issues/17) —
hollow cursor, exit status. `TerminalView` does not yet call `rill_client_alive`.

## 8. Measurement hooks

For T-NFR (ADR 0003 D5–D8), the view exposes:

- the `NSEvent.timestamp` of the key that produced the current pending sentinel;
- a per-drawable `addPresentedHandler:` that records `presentedTime` against the
  sentinel that frame carried;
- accepted/discarded sample counts, refresh rate, vsync state, power source.

These hooks are compiled into the shipped binary. A measurement path that only
exists in a test build is not measuring the shipped app.

Future product surfaces also expose the measurement inventory required by
SPEC-TERMINAL-PERFORMANCE §6: PTY-drain progress, byte sequence gaps/reordering,
frame/missed-frame timing, CPU, memory, bounded-queue high-water/overflow,
Raw-fallback time, cross-pane/client interference, protected-path control RPCs
and callback allocations where tooling permits. Missing instrumentation fails
the corresponding Red gate; it does not weaken historical T-NFR or change its
Proven Spike 0 evidence.

## 9. Out of scope for Spike 0

Tabs, nested PTY splits, structured content, scrollback UI, mouse reporting,
selection, search, shaping/ligatures, colour emoji, any second window, and
session-graph UI in the kernel
([ADR 0011](../adr/0011-session-graph.md) D5). Three-pane chrome around **one**
leaf is M2 ([SPEC-CHROME](SPEC-CHROME.md), [ADR 0018](../adr/0018-three-pane-host-chrome.md)).
Theme *store* and cmux/Herdr chrome stay out. Look overlay is §10 (ADR 0017),
not a Spike 0 reopen.

ASAP keep-alive presents, CA pin, `CAMetalDisplayLink`, and a borderless
cover of `screen.frame` are not the closer (ADR 0004–0008). The closer is
ADR 0009. Do not flatten the oracle (ADR 0003 D5–D8).

## 10. Look overlay (ADR 0017)

`host-surface.toml` is the bundled fallback (`font-family`, `font-size`,
`font-fallbacks`, optional `theme =`). The host then overlays the first
look file that exists: `RILL_CONFIG`, then `~/.config/rill/config`.

Do not live-read `~/.config/ghostty/config` or cmux files.

Applied keys: `theme`, `font-family`, `font-size`, `font-family-fallback`,
`background` / `foreground` / `cursor` / `palette`, `window-padding-x` /
`window-padding-y`, `background-opacity`, `split-divider-color`,
`macos-option-as-alt`. `theme =` resolves a Ghostty-grammar **file** under
`themes/` next to that config, next to `host-surface.toml` (packaged
`Resources/themes/`), or `~/.config/rill/themes/`. Rust MUST NOT contain a
theme RGB catalog. Unknown theme names do not replace host-surface colors.
Unquoted `#hex` is a color.

Empty cells whose fg/bg equal the VT default are remapped to the
file-resolved default colours. The Chip 0 adapter MUST set libghostty-vt
default fg/bg/cursor and palette 0–15 from that same file so SGR colours
match Ghostty/cmux (T-LOOK-ANSI). The Metal layer and `NSWindow` stay opaque.
`background-opacity` and `background-blur-radius` are parsed and not
applied. Do not invent a compiled Chip 0 default-RGB map
([#267](https://github.com/mahboobmonnamd/RILL/issues/267) remains Chip 1
palette-index cells). `cmux.json`, Herdr, and zshrc are not this file.

`font-fallbacks` are live: `TerminalView` tries each name with CoreText
before failing init. Production faces MUST NOT use
`NSFont.monospacedSystemFont` when a family is configured.
