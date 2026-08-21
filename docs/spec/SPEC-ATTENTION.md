# SPEC-ATTENTION — the attention queue (`lane:host`, orchestration plane)

- **Status:** Accepted — 2026-08-18. `crates/rill-orchestrate/src/attention.rs`
  implements the queue, rollup ordering, view-based read-clearing, and
  bounded enqueue. **T-ATT-ONEQUEUE, T-ATT-ROLLUP and T-ATT-READ are Proven
  at the library level**, plus the bounded-queue (never-drop-actionable)
  half of §2 — `cargo test -p rill-orchestrate --test attention_gates`,
  red-then-green under `--features mutate` (evidence below). The
  zero-control-plane-RPC half of T-ATT-COLD, T-ATT-UNTRUSTED (needs Chip 0's
  OSC parser), T-ATT-SOCKET (needs a running daemon) and T-ATT-HOOK are not
  attempted here and stay **Red**.
- **Authority:** [ADR 0047](../adr/0047-attention-is-an-orchestration-queue.md),
  amended by [ADR 0053](../adr/0053-runtime-domain-content-and-client-authority.md)
  D18. Existing library evidence does not prove the structured product
  contract below.
- **Requires:** [SPEC-NAV](SPEC-NAV.md), [SPEC-TRUST](SPEC-TRUST.md),
  [SPEC-CHIP0](SPEC-CHIP0.md), [SPEC-REMOTE](SPEC-REMOTE.md)
- **Milestone:** M3 — Conversations

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. One queue

- There MUST be exactly one attention queue. Badges, mailbox, OS notifications,
  rollups and the next-attention shortcut all read it.
- A product entry has stable AttentionId, exact WorkspaceId, SessionId, TabId
  and PaneId, optional TerminalExecutionId/TaskId/StructuredRequestId, state,
  type, urgency, origin, monotonic sequence, timestamp, lifecycle/expiry,
  authorization policy, navigation target and allowed actions. The existing
  library entry is a partial representation.
- States are exactly: `needs_input`, `approval`, `completed_unread`, `failed`,
  `disconnected`.
- No surface MAY invent a state or derive attention independently. Workspace
  status lanes (SPEC-SURFACES §7) read this queue.

## 2. Producing entries

- Producers are orchestration events: a sequence Chip 0 already parsed, a CLI
  call, a kernel event, or an agent adapter.
- Enqueue MUST NOT allocate on the key path, MUST NOT block the PTY reader, and
  MUST NOT require chrome to be visible.
- The queue is bounded per leaf. On overflow the oldest **non-actionable**
  entries drop first. `needs_input` and `approval` MUST NOT be dropped in favour
  of `completed_unread`.
- An `--nfr-key` run MUST report zero control-plane RPCs with the queue live.

## 3. Escape-sequence notify

- OSC 9, 99 and 777 MUST be parsed inside the Chip 0 adapter. The host MUST NOT
  scan the byte stream.
- Notification content is untrusted. A body MUST NOT be rendered as markup, MUST
  be length-capped, MUST be redacted at the OS-notification sink, and MUST be
  attributed to its pane and host.
- An OSC notification MUST NOT spoof another pane's identity, raise urgency past
  `needs_input`, or suppress another notification.
- BEL raises an entry here (SPEC-FIDELITY §8).

## 4. CLI and remote notify

- `rill notify` goes over the daemon socket under SPEC-TRUST §7 rules.
- It MUST target a `NodeId` the caller can already reach and MUST NOT address
  another user's pane.
- Remote notify carries its host's verified identity and MUST NOT claim to be
  local.

## 5. Delivery

- No OS notification fires when the target pane is visible **and** the window is
  key **and** the pane is focused. The entry is still recorded, then marked read.
- Rate limiting is per origin with storm grouping; N entries collapse to one
  grouped notification naming the count.
- Rate limiting MUST NOT drop `approval` or `needs_input` — they group, never
  suppress.
- Long-command completion produces `completed_unread` past a configurable
  threshold when unfocused. With no duration source, no entry — it MUST NOT
  guess.
- A hidden pane that turns off echo is `needs_input`. This is a terminal-mode
  observation. RILL MUST NOT scan output for the word "password" and MUST NOT
  record what is typed.

## 6. Hooks

- Hooks are declared in trusted config only (SPEC-TRUST §2).
- A hook MAY suppress, retag, or rewrite title and body.
- A hook MUST NOT run a command, raise urgency above the producer's, alter the
  target `NodeId`, unsuppress a muted entry, or observe an ungranted workspace.
- Hook failure MUST deliver the entry unmodified (PRD NFR-FAIL), not drop it.

## 7. Jump, walk, read

- Selection focuses the exact target and MUST NOT write to the pane.
- The next-attention walk MUST be stable: the same queue yields the same order.
- An entry clears only when its target is **shown**: visible, active Space,
  window key, minimum dwell. A hidden render MUST NOT clear it.
- Clearing is idempotent and MUST NOT resurrect on re-render.
- Badges are a pure function of the queue.

## 7.1 Structured response

- Safe, single-step structured requests MAY expose an inline allowed action.
- Raw-mode prompts, TUI-owned input, ambiguous/multi-step interactions and
  secret/password input MUST navigate to the exact owning pane.
- Secret input and sensitive request payloads MUST NOT appear in notification
  previews or be copied into an attention item.
- A response authenticates the client/role and binds StructuredRequestId,
  request generation and allowed action. Expired, stale, duplicate and replayed
  responses are rejected and audited.
- Read-only clients cannot respond, write terminal input or acquire approval
  capability by rendering an item.
- Terminal-cell scraping MUST NOT fabricate an actionable request.

## 8. Rollup

Priority order:

`approval` > `needs_input` > `failed` > `disconnected` > `completed_unread`

- A parent shows the highest-urgency descendant state.
- A rollup MUST NOT collapse two actionable children into one entry answerable
  once. Expanding MUST reach every underlying entry.

## 9. Gates

| ID | Status | Closes |
|---|---|---|
| T-ATT-ONEQUEUE | **Proven** (library) | §1 |
| T-ATT-COLD | Red (bounded-queue half Proven, library; zero-RPC half not attempted) | §2 |
| T-ATT-UNTRUSTED | Red (needs Chip 0's OSC parser, not attempted) | §3 |
| T-ATT-SOCKET | Red (needs a running daemon, not attempted) | §4 |
| T-ATT-SUPPRESS | Red (not attempted) | §5 |
| T-ATT-HOOK | Red (not attempted) | §6 |
| T-ATT-READ | **Proven** (library) | §7 |
| T-ATT-ROLLUP | **Proven** (library) | §8 |
| T-ATT-STRUCTURED-IDENTITY | Red | §§1, 7.1 |
| T-ATT-RESPONSE-AUTH | Red | §7.1 |
| T-ATT-REPLAY-REJECT | Red | §7.1 |
| T-ATT-SECRET-NAVIGATION | Red | §7.1 |

**Library evidence (2026-08-18).** `crates/rill-orchestrate/tests/attention_gates.rs`,
green, each mutation confirmed to turn only its own test red under
`--features mutate`: `second_attention_classifier`,
`overflow_drops_actionable`, `rollup_wrong_order`, `clear_on_any_render`.

## 10. What we will not do

- Run a second classifier in any surface.
- Inspect output text for prompts or passwords.
- Render notification bodies as rich markup.
- Let a hook execute a command.
- Clear unread on an offscreen render.
- Rate-limit an approval away.
