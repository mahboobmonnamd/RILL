# ADR 0047: Attention is an orchestration-plane queue

- **Status:** Accepted — 2026-08-18
- **Historical identifier:** merged as ADR 0029 in PR #278; renumbered to ADR
  0047 on 2026-08-21 with its series. Renumbering changed no decision.
- **Tree:** this repository only
- **Issue:** catalog epic [#33](https://github.com/mahboobmonnamd/RILL/issues/33).
  Rows authorized by this ADR:
  F-110 [#144](https://github.com/mahboobmonnamd/RILL/issues/144), F-111
  [#145](https://github.com/mahboobmonnamd/RILL/issues/145), F-112
  [#146](https://github.com/mahboobmonnamd/RILL/issues/146), F-113
  [#147](https://github.com/mahboobmonnamd/RILL/issues/147), F-114
  [#148](https://github.com/mahboobmonnamd/RILL/issues/148), F-115
  [#149](https://github.com/mahboobmonnamd/RILL/issues/149), F-116
  [#150](https://github.com/mahboobmonnamd/RILL/issues/150), F-117
  [#151](https://github.com/mahboobmonnamd/RILL/issues/151), F-118
  [#152](https://github.com/mahboobmonnamd/RILL/issues/152), F-119
  [#153](https://github.com/mahboobmonnamd/RILL/issues/153), F-120
  [#154](https://github.com/mahboobmonnamd/RILL/issues/154), F-121
  [#155](https://github.com/mahboobmonnamd/RILL/issues/155), F-122
  [#156](https://github.com/mahboobmonnamd/RILL/issues/156), F-123
  [#157](https://github.com/mahboobmonnamd/RILL/issues/157), F-124
  [#158](https://github.com/mahboobmonnamd/RILL/issues/158).
- **Requires:** [ADR 0001](0001-session-operating-system.md) (planes),
  [ADR 0011](0011-session-graph.md), [ADR 0013](0013-cwd-tap.md) (cold taps),
  [ADR 0038](0038-session-graph-navigation-model.md) (`NodeId`, projection),
  [ADR 0044](0044-trust-secrets-and-automation-boundary.md) (untrusted input,
  redaction)
- **Amends:** nothing.
- **Does not authorize:** agents or the `Task` object (ADR 0048), a cloud relay,
  push notifications through a server, an account, JSON on the warm path, a
  classifier that decides whether input is a prompt (ADR 0049 D9).
- **Milestone:** M3 — Conversations

## Context

Fifteen rows: attention queue (F-110), sidebar badges (F-111), in-app mailbox
(F-112), jump to exact target (F-113), next-attention shortcut (F-114), native
OS notifications (F-115), suppress when focused (F-116), OSC 9/99/777 (F-117),
CLI notify (F-118), notification hooks (F-119), quiet and rate limit (F-120),
long-command complete (F-121), password-prompt notify (F-122), agent blocked
rollup (F-123), mark read on view (F-124).

These are the product's answer to the real problem it exists for: many panes,
most of them waiting, one of them needing you. Getting it wrong in either
direction is fatal — a terminal that cries wolf gets muted, and a terminal that
stays quiet while an agent waits for approval wastes the time it promised to
save.

Every row is `plane: orchestration`. That is the load-bearing fact: attention
MUST NOT be computed on, or cost anything to, the warm path.

## Decision

### 2026-08-21 amendment — stable structured attention and responses

ADR 0053 D18 governs the product contract beyond the already-Proven queue
library. Every product entry gains a stable AttentionId, exact Workspace,
Session, Tab, Pane, optional TerminalExecution and Task references, a source
StructuredRequestId when actionable, lifecycle/expiry, authorization policy,
navigation target and allowed actions. The queue projects source request and
runtime events; it does not become a second approval authority.

Only safe single-step structured requests may be answered inline. Raw/TUI,
secret and ambiguous interactions navigate to the exact owning pane.
Authenticated responses bind request ID and generation and reject expired,
stale, duplicate or replayed input. Terminal-cell scraping never creates an
actionable control, and secret values never enter attention or notification
previews. These clauses require new Red product gates; existing library gates
do not prove them.

### D1 — One queue, one state machine, one source of truth

There is exactly one attention queue. Every surface in this ADR — badges,
mailbox, OS notifications, rollups, the next-attention shortcut — reads it.

An entry has: a `NodeId` target (ADR 0038 D1), a state, an origin, a monotonic
sequence number, and a timestamp. States are exactly F-110's:

`needs_input` · `approval` · `completed_unread` · `failed` · `disconnected`

No surface may invent a state, and no surface may derive attention
independently. ADR 0042 D7's workspace status lane reads this queue; it does not
run a second classifier. Two classifiers disagree, and the user believes
whichever one is louder.

Mutation `second_attention_classifier` MUST turn T-ATT-ONEQUEUE red.

### D2 — Producing an entry is cold and bounded

Entries are produced by orchestration-plane events: an escape sequence Chip 0
already parsed (D3), a CLI call (D4), a kernel event (child exit, disconnect), or
later an agent adapter (ADR 0049 D3).

Enqueue MUST NOT allocate on the key path, MUST NOT block the PTY reader, and
MUST NOT require chrome to be visible. The queue is bounded per leaf; on
overflow the **oldest non-actionable** entries drop first, and `needs_input` and
`approval` MUST NOT be dropped in favour of `completed_unread`.

An `--nfr-key` run MUST show zero control-plane RPCs with the queue live
(PRD NFR-KEY, ADR 0039 D1).

### D3 — Escape-sequence notify is parsed by Chip 0 and is untrusted

F-117. OSC 9, OSC 99 and OSC 777 are parsed **inside the Chip 0 adapter** like
every other sequence (ADR 0040 D1). The host MUST NOT scan the byte stream.

Their content is untrusted (ADR 0044 D1): it comes from any process, local or
remote, that can write to a PTY. Therefore a notification body MUST NOT be
rendered as markup, MUST NOT carry a clickable action other than "jump to this
pane", MUST be length-capped, MUST be redacted at the OS-notification sink
(ADR 0044 D4), and MUST be attributed to the pane and host it came from.

An OSC notification MUST NOT be able to spoof another pane's identity, raise its
own urgency past `needs_input`, or suppress another notification.

BEL (F-103, ADR 0040 D8) raises an entry here rather than only making a sound.

Mutation `render_notify_body_as_markup` MUST turn T-ATT-UNTRUSTED red.

### D4 — CLI notify is the socket, with the socket's rules

F-118. `rill notify --title --body` goes over the daemon socket (ADR 0044 D7):
user-scoped, foreign uid refused, explicit verb. It MUST target a `NodeId` the
caller can already reach, and MUST NOT be able to address another user's pane.

Remote notify (F-175, ADR 0041 D7) arrives on the same connection as its host's
attach and carries that host's verified identity. It MUST NOT claim to be local.

### D5 — Delivery is suppressed by focus, then by policy, and rate limits are per origin

- **Suppress when focused (F-116):** if the target pane is visible **and** the
  window is key **and** the pane is the focused one, no OS notification fires.
  The queue entry is still recorded, then immediately marked read (D7).
- **Quiet and rate limit (F-120):** per-origin cooldown with storm grouping. N
  entries from one origin inside the window collapse to one grouped
  notification that names the count. Rate limiting MUST NOT drop `approval` or
  `needs_input` — those are grouped, never suppressed.
- **Long-command complete (F-121):** a command that ran longer than a
  configurable threshold and finishes while its pane is unfocused produces
  `completed_unread`. Duration comes from shell integration where present and
  from process lifetime otherwise; when neither is available, no entry — it MUST
  NOT guess (ADR 0040 D4's degrade rule).
- **Password prompt (F-122):** a hidden pane that turns off echo is
  `needs_input`. This is a **capability observation** from Chip 0's terminal
  mode, not content inspection: RILL MUST NOT scan output for the word
  "password", and MUST NOT record what is typed.

### D6 — Hooks may filter and rewrite, and may not escalate or execute

F-119. Notification hooks are declared in **trusted** config only
(ADR 0044 D2). A hook MAY suppress, retag, or rewrite an entry's title and body.

A hook MUST NOT: run a command, raise urgency above what the producer set, alter
the `NodeId` target, unsuppress something the user muted, or observe entries from
a workspace it was not granted. Hook failure is fail-closed: on error the entry
is delivered unmodified (NFR-FAIL) rather than dropped.

Mutation `hook_can_exec` MUST turn T-ATT-HOOK red.

### D7 — Jump is exact, and viewing is what clears

- **Jump (F-113):** selecting an entry focuses the exact target — host,
  workspace, tab, pane, and later the section within a task (ADR 0048 D3). It
  navigates only; it MUST NOT write to the pane (ADR 0039 D2).
- **Next-attention (F-114):** a configurable shortcut walks entries in priority
  then sequence order. Order MUST be stable: the same queue yields the same
  walk, so muscle memory works.
- **Mark read on view (F-124):** an entry clears when its target is actually
  **shown to the user** — visible, on the active Space, window key, for a
  minimum dwell. Rendering into a hidden pane MUST NOT clear it. Clearing is
  idempotent and MUST NOT resurrect on re-render.
- **Badges (F-111):** counts on workspace/tab/pane are a pure function of the
  queue. A badge that disagrees with the mailbox is a bug in the projection.

### D8 — Rollup shows the highest urgency and never hides the actionable

F-123. A parent node displays the highest-urgency state among its descendants,
in this order:

`approval` > `needs_input` > `failed` > `disconnected` > `completed_unread`

Rollup MUST NOT collapse two different actionable children into one entry that
can only be answered once. Expanding a rollup MUST reach every underlying entry.

Agent-specific rollup semantics arrive with ADR 0048; the ordering above is
fixed here so both agree.

### D9 — Oracle

| ID | Closes |
|---|---|
| T-ATT-ONEQUEUE | D1 — every surface reads one queue; no second classifier |
| T-ATT-COLD | D2 — no key-path cost; zero RPCs under `--nfr-key` |
| T-ATT-UNTRUSTED | D3 — OSC body inert, attributed, capped, redacted |
| T-ATT-SOCKET | D4 — foreign uid refused; no cross-user target |
| T-ATT-SUPPRESS | D5 — focused pane silent; actionable never dropped |
| T-ATT-HOOK | D6 — hook cannot exec or escalate; fails open to delivery |
| T-ATT-READ | D7 — clears only on real view; hidden render does not |
| T-ATT-ROLLUP | D8 — ordering; actionable children individually reachable |

## Consequences

- [SPEC-ATTENTION](../spec/SPEC-ATTENTION.md) is the queue contract.
- ADR 0042 D7's `needs-attention` lane is a read of this queue.
- ADR 0040 D8's bell and ADR 0041 D7's remote relay are producers under D2.
- ADR 0048 adds agent producers without adding states.

## Rejected alternatives

- **Per-surface attention logic.** Rejected: D1. Badge and mailbox disagree, and
  trust in both is gone.
- **Scanning output text for prompts or the word "password".** Rejected: D5.
  Content inspection of a PTY is both wrong and invasive; terminal mode is the
  honest signal.
- **Rendering notification bodies as rich markup.** Rejected: D3, untrusted.
- **Hooks that can run a command.** Rejected: D6 — a notification becomes an
  execution path.
- **Clearing unread when a pane renders offscreen.** Rejected: D7. The user did
  not see it.
- **Rate-limiting approvals during a storm.** Rejected: D5. The one thing that
  actually needs a human must never be the thing that gets dropped.
