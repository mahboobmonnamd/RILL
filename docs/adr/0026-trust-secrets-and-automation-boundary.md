# ADR 0026: Trust, secrets, updates and the automation boundary

- **Status:** Accepted — 2026-08-18
- **Tree:** this repository only
- **Issue:** catalog epic [#33](https://github.com/mahboobmonnamd/RILL/issues/33).
  Rows authorized by this ADR:
  F-217 [#228](https://github.com/mahboobmonnamd/RILL/issues/228), F-220
  [#231](https://github.com/mahboobmonnamd/RILL/issues/231), F-221
  [#232](https://github.com/mahboobmonnamd/RILL/issues/232), F-222
  [#233](https://github.com/mahboobmonnamd/RILL/issues/233), F-223
  [#234](https://github.com/mahboobmonnamd/RILL/issues/234), F-224
  [#235](https://github.com/mahboobmonnamd/RILL/issues/235), F-225
  [#236](https://github.com/mahboobmonnamd/RILL/issues/236), F-229
  [#240](https://github.com/mahboobmonnamd/RILL/issues/240).
- **Requires:** [ADR 0001](0001-session-operating-system.md) (fail closed),
  [ADR 0011](0011-session-graph.md),
  [ADR 0021](0021-inventories-are-cold-readers.md) D5 (deep links),
  [ADR 0025](0025-one-look-schema-one-config-file.md) D3 (resolution order)
- **Amends:** nothing.
- **Does not authorize:** an account system, a hosted control plane, a plugin
  marketplace backend, in-process plugins, agents (ADR 0031), telemetry of user
  content, shipping a plugin runtime in M2.

## Context

Eight rows are the trust surface: project-local trusted config (F-217), plugins
out of process (F-220), plugin marketplace (F-221), socket + CLI (F-222), secret
redaction (F-223), no account for local use (F-224), signed updates (F-225),
accessibility (F-229).

PRD §5 already requires fail-closed on library and daemon paths (NFR-FAIL). This
is production software that will hold real PTYs and real secrets (AGENTS.md
preamble). Every row here is a place where a convenience becomes an execution
path or an exfiltration path, so they get one ADR and one consistent rule rather
than eight local judgements.

Accessibility is in this ADR deliberately. A keyboard-only path is not a
cosmetic nicety — it is the same property that makes the app automatable and
testable, and it belongs with the automation boundary.

## Decision

### D1 — Anything from outside the user's own config is untrusted

One rule covers project files, deep links, page content, agent output, remote
hosts, and plugin manifests: it is **data** until the user grants it authority,
and grants are explicit, scoped, and revocable.

Untrusted input MUST NOT be able to execute a command, install a binding, open a
tunnel, change settings, or reach a leaf, without a confirmation naming the
concrete action.

Fail closed: an unparseable manifest, an unknown verb, a missing signature, an
ambiguous scope — all refuse (NFR-FAIL).

### D2 — Project-local config is trusted per directory, per user, and per content

F-217. A repository may carry actions (`.rill.toml` or equivalent). RILL MUST
NOT read it for execution until the user trusts **that path**.

Trust is bound to the resolved real path *and* to a content hash. When the file
changes, trust MUST be re-confirmed with the diff shown. "Trust this folder
forever, including whatever it later contains" is not offered — a repository
that gains a malicious action on `git pull` must not gain execution with it.

Trusted config MAY contribute palette actions (ADR 0025 D6) and env. It MUST NOT
contribute keybindings that shadow control characters (ADR 0025 D5), MUST NOT
disable redaction, MUST NOT grant plugin capabilities, and MUST NOT raise its own
trust level.

Named test `t_untrusted_project_config_does_not_execute`, plus
`t_changed_trusted_config_requires_reconfirm`. Mutation `trust_by_path_only`
MUST turn T-TRUST-CONFIG red.

### D3 — Plugins run out of process, declare capabilities, and hold none by default

F-220. A plugin is a separate process with a manifest declaring the capabilities
it requests. It receives exactly what was granted and nothing implicitly.

A plugin MUST NOT: be loaded in the host address space, receive the PTY master
fd (FR-SOLE), write to a leaf without a granted capability, read config outside
its own namespace, or observe another plugin. A crashing or hanging plugin MUST
NOT stall the display link or take the window (same constraint as ADR 0024 D2).

The marketplace (F-221) is **discovery only**. Installing MUST show the
requested capabilities and MUST require confirmation. Discovery MUST NOT
auto-update an installed plugin's capability grant — a new version asks again.

No plugin runtime ships in M2. This ADR fixes the boundary so nothing is
designed against a weaker one.

### D4 — Redaction is a property of the sink, and it fails closed

F-223. Secrets MUST NOT appear in journals, diagnostics, crash reports, exported
Blocks, shared permalinks, agent context, or telemetry.

Redaction MUST be applied at the **sink** — the thing that persists or
transmits — not hopefully at the source. A sink that has no redaction pass MUST
NOT be given content, and adding a new sink without one is a bug in the sink.

Redaction MUST NOT be applied to the live terminal surface: the user typed it,
they may look at it, and rewriting the grid would corrupt what the child
actually emitted (NFR-BYTES). The boundary is persistence and transmission.

Trusted project config MUST NOT be able to weaken redaction (D2).

Named test `t_no_secret_reaches_a_persisting_sink`. Mutation
`skip_redaction_on_export` MUST turn T-TRUST-REDACT red.

### D5 — No account, and the local path never degrades

F-224. RILL MUST be fully functional with no account, no network, and no
sign-in. There MUST NOT be a feature that exists only behind an account in this
tree.

Any future optional service (settings sync ADR 0025 D7, cloud agents ADR 0031
D8, permalinks ADR 0032 D7) is additive. First run MUST NOT ask for an account,
and MUST NOT require network access to reach a working terminal.

Named test `t_first_run_is_fully_functional_offline`, executed with the network
denied. Mutation `require_account_for_feature` MUST turn T-TRUST-NOACCT red.

### D6 — Updates are signed, notarized, and reversible

F-225. Every shipped build is signed and notarized. The updater MUST verify
signature and version **before** applying, MUST refuse on failure, and MUST
support rollback to the prior version.

An update MUST NOT be applied while a leaf is attached without confirmation
(ADR 0025 D8). Applying an update MUST NOT kill the daemon's children — the
persist wedge (ADR 0001 §7 puts app update out of wedge; this decision narrows
that: the updater must not make it worse than a normal quit).

Downgrade to an unsigned or older-than-installed build MUST be refused unless
the user explicitly rolls back.

### D7 — The socket and CLI are the canonical automation surface

F-222. Automation goes through the daemon socket and a CLI over it. It is
orchestration: cold, framed, versioned. It MUST NOT put JSON, cells, or extra
RPCs on the warm path (ADR 0011 D6).

The socket MUST be user-scoped with restrictive permissions and MUST refuse
connections from another uid. Every mutating verb MUST be explicit; there is no
"eval" verb and no shell passthrough that bypasses D2's trust.

The CLI is how tests drive the app, which is why it must not become a second,
weaker permission model.

### D8 — Accessibility is a keyboard-complete path with real labels

F-229. Every action reachable by mouse MUST be reachable by keyboard, including
pane focus, tab and workspace switching, palette, pickers, and every
confirmation dialog in this ADR. A confirmation that can only be accepted by
mouse is a broken security control, not only a broken a11y story.

Chrome elements MUST carry accessibility labels and identifiers — SPEC-CHROME §1
already requires `chrome-split` / `chrome-left` / `chrome-center` /
`chrome-right`, and that convention extends to every surface. VoiceOver MUST be
able to name the focused pane and its state.

The terminal grid MUST expose its text to assistive technology without copying
per-cell `String`s on the warm path: the accessibility read is cold, on demand,
from the same POD buffer (FR-CHIP0), never a per-frame mirror.

Mutation `drop_a11y_labels` MUST turn T-TRUST-A11Y red.

### D9 — Oracle

| ID | Closes |
|---|---|
| T-TRUST-CONFIG | D2 — untrusted config inert; content change re-asks |
| T-TRUST-PLUGIN | D3 — out of process; no implicit capability; crash contained |
| T-TRUST-REDACT | D4 — no secret at a persisting sink; grid unmodified |
| T-TRUST-NOACCT | D5 — fully functional offline, no account |
| T-TRUST-UPDATE | D6 — unsigned refused; rollback works |
| T-TRUST-SOCKET | D7 — foreign uid refused; no eval verb |
| T-TRUST-A11Y | D8 — keyboard-complete; labels present; cold grid read |

## Consequences

- [SPEC-TRUST](../spec/SPEC-TRUST.md) is the contract for all seven surfaces.
- ADR 0021 D5 (deep links) and ADR 0024 D2 (browser) are instances of D1.
- ADR 0031's agent permission profiles inherit D1 and D3 rather than inventing a
  second grant model.
- No plugin runtime and no marketplace ship in M2.

## Rejected alternatives

- **In-process plugins for speed.** Rejected: D3. A plugin crash that takes the
  window is a worse outcome than any latency it saves.
- **Trust a directory permanently.** Rejected: D2. `git pull` becomes an
  execution vector.
- **Redact at the source, on the grid.** Rejected: D4, NFR-BYTES.
- **An account for "better defaults".** Rejected: D5.
- **A generic `exec` verb on the socket for convenience.** Rejected: D7 — it
  routes around every control in this ADR.
- **A per-frame accessibility mirror of the grid.** Rejected: D8, ADR 0001.
