# ADR 0003: Display pipeline and key→present

- **Status:** Accepted — 2026-08-16
- **Tree:** this repository only
- **Amends:** ADR 0001 §1 (fills in what "our Metal" means). Supersedes the
  T-NFR definition in [SPIKE-0](../SPIKE-0.md) and [PRD](../PRD.md) §NFR-KEY.
- **Amended by:** [ADR 0005](0005-mtkview-presenter.md)–[0008](0008-cametal-display-link.md)
  (exhausted schedulers) and [ADR 0009](0009-direct-to-display-echo.md) (closer
  presenter). D1 atlas and D5–D8 oracle stand.
- **Evidence:** [SPIKE-0-AUDIT](../SPIKE-0-AUDIT.md) S1-2, S3-8b, S3-8g, S3-8h

## Context

FR-CHIP0 says the window runs libghostty-vt plus **our Metal**, painting from a
flat POD buffer plus damage. What the tree does instead:

1. A 60 Hz `NSTimer` calls `pump`, so PTY bytes that arrive at t+0.1ms are not
   read until t+16.6ms. The largest single term in the latency budget is a
   polling interval we chose.
2. `paintGrid` walks every cell on the CPU, calls `CTFontDrawGlyphs` per cell
   into a `CGBitmapContext`, allocates a fresh `MTLTexture`, uploads the whole
   bitmap, and blits it as one full-screen quad.
3. The damage rows computed in C and carried through the FFI are ignored.

So the GPU draws two triangles and CoreText does the terminal rendering, once
per cell per frame, on the UI thread. That is a software renderer with a Metal
present call at the end.

And T-NFR did not measure any of it: it stopped at the POD snapshot, inside the
Rust client, before the host was involved. It also never round-tripped the PTY
(audit S1-2). The number `0.032ms` describes neither the pipeline nor a
keystroke.

`NSTimer` at 60 Hz plus per-cell CoreText plus a full texture upload cannot make
a one-frame budget, and no measurement stopping at the snapshot would ever say
so. Fixing the instrument and fixing the renderer are the same decision.

## Decision

### D1 — Chip 0's presenter is a glyph-atlas instanced renderer

`TerminalView`'s CPU raster is deleted and replaced with:

- **Atlas.** A single-channel (`MTLPixelFormatR8Unorm`) texture, 2048×2048,
  shelf-packed. Entries are keyed by `(codepoint, bold, italic)` and rasterised
  on demand with CoreText at the backing scale factor. Misses are filled before
  the frame that needs them.
- **Instances.** One instance per visible cell:
  `{ atlas_uv: ushort4, cell: ushort2, fg: uchar4, bg: uchar4, flags: ushort }`.
  16 bytes. `flags` carries bold, underline, strikethrough, inverse, wide-lead,
  wide-tail.
- **One draw call.** `drawPrimitives:vertexStart:0 vertexCount:6
  instanceCount:cols*rows`. The vertex shader expands the cell index to a quad;
  the fragment shader returns `mix(bg, fg, atlas.sample(...).r)`, which gives
  background and glyph in a single pass because instances tile exactly.
  Underline and strikethrough are computed from cell-local UV against `flags`.
  The cursor is one additional instance.
- **Triple buffering.** Three shared-storage instance buffers behind a
  `dispatch_semaphore_t` of 3. No allocation on the render path.

Colour emoji (`sbix`/`CBDT`) do not fit an R8 atlas. Spike 0 renders them as an
explicit tofu box and **records the limitation in the gate output**. Silent
mis-rendering is not acceptable; a second BGRA atlas is Milestone 1 work.

### D2 — The warm path is event-driven, not polled

The `NSTimer` is deleted. A `dispatch_source_t` (`DISPATCH_SOURCE_TYPE_READ`)
on the attach socket wakes the client the moment kernel bytes land. That wake
feeds the VT and rebuilds the damaged instance rows.

`displaySyncEnabled` stays **on** for the recorded gate number, because that
is what a person perceives. An off-vsync figure may be reported alongside as a
diagnostic; it does not close the gate.

Bytes arriving faster than the refresh accumulate into the VT and paint on the
next frame — feeding is decoupled from presenting, which is what keeps a `yes`
flood from stalling input.

Present ownership is [ADR 0005](0005-mtkview-presenter.md): `MTKView` takes one
drawable per vsync. Socket wake MUST NOT call `nextDrawable`.
`CAMetalLayer.maximumDrawableCount = 2`.

### D3 — Damage is honoured end to end

`PodGrid` already carries `full_damage` and `damage_row0..damage_row1`. The host
rebuilds instances only for damaged rows; the rest of the buffer persists across
frames. The FFI gains an explicit row range so the host never rebuilds a clean
row.

The snapshot copy chain in the audit's S3-8b (C `calloc` → Rust `collect` →
ObjC walk) collapses to one write into the mapped instance buffer, for damaged
rows only.

### D4 — VT feed and render share one thread, for now

Chip 0 feeds and snapshots on the main thread, woken by D2's dispatch source.
libghostty-vt's two-phase `begin_update`/`end_update` exists precisely to split
this across an IO thread and a render thread under a lock; that is the
documented upgrade path and it is **not** taken in Spike 0. One thread first,
measured, then split if the number demands it.

### D5 — T-NFR is redefined as key-down → drawable presented

**Old (revoked):** key bytes → a POD snapshot containing the glyph, inside the
Rust client.

**New:** from the `NSEvent` key-down timestamp to the `presentedTime` of the
drawable whose contents first contain the echoed glyph.

- **Start:** `NSEvent.timestamp`, set by the window server when the event was
  created — not when our code first sees it. Same timebase as
  `CACurrentMediaTime()`.
- **End:** `[drawable addPresentedHandler:]` → `drawable.presentedTime`. The
  frame is tagged with the sentinel it carries, so the handler attributes the
  presentation to the right keystroke.
- **Segment measured:** window-server event → app → attach socket → PTY write →
  shell echo → PTY read → attach frame → VT feed → instance build → encode →
  compositor presentation. The full loop, both directions.

### D6 — The sentinel must not be able to pre-exist

The audit's S1-2 root cause. The oracle is now **cell-position specific**:

1. Before sending, snapshot and record the cursor cell `(c, r)` and the
   codepoint currently at `(c, r)`.
2. Choose a printable codepoint **different from the one already at `(c, r)`**.
3. The sample completes when the cell at exactly `(c, r)` holds the chosen
   codepoint. Not "somewhere on the grid."
4. If the shell wraps, scrolls, or the cursor is not where we predicted, the
   sample is **discarded and counted**. Discards above 2% fail the run — a high
   discard rate means the oracle is unreliable, and an unreliable oracle does
   not get to report a p95.

### D7 — Two injection paths, and the gate names which one it used

| Mode | Injection | Includes | Closes the gate |
|---|---|---|---|
| `--nfr-key=hid` | `CGEventPost(kCGSessionEventTap)` | window-server delivery, full input stack | **Yes** |
| `--nfr-key=app` | `NSEvent` constructed in-process → `[NSApp sendEvent:]` | everything after window-server delivery | No — CI diagnostic only |

`hid` needs Accessibility trust; the harness checks
`AXIsProcessTrustedWithOptions` and **fails with instructions** rather than
silently degrading (ADR 0002 D5). The gate output always states the mode, and
`app` mode is explicitly marked as not gate-closing.

### D8 — Reported statistics

`n ≥ 1000` accepted samples. Report p50, p95, p99, max, discard count, display
refresh rate, whether vsync was on, power source, and the libghostty-vt SHA.

**Pass:** p95 < one display refresh interval, measured at the display's actual
rate — 16.7ms at 60 Hz, 8.3ms on a 120 Hz ProMotion panel. Reporting a 60 Hz
budget on a 120 Hz display would be grading against the easier target.

Three runs are required: warm and idle, under a `yes` flood in a second pane's
worth of load, and **on battery**. Battery is the gate (PRD NFR-KEY); the others
are recorded.

### D9 — Zero control RPCs, measured properly

The audit's S1-3 oracle is deleted. Replaced with: during the measurement
window the client asserts that the only frames it sent were `DATA` and `CREDIT`,
the only frames received were `DATA`, and that **no file descriptor other than
the attach socket and the Metal device was written to** — enforced by counting
writes at the single `send` chokepoint and asserting the process opened no new
sockets (`lsof`-equivalent snapshot before and after, compared).

## Consequences

- Lane C and Lane D grow substantially. The atlas, packer, and shaders are new
  code with no equivalent in the tree.
- T-NFR cannot run headless. It needs a window, a display, and — for
  gate-closing `hid` mode — Accessibility trust. CI runs `app` mode for
  regression detection; closure is a recorded run on a real Mac on battery.
- The honest first number will very likely miss. That is the spike working. ADR
  0001's stop rule and ADR 0002 D11 apply: do not add surface area to hide it,
  and do not re-cut the instrument to flatter it.
- `S3-8g` (no Ctrl, no IME) becomes blocking: `^C` must be typeable before
  T-DROP can be exercised through the GUI at all.

## Rejected alternatives

- **Keep the CPU raster, fix only the measurement.** Rejected under the chosen
  scope: it produces a true number for a pipeline we have already decided to
  replace, and the replacement invalidates the number.
- **Ghostty's own renderer.** Rejected by ADR 0001 §1 — full libghostty exec
  spawns the shell, which breaks FR-PTY and FR-SPAWN. We take the VT, not the
  GPU stack.
- **SDF / vector glyph rendering.** Better scaling across sizes, materially more
  work, and terminals render at a fixed size per session. Bitmap atlas is the
  right first cut.
- **Measure to `commit` rather than `presentedTime`.** Cheaper and permission-
  free, but it excludes the compositor — the part users actually see, and the
  part most likely to hide a stall. Rejected as the same class of error as the
  original T-NFR.
- **Off-vsync presentation to make the number smaller.** Rejected explicitly.
  ADR 0002 D11: instrumentation that flatters a miss is a stop-rule violation.
