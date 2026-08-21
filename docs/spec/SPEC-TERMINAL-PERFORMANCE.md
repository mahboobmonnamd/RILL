# SPEC-TERMINAL-PERFORMANCE — protected terminal path and regression gates

- **Status:** Red. Specification and test plan only; no implementation is
  authorized.
- **Authority:** [ADR 0002](../adr/0002-falsifiable-evidence.md),
  [ADR 0003](../adr/0003-display-pipeline.md),
  [ADR 0009](../adr/0009-direct-to-display-echo.md), and
  [ADR 0053](../adr/0053-runtime-domain-content-and-client-authority.md) D22.
- **Requires:** [SPEC-ATTACH](SPEC-ATTACH.md),
  [SPEC-DISPLAY](SPEC-DISPLAY.md), [SPEC-FIDELITY](SPEC-FIDELITY.md),
  [SPEC-CLIENT-AUTHORITY](SPEC-CLIENT-AUTHORITY.md),
  [SPEC-CONTENT](SPEC-CONTENT.md), and
  [SPEC-COMPOSITOR](SPEC-COMPOSITOR.md).
- **Lanes:** every lane that can produce, transport, interpret or present
  terminal traffic. A feature lane cannot waive this cross-cutting gate.

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. Binding invariant and protected path

No product feature may slow, block, reorder or reduce the fidelity of raw
terminal input, PTY draining, terminal-core/VT processing or terminal-grid
presentation.

The protected terminal path is:

```text
keyboard/input
→ binary attach path
→ PTY
→ TerminalExecution worker and terminal core
→ client terminal mirror
→ POD damage
→ Metal terminal-grid presenter
```

The existing NFR-KEY oracle and acceptance boundary remain authoritative. This
spec does not replace, rename or relax T-NFR. It extends the required evidence
to product configurations and failures that did not exist in Spike 0.

## 2. Systems forbidden as synchronous dependencies

None of the following may be a synchronous dependency of any protected-path
stage:

- semantic transcript or ContentTimeline;
- Flow Blocks, workspace activity timeline, inspector or navigation badges;
- agent state, approvals, questions or attention;
- artifact, diff, Markdown, rich-text, image or export processing;
- persistence or database writes, search indexing or retention compaction;
- redaction or export processing;
- mobile clients, observers or another pane's delivery credit;
- telemetry, diagnostics or crash reporting; or
- JSON serialization or control-plane RPC.

Raw terminal presentation MUST continue when any listed system is slow,
unavailable, overloaded, disconnected or crashed. Missing semantic metadata
reduces enhancement only. It never reduces terminal correctness.

## 3. Authoritative sources for enhanced fields

Enhanced presentation MUST use these sources. A renderer or terminal cell is
not an authoritative producer merely because it contains similar text.

| Displayed field | Required authoritative source |
|---|---|
| Timestamp | Runtime monotonic event timestamp; wall-clock mapping is display-only |
| Command text | Known native submission or explicit shell/protocol integration mark |
| Start, finish and duration | Runtime lifecycle events measured on one monotonic clock |
| Exit status | Authoritative process or shell completion event |
| Test count and result | Structured test reporter, trusted tool/agent adapter or explicit protocol event |
| Changed files and diff | Git/tool artifact event |
| Agent status | Durable Task lifecycle event |
| CWD | Existing authoritative cwd tap or explicit shell event |
| Branch and repository | Git adapter result bound to repository identity |
| Approval and question | StructuredRequest with stable ID and generation |
| Terminal output | PTY byte stream correlated by TerminalExecution generation and monotonic offsets |

Terminal cells MUST NOT be scraped to create product state. Command, success,
duration, branch, test, artifact, approval and agent claims MUST NOT be inferred
from prompt regex, language classification, cursor position, geometry or
timing. When structured information is unavailable, the product shows honest
raw or `Unstructured` output. Semantic events may correlate to terminal
offsets; cells and renderer geometry are never semantic identity.

## 4. Asynchronous bounded architecture

- A TerminalExecution worker drains PTY bytes independently of semantic and
  persistence consumers. Per-client delivery credit is not worker-drain
  credit.
- Terminal-grid presentation does not wait for transcript construction,
  semantic classification, database work, indexing, redaction or persistence.
- Semantic processing receives references or bytes through independently
  bounded asynchronous queues. Each queue declares capacity, ownership,
  overflow counter and recovery cursor before implementation.
- Semantic overflow preserves the PTY byte stream and raw rendering. It may
  emit an `Unstructured` range or explicit `Discontinuity` with execution
  generation, offsets and reason. It never fabricates or silently drops bytes.
- Transcript persistence is asynchronous, batched, policy-controlled and
  separately bounded. Disk or store latency cannot hold a present, PTY read or
  input write.
- Flow, activity, inspector, Markdown, diffs, artifacts, images, shaping and
  accessibility have separate bounded work/resource budgets. Flow and activity
  construct only visible items plus declared bounded overscan.
- Inspector, attention and activity are event-driven and cold. They do not
  poll, scrape or scan a terminal per frame.
- A slow client, observer, semantic channel, pane or agent cannot stall another
  client, controller, pane or TerminalExecution worker.
- Raw and alternate-screen TUI modes bypass Flow, rich content and the native
  composer. Historical VT replay is not routine Flow/Block rendering.
- Queues, event ledgers, retained transcript, glyphs, images and offscreen
  layout have explicit bounds. An implementation with an unbounded resource is
  ineligible for its performance gate.

## 5. Failure and degradation behavior

Every row preserves bounded PTY draining, available raw terminal operation,
cross-pane/client isolation, original byte order and honest visible
degradation. Recovery uses compatible checkpoints/deltas or an explicit
discontinuity. No fallback may add synchronous work to the protected path.

| Failure | Required degradation and recovery |
|---|---|
| Semantic processor stalls | Stop granting only its queue credit; raw bytes and paint continue; mark affected offset range `Unstructured` if recovery exceeds retained semantic input |
| Semantic processor crashes | Isolate/restart the processor; raw remains available; resume from cursor or show a discontinuity |
| Transcript store is slow | Batch/queue only within its declared bound; do not delay PTY read, input or present |
| Transcript store is unavailable | Disable durable writes visibly; bounded live semantic projection and Raw continue under policy |
| Disk is full | Fail the durable sink closed, report it, preserve bounded live terminal operation, and never spin/retry on a display callback |
| Retention is disabled | Perform no durable transcript/raw/history write; bounded live attach/recovery and Raw remain available |
| Flow projection fails | Show Raw and an honest Flow degradation marker; do not replay arbitrary historical ranges as routine rendering |
| Rich-content renderer fails | Remove/isolate the failed rich surface; preserve the Metal grid and terminal input |
| Inspector or timeline is slow | Drop/defer only its projection work within bounds; no per-frame terminal scan |
| Agent emits excessive output | Preserve numbered PTY byte order, bound semantic projection, isolate other panes and expose overflow honestly |
| Mobile or observer stops granting credit | Advance worker and healthy-client offsets; require the slow client to resync independently |
| One pane produces unlimited output | Keep per-pane drain/recovery/resource accounting; other panes remain interactive and within their own bounds |
| State-hash or checkpoint reconciliation fails | Stop the divergent mirror from presenting or accepting input, while the worker and healthy raw clients continue; rebuild from a compatible checkpoint/deltas or show an explicit discontinuity before restoring that mirror |

Continuing to present a known-divergent mirror is not raw availability. Raw
availability means the canonical worker keeps draining and a consistent raw
surface can continue or be rebuilt without semantic dependencies.

## 6. Measurement and regression policy

T-NFR remains the primary latency boundary: packaged application, gate-closing
HID injection on a real display and battery, at least 1,000 accepted samples,
discard rate at most 2%, p95 below one actual display refresh interval, and
zero control-plane RPCs. T-DROP/NFR-DROP and NFR-BYTES remain mandatory and
require zero dropped or reordered PTY bytes.

Every product comparison uses the same commit/build, machine class, power
state, display/refresh rate, workload, viewport, fixture, sample count and
instrumentation. Run the raw terminal baseline with enhancements disabled,
then the corresponding enabled configuration, with repeated interleaved runs
to expose thermal and ordering effects. Report raw observations and paired
deltas; do not normalize them to an easier display rate or machine.

Flow, attention, inspector, agents and activity timeline MUST NOT introduce a
statistically defensible key-to-present regression beyond measurement noise.
The repository does not yet authorize a new millisecond tolerance, a
significance test/alpha, or numeric limits for the secondary metrics below.
Until a later consolidated product decision sets any necessary numeric
boundary, those metrics are baseline comparisons and cannot be silently marked
passing. The existing NFR-KEY, zero-RPC and zero-byte-loss boundaries still
apply without change.

Each matrix run records at minimum:

- key-to-present p50, p95 and p99;
- PTY read/drain throughput;
- dropped and reordered byte counts;
- frame time and missed frames;
- CPU usage and memory growth;
- queue depth, high-water mark and overflow count;
- time to an available consistent Raw fallback;
- cross-pane and cross-client interference;
- control-plane RPC count on the protected path; and
- allocations on key, PTY-read and display callbacks where existing tooling
  permits.

A feature that misses an authoritative boundary remains disabled, deferred,
off-path or redesigned. A PR MUST NOT weaken T-NFR, remove an existing gate or
substitute a new instrument. `make fast`, `make gates`, packaged application
tests, real-PTY tests, negative controls/mutations and required CI remain
mandatory in addition to the new Red gates.

## 7. Named gates

All gates below are Red until they have a named implementation issue, a
demonstrated behavior-absent red, the required mutation red, an unmutated
green, and the packaged/CI evidence required by ADR 0002.

- T-PERF-BASELINE-PARITY
- T-PERF-PTY-DRAIN-INDEPENDENT
- T-PERF-PRESENT-INDEPENDENT
- T-PERF-SEMANTIC-DEGRADATION
- T-PERF-PANE-ISOLATION
- T-PERF-CLIENT-ISOLATION
- T-PERF-BOUNDED-RESOURCES
- T-PERF-BYTE-FIDELITY
- T-PERF-RAW-TUI-BYPASS
- T-PERF-RECOVERY-ISOLATION
- T-CONTENT-SOURCE-AUTHORITY

The normative procedures, mutations and 20-workload matrix are in
[TEST-CASES](../TEST-CASES.md#terminal-performance-invariant-red).

## 8. Out of scope

This specification does not implement a profiler, select a database, authorize
telemetry, set a new latency tolerance, make a remote-network result NFR-KEY or
claim that documentation-only gates are Proven.
