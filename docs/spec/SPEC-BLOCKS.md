# SPEC-BLOCKS — optional terminal content grouping (`lane:host`)

- **Status:** Red. [ADR 0050](../adr/0050-blocks-are-a-cold-overlay.md) is
  superseded in part by
  [ADR 0053](../adr/0053-runtime-domain-content-and-client-authority.md) D7–D8.
- **Authority:** ADR 0053 and [SPEC-CONTENT](SPEC-CONTENT.md).
- **Requires:** [SPEC-DISPLAY](SPEC-DISPLAY.md),
  [SPEC-FIDELITY](SPEC-FIDELITY.md), [SPEC-TRUST](SPEC-TRUST.md),
  [SPEC-COMPOSITOR](SPEC-COMPOSITOR.md),
  [SPEC-TERMINAL-PERFORMANCE](SPEC-TERMINAL-PERFORMANCE.md).
- **Milestone:** after ContentTimeline and checkpoint/retention gates; historical
  M6 numbering does not override that dependency.

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. What a Block is

`Block` is an optional product label and derived grouping over one or more
ContentTimeline items. It is not the native storage/display model.

A Block may hold stable ContentItemIds, command metadata, exit status, timing,
cwd and source TerminalExecution ranges/checkpoint identity. It MUST NOT be
identified only by byte-ring offsets. Materialized timeline content survives
ordinary hot-ring eviction according to retention policy.

Normal rendering reads virtualized ContentTimeline data. Raw bytes may be
replayed from a compatible checkpoint for reconstruction, audit or disaster
recovery, not for routine Block presentation.

## 2. Boundaries

Boundaries come from explicit shell/protocol marks or a known RILL structured
input submission. Correctness MUST NOT depend on shell integration. Without a
mark, output remains an honest terminal/background region. RILL does not infer
commands from prompt regex, language classification, cursor position or timing.

A running item has no fabricated exit status. Pre-command bytes do not attach
to a new command item without an explicit ordering boundary.

## 3. Warm path

Content grouping, materialization and virtualization do not run on the
key-down-to-present path, allocate per keystroke or run on the display-link
callback. NFR-KEY remains unchanged and must pass with the content presentation
enabled and disabled before shipment.

## 4. Raw and alternate-screen mode

Raw terminal/TUI mode routes leased input directly to the PTY. Alternate screen
selects the mutable grid presentation of the same TerminalPane and
TerminalExecution. It does not spawn, allocate a second PTY, or create a Block
containing the live TUI grid.

Nested tmux, Vim, Neovim and other full-screen programs remain ordinary raw
terminal children.

## 5. Actions

Copy, search, rerun, bookmark, share and attach-as-context operate on explicit
ContentItems and declared source ranges. Missing or policy-deleted source is
shown as unavailable.

Rerun places the exact retained command in structured input for explicit
submission; it does not execute on click. Clipboard, share and context attach
are derived sinks governed by capture/redaction/transmission policy. A redacted
copy does not rewrite canonical source or claim perfect secret removal.

## 6. Rich input and agent content

Structured input is owned by `rill-editor` and creates explicit timeline,
Conversation or Task events. Writing into a CLI agent's PTY uses the same
client lease and adapter capability as other PTY input. It does not bypass
permissions or create a second input owner.

Agent messages, tools, approvals, questions and diffs are first-class typed
ContentItems. They are not encoded as fake terminal Blocks or merged with the
Session/transcript identity.

## 7. Retention

Blocks inherit the most restrictive applicable retention policy. Durable
capture may be disabled entirely. A Block whose retained material or source was
deleted becomes a visible tombstone or truncated item; it does not reconstruct
different output from a moving ring.

## 8. Gates

| ID | Status | Closes |
|---|---|---|
| T-BLOCK-CONTENT-IDENTITY | Red | §1 |
| T-BLOCK-BOUNDARY | Red | §2 |
| T-BLOCK-WARM-BOUNDARY | Red | §3; paired with T-PERF-BASELINE-PARITY |
| T-BLOCK-RAW | Red | §4 |
| T-BLOCK-RERUN | Red | §5 |
| T-BLOCK-RETENTION | Red | §7 |

## 9. What we will not do

- Treat ring offsets as durable content identity.
- Replay arbitrary byte slices for normal rendering.
- Infer command boundaries by regex or timing.
- Put a live TUI in a scrolling Block with another VT.
- Require shell integration for correctness.
- Execute on rerun click or bypass input leases.
- Claim encryption/redaction authorizes capture.
