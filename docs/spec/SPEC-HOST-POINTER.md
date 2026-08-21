# SPEC-HOST-POINTER — host encodes mouse for Chip 1 (`lane:host`)

- **Status:** Accepted — 2026-08-21. Gate T-MOUSE-SGR **Red** until
  demonstrated red-then-green (ADR 0002 D2).
- **Authority:** [ADR 0055](../adr/0055-mockup-is-destination-mouse-first.md),
  [ADR 0036](../adr/0036-chip1-mode-state-channel.md),
  [SPEC-FIDELITY](SPEC-FIDELITY.md) §3, [SPEC-VT-MODE](SPEC-VT-MODE.md)
- **Issue:** [#344](https://github.com/mahboobmonnamd/RILL/issues/344)
- **Code:** `crates/rill-host` (`encode_pointer`), `host/macos/TerminalView`
- **Does not specify:** Flow Blocks, selection highlights, OSC 8, extra tabs

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. Who encodes

Chip 1 tracks mouse private modes. The host MUST encode pointer events from
`Client::mode_state()` after `feed`. The host MUST NOT parse CSI in ObjC.

## 2. When to encode

Encode when any of `mouse_x10`, `mouse_button`, `mouse_any`, `mouse_sgr` is
true, unless Shift is held (SPEC-FIDELITY §3 reclaim). Otherwise MUST NOT
write pointer CSI.

## 3. Encoding

- Columns and rows are **1-based** grid cells from the live Chip 1 geometry
  (padding subtracted; row 0 is the top cell).
- If `mouse_sgr`: SGR `CSI < Cb ; Cx ; Cy M` (press) / `m` (release). Left
  button `Cb=0`. Wheel up `Cb=64`, down `Cb=65`, sent as press (`M`) only.
- Else: X10 `ESC [ M` + three bytes `(32+Cb)`, `(32+Cx)`, `(32+Cy)` with
  coordinates clamped to 1..=223.
- Payloads travel as ordinary attach `DATA` (`send_input`). MUST NOT add a
  frame tag.

## 4. Wheel

With reporting off, the wheel MUST keep host history scroll (T-SCROLL-OFFSCREEN).
With reporting on, the wheel MUST encode for the child and MUST NOT change
host `scroll_offset`.

## 5. Gate

**T-MOUSE-SGR** — `t_host_encodes_sgr_mouse_when_mode_is_on`. A `TerminalModeState`
with `mouse_sgr` produces SGR bytes for press at (1,1). Reporting off produces
empty. Mutation `skip_mouse_encode` MUST yield empty while `mouse_sgr` is on.

## 6. What we will not do

- Build Block hit-testing in this spec.
- Heuristic "is this vim" routing.
- Copy-on-click (ADR 0052).
