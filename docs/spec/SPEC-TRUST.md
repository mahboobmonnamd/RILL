# SPEC-TRUST — trust, secrets, updates, automation, accessibility (`lane:host`)

- **Status:** Accepted — 2026-08-18. `crates/rill-orchestrate/src/trust.rs`
  implements §2 (path+content-hash trust) and §4 (redact-at-sink). **T-TRUST-CONFIG
  and T-TRUST-REDACT are Proven at the library level** — `cargo test -p
  rill-orchestrate --test trust_gates`, red-then-green under `--features
  mutate` (evidence below). §3 plugins, §6 updates, §7 the daemon socket, and
  §8 accessibility all need a running daemon or a window and are not
  attempted here; those gates stay **Red**.
- **Authority:** [ADR 0044](../adr/0044-trust-secrets-and-automation-boundary.md),
  amended by
  [ADR 0053](../adr/0053-runtime-domain-content-and-client-authority.md) D4,
  D8 and D11
- **Requires:** [SPEC-CONFIG](SPEC-CONFIG.md), [SPEC-NAV](SPEC-NAV.md),
  [SPEC-KERNEL](SPEC-KERNEL.md)
- **Crates:** `crates/rilld`, `crates/rill-host`, `host/macos/`
- **Milestone:** M2 — Chrome

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. Untrusted by default

- Project files, deep links, page content, agent output, remote hosts and plugin
  manifests are **data** until the user grants authority.
- Grants MUST be explicit, scoped and revocable.
- Untrusted input MUST NOT execute a command, install a binding, open a tunnel,
  change settings, or reach a leaf without a confirmation naming the concrete
  action.
- Unparseable manifests, unknown verbs, missing signatures and ambiguous scopes
  MUST refuse (PRD NFR-FAIL).

## 2. Project-local config

- A repository's actions file MUST NOT be read for execution until the user
  trusts that path.
- Trust binds the resolved real path **and** a content hash. On change, trust
  MUST be re-confirmed with the diff shown.
- Trust MUST NOT be granted for "this folder and whatever it later contains".
- Trusted config MAY contribute palette actions and env. It MUST NOT contribute
  bindings that shadow control characters, disable redaction, grant plugin
  capabilities, or raise its own trust level.

## 3. Plugins

- A plugin MUST run out of process with a manifest declaring requested
  capabilities, and holds nothing implicitly.
- A plugin MUST NOT load in the host address space, receive the master fd
  (FR-SOLE), write to a leaf without a granted capability, read config outside
  its namespace, or observe another plugin.
- A crashing or hanging plugin MUST NOT stall the display link or take the
  window.
- Marketplace is discovery only. Install MUST show requested capabilities and
  confirm. A new version MUST re-ask rather than inherit a grant.
- No plugin runtime ships in M2.

## 4. Capture, retention and redaction

- Capturing terminal output, history, transcript, conversations or task content
  requires an applicable retention policy. Durable capture MAY be disabled
  entirely. Local encryption and available redaction do not authorize capture
  and MUST NOT be described as making corporate output automatically safe.
- Service diagnostics, crash reports and telemetry MUST NOT receive terminal or
  secret content.
- Clipboard, export, share, agent context and other transmission are derived
  sinks. Redaction MUST run at those sinks and report that detection is
  incomplete. A sink without its required redaction pass receives no content.
- Canonical policy-authorized source is not silently rewritten by redaction.
  Destructive deletion is a separate retention operation.
- Redaction MUST NOT be applied to the live terminal surface (NFR-BYTES).
- Trusted project config MUST NOT weaken redaction.

The existing T-TRUST-REDACT library gate proves a derived export transform only.
It does not prove capture authorization, complete secret detection, durable
storage, deletion or transmission policy.

## 5. No account

- RILL MUST be fully functional with no account, no network and no sign-in.
- No feature MAY exist only behind an account in this tree.
- First run MUST NOT ask for an account or require network access to reach a
  working terminal.
- Optional services (settings sync, cloud agents, permalinks) are additive.

## 6. Updates

- Builds MUST be signed and notarized.
- The updater MUST verify signature and version before applying and MUST refuse
  on failure. Rollback MUST be supported.
- An update MUST NOT terminate a healthy TerminalExecution. It proceeds with
  live workers only under the version/checkpoint compatibility matrix in
  SPEC-RUNTIME-SUPERVISION; otherwise it is deferred with blockers named.
- Downgrade to unsigned or older builds MUST be refused unless the user
  explicitly rolls back.

## 7. Socket and CLI

- Automation goes through the daemon socket: cold, framed, versioned.
- It MUST NOT put JSON, cells, or extra RPCs on the warm path.
- The socket MUST be in a protected user-owned runtime directory with
  restrictive permissions and MUST verify/refuse a foreign uid before frames.
- Every mutating verb MUST be explicit. There MUST NOT be an `exec` or `eval`
  verb, or a shell passthrough bypassing §2.

## 8. Accessibility

- Every mouse-reachable action MUST be keyboard-reachable, including pane focus,
  tab and workspace switching, palette, pickers, and **every confirmation
  dialog in this spec**.
- Chrome elements MUST carry accessibility labels and identifiers, extending
  SPEC-CHROME §1's convention.
- VoiceOver MUST be able to name the focused pane and its state.
- Live terminal accessibility uses a cold projection of terminal state;
  structured content uses virtualized semantic accessibility nodes. There MUST
  NOT be a per-frame String mirror or a full offscreen timeline rebuild.

## 9. Gates

| ID | Status | Closes |
|---|---|---|
| T-TRUST-CONFIG | **Proven** (library) | §2 |
| T-TRUST-PLUGIN | Red (not attempted) | §3 |
| T-TRUST-REDACT | **Proven** (derived-transform library sub-gate) | §4 |
| T-TRUST-CAPTURE-POLICY | Red | §4 capture/disable/delete authority |
| T-TRUST-NOACCT | Red (not attempted) | §5 |
| T-TRUST-UPDATE | Red (not attempted) | §6 |
| T-TRUST-SOCKET | Red (not attempted) | §7 |
| T-TRUST-A11Y | Red (not attempted) | §8 |

**Library evidence (2026-08-18).** `crates/rill-orchestrate/tests/trust_gates.rs`,
green, with `trust_by_path_only` and `skip_redaction_on_export` each
confirmed to turn only their own test red under `--features mutate`.

T-TRUST-NOACCT MUST be executed with the network denied.

## 10. What we will not do

- Load plugins in process.
- Trust a directory permanently across content changes.
- Redact by rewriting the grid.
- Ship a generic `exec` verb on the socket.
- Mirror the grid per frame for accessibility.
