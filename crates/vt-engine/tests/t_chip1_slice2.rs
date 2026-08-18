//! Slice 2 T-CHIP1 gates. Authority: ADR 0020, SPEC-VT-PARSER, SPEC-VT-SCREEN.
//!
//! Observed red on empty `vt-engine` (no print / no grid), then green.
//! First CSI-consuming parser in this tree; cites S-VT / ADR 0020 D1/D3/D6.

use rill_vt_types::TerminalEmulation;
use std::path::PathBuf;
use vt_engine::VtEngine;

fn engine(cols: u16, rows: u16) -> VtEngine {
    VtEngine::new(cols, rows).expect("vt-engine")
}

fn row_codepoints(grid: &rill_vt_types::PodGrid, row: u16) -> Vec<u32> {
    (0..grid.cols)
        .filter_map(|c| grid.cell(c, row).map(|cell| cell.codepoint))
        .take_while(|cp| *cp != 32)
        .collect()
}

/// T-CHIP1-ASCII — printable lands in the POD grid.
///
/// Required mutation: `RILL_MUTATE=drop_print`.
#[test]
fn t_chip1_ascii_printable_lands_in_the_pod_grid() {
    let mut vt = engine(40, 5);
    vt.feed(b"Hello").expect("feed");
    let grid = vt.snapshot().expect("snapshot");
    let got: String = (0..5)
        .map(|c| char::from_u32(grid.cell(c, 0).expect("cell").codepoint).unwrap_or('?'))
        .collect();
    assert_eq!(
        got, "Hello",
        "printable did not land in row 0 (SPEC-VT-SCREEN §6)"
    );
}

/// T-CHIP1-BYTES — invalid UTF-8 reaches the parser unmodified.
///
/// Required mutation: `RILL_MUTATE=drop_high_bytes`.
/// `csi_high_param` is blind to that mutation and is only a no-crash case
/// (ADR 0020 D7).
#[test]
fn t_chip1_bytes_invalid_utf8_reaches_the_parser() {
    for (name, fixture) in byte_fixtures() {
        let mut vt = engine(80, 24);
        vt.feed(&fixture).expect("feed");
        let grid = vt.snapshot().expect("snapshot");
        let row0 = row_codepoints(&grid, 0);

        if fixture.contains(&0x41) {
            assert!(
                row0.contains(&u32::from(b'A')),
                "{name}: ASCII 'A' from the fixture did not land: {row0:x?}"
            );
        }
        let csi_high = fixture.first() == Some(&0x1b);
        if !csi_high && fixture.iter().any(|b| *b >= 0x80) {
            assert!(
                row0.iter().any(|cp| *cp > 127),
                "{name}: high bytes never reached the parser (no non-ASCII cell): {row0:x?}"
            );
        }
    }
}

/// T-CHIP1-C1 — a decoded C1 scalar paints and does not open a CSI.
///
/// Bug: `vte` 0.15 `execute()`s `0x80..=0x9f`, so T-CHIP1-BYTES could not see
/// `drop_high_bytes` on C1 fixtures (ADR 0020 D3).
///
/// Required mutation: `RILL_MUTATE=c1_as_control`.
#[test]
fn t_chip1_c1_decoded_scalar_paints_and_does_not_open_csi() {
    let mut vt = engine(80, 24);
    vt.feed(&[0xc2, 0x9b, 0x41]).expect("feed utf8 c1");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(
        grid.cell(0, 0).expect("c0").codepoint,
        0x9b,
        "decoded U+009B must paint (ADR 0020 D3)"
    );
    assert_eq!(
        grid.cell(1, 0).expect("c1").codepoint,
        u32::from(b'A'),
        "0x9b must not open CSI and consume A"
    );
    assert_eq!(grid.cursor_col, 2, "cursor advanced two columns");

    let mut vt = engine(80, 24);
    vt.feed(&[0x80, 0x41]).expect("feed invalid c1 byte");
    let grid = vt.snapshot().expect("snapshot");
    let row0 = row_codepoints(&grid, 0);
    assert_eq!(
        row0,
        vec![0xfffd, u32::from(b'A')],
        "invalid 0x80 is one U+FFFD then A"
    );
}

/// T-CHIP1-CRLF — CR LF moves the cursor.
///
/// Required mutation: `RILL_MUTATE=ignore_crlf`.
#[test]
fn t_chip1_crlf_moves_the_cursor() {
    let mut vt = engine(40, 5);
    vt.feed(b"A\r\nB").expect("feed");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(
        grid.cell(0, 1).expect("row1").codepoint,
        u32::from(b'B'),
        "B must land at row 1 col 0 after CR LF"
    );
}

/// T-CHIP1-WRAP — the last column defers its wrap.
///
/// Required mutation: `RILL_MUTATE=eager_wrap`.
#[test]
fn t_chip1_wrap_last_column_defers() {
    let mut vt = engine(10, 6);
    vt.feed(b"0123456789X").expect("feed");
    let grid = vt.snapshot().expect("snapshot");
    let row0: String = (0..10)
        .map(|c| char::from_u32(grid.cell(c, 0).expect("r0").codepoint).unwrap_or('?'))
        .collect();
    assert_eq!(row0, "0123456789", "row 0 must hold all ten printables");
    assert_eq!(
        grid.cell(0, 1).expect("r1").codepoint,
        u32::from(b'X'),
        "11th printable lands at row 1 col 0"
    );
}

/// T-CHIP1-SIZE — snapshot is exactly cols×rows.
///
/// Required mutation: `RILL_MUTATE=unbounded_history`.
#[test]
fn t_chip1_size_snapshot_is_cols_times_rows() {
    let mut vt = engine(80, 24);
    vt.resize(40, 5, 8, 16).expect("resize");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(
        grid.cells.len(),
        200,
        "snapshot must be exactly 40×5, not a history ring"
    );
    assert_eq!(grid.cols, 40);
    assert_eq!(grid.rows, 5);
}

/// T-CHIP1-DAMAGE — an untouched frame can be skipped.
///
/// Required mutation: `RILL_MUTATE=always_full_damage`.
#[test]
fn t_chip1_damage_untouched_frame_can_be_skipped() {
    let mut vt = engine(40, 8);
    // Construction is full_damage; clear it so the gate observes feed damage.
    let _ = vt.snapshot().expect("clear");
    vt.feed(b"\n\n\nZ").expect("feed");
    let grid = vt.snapshot().expect("snapshot");
    assert!(
        grid.full_damage || (grid.damage_row0 <= 3 && grid.damage_row1 >= 3),
        "damage must cover row 3"
    );
    assert!(
        grid.full_damage || grid.damage_row0 > 0,
        "damage must not cover row 0; got full={} range={}..{}",
        grid.full_damage,
        grid.damage_row0,
        grid.damage_row1
    );

    let grid = vt.snapshot().expect("second snapshot");
    assert!(
        !grid.full_damage && grid.damage_row0 > grid.damage_row1,
        "second snapshot with no feed must be skippable"
    );
}

fn byte_fixtures() -> Vec<(String, Vec<u8>)> {
    let mut out = vec![
        ("lone_continuation".into(), vec![0x80, 0x41]),
        ("truncated_3byte".into(), vec![0xe2, 0x82, 0x41]),
        ("overlong_slash".into(), vec![0xc0, 0xaf]),
        ("lone_surrogate".into(), vec![0xed, 0xa0, 0x80]),
        ("bom_then_high".into(), vec![0xff, 0xfe, 0x80, 0x41]),
        ("csi_high_param".into(), vec![0x1b, 0x5b, 0x80, 0x6d, 0x41]),
        ("c1_in_utf8".into(), vec![0xc2, 0x9b, 0x41]),
    ];
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/bytes");
    assert!(
        dir.is_dir(),
        "fixtures/bytes/ is required (SPEC-VT-CONFORMANCE §2, ADR 0002 D5)"
    );
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("bin"))
        .collect();
    files.sort();
    assert!(
        !files.is_empty(),
        "fixtures/bytes/ has no .bin files (SPEC-VT-CONFORMANCE §2)"
    );
    for path in files {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("bin")
            .to_string();
        let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {name}: {e}"));
        out.push((name, bytes));
    }
    let invalid = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/invalid_utf8.bin");
    assert!(
        invalid.is_file(),
        "fixtures/invalid_utf8.bin is required (SPEC-VT-CONFORMANCE §2, ADR 0002 D5)"
    );
    out.push((
        "invalid_utf8.bin".into(),
        std::fs::read(&invalid).unwrap_or_else(|e| panic!("read invalid_utf8.bin: {e}")),
    ));
    out
}
