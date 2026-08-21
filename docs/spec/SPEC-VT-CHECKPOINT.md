# SPEC-VT-CHECKPOINT — Chip 1 compact checkpoint codec (`lane:chip1-vt-engine`)

- **Status:** Accepted for the isolated-crate contract — 2026-08-21
  ([#312](https://github.com/mahboobmonnamd/RILL/issues/312)). Named tests are
  **Red** until demonstrated red-then-green in `fast.yml`.
- **Authority:** [ADR 0002](../adr/0002-falsifiable-evidence.md),
  [ADR 0012](../adr/0012-chip1-isolated-vt.md),
  [ADR 0053](../adr/0053-runtime-domain-content-and-client-authority.md) D5–D6,
  [SPEC-CHIP1](SPEC-CHIP1.md) §2
- **Crate:** `crates/vt-engine`. Not `rill-host` / `rilld` / `rill-kernel` /
  `rill-attach`. Not Chip 0.
- **Gates:** T-CHIP1-CHECKPOINT-ROUNDTRIP, T-CHIP1-CHECKPOINT-HASH,
  T-CHIP1-CHECKPOINT-VERSION, T-CHIP1-CHECKPOINT-NOT-RESYNC

Normative keywords: MUST, MUST NOT, SHOULD, MAY.

## 1. Boundary

`repaint_bytes` and `resync_from_history` remain byte-replay helpers. They
MUST NOT be the product checkpoint contract.

A checkpoint is cold, versioned, compact binary state for one `VtEngine`.
The kernel later supplies a monotonic ending offset; the engine records it
and does not assign execution identity. Attach framing is out of this spec.

Checkpoints MUST NOT contain per-cell `String`, JSON, or a live grid dump
intended for the warm path.

## 2. Format version 1

Little-endian. Layout:

| Offset | Field |
|---|---|
| 0 | magic `R1CK` (4 bytes) |
| 4 | `version` `u16` — MUST be `1` |
| 6 | `ending_offset` `u64` (caller-supplied) |
| 14 | `hash` `u64` — FNV-1a of bytes `[0, 14)` concatenated with payload, with this field treated as zero during the hash |
| 22 | payload |

Payload is canonical screen state: cols, rows, cursor, pending wrap, autowrap,
scroll region, pen colours (identity, not materialised RGB), palette identity,
mode flags including alt screen, visible cells as identity+attrs, optional
saved primary grid and saved cursors.

v1 MUST refuse export while the parser is not in Ground with no pending UTF-8
(`Error::Vt`). An open grapheme cluster MUST be encoded so combining marks
that arrive after import still attach.

Import MUST replace engine state from the payload, reset the parser to Ground,
and clear the reply buffer (replies are not checkpoint authority).

Unknown `version` MUST return `Err` and MUST NOT decode payload. Truncated
input, bad magic, or a hash that does not match MUST return `Err`.

## 3. Hash

FNV-1a 64-bit (offset `0xcbf29ce484222325`, prime `0x100000001b3`) over the
canonical blob with the hash field zeroed. The stored hash MUST equal that
value. Identical canonical state and offset MUST yield the same hash. A
single cell or mode change MUST change the hash.

This hash detects mirror divergence in later slices. It is not a MAC.

## 4. API

```rust
impl VtEngine {
    pub fn export_checkpoint(&self, ending_offset: u64) -> Result<Vec<u8>, Error>;
    pub fn import_checkpoint(&mut self, bytes: &[u8]) -> Result<u64, Error>;
}
```

`import_checkpoint` returns the encoded `ending_offset`. It MUST NOT feed the
blob as a VT byte stream.

## 5. What we will not do

- Ghostty / `libghostty-vt` / `rill-chip0`
- Live swap, attach frames, kernel journals, host presenter
- A second crate (`rill-terminal-core`) in this slice
- Treating ED/resync emit as import
