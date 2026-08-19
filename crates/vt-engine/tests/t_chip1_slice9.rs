//! Slice 9 T-CHIP1-RESYNC.
//!
//! Authority: ADR 0012 D4, SPEC-CHIP1 §2. Oracle is a second instance's grid,
//! not the `\x1b[2J\x1b[H` prefix the emit path prepends (ADR 0002 D4). Chip 0
//! stays live.

use rill_vt_types::{PodGrid, TerminalEmulation};
use vt_engine::VtEngine;

fn engine() -> VtEngine {
    VtEngine::new(80, 24).expect("vt-engine")
}

fn row0(grid: &PodGrid) -> String {
    (0..grid.cols)
        .filter_map(|c| grid.cell(c, 0).and_then(|x| char::from_u32(x.codepoint)))
        .collect()
}

/// T-CHIP1-RESYNC — emit bytes reconstruct the grid.
///
/// Required mutation: `RILL_MUTATE=empty_resync`.
#[test]
fn t_chip1_resync_emit_bytes_reconstruct_the_grid() {
    let mut source = engine();
    source.feed(b"RILL-RESYNC-MARK\r\n").expect("feed");
    let before = source.snapshot().expect("snapshot");

    let resync = source
        .resync_from_history(b"RILL-RESYNC-MARK\r\n")
        .expect("resync");

    let mut replay = engine();
    replay.feed(&resync).expect("feed resync");
    let after = replay.snapshot().expect("snapshot");

    assert_eq!(
        row0(&before),
        row0(&after),
        "resync bytes did not reconstruct row 0"
    );
    assert!(
        row0(&after).contains("RILL-RESYNC-MARK"),
        "second instance must show the marker, not a self-asserted prefix"
    );
}

/// Cold resync discards DA/DSR so replay does not inject replies toward a PTY
/// (SPEC-VT-REPLY §5).
#[test]
fn t_chip1_resync_discards_replies_from_history() {
    let mut vt = engine();
    let resync = vt
        .resync_from_history(b"\x1b[6nRILL-RESYNC-MARK")
        .expect("resync");
    assert!(
        !vt.has_replies(),
        "resync_from_history must drain/discard replies"
    );
    let grid = vt.snapshot().expect("snapshot");
    assert!(
        grid.replies_dropped >= 1,
        "discarded replies are counted, not pretended sent"
    );
    let mut replay = engine();
    replay.feed(&resync).expect("replay");
    assert!(
        replay.take_replies().expect("take").is_empty(),
        "resync emit must not include DA/DSR answers"
    );
}
