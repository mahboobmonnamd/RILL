# SPEC-VT-LIVE-SWAP — Chip 1 on the warm path (M7)

- **Status:** Accepted for wiring. Implementation is [#24](https://github.com/mahboobmonnamd/RILL/issues/24).
- **Authority:** [ADR 0037](../adr/0037-chip1-live-swap.md). Checkpoint/mirror
  gates (T-CLIENT-MIRROR-*, T-CLIENT-RING-EVICTION-RESYNC) are already
  demonstrated in `crates/rill-attach/tests/t_client_mirror.rs`.
- **Does not authorize:** a feature-flagged half swap, a second live VT,
  Stage 3 content ledger, recutting T-NFR, dumping the grid into `Text`,
  linking Chip 1 before the named tests exist.

## 1. One live type

`rill-host::Client` and `rilld::Daemon` MUST use `vt_engine::VtEngine`.
Neither crate MUST depend on `rill-chip0`. That crate is deleted
([ADR 0054](../adr/0054-chip0-retired.md)). Look-file load lives in `rill-look`.

## 2. Replies

After each successful `feed` on the attach client, the host MUST
`take_replies()` and enqueue each drained sequence as `Frame::Data` toward the
PTY (same path as keystrokes). Daemon history `resync_from_history` MUST
discard replies (already the Chip 1 inherent method).

No new attach tag. No JSON.

## 3. Palette and modes

The host MUST `set_palette` from the resolved look-file colours after connect
and MUST NOT parse CSI for mouse/DECCKM/paste. After `feed` it MAY read
`mode_state()` for a later encoder; it MUST NOT grow a second VT.

## 4. Wide cells

The Metal presenter MUST skip `ATTR_WIDE_TAIL` (bit 4) when placing a glyph and
MUST treat `ATTR_WIDE_LEAD` as two columns for cursor probes. Mutation
`ignore_wide_bits` paints tails as independent glyphs.

## 5. Gates

- T-CLIENT-MIRROR-* / T-CLIENT-RING-EVICTION-RESYNC (library; already gated)
- T-VT-LIVE-REPLIES — DSR/DA from the child returns as PTY writes
- T-VT-LIVE-RESYNC — `rilld` resync matches `VtEngine::resync_from_history`
- Packaged T-NFR hid (same instrument), T-RESYNC, T-LOOK-ANSI, T-LOOK-CELL,
  T-SPLIT-LOOK ([#272](https://github.com/mahboobmonnamd/RILL/issues/272))

## 6. Lint

`no-host-dep-on-vt-engine` lifts in the swap PR. `rilld` MUST depend on
`vt-engine` and MUST NOT depend on `rill-chip0`. Zig remains for `rill-chip0`
measurement jobs only after host stops constructing Chip 0.
