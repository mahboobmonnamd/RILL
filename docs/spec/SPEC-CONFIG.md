# SPEC-CONFIG — canonical schema, resolution and appearance (`lane:host`)

- **Status:** Accepted — 2026-08-18. Gates **Red** until demonstrated
  red-then-green (ADR 0002 D2).
- **Authority:** [ADR 0025](../adr/0025-one-look-schema-one-config-file.md)
- **Requires:** [ADR 0017](../adr/0017-ghostty-look-windowed-default.md),
  [SPEC-CHROME](SPEC-CHROME.md), [SPEC-DISPLAY](SPEC-DISPLAY.md)
- **Files:** `host-surface.toml`, `~/.config/rill/config`, `RILL_CONFIG`
- **Milestone:** M2 — Chrome

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. One schema

- There MUST be exactly one configuration schema. Every importer is an adapter
  producing values in it. There MUST NOT be a second internal representation.
- The schema is versioned. An unknown key MUST be reported and dropped, never
  guessed (T-LOOK-UNKNOWN).
- A change that would reinterpret an existing key requires a version bump.
- The host MUST NOT compile a palette into the binary. No vendored RGB
  constants (ADR 0018 D5).

## 2. One file

- Configuration lives in the user's dotfiles. No account is required or offered.
- A settings UI MUST write that file, readable and diffable. It MUST NOT keep a
  parallel binary store and MUST NOT rewrite keys it did not change.

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

- Cloud settings sync, if it exists, is optional and additive. The local file
  MUST remain complete and authoritative with sync off.
- Secrets MUST NOT be synced (SPEC-TRUST §4). Nothing ships in M2.

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
| T-CFG-ONEFILE | Red | §2 |
| T-CFG-COLD | Red | §3 |
| T-CFG-ORDER | Red | §3 |
| T-CFG-OPAQUE | Red | §4 |
| T-CFG-BIND | Red | §5 |
| T-CFG-UPDATE | Red | §8 |

T-LOOK-FILE, T-LOOK-UNKNOWN, T-SPLIT-LOOK MUST stay green.

## 10. What we will not do

- Keep a GUI settings store separate from the dotfile.
- Ship a theme marketplace with vendored palettes.
- Use `NSWindow.alphaValue` for opacity.
- Look up bindings per event through the config tree.
- Require an account for any feature.
