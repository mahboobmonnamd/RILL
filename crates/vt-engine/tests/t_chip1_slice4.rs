//! Slice 4 T-CHIP1-SCROLL / T-CHIP1-ALT (SPEC-VT-SCREEN §5).
//!
//! DECSTBM, SU/SD, alt screen, DECSC/DECRC, DECTCEM, DECAWM.
//! Chip 0 stays live.

use rill_vt_types::TerminalEmulation;
use vt_engine::VtEngine;

fn engine(cols: u16, rows: u16) -> VtEngine {
    VtEngine::new(cols, rows).expect("vt-engine")
}

fn cp(grid: &rill_vt_types::PodGrid, col: u16, row: u16) -> u32 {
    grid.cell(col, row).expect("cell").codepoint
}

fn fill_rows(vt: &mut VtEngine, labels: &[u8]) {
    for (i, ch) in labels.iter().enumerate() {
        let row = u8::try_from(i + 1).expect("row");
        vt.feed(&[0x1b, b'[', b'0' + row, b';', b'1', b'H', *ch])
            .expect("fill row");
    }
}

/// T-CHIP1-SCROLL — DECSTBM confines the scroll.
///
/// Required mutation: `RILL_MUTATE=ignore_decstbm`.
#[test]
fn t_chip1_scroll_decstbm_confines_the_scroll() {
    let mut vt = engine(8, 6);
    fill_rows(&mut vt, b"ABCDEF");
    vt.feed(b"\x1b[2;4r").expect("DECSTBM 2;4");
    vt.feed(b"\x1b[S").expect("SU 1");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(
        cp(&grid, 0, 0),
        u32::from(b'A'),
        "row 0 is outside the region and must not move"
    );
    assert_eq!(
        cp(&grid, 0, 5),
        u32::from(b'F'),
        "row 5 is outside the region and must not move"
    );
    assert_eq!(
        cp(&grid, 0, 1),
        u32::from(b'C'),
        "region row 1 (was B) must shift up to C"
    );
    assert_eq!(
        cp(&grid, 0, 2),
        u32::from(b'D'),
        "region row 2 must shift up to D"
    );
    assert_eq!(
        cp(&grid, 0, 3),
        32,
        "vacated region bottom is space (content scrolled off is discarded)"
    );
    assert_eq!(
        cp(&grid, 0, 4),
        u32::from(b'E'),
        "row 4 is outside the region"
    );
}

/// T-CHIP1-ALT — 1049 preserves primary.
///
/// Required mutation: `RILL_MUTATE=single_buffer`.
#[test]
fn t_chip1_alt_1049_preserves_primary() {
    let mut vt = engine(8, 4);
    vt.feed(b"A").expect("primary");
    vt.feed(b"\x1b[?1049h").expect("alt on");
    vt.feed(b"B").expect("alt");
    vt.feed(b"\x1b[?1049l").expect("alt off");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(
        cp(&grid, 0, 0),
        u32::from(b'A'),
        "primary A must be visible after 1049l"
    );
    let has_b = (0..grid.cols).any(|c| (0..grid.rows).any(|r| cp(&grid, c, r) == u32::from(b'B')));
    assert!(!has_b, "alt B must be gone after 1049l");
}

/// LF at the region bottom scrolls only the region.
#[test]
fn t_chip1_lf_scrolls_inside_the_region() {
    let mut vt = engine(8, 6);
    fill_rows(&mut vt, b"ABCDEF");
    vt.feed(b"\x1b[2;4r").expect("DECSTBM");
    vt.feed(b"\x1b[4;1H\n").expect("LF at region bottom");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(cp(&grid, 0, 0), u32::from(b'A'));
    assert_eq!(cp(&grid, 0, 5), u32::from(b'F'));
    assert_eq!(cp(&grid, 0, 1), u32::from(b'C'));
}

/// 1047 switches buffers without saving the cursor; leaving when not in alt
/// is a no-op; a second 1049h must not overwrite the saved primary.
#[test]
fn t_chip1_alt_1047_and_reenter_do_not_clobber_primary() {
    let mut vt = engine(8, 4);
    vt.feed(b"A").expect("primary");
    vt.feed(b"\x1b[?1047h").expect("1047h");
    vt.feed(b"B").expect("alt");
    vt.feed(b"\x1b[?1047l").expect("1047l");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(cp(&grid, 0, 0), u32::from(b'A'), "1047l restores primary");
    let has_b = (0..grid.cols).any(|c| (0..grid.rows).any(|r| cp(&grid, c, r) == u32::from(b'B')));
    assert!(!has_b, "1047l must not leave alt B on the primary");

    let mut vt = engine(8, 4);
    vt.feed(b"A").expect("primary");
    vt.feed(b"\x1b[?1049h").expect("first 1049h");
    vt.feed(b"B").expect("alt1");
    vt.feed(b"\x1b[?1049h").expect("second 1049h");
    vt.feed(b"C").expect("alt2");
    vt.feed(b"\x1b[?1049l").expect("1049l");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(
        cp(&grid, 0, 0),
        u32::from(b'A'),
        "second 1049h must not overwrite the saved primary"
    );

    let mut vt = engine(8, 4);
    vt.feed(b"A").expect("primary");
    vt.feed(b"\x1b[?1049l")
        .expect("leave while already primary");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(
        cp(&grid, 0, 0),
        u32::from(b'A'),
        "1049l when not in alt is a no-op, not a clear"
    );
}

/// DECSC/DECRC, DECTCEM, DECAWM.
#[test]
fn t_chip1_decsc_dectcem_decawm() {
    let mut vt = engine(10, 4);
    vt.feed(b"A").expect("print");
    vt.feed(b"\x1b7").expect("DECSC");
    vt.feed(b"\x1b[3;3H").expect("CUP");
    vt.feed(b"B").expect("print");
    vt.feed(b"\x1b8").expect("DECRC");
    vt.feed(b"C").expect("print");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(cp(&grid, 0, 0), u32::from(b'A'));
    assert_eq!(
        cp(&grid, 1, 0),
        u32::from(b'C'),
        "DECRC restores the saved cursor so C follows A"
    );

    let mut vt = engine(8, 4);
    vt.feed(b"\x1b[?25l").expect("DECTCEM hide");
    let grid = vt.snapshot().expect("snapshot");
    assert!(!grid.cursor_visible, "CSI ?25l hides the cursor");
    drop(grid);
    vt.feed(b"\x1b[?25h").expect("DECTCEM show");
    let grid = vt.snapshot().expect("snapshot");
    assert!(grid.cursor_visible, "CSI ?25h shows the cursor");

    let mut vt = engine(10, 4);
    vt.feed(b"\x1b[?7l").expect("DECAWM off");
    vt.feed(b"0123456789X").expect("overflow last column");
    let grid = vt.snapshot().expect("snapshot");
    let row0: String = (0..10)
        .map(|c| char::from_u32(cp(&grid, c, 0)).unwrap_or('?'))
        .collect();
    assert_eq!(
        row0, "012345678X",
        "DECAWM off overwrites the last column and does not wrap"
    );
    assert_eq!(cp(&grid, 0, 1), 32, "no wrap onto row 1");
}

/// CSI r with no params resets the region to the full grid.
#[test]
fn t_chip1_decstbm_reset_and_sd() {
    let mut vt = engine(8, 6);
    fill_rows(&mut vt, b"ABCDEF");
    vt.feed(b"\x1b[2;4r").expect("region");
    vt.feed(b"\x1b[r").expect("reset region");
    vt.feed(b"\x1b[S").expect("SU full grid");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(
        cp(&grid, 0, 0),
        u32::from(b'B'),
        "CSI r resets to the full grid so SU moves row 0"
    );

    let mut vt = engine(8, 6);
    fill_rows(&mut vt, b"ABCDEF");
    vt.feed(b"\x1b[2;4r").expect("region");
    vt.feed(b"\x1b[T").expect("SD 1");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(cp(&grid, 0, 0), u32::from(b'A'), "SD must not move row 0");
    assert_eq!(cp(&grid, 0, 1), 32, "SD inserts a blank at the region top");
    assert_eq!(cp(&grid, 0, 2), u32::from(b'B'), "B shifted down");
    assert_eq!(cp(&grid, 0, 5), u32::from(b'F'));
}
