# SPEC-COMPOSITOR — terminal primitive, rich scene and library seams

- **Status:** Partial. ContentTimeline exists (#348). Named Flow/Raw/TUI gates
  still require packaged evidence.
- **Authority:** [ADR 0053](../adr/0053-runtime-domain-content-and-client-authority.md)
  D7, D9 and D22.
- **Requires:** [SPEC-TERMINAL-PERFORMANCE](SPEC-TERMINAL-PERFORMANCE.md).
- **Lane:** `lane:host` for the first macOS presenter. Core boundaries remain
  platform-neutral where specified; platform UI remains native.

## 1. Preserve the terminal primitive

The current Metal glyph-atlas and instanced-cell renderer remains the terminal
grid primitive. Chip 0 and later Chip 1 continue to provide terminal state and
POD damage. The compositor does not replace the VT, route PTY bytes through
rich-content nodes or put JSON/per-cell strings on the warm path.

The terminal primitive supports primary and alternate screens, cursor, damage,
IME interaction, selection mapping and accessibility projection for one
TerminalPane. It remains eligible for NFR-KEY measurement without orchestration
or content-timeline RPC.

## 2. Retained scene

The RILL compositor accepts a retained scene containing at least:

- shaped text runs;
- terminal-grid primitive instances;
- rectangles, borders and backgrounds;
- images with explicit decode/resource bounds;
- clips, transforms, layers and opacity;
- controls and editor surfaces;
- diff/change decoration;
- virtualized workspace activity timelines and rich agent content;
- hit-test and accessibility nodes; and
- stable keys, damage and virtualization metadata.

ContentTimeline virtualization constructs only visible/overscan scene nodes.
The compositor does not own ContentItem, Workspace, Session, Task or Conversation
lifecycles. It receives projections and returns presentation events.
Inspector layout, Flow card/spine styling, navigation chrome, timeline geometry,
attention overlays, hover and focus decoration are client projection state.

## 3. Text and font ownership

`rill-text` owns platform font discovery adapters, fallback resolution, shaping,
grapheme-to-glyph mapping, metrics and reusable glyph data. Terminal and rich
content use the same shaping contract while retaining different layout rules.

Shaping operates on runs/clusters, not one Unicode scalar at a time. Missing
glyphs, fallback and invalid clusters are explicit diagnostics. The Metal atlas
is a presenter resource, not font-discovery authority.

## 4. Editor, input and raw terminal arbitration

`rill-editor` owns structured input text, caret, selection, IME composition,
history references and completion presentation. It emits explicit submission
events into ContentTimeline or agent/task routing.

Raw terminal/TUI mode bypasses the structured editor and encodes input for the
leased TerminalExecution. No language or PATH heuristic chooses the route.
Input ownership is still governed by SPEC-CLIENT-AUTHORITY.

## 5. Selection and accessibility

Selection is client view state expressed with surface-specific anchors:
terminal grid position plus execution/checkpoint identity, ContentTimeline item
and grapheme offsets, or editor document offsets. Cross-surface copy creates a
derived ordered result without mutating sources.

Accessibility nodes expose role, label, value, selection and actions without
dumping the full offscreen timeline on each frame. Alternate-screen terminal
accessibility reflects the live grid; primary structured history reflects
virtualized semantic items.

## 6. Internal library boundaries

The boundaries named by ADR 0053 are ownership rules, not deployment units.
They may begin as modules or existing crates. A split into a crate requires a
demonstrated dependency or safety benefit and must not create per-frame copies.

Internal terminal-core interfaces avoid app UI types and preserve possible C
ABI and WebAssembly adapters. No API is public-stable. Browser reuse requires a
proved WASM terminal core, WebGPU/Canvas presenter, TypeScript facade and the
same conformance/security gates. A third-party consumer may not weaken RILL's
content, lifecycle or fidelity contracts.

## 7. Performance boundaries

- Warm PTY DATA does not traverse ContentTimeline serialization.
- Terminal grid damage does not allocate a String per cell.
- Hidden rich surfaces do no frame-rate polling or terminal-cell scans.
- Inspector, attention and activity are event-driven/cold.
- Virtualized content has explicit item, glyph, image and overscan bounds.
  Markdown, diffs, artifacts, images and text shaping have separate bounded
  work budgets; offscreen content is not fully laid out.
- Present, PTY read and key handling MUST NOT wait on transcript construction,
  persistence, indexing, redaction or rich layout.
- One slow content surface, observer or pane cannot stall PTY drain or another
  terminal pane.
- Historical VT replay is not routine Flow/Block rendering.
- NFR-KEY remains measured on the terminal path. Enabled/disabled comparisons
  follow SPEC-TERMINAL-PERFORMANCE; no new millisecond tolerance is invented
  here.

## 8. Gates

- T-COMPOSITOR-PRESERVES-METAL-GRID
- T-COMPOSITOR-VIRTUALIZED-CONTENT
- T-COMPOSITOR-NO-DOMAIN-OWNERSHIP
- T-TEXT-CLUSTER-SHAPING
- T-EDITOR-RAW-BYPASS
- T-SELECTION-SURFACE-ANCHORS
- T-A11Y-VIRTUALIZED-BOUNDS
- T-COMPOSITOR-WARM-PATH-BOUNDARY
- T-PERF-PRESENT-INDEPENDENT
- T-PERF-BOUNDED-RESOURCES
- T-PERF-RAW-TUI-BYPASS
- T-LIBRARY-NO-UI-DEPENDENCY
- T-LIBRARY-WASM-PROOF-BEFORE-PUBLIC

## 9. Out of scope

This spec does not select a cross-platform UI toolkit, replace AppKit, promise a
public library, implement an IDE or require every internal boundary to become a
separately compiled crate.
