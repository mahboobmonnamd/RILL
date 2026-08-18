# ADR 0025: One look schema, one local config file

- **Status:** Accepted — 2026-08-18
- **Tree:** this repository only
- **Issue:** catalog epic [#33](https://github.com/mahboobmonnamd/RILL/issues/33).
  Rows authorized by this ADR:
  F-210 [#221](https://github.com/mahboobmonnamd/RILL/issues/221), F-211
  [#222](https://github.com/mahboobmonnamd/RILL/issues/222), F-212
  [#223](https://github.com/mahboobmonnamd/RILL/issues/223), F-213
  [#224](https://github.com/mahboobmonnamd/RILL/issues/224), F-214
  [#225](https://github.com/mahboobmonnamd/RILL/issues/225), F-215
  [#226](https://github.com/mahboobmonnamd/RILL/issues/226), F-216
  [#227](https://github.com/mahboobmonnamd/RILL/issues/227), F-218
  [#229](https://github.com/mahboobmonnamd/RILL/issues/229), F-219
  [#230](https://github.com/mahboobmonnamd/RILL/issues/230), F-230
  [#241](https://github.com/mahboobmonnamd/RILL/issues/241).
- **Requires:** [ADR 0017](0017-ghostty-look-windowed-default.md) (look files,
  windowed default, unknown-key behaviour),
  [ADR 0018](0018-three-pane-host-chrome.md) D5 (chrome surface formula),
  [ADR 0022](0022-terminal-fidelity-is-chip0.md) D7 (Ghostty import)
- **Amends:** nothing. ADR 0017's resolution order stands and is generalized.
- **Does not authorize:** an account, a hosted settings service, a theme
  marketplace, plugins (ADR 0026 D3), `NSVisualEffectView` over Metal, per-cell
  RGB rewriting in the host, translucency on the T-NFR path.

## Context

Ten rows are configuration and appearance: canonical theme schema (F-210), OS
light/dark sync (F-211), pane dimming and focus-follows-mouse (F-212), opacity
and blur (F-213), custom dock icons (F-214), settings sync via cloud (F-215),
local settings file (F-216), custom keybindings (F-218), palette custom actions
(F-219), auto-update pill (F-230).

ADR 0017 already decided the hard part: look comes from a theme **file**, keys
overlay `host-surface.toml`, and unknown keys are reported rather than guessed.
`background-opacity` is already read and already MUST NOT make the window
translucent (ADR 0018 D5).

What is left is scope creep control. Configuration is where terminals acquire a
second source of truth: a GUI settings panel that writes a different file than
the one dotfiles track, a theme store that ships RGB tables, and a cloud sync
that becomes the reason an account exists.

## Decision

### D1 — One canonical schema; every importer targets it

F-210. There is exactly one configuration schema in this tree. Ghostty import
(F-098), theme files (ADR 0017), and any future importer are **adapters** that
produce values in that schema. There is no second internal representation.

The schema is versioned. An unknown key is reported and dropped, never guessed
(ADR 0017, T-LOOK-UNKNOWN). A schema change that would silently reinterpret an
existing key requires a version bump.

Colors resolve to the look file's values. The host MUST NOT compile a palette
into a binary — ADR 0018 D5 already rejected compiled Catppuccin tables and that
rejection generalizes: no vendored RGB constants, anywhere.

### D2 — The local file is the only source of truth

F-216. Configuration is a file in the user's dotfiles (`~/.config/rill/config`
or `RILL_CONFIG`, already ADR 0017's path). No account is required and none is
offered (F-224, ADR 0026 D5).

If a settings UI exists, it MUST write **that** file, in a form the user can
read, diff, and commit. It MUST NOT keep a parallel binary store, and it MUST
NOT silently rewrite or reformat keys it did not change.

Mutation `settings_ui_writes_shadow_store` MUST turn T-CFG-ONEFILE red.

### D3 — Resolution order is fixed and total

Highest wins:

1. command-line flags (`--nfr-key` and friends)
2. environment (`RILL_CONFIG`, `RILL_MUTATE`)
3. project-local trusted config (ADR 0026 D2) — only after trust
4. user config file
5. `host-surface.toml` shipped defaults

Every key resolves to a value; there is no "unset" at the bottom. Resolution
MUST be a cold, one-time computation per launch and per explicit reload. It MUST
NOT be consulted per frame or per key.

Mutation `resolve_config_per_frame` MUST turn T-CFG-COLD red.

### D4 — Appearance follows the file, and translucency stays refused

- **Light/dark sync (F-211):** the config MAY name a light look and a dark look.
  When it does, RILL follows `NSApp.effectiveAppearance`. A theme switch MUST
  re-resolve cold and MUST NOT drop frames or detach; it MUST NOT re-cut T-NFR.
- **Dimming and focus-follows-mouse (F-212):** dimming an inactive pane is a
  paint-time constant in the existing shader path. It MUST NOT introduce a
  second render pass over the terminal surface, and MUST NOT dim the focused
  pane during measurement. Focus-follows-mouse is opt-in, default off, and MUST
  NOT change first responder while a modifier drag is in progress.
- **Opacity and blur (F-213):** `background-opacity` tints the **background
  color** at paint. It MUST NOT set `NSWindow.alphaValue` and MUST NOT install
  `NSVisualEffectView` behind the Metal layer (ADR 0017, ADR 0018 D5). The
  T-NFR path stays opaque (ADR 0007). Blur, if ever offered, is refused on the
  measured path by construction, not by a runtime check.
- **Dock icons (F-214):** an icon set is a file reference in config. Icons are
  images, not code, and MUST NOT be executable or scripted.

### D5 — Keybindings are data, resolve at bind time, and cannot capture the shell

F-218. Bindings live in the config file. Two-step chords are allowed.

A binding MUST NOT be able to shadow a key the focused leaf needs unless the
user wrote it deliberately: the resolver MUST detect and **report** a binding
that swallows a control character a raw-mode child would receive, at load time,
not at press time. Ctrl+C that silently stops reaching the child is the worst
bug a terminal can ship.

Resolution is a cold table built at load. Key handling MUST NOT walk a config
structure per event.

Mutation `binding_swallows_ctrl_c` MUST turn T-CFG-BIND red.

### D6 — Palette actions are declarative, confirmed, and scoped

F-219. Project commands in the palette (ADR 0021 D2) are **declarations** in
config: a name, a command, a target. Running one MUST show the resolved command
before it runs.

A project-local action requires trust first (ADR 0026 D2). An action MUST NOT
run at load, at index, or on selection — only on explicit invocation.

### D7 — Cloud settings sync is optional, off, and never a prerequisite

F-215. Sync, if it ever exists, is an **optional** layer above D2's file. The
local file MUST remain complete and authoritative with sync off. RILL MUST work
fully with no account (F-224).

Nothing in M2 implements sync. This ADR authorizes only the constraint, so that
no later feature is designed assuming a server. Secrets MUST NOT be synced at
all (ADR 0026 D4).

### D8 — Updates are signed, and the pill only reports

F-230. The auto-update pill surfaces an available update and nothing else. It
MUST NOT download-and-apply silently, MUST NOT restart the app while a leaf is
attached without confirmation, and MUST NOT be dismissible in a way that hides a
**security** update permanently.

Signature and notarization requirements are ADR 0026 D6; the pill MUST refuse to
offer an update that fails them.

### D9 — Oracle

| ID | Closes |
|---|---|
| T-CFG-ONEFILE | D2 — settings UI writes the user's file; no shadow store |
| T-CFG-COLD | D3 — resolution is cold; not per frame, not per key |
| T-CFG-ORDER | D3 — precedence, over both shipped themes |
| T-CFG-OPAQUE | D4 — no `alphaValue`, no blur layer, T-NFR opaque |
| T-CFG-BIND | D5 — swallowed control character is reported at load |
| T-CFG-UPDATE | D8 — unsigned update is refused |

T-LOOK-FILE, T-LOOK-UNKNOWN and T-SPLIT-LOOK (ADR 0017, ADR 0018) MUST stay
green throughout.

## Consequences

- [SPEC-CONFIG](../spec/SPEC-CONFIG.md) is the schema and resolution contract.
- ADR 0017's look resolution is a **subset** of D3's order, not a parallel path.
- F-215 ships nothing in M2 and is recorded as constraint-only.

## Rejected alternatives

- **A GUI settings store separate from the dotfile.** Rejected: D2. Two sources
  of truth, and the dotfile silently stops being real.
- **A theme marketplace with vendored palettes.** Rejected: D1, ADR 0018 D5.
- **`NSWindow.alphaValue` for opacity.** Rejected: D4, ADR 0017.
- **Per-event keybinding lookup through the config tree.** Rejected: D5, cost on
  the key path.
- **Sync-first settings with a local cache.** Rejected: D7 and F-224. The local
  file is the product; the server is not.
- **Silent auto-update.** Rejected: D8. Restarting a terminal that owns live
  sessions is not a background task.
