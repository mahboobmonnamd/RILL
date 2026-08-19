//! Slice 3 T-CHIP1-CUP / T-CHIP1-ED and the CSI screen work in SPEC-VT-SCREEN §4.
//!
//! First CSI-executing PR in this tree. Cites ADR 0020 D1/D4/D6 and S-VT #21.
//! REP (`CSI b`) is a named miss: consumed and ignored.

use rill_vt_types::TerminalEmulation;
use vt_engine::VtEngine;

fn engine(cols: u16, rows: u16) -> VtEngine {
    VtEngine::new(cols, rows).expect("vt-engine")
}

fn cp(grid: &rill_vt_types::PodGrid, col: u16, row: u16) -> u32 {
    grid.cell(col, row).expect("cell").codepoint
}

/// T-CHIP1-CUP — CSI CUP positions the cursor.
///
/// Required mutation: `RILL_MUTATE=ignore_csi`.
#[test]
fn t_chip1_cup_positions_the_cursor() {
    let mut vt = engine(80, 24);
    vt.feed(b"\x1b[5;10H").expect("feed CUP");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(
        grid.cursor_row, 4,
        "ESC[5;10H is 1-based (5,10) → 0-based row 4 (SPEC-VT-SCREEN §4)"
    );
    assert_eq!(
        grid.cursor_col, 9,
        "ESC[5;10H is 1-based (5,10) → 0-based col 9"
    );

    let mut vt = engine(80, 24);
    vt.feed(b"\x1b[H").expect("feed default CUP");
    vt.feed(b"\x1b[3;3H").expect("move");
    vt.feed(b"\x1b[H").expect("home");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(
        (grid.cursor_row, grid.cursor_col),
        (0, 0),
        "CSI H with no params is 1;1"
    );

    let mut vt = engine(10, 6);
    vt.feed(b"\x1b[999;999H").expect("feed clamped CUP");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(
        (grid.cursor_row, grid.cursor_col),
        (5, 9),
        "CUP must clamp to the grid, not wrap or scroll"
    );

    let mut vt = engine(80, 24);
    vt.feed(b"\x1b[5;10f").expect("feed HVP");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!((grid.cursor_row, grid.cursor_col), (4, 9), "HVP is CUP");
}

/// T-CHIP1-ED — erase display clears to space.
///
/// Required mutation: `RILL_MUTATE=noop_ed`.
#[test]
fn t_chip1_ed_erase_display_clears_to_space() {
    let mut vt = engine(8, 3);
    vt.feed(b"ABCDEFGH").expect("fill");
    vt.feed(b"\x1b[2J").expect("ED 2");
    let grid = vt.snapshot().expect("snapshot");
    for row in 0..grid.rows {
        for col in 0..grid.cols {
            assert_eq!(
                cp(&grid, col, row),
                32,
                "ED 2 must leave space 32 at ({col},{row})"
            );
        }
    }
}

/// CUU/CUD/CUF/CUB, CHA/VPA, CNL/CPL: relative and absolute moves, defaults.
#[test]
fn t_chip1_csi_relative_and_absolute_moves() {
    let mut vt = engine(10, 6);
    vt.feed(b"\x1b[3C").expect("CUF 3");
    vt.feed(b"X").expect("print");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(cp(&grid, 3, 0), u32::from(b'X'), "CUF 3 then X at col 3");

    let mut vt = engine(10, 6);
    vt.feed(b"AB").expect("print");
    vt.feed(b"\x1b[D").expect("CUB default 1");
    vt.feed(b"C").expect("print");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(
        cp(&grid, 1, 0),
        u32::from(b'C'),
        "CUB default 1 overwrites B"
    );

    let mut vt = engine(10, 6);
    vt.feed(b"A\r\nB\r").expect("print");
    vt.feed(b"\x1b[A").expect("CUU 1");
    vt.feed(b"C").expect("print");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(
        cp(&grid, 0, 0),
        u32::from(b'C'),
        "CUU from row 1 col 0 overwrites A"
    );

    let mut vt = engine(10, 6);
    vt.feed(b"\x1b[5G").expect("CHA 5");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(grid.cursor_col, 4, "CHA 5 → col 4");

    let mut vt = engine(10, 6);
    vt.feed(b"\x1b[4d").expect("VPA 4");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(grid.cursor_row, 3, "VPA 4 → row 3");

    let mut vt = engine(10, 6);
    vt.feed(b"\x1b[2E").expect("CNL 2");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(
        (grid.cursor_row, grid.cursor_col),
        (2, 0),
        "CNL 2 → row 2 col 0"
    );

    let mut vt = engine(10, 6);
    vt.feed(b"\x1b[3;5H\x1b[2F").expect("CPL 2");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(
        (grid.cursor_row, grid.cursor_col),
        (0, 0),
        "CPL 2 from row 2 col 4 → row 0 col 0"
    );
}

/// EL and ECH: erase in line / characters, cursor stays, no shift (ECH ≠ DCH).
#[test]
fn t_chip1_el_ech_erase() {
    let mut vt = engine(5, 1);
    vt.feed(b"ABCDE").expect("fill");
    vt.feed(b"\x1b[1;3H").expect("col 2");
    vt.feed(b"\x1b[K").expect("EL 0");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(cp(&grid, 0, 0), u32::from(b'A'));
    assert_eq!(cp(&grid, 1, 0), u32::from(b'B'));
    assert_eq!(cp(&grid, 2, 0), 32, "EL 0 includes the cursor cell");
    assert_eq!(cp(&grid, 4, 0), 32);
    assert_eq!(grid.cursor_col, 2, "EL does not move the cursor");

    let mut vt = engine(5, 1);
    vt.feed(b"ABCDE").expect("fill");
    vt.feed(b"\x1b[1;2H").expect("col 1");
    vt.feed(b"\x1b[2X").expect("ECH 2");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(cp(&grid, 0, 0), u32::from(b'A'));
    assert_eq!(cp(&grid, 1, 0), 32);
    assert_eq!(cp(&grid, 2, 0), 32);
    assert_eq!(
        cp(&grid, 3, 0),
        u32::from(b'D'),
        "ECH must not shift cells to the right (unlike DCH)"
    );
    assert_eq!(grid.cursor_col, 1, "ECH does not move the cursor");
}

/// IL/DL shift lines in the region; ICH/DCH shift cells on the cursor row.
#[test]
fn t_chip1_il_dl_ich_dch() {
    let mut vt = engine(1, 3);
    vt.feed(b"A\nB\nC").expect("fill");
    vt.feed(b"\x1b[2;1H").expect("row 1");
    vt.feed(b"\x1b[L").expect("IL 1");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(cp(&grid, 0, 0), u32::from(b'A'), "row 0 untouched");
    assert_eq!(cp(&grid, 0, 1), 32, "inserted blank at cursor row");
    assert_eq!(
        cp(&grid, 0, 2),
        u32::from(b'B'),
        "B shifted down; C dropped"
    );

    let mut vt = engine(1, 3);
    vt.feed(b"A\nB\nC").expect("fill");
    vt.feed(b"\x1b[2;1H").expect("row 1");
    vt.feed(b"\x1b[M").expect("DL 1");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(cp(&grid, 0, 0), u32::from(b'A'));
    assert_eq!(cp(&grid, 0, 1), u32::from(b'C'), "C shifted up");
    assert_eq!(cp(&grid, 0, 2), 32, "vacated last row is space");

    let mut vt = engine(5, 1);
    vt.feed(b"ABCDE").expect("fill");
    vt.feed(b"\x1b[1;2H").expect("col 1");
    vt.feed(b"\x1b[@").expect("ICH 1");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(cp(&grid, 0, 0), u32::from(b'A'));
    assert_eq!(cp(&grid, 1, 0), 32, "inserted blank at cursor");
    assert_eq!(cp(&grid, 2, 0), u32::from(b'B'));
    assert_eq!(cp(&grid, 4, 0), u32::from(b'D'), "E dropped off the row");

    let mut vt = engine(5, 1);
    vt.feed(b"ABCDE").expect("fill");
    vt.feed(b"\x1b[1;2H").expect("col 1");
    vt.feed(b"\x1b[P").expect("DCH 1");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(cp(&grid, 0, 0), u32::from(b'A'));
    assert_eq!(cp(&grid, 1, 0), u32::from(b'C'));
    assert_eq!(cp(&grid, 4, 0), 32, "vacated last cell is space");
}

/// Overflow sets ignore (ADR 0020 D4). REP is consumed and does not repeat.
#[test]
fn t_chip1_csi_overflow_is_ignored_and_rep_is_a_named_miss() {
    let mut vt = engine(10, 6);
    let mut flood = Vec::from(b"\x1b[");
    for _ in 0..40 {
        flood.extend_from_slice(b"1;");
    }
    flood.push(b'H');
    vt.feed(&flood).expect("feed overflowing CSI");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(
        (grid.cursor_row, grid.cursor_col),
        (0, 0),
        "CSI with >32 params must be discarded, not executed truncated"
    );

    let mut vt = engine(10, 6);
    vt.feed(b"A\x1b[5b").expect("REP");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(cp(&grid, 0, 0), u32::from(b'A'));
    assert_eq!(
        cp(&grid, 1, 0),
        32,
        "REP (CSI b) is a named miss: consumed, not repeated"
    );
}
