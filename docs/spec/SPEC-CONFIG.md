# SPEC-CONFIG — canonical schema, resolution and appearance (`lane:host`)

- **Status:** Accepted — 2026-08-18. `crates/rill-orchestrate/src/config.rs`
  implements the resolution *mechanism* (§3 precedence, cold single-read
  snapshots, §2 one-file writeback, §5 keybinding-swallow detection) as a
  pure library, independent of the look/theme schema itself. **T-CFG-ORDER,
  T-CFG-COLD, T-CFG-ONEFILE and T-CFG-BIND are Proven at the library
  level** — `cargo test -p rill-orchestrate --test config_gates`,
  red-then-green under `--features mutate` (evidence below). T-CFG-OPAQUE
  (needs `NSWindow`/Metal) and T-CFG-UPDATE (needs a signed-binary updater)
  are host/platform work, not attempted here, and stay **Red**.
- **Authority:** [ADR 0043](../adr/0043-one-look-schema-one-config-file.md),
  amended by
  [ADR 0053](../adr/0053-runtime-domain-content-and-client-authority.md) D14–D15
- **Requires:** [ADR 0017](../adr/0017-ghostty-look-windowed-default.md),
  [SPEC-CHROME](SPEC-CHROME.md), [SPEC-DISPLAY](SPEC-DISPLAY.md)
- **Files:** `host-surface.toml`, `~/.config/rill/config`, `RILL_CONFIG`
- **Milestone:** M2 — Chrome

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. One versioned TOML schema

- There MUST be exactly one configuration schema. Every importer is an adapter
  producing values in it. There MUST NOT be a second internal representation.
- The canonical serialization is TOML with an explicit schema version. It
  covers app and terminal themes, fonts, font sizes, keybindings, rendering,
  line height, cursor, shell selection, window/pane and Workspace/Session
  behavior, Flow/Raw preferences, attention, inspector, notifications, remote
  connections, privacy, logging, retention and export settings. Platform
  adapters MAY expose only supported fields but MUST NOT invent a parallel
  schema.
- The schema is versioned. An unknown key MUST be reported and dropped, never
  guessed (T-LOOK-UNKNOWN).
- A change that would reinterpret an existing key requires a version bump.
- The host MUST NOT compile a palette into the binary. No vendored RGB
  constants (ADR 0018 D5).

## 2. One file

- Configuration lives in the user's dotfiles. No account is required or offered.
- A settings UI MUST write that file, readable and diffable. It MUST NOT keep a
  parallel binary store and MUST NOT rewrite keys it did not change.
- Credentials, private keys, tokens, secret values, device authentication
  material and host credentials MUST NOT be represented in TOML. Opaque
  references to a separately governed platform credential store are allowed.

## 3. Resolution order

Highest wins:

1. command-line flags
2. environment (`RILL_CONFIG`, `RILL_MUTATE`)
3. project-local trusted config, only after trust (SPEC-TRUST §2)
4. user config file
5. `host-surface.toml` shipped defaults

- Every key MUST resolve to a value.
- Resolution MUST be cold: once per launch and per explicit reload. It MUST NOT
  be consulted per frame or per key.
- ADR 0017's look resolution is a subset of this order, not a parallel path.

## 4. Appearance

- The config MAY name a light look and a dark look; when it does, RILL follows
  `NSApp.effectiveAppearance`. A theme switch MUST re-resolve cold, MUST NOT
  drop frames or detach, and MUST NOT re-cut T-NFR.
- Pane dimming MUST be a paint-time constant in the existing shader path. It
  MUST NOT add a second render pass and MUST NOT dim the focused pane during
  measurement.
- Focus-follows-mouse MUST be opt-in, default off, and MUST NOT change first
  responder during a modifier drag.
- `background-opacity` tints the background **color** at paint. It MUST NOT set
  `NSWindow.alphaValue` and MUST NOT install `NSVisualEffectView` behind the
  Metal layer. The T-NFR path stays opaque (ADR 0007).
- Dock icon sets are file references. Icons MUST NOT be executable or scripted.
- A named theme MUST resolve coherent application chrome, terminal palette,
  ContentTimeline, editor, diff, control and accessibility/contrast tokens.
  Role-specific derived tokens are allowed; an unrelated fallback palette or
  surface-local theme identity is not.

## 5. Keybindings

- Bindings are data in the config file. Two-step chords are allowed.
- The resolver MUST detect and **report at load time** a binding that swallows a
  control character a raw-mode child would receive.
- Key handling MUST use a cold table built at load. It MUST NOT walk a config
  structure per event.

## 6. Palette actions

- Project commands are declarations: name, command, target.
- Invoking one MUST show the resolved command first.
- A project-local action requires trust (SPEC-TRUST §2).
- An action MUST NOT run at load, at index, or on selection.

## 7. Sync

- Export and backup serialize the validated canonical model and MUST NOT include
  credential-store values, secrets, host credentials or device authentication
  material.
- Sync, if it exists, is optional, opt-in and additive. The local file MUST
  remain complete and authoritative with sync off. Only explicitly allowlisted
  non-secret schema fields may sync.
- Backup and sync inherit SPEC-TRUST privacy, encryption, isolation, retention
  and deletion requirements. Nothing ships in M2.

## 7a. Validation and migration

- Parse and schema validation complete before activation. Invalid input keeps
  the last valid configuration active and reports exact field diagnostics.
- A version migration MUST be explicit, previewable and atomic. It creates a
  recoverable pre-migration backup and verifies the migrated file before
  replacing the prior file.
- Failed validation or migration MUST NOT partially apply settings, erase
  unknown user text or expose secrets in diagnostics.

## 8. Updates

- The update pill reports availability only. It MUST NOT download-and-apply
  silently and MUST NOT restart the app while a leaf is attached without
  confirmation.
- It MUST refuse to offer an update failing signature or notarization
  (SPEC-TRUST §6).
- A security update MUST NOT be permanently dismissible.

## 9. Gates

| ID | Status | Closes |
|---|---|---|
| T-CFG-ONEFILE | **Proven** (library) | §2 |
| T-CFG-COLD | **Proven** (library) | §3 |
| T-CFG-ORDER | **Proven** (library) | §3 |
| T-CFG-OPAQUE | Red (host, not attempted) | §4 |
| T-CFG-BIND | **Proven** (library) | §5 |
| T-CFG-SCHEMA-COVERAGE | Red | §1 |
| T-CFG-THEME-CONSISTENCY | Red | §4 |
| T-CFG-MIGRATE | Red | §7a |
| T-CFG-PORTABLE-SECRETS | Red | §§2, 7 |
| T-CFG-UPDATE | Red (host, not attempted) | §8 |

**Library evidence (2026-08-18).** `crates/rill-orchestrate/tests/config_gates.rs`,
`cargo test -p rill-orchestrate --test config_gates` (green), each required
mutation run individually under `--features mutate` and confirmed to turn
only its own test red: `resolution_order_reversed`, `resolve_reads_per_query`,
`settings_write_shadow_store`, `skip_swallow_check`.

T-LOOK-FILE, T-LOOK-UNKNOWN, T-SPLIT-LOOK MUST stay green.

## 10. What we will not do

- Keep a GUI settings store separate from the dotfile.
- Ship a theme marketplace with vendored palettes.
- Use `NSWindow.alphaValue` for opacity.
- Look up bindings per event through the config tree.
- Require an account for any feature.
- Store credentials or secret values in TOML, exports, backups or sync.
- Partially apply an invalid or failed migration.
