//! Slice 11 T-CHIP1-CHECKPOINT-* (#312).
//!
//! Authority: SPEC-VT-CHECKPOINT. Oracle is a second instance's snapshot and
//! mode_state, plus the stored hash field — never a copy of the export buffer
//! as the expected grid (ADR 0002 D4).

use rill_vt_types::{PodGrid, TerminalEmulation};
use vt_engine::VtEngine;

fn engine() -> VtEngine {
    VtEngine::new(40, 8).expect("vt-engine")
}

fn row0(grid: &PodGrid) -> String {
    (0..grid.cols)
        .filter_map(|c| grid.cell(c, 0).and_then(|x| char::from_u32(x.codepoint)))
        .collect()
}

fn stored_hash(blob: &[u8]) -> u64 {
    assert!(blob.len() >= 22, "checkpoint too short to carry a hash");
    let mut raw = [0u8; 8];
    raw.copy_from_slice(&blob[14..22]);
    u64::from_le_bytes(raw)
}

fn primed(vt: &mut VtEngine) {
    vt.feed(b"\x1b[1;32mCKPT-MARK\x1b[m").expect("marker");
    vt.feed(b"\x1b[?1049h\x1b[?1hALT").expect("alt and DECCKM");
}

/// T-CHIP1-CHECKPOINT-ROUNDTRIP — import restores grid and modes.
///
/// Required mutation: `RILL_MUTATE=empty_checkpoint`.
#[test]
fn t_chip1_checkpoint_roundtrip_restores_grid_and_modes() {
    let mut source = engine();
    primed(&mut source);
    let before = source.snapshot().expect("source snapshot");
    let modes = source.mode_state();
    let blob = source.export_checkpoint(42).expect("export");

    let mut dest = engine();
    dest.feed(b"NOISE").expect("dest noise");
    let offset = dest.import_checkpoint(&blob).expect("import");
    let after = dest.snapshot().expect("dest snapshot");

    assert_eq!(offset, 42);
    assert_eq!(row0(&before), row0(&after));
    assert!(
        row0(&after).contains("ALT"),
        "second instance must show alt-screen text, not a copied buffer"
    );
    assert_eq!(after.cursor_col, before.cursor_col);
    assert_eq!(after.cursor_row, before.cursor_row);
    assert_eq!(dest.mode_state(), modes);
    assert!(dest.mode_state().alternate_screen);
    assert!(dest.mode_state().application_cursor_keys);
}

/// T-CHIP1-CHECKPOINT-HASH — a cell or mode flip changes the hash.
///
/// Required mutation: `RILL_MUTATE=constant_hash`.
#[test]
fn t_chip1_checkpoint_hash_changes_when_cell_flips() {
    let mut vt = engine();
    primed(&mut vt);
    let a = vt.export_checkpoint(7).expect("export a");
    let b = vt.export_checkpoint(7).expect("export b");
    assert_eq!(
        stored_hash(&a),
        stored_hash(&b),
        "identical state, same hash"
    );
    vt.feed(b"Z").expect("flip");
    let c = vt.export_checkpoint(7).expect("export c");
    assert_ne!(
        stored_hash(&a),
        stored_hash(&c),
        "one extra printable must change the stored hash"
    );
}

/// T-CHIP1-CHECKPOINT-VERSION — unknown version fails closed.
///
/// Required mutation: `RILL_MUTATE=accept_unknown_version`.
#[test]
fn t_chip1_checkpoint_unknown_version_fails_closed() {
    let mut source = engine();
    primed(&mut source);
    let mut blob = source.export_checkpoint(1).expect("export");
    blob[4] = 99;
    blob[5] = 0;

    let mut dest = engine();
    dest.feed(b"KEEP-ME").expect("pre-import");
    let before = dest.snapshot().expect("before");
    assert!(
        dest.import_checkpoint(&blob).is_err(),
        "version 99 must not decode"
    );
    let after = dest.snapshot().expect("after");
    assert_eq!(
        row0(&before),
        row0(&after),
        "failed import must leave the destination grid"
    );
    assert!(row0(&after).contains("KEEP-ME"));
}

/// T-CHIP1-CHECKPOINT-NOT-RESYNC — import is not VT replay.
///
/// Required mutation: `RILL_MUTATE=import_is_resync_bytes`.
#[test]
fn t_chip1_checkpoint_import_is_not_vt_replay() {
    let mut source = engine();
    primed(&mut source);
    let blob = source.export_checkpoint(9).expect("export");

    let mut dest = engine();
    dest.import_checkpoint(&blob).expect("import codec");
    let restored = dest.snapshot().expect("restored");
    assert!(
        row0(&restored).contains("ALT"),
        "codec import must restore source cells"
    );

    let mut as_vt = engine();
    as_vt.feed(&blob).expect("feed blob as VT");
    let replayed = as_vt.snapshot().expect("replayed");
    assert!(
        !row0(&replayed).contains("ALT"),
        "feeding checkpoint bytes as VT must not reconstruct the grid"
    );
}
