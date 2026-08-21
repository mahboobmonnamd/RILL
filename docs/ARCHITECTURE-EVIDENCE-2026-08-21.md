# Architecture evidence — 2026-08-21

- **Status:** Research record; not authority and not implementation proof.
- **Repository snapshot:** live `main` at `1988eea` before ADR 0053.
- **Decision:** [ADR 0053](adr/0053-runtime-domain-content-and-client-authority.md).

This record preserves the repository and external workflow evidence used for
the 2026-08-21 architecture decision. External products are evidence about
workflows and capability boundaries; none is adopted as RILL's internal
architecture.

## Accepted follow-up requirements — verbatim

The product owner supplied and accepted this follow-up on 2026-08-21:

1. **Shell compatibility:** RILL must preserve normal behaviour for zsh, fish,
   bash, and other PTY-compatible shells. Existing shell configurations,
   prompts, themes, plugins, ANSI colours, and interactive features must work
   without RILL-specific replacement or modification. Remote hosts must remain
   zero-footprint by default.
2. **Unified configuration:** Define one versioned TOML configuration model for
   RILL. It must cover app and terminal themes, fonts, font sizes, keybindings,
   rendering, workspace behaviour, and other user settings. A named theme must
   apply consistently throughout the application. Configuration must support
   validation, migration, export, backup, and optional sync; secrets and host
   credentials must never be included.
3. **Privacy and PII:** Establish privacy-by-design requirements covering
   terminal output, commands, transcripts, logs, telemetry, crash reports,
   clipboard data, agent context, local retention, backup, and sync. Minimize
   collection, disable sensitive persistence where required, enforce
   policy-controlled retention and redaction, encrypt stored data, and prevent
   credentials, secrets, and PII from leaking across users, hosts, sessions,
   logs, agents, or external services.

## Repository findings

| Concern | Authority at snapshot | Implementation at snapshot | Finding |
|---|---|---|---|
| ADR identity | two Accepted ADR 0020–0023 series | references used number and slug inconsistently | authority was ambiguous; repaired by the ADR registry and 0038–0052 renumbering |
| GUI persistence | ADR 0010/0014 | GUI launched detached `rilld`; daemon owned PTY | sound GUI-close foundation, not a managed service or daemon-crash boundary |
| Session meaning | ADR 0011/0038 and SPEC-GRAPH/NAV | `Session` owned PTY/ring/credit; later Workspace acted as named session | execution and durable grouping were conflated |
| Multi-client | attach and persist ADRs | writer/observer flags shared execution credit/resize paths | connection multiplicity existed; independent roles/credit/leases were not proven |
| Recovery | history/resync ADRs | bounded moving byte ring and headless VT repaint | no stable offsets/checkpoint contract for long disconnect or eviction |
| Blocks | ADR 0050 historical decision | no integrated content model | arbitrary byte ranges lack preceding VT state and durable display identity |
| Agent Task | ADR 0048 and SPEC-TASK | reduced library Task plus text serialization | section mechanics were proven; complete object and runtime persistence were not |
| Display | ADR 0003/0009 and display specs | Metal glyph atlas and instanced terminal cells | strong terminal primitive; no general rich-content compositor/text boundary |
| Remote/mobile | ADR 0041 | local Unix transport only | remote/mobile semantics remained Red; SSH alone was insufficient |
| Security/failure isolation | attach/kernel specs | local 0600 socket and shared daemon loop | protected runtime root, peer/role isolation and malformed-client containment remained gaps |

The snapshot's existing tests remain evidence only for their named downstream
oracles. Static code/document inspection did not reclassify a Red product gate
as Proven.

## UI/workflow concept input — 2026-08-21

The product owner supplied a RILL interactive prototype, two screenshots and a
detailed architecture-reconciliation prompt. The source references were:

- [interactive prototype](https://rill-interactive-prototype.mahboobmonnamd.chatgpt.site/)
- `ChatGPT Image Aug 21, 2026, 07_20_46 AM.png`
- `ChatGPT Image Aug 21, 2026, 07_20_38 AM.png`

The prototype is explicitly non-authoritative visual evidence. From the supplied
screenshots it demonstrates terminal-first split panes, normal-shell Flow,
full-grid agent/TUI presentation, server/log panes, optional inspector,
attention/approval affordances and keyboard/mouse navigation. Cards, spines,
gutters, tabs, inspector geometry, badges and timeline layout remain client
choices.

The live prototype URL returned HTTP 401 from the architecture-review
environment on 2026-08-21, so no claim is made about interactions that were not
visible in the supplied screenshots. This limitation does not affect the
normative requirements supplied in the prompt.

Repository reconciliation found no conflict with Q1–Q10. It confirmed these
deltas:

- typed non-terminal panes require stable lifecycle but never PTY ownership;
- Flow/Raw/TUI are presentations of one execution, with Raw independently
  operable when semantic work fails;
- the semantic transcript needs stable event envelopes, ordering,
  idempotency, byte correlation and snapshot/delta recovery;
- the optional activity timeline is derived rather than authoritative;
- attention needs structured request identity, authentication, exact deep
  links and stale/replay rejection;
- forks need durable parentage, hidden-by-default navigation, isolation and
  conflict semantics;
- input arbitration needs explicit modes, and unsent composer drafts default
  to sensitive client-local non-durable state;
- protocol channels need independent bounds/recovery so semantic failure cannot
  block raw terminal traffic; and
- ADR 0050's historical byte-range/replay model is explicitly superseded for
  normal Block presentation.

The accepted dependency order is PTY compatibility, host terminal authority,
the authoritative semantic transcript runtime model with policy-governed
retention, Flow projection, persistent topology, Task state,
attention/approvals, artifacts/diffs, then optional activity timeline. Domain
identity is specified before those slices but does not claim early topology
implementation.

## Workflow coverage

| Workflow | Evidence | RILL implication |
|---|---|---|
| Traditional one-shell use | WezTerm can hide tab chrome while retaining terminal behavior ([appearance](https://wezterm.org/config/appearance.html)) | implicit Workspace/Session identities with zero compulsory chrome |
| Tabs and panes | tmux separates server, session, window and pane ([manual](https://man.openbsd.org/tmux.1)) | explicit grouping and one PTY-owning execution per terminal pane |
| Persistent multiplexer | tmux detach leaves the server/session alive | client loss is detach, never process intent |
| WezTerm mux | mux manages panes/tabs/windows/workspaces without a GUI ([multiplexing](https://wezterm.org/multiplexing.html)) | process-host runtime is independent of presenters |
| Remote SSH | SSH supplies authenticated secure channels ([RFC 4251](https://www.rfc-editor.org/rfc/rfc4251)) | transport does not supply RILL graph, transcript, content or leases |
| SRE and multi-source logs | Kubernetes supports follow, multiple pods/containers, prefixes, limits and timestamps ([kubectl logs](https://kubernetes.io/docs/reference/kubectl/generated/kubectl_logs/)) | source attribution, independent bounds and background output are first-class |
| Development servers/logs | VS Code reconnects terminal processes and content ([terminal persistence](https://code.visualstudio.com/docs/terminal/advanced)) | process persistence and content restoration are separate requirements |
| Vim/Neovim/TUIs | xterm defines normal/alternate-screen controls ([control sequences](https://invisible-island.net/xterm/ctlseqs/ctlseqs.html)) | alternate screen stays a mutable grid on the same PTY |
| One coding agent | remote work exposes approvals/changes while execution stays on a host ([remote engineering](https://developers.openai.com/blog/mastering-codex-remote-for-engineering)) | Task/approval content is distinct from terminal execution |
| Concurrent coding agents | subagents and worktrees create parallel independently inspectable work ([subagents](https://learn.chatgpt.com/docs/agent-configuration/subagents), [worktrees](https://learn.chatgpt.com/docs/environments/git-worktrees)) | Task, Conversation, Workspace and execution identities stay separate |
| Mobile control | phone is a control plane for work running on a connected host ([Codex Remote](https://learn.chatgpt.com/docs/remote)) | host authority, disposable mobile state and no offline input injection |
| Users hiding product UI | WezTerm can suppress tab rendering ([hidden tabs](https://wezterm.org/config/lua/config/show_tabs_in_tab_bar.html)) | hidden is presentation state, not object deletion or a second runtime path |

Public discussions supplied anecdotal workflow confirmation, not normative
authority: persistence after closing a GUI
([WezTerm #2923](https://github.com/wezterm/wezterm/discussions/2923)), moving
between desktop and laptop clients
([#3901](https://github.com/wezterm/wezterm/discussions/3901)), and the limit of
workspaces that survive only while the mux server lives
([#1665](https://github.com/wezterm/wezterm/discussions/1665)).

## Content and compositor evidence

Warp's published architecture shows why arbitrary mixed content needs an
element/scene system beyond terminal cells and why its alternate-screen grid is
separate from ordered command content
([architecture](https://www.warp.dev/blog/how-warp-works),
[block model](https://www.warp.dev/blog/block-model-behind-warps-agentic-development-environment)).
RILL uses that as capability evidence only and adopts its own ContentTimeline.

Mosh demonstrates state synchronization and sequence/recovery principles for
intermittent clients ([technical information](https://mosh.org/)). It does not
replace RILL transport, terminal state or content semantics.

Ghostty and xterm.js demonstrate useful library/client boundaries:

- Ghostty exposes terminal components while retaining native platform shells
  ([Ghostty overview](https://ghostty.org/docs/about)).
- xterm.js has browser/headless/render-addon boundaries
  ([repository](https://github.com/xtermjs/xterm.js)), but a browser RILL client
  would still need RILL protocol, content, compositor and TypeScript APIs.

## Platform service evidence

Apple directs apps needing a persistent background service to Service
Management and a LaunchAgent/daemon, with visible user control
([background processes](https://developer.apple.com/documentation/appkit/managing-ongoing-background-processes-in-your-mac),
[SMAppService](https://developer.apple.com/documentation/servicemanagement/smappservice)).
That supports replacing direct production GUI launch with a registered per-user
runtime while preserving the proven GUI/PTY separation.

## Evidence limits

- No comparative renderer-speed claim was made; equivalent benchmarks do not
  exist.
- External workflow examples do not prove RILL implementation.
- Plain SSH does not prove RILL persistence, transcript, rich content or
  multi-client behavior.
- Encryption and redaction do not prove that capture is permitted or safe.
- An Accepted ADR or passing library unit test is not packaged/product E2E
  evidence.
