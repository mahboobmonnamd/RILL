# SPEC-VT-MODE — Chip 1 terminal mode state (`lane:chip1-vt-engine`, M7 prep)

- **Status:** Accepted — 2026-08-20. Gate **Red** until demonstrated red-then-green.
- **Authority:** [ADR 0036](../adr/0036-chip1-mode-state-channel.md)
- **Requires:** [SPEC-CHIP1](SPEC-CHIP1.md) §2, [SPEC-VT-SCREEN](SPEC-VT-SCREEN.md) §5
- **Issue:** T-CHIP1-MODE (M7 precondition 6)

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. Boundary

- Mode state is **input encoding**, not paint. It MUST NOT appear on
  `PodGrid` or grow `PodCell`.
- Chip 1 tracks modes consumed from the byte stream. The **host encodes**
  keys and mouse from `mode_state()` after `feed`. Chip 1 MUST NOT write a
  PTY or attach socket.
- Transport is an inherent poll on `VtEngine`, not a new attach frame tag,
  not JSON, not bytes mixed into `take_replies`.

## 2. `TerminalModeState`

`#[derive(Clone, Copy, Debug, PartialEq, Eq)]` in `rill-vt-types`. All
fields are `bool`. Defaults are `false` except `cursor_visible` which is
`true` on a fresh grid (DECTCEM on).

| Field | Set by |
|---|---|
| `application_cursor_keys` | `CSI ? 1 h` / `l` |
| `application_keypad` | `ESC =` / `ESC >` |
| `bracketed_paste` | `CSI ? 2004 h` / `l` |
| `mouse_x10` | `CSI ? 1000 h` / `l` |
| `mouse_button` | `CSI ? 1002 h` / `l` |
| `mouse_any` | `CSI ? 1003 h` / `l` |
| `mouse_sgr` | `CSI ? 1006 h` / `l` |
| `focus_events` | `CSI ? 1004 h` / `l` |
| `alternate_screen` | internal alt-screen flag (`?1047` / `?1049`) |
| `cursor_visible` | internal DECTCEM (`?25`) |

Multiple mouse mode flags MAY be true if the program set them; the host
chooses encoding precedence at M7. Chip 1 only records what was requested.

## 3. API

```rust
impl VtEngine {
    /// Current mode flags for the host encoder (ADR 0036 D2). Not on the trait.
    pub fn mode_state(&self) -> TerminalModeState;
}
```

- `mode_state` MUST NOT allocate.
- `reset()` MUST restore defaults (including `cursor_visible = true`).
- Unknown private modes MUST be ignored without panic.

## 4. Gate

**T-CHIP1-MODE** — after each sequence in §2, `mode_state()` matches the
expected field. Doc comment cites ADR 0036.

**Required mutation.** `RILL_MUTATE=ignore_mode_updates` — private modes and
`ESC =` / `ESC >` do not change `mode_state`. That MUST turn T-CHIP1-MODE red.

## 5. What we will not do

- Wire the host encoder in this slice.
- Link `vt-engine` into `rill-host` / `rilld`.
- Add attach frame tags or JSON.
- Put mode bits on `PodCell`.
