# SPEC-VT-LIVE-SWAP — M7 warm-path wiring (`lane:host` + `lane:chip1-vt-engine`)

- **Status:** Accepted — 2026-08-20. Gates **Red** until demonstrated red-then-green.
- **Authority:** [ADR 0037](../adr/0037-chip1-live-swap.md)
- **Issue:** [#24](https://github.com/mahboobmonnamd/RILL/issues/24)
- **Requires:** [SPEC-CHIP1](SPEC-CHIP1.md), [SPEC-VT-REPLY](SPEC-VT-REPLY.md),
  [SPEC-VT-MODE](SPEC-VT-MODE.md), [SPEC-ATTACH](SPEC-ATTACH.md) §8

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. Boundary

- `vt_engine::VtEngine` replaces `Chip0` in `rill-host::Client` and
  `rilld::Daemon` in **one** PR. No feature flag. `rill-chip0` MUST NOT remain
  in either manifest (ADR 0037 D1).
- Look file parsing stays in `rill-look`. The host calls `set_palette` after
  resolve (ADR 0037 D4).
- Warm path frames remain `DATA` and `CREDIT` only toward the attach client
  (ADR 0003 D9).

## 2. Attach client (`rill-host`)

After each successful `feed` on inbound `DATA`:

1. `take_replies()` until empty; each non-empty drain MUST become outbound
   `Frame::Data` toward the PTY (ADR 0037 D2).
2. Read `mode_state()` for the key/mouse encoder (ADR 0037 D5). The encoder
   MUST NOT parse escape sequences.

`pump()` order: read/decode → `feed` → drain replies → invalidate snapshot
cache → replenish credit for bytes consumed.

`resize` MUST call `VtEngine::resize` then send attach `Resize`.

Connect MUST call `set_palette` when `HostSurface.colors` is present.

## 3. Daemon resync (`rilld`)

`maybe_resync` MUST call `VtEngine::resync_from_history`. Replies produced
during replay MUST be discarded (ADR 0037 D2). Emitted bytes are attach `DATA`
unchanged.

## 4. Metal presenter

`TerminalView` MUST skip atlas instances for cells with `attrs` bit4
(wide-tail). Cursor probes MUST advance two columns when the cursor sits on a
wide lead (ADR 0037 D6, `ATTR_WIDE_TAIL` / `ATTR_WIDE_LEAD` in
`rill-vt-types`).

## 5. Lints

The swap PR MUST remove `no-host-dep-on-vt-engine` and add
`no-chip0-on-warm-path`: `rill-host` and `rilld` MUST NOT depend on
`rill-chip0` (ADR 0037 D7).

## 6. Gates

| ID | Oracle |
|---|---|
| **T-LIVE-REPLY** | After `feed` of `CSI 5 ; 3 H` + `CSI 6 n`, the attach client sends `CSI 5 ; 3 R` as outbound `DATA` |
| **T-LIVE-MODE** | After `feed` of `CSI ? 1 h`, `mode_state().application_cursor_keys` is true |
| **T-LIVE-WIDE** | Snapshot with a wide lead at `(0,0)` yields one Metal instance at col 0, none at col 1 (tail) |
| **T-RESYNC** | Existing daemon gate — green with Chip 1 resync engine |
| **T-LOOK-*** | Existing packaged look gates — unchanged fixtures |
| **T-NFR** | Existing packaged hid instrument — re-run on battery after swap |

**Required mutations.**

- `RILL_MUTATE=skip_reply_drain` — `take_replies` bytes are not sent. MUST turn
  T-LIVE-REPLY red.
- `RILL_MUTATE=skip_mode_poll` — `mode_state` is not updated after `feed`. MUST
  turn T-LIVE-MODE red.
- `RILL_MUTATE=draw_wide_tail` — wide-tail cells get glyph instances. MUST turn
  T-LIVE-WIDE red.

## 7. What we will not do

- Link Chip 0 on the warm path after this lands.
- Add attach frame tags or JSON for replies or modes.
- Recut T-NFR or look fixtures.
