# ADR 0031: Agent adapters, lifecycle authority and capability gates

- **Status:** Accepted — 2026-08-18
- **Tree:** this repository only
- **Issue:** catalog epic [#33](https://github.com/mahboobmonnamd/RILL/issues/33).
  Rows authorized by this ADR:
  F-135 [#164](https://github.com/mahboobmonnamd/RILL/issues/164), F-136
  [#165](https://github.com/mahboobmonnamd/RILL/issues/165), F-137
  [#166](https://github.com/mahboobmonnamd/RILL/issues/166), F-138
  [#167](https://github.com/mahboobmonnamd/RILL/issues/167), F-139
  [#168](https://github.com/mahboobmonnamd/RILL/issues/168), F-141
  [#170](https://github.com/mahboobmonnamd/RILL/issues/170), F-143
  [#172](https://github.com/mahboobmonnamd/RILL/issues/172), F-144
  [#173](https://github.com/mahboobmonnamd/RILL/issues/173), F-149
  [#178](https://github.com/mahboobmonnamd/RILL/issues/178), F-150
  [#179](https://github.com/mahboobmonnamd/RILL/issues/179), F-151
  [#180](https://github.com/mahboobmonnamd/RILL/issues/180), F-153
  [#182](https://github.com/mahboobmonnamd/RILL/issues/182), F-154
  [#183](https://github.com/mahboobmonnamd/RILL/issues/183), F-155
  [#184](https://github.com/mahboobmonnamd/RILL/issues/184), F-156
  [#185](https://github.com/mahboobmonnamd/RILL/issues/185), F-157
  [#186](https://github.com/mahboobmonnamd/RILL/issues/186), F-158
  [#187](https://github.com/mahboobmonnamd/RILL/issues/187), F-159
  [#188](https://github.com/mahboobmonnamd/RILL/issues/188), F-163
  [#192](https://github.com/mahboobmonnamd/RILL/issues/192), F-164
  [#193](https://github.com/mahboobmonnamd/RILL/issues/193).
- **Requires:** [ADR 0011](0011-session-graph.md),
  [ADR 0022](0022-terminal-fidelity-is-chip0.md) D1 (Chip 0 owns parsing),
  [ADR 0026](0026-trust-secrets-and-automation-boundary.md),
  [ADR 0029](0029-attention-is-an-orchestration-queue.md),
  [ADR 0030](0030-task-is-the-agent-runtime-object.md) (sections, replay first)
- **Amends:** nothing.
- **Does not authorize:** an account, a hosted control plane, sending code or
  transcripts off the machine by default, bundling provider credentials, a
  natural-language router on the Enter path (PRD §6), Blocks, Chip 1 live.
- **Milestone:** M3 — Conversations

## Context

Twenty rows: detect CLI agents (F-135), lifecycle authority (F-136), broad
coverage (F-137), native structured adapter (F-138), CLI-native path (F-139),
permission profiles (F-141), MCP servers (F-143), rules/skills (F-144), direct
agent attach (F-149), agent CLI orchestration (F-150), integrations install
(F-151), worktrees for parallel tasks (F-153), model picker / BYOK (F-154), voice
input (F-155), computer/browser use (F-156), cloud agents (F-157), cloud-synced
conversations (F-158), session sharing (F-159), agent drives TUI (F-163), NL
autodetection (F-164).

ADR 0030 defined the `Task` and required the replay adapter to land first. This
ADR is how real agents attach to that object.

The central difficulty is stated in F-135 and F-136 together: a CLI agent is a
process in a PTY, and the only universally available signal is what it printed.
Screen scraping is unavoidable as a **fallback** and unacceptable as a
**foundation** — every scraper is one prompt-format change away from silently
reporting the wrong state. F-136 already names the resolution: hooks beat
scraping. This ADR makes that a hierarchy with a fail-closed bottom.

## Decision

### D1 — Three adapter kinds, one section contract

Every adapter, whatever its transport, emits ADR 0030 D3's closed section set:

1. **Replay** (ADR 0030 D2) — the conformance reference. Ships first.
2. **Structured** (F-138) — a provider SDK or protocol translated to sections.
3. **CLI/PTY** (F-139) — an agent running in a leaf, observed.

A surface MUST NOT branch on adapter kind. If a surface needs to know which
adapter it is talking to, the section contract is incomplete and that is the bug
to fix.

Broad coverage (F-137) means many adapters, not many code paths.

### D2 — Lifecycle authority is ranked, and unknown is a state

F-135, F-136. Signals about an agent's state are ranked:

1. **Hooks / integrations** the agent itself emits (F-151) — authoritative
2. **Structured protocol** from an SDK adapter — authoritative
3. **Process state** — the child exists, exited, is reading — reliable, coarse
4. **Screen manifest** — matching known output shapes — advisory only

A lower-ranked signal MUST NOT override a higher one. When only rank 4 is
available and it does not match, the state is **`unknown`**, not a guess.
`unknown` MUST be visible in the UI as unknown.

This is the fail-closed rule (NFR-FAIL) applied to observation: reporting `idle`
for an agent that is actually waiting is the failure that costs the user an
hour, and it is exactly what a confident scraper produces.

Screen manifests MUST be data (config, ADR 0025 D1), versioned and
user-overridable — never compiled-in regexes that require a release to fix.

Named tests `t_hook_signal_beats_screen_manifest` and
`t_unmatched_manifest_reports_unknown_not_idle`. Mutation
`manifest_overrides_hook` MUST turn T-AGENT-AUTHORITY red.

### D3 — Observation is cold and does not touch the warm path

Detection reads process state and Chip 0's existing parse output. It MUST NOT
add a parser to the byte stream (ADR 0022 D1), MUST NOT copy the grid per frame,
and MUST NOT sample faster than ADR 0021 D1's 2 Hz ceiling.

An attached agent leaf MUST still meet NFR-KEY when the user types into it. A
CLI agent pane is a terminal pane first.

Mutation `detect_scans_every_frame` MUST turn T-AGENT-COLD red.

### D4 — Writing to an agent goes through one gated path

F-149, F-150, F-163. Attaching the terminal to an agent PTY, orchestrating it by
CLI (`start`/`wait`/`prompt`/`read`/`cancel`), and driving a TUI by synthetic
keystrokes are all the same dangerous capability: **RILL writes bytes a human
did not type**.

There MUST be exactly one code path that does this, and it MUST:

- require an explicit granted capability (D5),
- respect FR-ONE — it is the leaf's single writer, or it does not write,
- be visibly attributed in the UI while it holds the write,
- refuse when the target is not the task's declared target `NodeId`.

Agent-drives-TUI (F-163) additionally MUST be per-session opt-in, MUST NOT be a
default, and MUST be interruptible by the user at any keystroke — the human's
key always wins.

The CLI verbs live on the daemon socket under ADR 0026 D7's rules. There is no
`exec` verb.

Mutation `agent_writes_without_capability` MUST turn T-AGENT-WRITE red.

### D5 — Permission profiles are the one grant model, scoped and revocable

F-141. Allow / deny / ask per host, workspace, and task. This is the *same*
model as ADR 0026 D3's plugin capabilities, not a second one.

Defaults are **deny** for anything that writes, spawns, reaches the network, or
leaves the machine. "Ask" is the strongest default RILL ships for a
consequential capability; "allow everything" MUST NOT be a preset, and MUST NOT
be reachable in one click.

A grant is scoped (host + workspace + task), revocable, visible in one place,
and MUST NOT widen implicitly — including on agent update, on a new task in the
same workspace, or via ADR 0030 D4's remembered choices.

MCP servers (F-143) are capability-gated the same way: a server declares what it
wants, the user grants it, and the grant does not extend to another server or
another workspace. An MCP server is untrusted (ADR 0026 D1) and MUST NOT reach a
leaf without D4's path.

Rules and skills (F-144) are **local files** the user owns. They MUST NOT be
fetched from a service, and MUST NOT grant capabilities — a rules file that
could widen a permission would be a trust bypass by text.

### D6 — Credentials are the user's and stay on the machine

F-154. Model picker and BYOK read keys from local config or the OS keychain.
RILL MUST NOT ship provider credentials, MUST NOT proxy requests through any
RILL-operated service, and MUST NOT log or transmit a key. Keys are secrets
under ADR 0026 D4 at every sink.

Switching models MUST NOT silently change a task's permission profile.

### D7 — Parallel tasks get explicit worktrees

F-153. Concurrent agent tasks in one repository MUST use explicit git worktrees
(ADR 0024 D4), created with the path shown and confirmed.

Two tasks MUST NOT write the same working tree concurrently. If a worktree
cannot be created, the second task MUST refuse to start rather than share —
silent overlap corrupts work in a way no checkpoint reliably undoes.

### D8 — Everything cloud is optional, off, and additive

F-157 (cloud agents), F-158 (cloud-synced conversations), F-159 (session
sharing) are optional layers. With all of them off, RILL MUST be fully
functional (ADR 0026 D5): local history complete, local agents working, no
account.

None ships in M2 or M3. When one does:

- sharing a running session MUST be per-session, explicit, revocable, and MUST
  show what is exposed while it is exposed,
- sync MUST apply redaction (ADR 0026 D4) before anything leaves,
- a trigger from outside (Slack, GitHub) is untrusted input (ADR 0026 D1) and
  MUST NOT start a capability-holding task without a grant that already existed.

### D9 — Enter is not classified, and voice is transcription only

F-164 asks for a local classifier deciding whether input is a shell command or
an agent prompt. **Rejected**, restating PRD §6: Enter → PTY, ⌘Enter →
conversation. No PATH heuristic, no English heuristic, no local model on the
input path.

The reason is not purity. A classifier that is wrong 1% of the time sends
commands to a model and prompts to a shell, and the user can never trust either
key again. An explicit key is 100% correct forever.

F-164 closes as `wontfix`.

Voice input (F-155) is transcription into the input field, subject to the same
rule: the transcript lands as text, and the user presses Enter or ⌘Enter. Audio
MUST be opt-in, MUST show when the microphone is live, and MUST NOT be
transmitted without the user having granted that specific capability.

### D10 — Computer and browser use is capability-gated and out of scope now

F-156. Agent-driven GUI or browser control is D4's write capability plus
ADR 0024 D2's browser boundary. It MUST run in the out-of-process browser, MUST
NOT drive the host UI, MUST NOT act on any surface outside its granted scope,
and MUST be interruptible.

Nothing ships in M3. The constraint is recorded so it is not designed weaker
later.

### D11 — Oracle

| ID | Closes |
|---|---|
| T-AGENT-AUTHORITY | D2 — hooks beat manifests; unmatched is `unknown` |
| T-AGENT-COLD | D3 — no per-frame scan; NFR-KEY holds in an agent pane |
| T-AGENT-WRITE | D4 — one gated writer; human keystroke interrupts |
| T-AGENT-PERM | D5 — deny by default; grants do not widen implicitly |
| T-AGENT-KEY | D6 — no key at any sink; no proxy |
| T-AGENT-WORKTREE | D7 — second task refuses rather than shares |
| T-AGENT-OFFLINE | D8 — fully functional with all cloud off |

All of these MUST be demonstrable against the replay adapter plus a fake CLI
agent fixture. No gate may require a live provider (ADR 0030 D2).

## Consequences

- [SPEC-AGENT](../spec/SPEC-AGENT.md) is the adapter, authority and permission
  contract.
- ADR 0030's `Task` gains live producers without new section types.
- ADR 0026 D3's capability model is reused, not duplicated.
- F-164 closes `wontfix`. F-156, F-157, F-158, F-159 record constraints and ship
  nothing in M3.

## Rejected alternatives

- **Screen scraping as the primary lifecycle signal.** Rejected: D2. One prompt
  redesign and every state is quietly wrong.
- **Report `idle` when no manifest matches.** Rejected: D2. `unknown` is honest;
  `idle` costs the user the hour the product exists to save.
- **Compiled-in regex manifests.** Rejected: D2 — a provider's cosmetic change
  should not require shipping a release.
- **Per-adapter UI branches for richer provider features.** Rejected: D1.
- **An "allow all" convenience preset.** Rejected: D5.
- **Proxying model requests through a RILL service.** Rejected: D6, ADR 0026 D5.
- **Sharing a worktree between two agent tasks with locking.** Rejected: D7 —
  locking a working tree between two writers is not a solved problem here.
- **A local NL classifier on Enter.** Rejected: D9, PRD §6.
