//! Slice 7 T-CHIP1-GRAPHEME / T-CHIP1-WIDTH.
//!
//! Authority: ADR 0035 (amends ADR 0023), SPEC-VT-SCREEN §9. Chip 0 stays live.
//! Width is an M7 precondition; this crate is not linked into the host.
//!
//! Observed red 2026-08-19 on Chip 1 v0 (one column per scalar, no lead/tail
//! bits) before the width implementation:
//!
//! ```text
//! cargo test -p vt-engine --test t_chip1_slice7 -- --nocapture
//! assertion `left == right` failed: 日本X must advance 5 columns (ADR 0035)
//!   left: 3
//!   right: 5
//! assertion `left == right` failed: ZWJ family occupies 2 columns (ADR 0035 D1)
//!   left: 1
//!   right: 2
//! assertion `left == right` failed: 日 must wrap to row 1, not split onto row 0 col 9
//!   left: 26085
//!   right: 32
//! ```
//!
//! Smash red 2026-08-19 before `smash_wide_at` used the current pen background
//! and DCH smashed the pair (ADR 0035):
//!
//! ```text
//! col 1 must take the current background (SGR 44), not the old cell bg
//!   left: Default
//!  right: Indexed(4)
//! orphan tail at col 0 row 0 (ADR 0035)
//! ```

use rill_vt_types::{Color, TerminalEmulation};
use std::path::PathBuf;
use vt_engine::VtEngine;

const ATTR_WIDE_LEAD: u16 = 1 << 3;
const ATTR_WIDE_TAIL: u16 = 1 << 4;

fn engine(cols: u16, rows: u16) -> VtEngine {
    VtEngine::new(cols, rows).expect("vt-engine")
}

fn zwj_fixture() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/bytes/zwj_emoji.bin");
    assert!(
        path.is_file(),
        "{} is required (ADR 0002 D5)",
        path.display()
    );
    std::fs::read(&path).expect("zwj fixture")
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

    let mut vt = engine(80, 24);
    vt.feed(&zwj_fixture()).expect("feed zwj");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(grid.cols, 80, "ZWJ sequence must not overrun the grid");
    assert_eq!(
        grid.cursor_col, 2,
        "ZWJ family occupies 2 columns (ADR 0035 D1)"
    );
    assert_eq!(
        grid.cell(0, 0).expect("zwj base").codepoint,
        0x1f469,
        "ZWJ sequence keeps the first scalar as the visible base"
    );
}

/// T-CHIP1-WIDTH — `日本X` advances five columns with lead/tail cells.
///
/// Replaces T-CHIP1-WIDTH-DEFERRED (ADR 0023 D3); do not delete that history
/// quietly. Authority: ADR 0035 D7. Chip 0 stays the live chip.
///
/// Required mutation: `RILL_MUTATE=narrow_cjk`.
#[test]
fn t_chip1_width_nihon_x_advances_five_columns() {
    let mut vt = engine(80, 24);
    vt.feed("日本X".as_bytes()).expect("feed CJK");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(
        grid.cursor_col, 5,
        "日本X must advance 5 columns (ADR 0035)"
    );

    let lead_hi = grid.cell(0, 0).expect("日 lead");
    assert_eq!(lead_hi.codepoint, '\u{65e5}' as u32, "col 0 is 日");
    assert_ne!(lead_hi.attrs & ATTR_WIDE_LEAD, 0, "日 lead sets attrs bit3");
    assert_eq!(
        lead_hi.attrs & ATTR_WIDE_TAIL,
        0,
        "lead must not also be tail"
    );

    let tail_hi = grid.cell(1, 0).expect("日 tail");
    assert_ne!(
        tail_hi.codepoint, 0,
        "tail MUST NOT be codepoint 0 (host paints 0 as space)"
    );
    assert_eq!(
        tail_hi.codepoint, '\u{65e5}' as u32,
        "tail stores the same base scalar as the lead (ADR 0035 D5)"
    );
    assert_ne!(tail_hi.attrs & ATTR_WIDE_TAIL, 0, "日 tail sets attrs bit4");
    assert_eq!(
        tail_hi.attrs & ATTR_WIDE_LEAD,
        0,
        "tail must not also be lead"
    );

    let lead_hon = grid.cell(2, 0).expect("本 lead");
    assert_eq!(lead_hon.codepoint, '\u{672c}' as u32, "col 2 is 本");
    assert_ne!(lead_hon.attrs & ATTR_WIDE_LEAD, 0, "本 lead sets bit3");

    let tail_hon = grid.cell(3, 0).expect("本 tail");
    assert_ne!(tail_hon.codepoint, 0, "本 tail MUST NOT be 0");
    assert_eq!(tail_hon.codepoint, '\u{672c}' as u32);
    assert_ne!(tail_hon.attrs & ATTR_WIDE_TAIL, 0, "本 tail sets bit4");

    let x = grid.cell(4, 0).expect("X");
    assert_eq!(x.codepoint, u32::from(b'X'), "col 4 is X");
    assert_eq!(
        x.attrs & (ATTR_WIDE_LEAD | ATTR_WIDE_TAIL),
        0,
        "ASCII X is neither lead nor tail"
    );
    // Secondary oracle only (ADR 0035 D3): not the gate. Primary is cursor
    // and lead/tail cells above.
    assert_eq!(
        unicode_width::UnicodeWidthStr::width("日本X"),
        5,
        "unicode-width agrees on this CJK fixture; Ambiguous divergence is named"
    );
}

/// ZWJ family width is 2, not the sum of scalar widths (SPIKE-WIDTH Result 2).
///
/// Primary oracle: cursor and lead/tail cells. Fail if the fixture is absent.
#[test]
fn t_chip1_width_zwj_family_occupies_two_columns() {
    let mut vt = engine(80, 24);
    vt.feed(&zwj_fixture()).expect("feed zwj");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(
        grid.cursor_col, 2,
        "ZWJ sequence is one cluster of width 2, not 1 per scalar"
    );
    let lead = grid.cell(0, 0).expect("zwj lead");
    assert_eq!(lead.codepoint, 0x1f469);
    assert_ne!(lead.attrs & ATTR_WIDE_LEAD, 0);
    let tail = grid.cell(1, 0).expect("zwj tail");
    assert_ne!(tail.codepoint, 0, "ZWJ tail MUST NOT be 0");
    assert_ne!(tail.attrs & ATTR_WIDE_TAIL, 0);
    assert_eq!(
        grid.cell(2, 0).expect("after cluster").codepoint,
        32,
        "women after ZWJ must append, not occupy further columns"
    );
}

/// A wide cluster must wrap rather than split across rows (ADR 0035 D6).
#[test]
fn t_chip1_width_wide_glyph_wraps_instead_of_splitting() {
    let mut vt = engine(10, 6);
    vt.feed("012345678日".as_bytes()).expect("feed wrap");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(
        grid.cell(8, 0).expect("8").codepoint,
        u32::from(b'8'),
        "col 8 stays ASCII 8"
    );
    assert_eq!(
        grid.cell(9, 0).expect("last").codepoint,
        32,
        "日 must wrap to row 1, not split onto row 0 col 9"
    );
    let lead = grid.cell(0, 1).expect("wrapped lead");
    assert_eq!(lead.codepoint, '\u{65e5}' as u32);
    assert_ne!(lead.attrs & ATTR_WIDE_LEAD, 0);
    let tail = grid.cell(1, 1).expect("wrapped tail");
    assert_ne!(tail.codepoint, 0);
    assert_ne!(tail.attrs & ATTR_WIDE_TAIL, 0);
    assert_eq!(grid.cursor_row, 1);
    assert_eq!(grid.cursor_col, 2);
}

/// Ambiguous East Asian Width occupies 1 column (ADR 0035 D4).
#[test]
fn t_chip1_width_ambiguous_is_one_column() {
    let mut vt = engine(80, 24);
    vt.feed("αX".as_bytes()).expect("feed ambiguous");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(grid.cursor_col, 2, "α (EAW Ambiguous) occupies 1, then X");
    assert_eq!(grid.cell(0, 0).expect("alpha").codepoint, '\u{03b1}' as u32);
    assert_eq!(
        grid.cell(0, 0).expect("alpha").attrs & (ATTR_WIDE_LEAD | ATTR_WIDE_TAIL),
        0
    );
    assert_eq!(grid.cell(1, 0).expect("X").codepoint, u32::from(b'X'));
}

/// ECH of a wide lead clears both halves to space + current bg (ADR 0035).
///
/// Required mutation: `RILL_MUTATE=orphan_wide_tail` — skip smash so the
/// tail keeps the wide-tail bit.
#[test]
fn t_chip1_width_ech_of_wide_lead_clears_both_halves() {
    let mut vt = engine(80, 24);
    vt.feed("日X".as_bytes()).expect("feed");
    vt.feed(b"\x1b[44m\x1b[1;1H\x1b[X")
        .expect("ECH lead under current bg");
    let grid = vt.snapshot().expect("snapshot");
    assert_cleared_wide_pair(&vt, &grid, 0, 1);
    assert_eq!(
        grid.cell(2, 0).expect("X").codepoint,
        u32::from(b'X'),
        "ECH must not shift cells to the right"
    );
}

/// ECH of a wide tail clears both halves to space + current bg (ADR 0035).
///
/// Required mutation: `RILL_MUTATE=orphan_wide_tail`.
#[test]
fn t_chip1_width_ech_of_wide_tail_clears_both_halves() {
    let mut vt = engine(80, 24);
    vt.feed("日X".as_bytes()).expect("feed");
    vt.feed(b"\x1b[44m\x1b[1;2H\x1b[X")
        .expect("ECH tail under current bg");
    let grid = vt.snapshot().expect("snapshot");
    assert_cleared_wide_pair(&vt, &grid, 0, 1);
    assert_eq!(grid.cell(2, 0).expect("X").codepoint, u32::from(b'X'));
}

/// DCH of a wide lead leaves no orphan tail; smashed half is space + current bg.
///
/// Required mutation: `RILL_MUTATE=orphan_wide_tail`.
#[test]
fn t_chip1_width_dch_of_wide_lead_clears_both_halves() {
    let mut vt = engine(80, 24);
    vt.feed("日X".as_bytes()).expect("feed");
    vt.feed(b"\x1b[44m\x1b[1;1H\x1b[P").expect("DCH lead");
    let grid = vt.snapshot().expect("snapshot");
    assert_no_orphan_wide(&grid);
    let c0 = grid.cell(0, 0).expect("col 0");
    assert_eq!(
        c0.codepoint, 32,
        "smashed partner shifts into the deleted lead"
    );
    assert_eq!(c0.attrs & (ATTR_WIDE_LEAD | ATTR_WIDE_TAIL), 0);
    assert_eq!(
        vt.color_at(0, 0).expect("bg").1,
        Color::Indexed(4),
        "smashed half takes the current background (SGR 44)"
    );
    assert_eq!(grid.cell(1, 0).expect("X").codepoint, u32::from(b'X'));
}

/// DCH of a wide tail leaves no orphan lead; smashed half is space + current bg.
///
/// Required mutation: `RILL_MUTATE=orphan_wide_tail`.
#[test]
fn t_chip1_width_dch_of_wide_tail_clears_both_halves() {
    let mut vt = engine(80, 24);
    vt.feed("日X".as_bytes()).expect("feed");
    vt.feed(b"\x1b[44m\x1b[1;2H\x1b[P").expect("DCH tail");
    let grid = vt.snapshot().expect("snapshot");
    assert_no_orphan_wide(&grid);
    let c0 = grid.cell(0, 0).expect("lead smashed");
    assert_eq!(
        c0.codepoint, 32,
        "DCH of the tail smashes the lead to space"
    );
    assert_eq!(c0.attrs & (ATTR_WIDE_LEAD | ATTR_WIDE_TAIL), 0);
    assert_eq!(vt.color_at(0, 0).expect("bg").1, Color::Indexed(4));
    assert_eq!(grid.cell(1, 0).expect("X").codepoint, u32::from(b'X'));
}

/// Overwrite of a wide lead with ASCII clears the tail to space + current bg.
///
/// Required mutation: `RILL_MUTATE=orphan_wide_tail`.
#[test]
fn t_chip1_width_overwrite_of_wide_lead_clears_tail() {
    let mut vt = engine(80, 24);
    vt.feed("日".as_bytes()).expect("feed");
    vt.feed(b"\x1b[44m\x1b[1;1HA").expect("overwrite lead");
    let grid = vt.snapshot().expect("snapshot");
    assert_no_orphan_wide(&grid);
    assert_eq!(grid.cell(0, 0).expect("A").codepoint, u32::from(b'A'));
    assert_eq!(
        grid.cell(0, 0).expect("A").attrs & (ATTR_WIDE_LEAD | ATTR_WIDE_TAIL),
        0
    );
    let tail = grid.cell(1, 0).expect("smashed tail");
    assert_eq!(
        tail.codepoint, 32,
        "overwriting the lead must not leave a tail"
    );
    assert_eq!(tail.attrs & (ATTR_WIDE_LEAD | ATTR_WIDE_TAIL), 0);
    assert_eq!(vt.color_at(1, 0).expect("bg").1, Color::Indexed(4));
}

/// Overwrite of a wide tail with ASCII clears the lead to space + current bg.
///
/// Required mutation: `RILL_MUTATE=orphan_wide_tail`.
#[test]
fn t_chip1_width_overwrite_of_wide_tail_clears_lead() {
    let mut vt = engine(80, 24);
    vt.feed("日".as_bytes()).expect("feed");
    vt.feed(b"\x1b[44m\x1b[1;2HA").expect("overwrite tail");
    let grid = vt.snapshot().expect("snapshot");
    assert_no_orphan_wide(&grid);
    let lead = grid.cell(0, 0).expect("smashed lead");
    assert_eq!(
        lead.codepoint, 32,
        "overwriting the tail must not leave a lead"
    );
    assert_eq!(lead.attrs & (ATTR_WIDE_LEAD | ATTR_WIDE_TAIL), 0);
    assert_eq!(vt.color_at(0, 0).expect("bg").1, Color::Indexed(4));
    assert_eq!(grid.cell(1, 0).expect("A").codepoint, u32::from(b'A'));
}

/// Pending cluster survives `feed()`: combining / ZWJ in the next feed append
/// (ADR 0035 D8).
#[test]
fn t_chip1_grapheme_cluster_continues_across_feed() {
    let mut vt = engine(80, 24);
    vt.feed("日".as_bytes()).expect("base");
    vt.feed("\u{0301}".as_bytes())
        .expect("combining in next feed");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(
        grid.cursor_col, 2,
        "combining mark in a later feed() must append, not consume a cell"
    );
    assert_eq!(grid.cell(0, 0).expect("base").codepoint, '\u{65e5}' as u32);
    assert_eq!(grid.cell(2, 0).expect("after").codepoint, 32);

    let mut vt = engine(80, 24);
    vt.feed("\u{1f469}".as_bytes()).expect("woman");
    let after_base = vt.snapshot().expect("snapshot");
    assert_eq!(after_base.cursor_col, 2, "👩 is width 2");
    vt.feed("\u{200d}".as_bytes()).expect("ZWJ in next feed");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(
        grid.cursor_col, 2,
        "ZWJ in a later feed() must append to the open cluster"
    );
    assert_eq!(grid.cell(2, 0).expect("after").codepoint, 32);
}

/// `snapshot()` after the first feed of 日 already shows width 2 (ADR 0035 D8).
/// The engine must not hold an unbounded buffer until the cluster closes.
#[test]
fn t_chip1_width_snapshot_places_wide_before_cluster_closes() {
    let mut vt = engine(80, 24);
    vt.feed("日".as_bytes()).expect("base only");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(grid.cursor_col, 2, "日 is placed at width 2 immediately");
    let lead = grid.cell(0, 0).expect("lead");
    assert_eq!(lead.codepoint, '\u{65e5}' as u32);
    assert_ne!(lead.attrs & ATTR_WIDE_LEAD, 0);
    let tail = grid.cell(1, 0).expect("tail");
    assert_eq!(tail.codepoint, '\u{65e5}' as u32);
    assert_ne!(tail.attrs & ATTR_WIDE_TAIL, 0);
}

/// Hostile: 40 combining marks in a later feed still truncate (ADR 0035 D8).
///
/// Required mutation: `RILL_MUTATE=fixed_grapheme_buf`.
#[test]
fn t_chip1_grapheme_hostile_combining_across_feed_still_truncated() {
    let mut vt = engine(80, 24);
    vt.feed(b"e").expect("base");
    let mut marks = String::new();
    for _ in 0..40 {
        marks.push('\u{0301}');
    }
    vt.feed(marks.as_bytes()).expect("hostile combiners");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(grid.cursor_col, 1, "combiners must not consume cells");
    assert_eq!(grid.cell(0, 0).expect("base").codepoint, u32::from(b'e'));
    assert!(
        grid.grapheme_truncated >= 1,
        "40 combiners past RILL_GRAPHEME_MAX must be counted"
    );
}

/// Regional-indicator pair is one cluster of width 2 (ADR 0035 D1).
///
/// Required mutation: `RILL_MUTATE=narrow_cjk`.
#[test]
fn t_chip1_width_regional_indicator_pair_is_two_columns() {
    let mut vt = engine(80, 24);
    vt.feed("🇺🇸X".as_bytes()).expect("feed RI pair");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(
        grid.cursor_col, 3,
        "US flag is 2 columns then X (must not split or stay 1+1)"
    );
    let lead = grid.cell(0, 0).expect("RI lead");
    assert_eq!(lead.codepoint, 0x1f1fa);
    assert_ne!(lead.attrs & ATTR_WIDE_LEAD, 0);
    let tail = grid.cell(1, 0).expect("RI tail");
    assert_eq!(tail.codepoint, 0x1f1fa, "tail copies the lead base");
    assert_ne!(tail.attrs & ATTR_WIDE_TAIL, 0);
    assert_eq!(grid.cell(2, 0).expect("X").codepoint, u32::from(b'X'));
}

/// A second RI MUST NOT place a tail on the next row (ADR 0035 D1).
#[test]
fn t_chip1_width_regional_indicator_pair_does_not_split_at_last_column() {
    let mut vt = engine(10, 6);
    vt.feed("012345678🇺🇸".as_bytes()).expect("feed RI at edge");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(
        grid.cell(9, 0).expect("last").codepoint,
        0x1f1fa,
        "first RI may sit in the last column at width 1"
    );
    assert_eq!(
        grid.cell(9, 0).expect("last").attrs & ATTR_WIDE_LEAD,
        0,
        "must not be a lead whose tail is on row 1"
    );
    assert_eq!(
        grid.cell(0, 1).expect("next row").codepoint,
        32,
        "second RI must not open a split tail on the next row"
    );
}

fn assert_cleared_wide_pair(
    vt: &VtEngine,
    grid: &rill_vt_types::PodGrid,
    lead_col: u16,
    tail_col: u16,
) {
    assert_no_orphan_wide(grid);
    for col in [lead_col, tail_col] {
        let cell = grid.cell(col, 0).expect("pair half");
        assert_eq!(
            cell.codepoint, 32,
            "wide half at col {col} must become space"
        );
        assert_eq!(
            cell.attrs & (ATTR_WIDE_LEAD | ATTR_WIDE_TAIL),
            0,
            "wide bits at col {col} must be cleared"
        );
        assert_eq!(
            vt.color_at(col, 0).expect("bg").1,
            Color::Indexed(4),
            "col {col} must take the current background (SGR 44), not the old cell bg"
        );
    }
}

fn assert_no_orphan_wide(grid: &rill_vt_types::PodGrid) {
    for row in 0..grid.rows {
        let mut col = 0u16;
        while col < grid.cols {
            let cell = grid.cell(col, row).expect("cell");
            let lead = cell.attrs & ATTR_WIDE_LEAD != 0;
            let tail = cell.attrs & ATTR_WIDE_TAIL != 0;
            assert!(
                !(lead && tail),
                "col {col} row {row} must not be both lead and tail"
            );
            if lead {
                let next = grid
                    .cell(col + 1, row)
                    .expect("wide lead must have a same-row tail");
                assert_ne!(
                    next.attrs & ATTR_WIDE_TAIL,
                    0,
                    "lead at col {col} row {row} without a tail is an orphan"
                );
                col += 2;
                continue;
            }
            assert!(!tail, "orphan tail at col {col} row {row} (ADR 0035)");
            col += 1;
        }
    }
}
