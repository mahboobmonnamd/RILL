//! Slice 7 T-CHIP1-GRAPHEME / T-CHIP1-WIDTH-DEFERRED.
//!
//! Authority: ADR 0023, SPEC-VT-SCREEN §9. Chip 0 stays live. Width is an M7
//! precondition, not a v0 feature.

use rill_vt_types::TerminalEmulation;
use std::path::PathBuf;
use vt_engine::VtEngine;

fn engine(cols: u16, rows: u16) -> VtEngine {
    VtEngine::new(cols, rows).expect("vt-engine")
}

/// T-CHIP1-GRAPHEME — long cluster does not overrun.
///
/// Required mutation: `RILL_MUTATE=fixed_grapheme_buf`.
#[test]
fn t_chip1_grapheme_long_cluster_does_not_overrun() {
    let mut vt = engine(80, 24);
    let mut s = String::from("e");
    for _ in 0..40 {
        s.push('\u{0301}');
    }
    vt.feed(s.as_bytes()).expect("feed combining cluster");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(grid.cols, 80, "snapshot survived a 41-codepoint cluster");
    assert_eq!(
        grid.cell(0, 0).expect("base").codepoint,
        u32::from(b'e'),
        "base codepoint must stay visible (not dropped, not overwritten)"
    );
    assert_eq!(
        grid.cursor_col, 1,
        "combining marks must append, not consume a cell each"
    );
    assert!(
        grid.grapheme_truncated >= 1,
        "cluster past RILL_GRAPHEME_MAX=32 must be counted, not silently dropped"
    );

    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/bytes/zwj_emoji.bin");
    assert!(
        path.is_file(),
        "{} is required (ADR 0002 D5)",
        path.display()
    );
    let mut vt = engine(80, 24);
    vt.feed(&std::fs::read(&path).expect("zwj fixture"))
        .expect("feed zwj");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(grid.cols, 80, "ZWJ sequence must not overrun the grid");
    assert_eq!(
        grid.cursor_col, 1,
        "printable after ZWJ appends to the cluster (SPEC-VT-SCREEN §9)"
    );
    assert_eq!(
        grid.cell(0, 0).expect("zwj base").codepoint,
        0x1f469,
        "ZWJ sequence keeps the first scalar as the visible base"
    );
}

/// T-CHIP1-WIDTH-DEFERRED — v0 advances one column per scalar.
///
/// Documents the v0 miss (ADR 0023 D1/D3): a conforming terminal advances 5
/// for `日本X`. When width lands, replace this gate with T-CHIP1-WIDTH
/// (cursor column 5); do not delete it quietly. Width is an M7 precondition.
///
/// Required mutation: `RILL_MUTATE=wide_advances_two`.
#[test]
fn t_chip1_width_deferred_one_column_per_scalar() {
    let mut vt = engine(80, 24);
    vt.feed("日本X".as_bytes()).expect("feed CJK");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(
        grid.cursor_col, 3,
        "v0 miss: 日本X is three scalars → column 3, not East Asian Width 5 (ADR 0023)"
    );
    assert_eq!(grid.cell(0, 0).expect("日").codepoint, '\u{65e5}' as u32);
    assert_eq!(grid.cell(1, 0).expect("本").codepoint, '\u{672c}' as u32);
    assert_eq!(grid.cell(2, 0).expect("X").codepoint, u32::from(b'X'));
}
