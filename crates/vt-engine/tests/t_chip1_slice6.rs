//! Slice 6 T-CHIP1-REPLY.
//!
//! Authority: ADR 0022, SPEC-VT-REPLY. Chip 0 stays live. Replies are parsed
//! from the drained buffer; the test does not prepend the expected CSI.

use rill_vt_types::TerminalEmulation;
use vt_engine::VtEngine;

fn engine(cols: u16, rows: u16) -> VtEngine {
    VtEngine::new(cols, rows).expect("vt-engine")
}

struct Csi {
    intermediates: Vec<u8>,
    params: Vec<u16>,
    final_byte: u8,
}

/// Parse concatenated CSI replies. Oracle is the parsed fields, not a byte
/// constant the engine could have copied from the test.
fn parse_csi_stream(bytes: &[u8]) -> Vec<Csi> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        assert_eq!(
            bytes.get(i),
            Some(&0x1b),
            "reply must start with ESC at {i}"
        );
        i += 1;
        assert_eq!(bytes.get(i), Some(&b'['), "CSI introducer at {i}");
        i += 1;
        let mut intermediates = Vec::new();
        while i < bytes.len() && (0x20..=0x2f).contains(&bytes[i]) {
            intermediates.push(bytes[i]);
            i += 1;
        }
        // Private markers `<=>?` sit in the param slot in ECMA-48.
        if i < bytes.len() && (0x3c..=0x3f).contains(&bytes[i]) {
            intermediates.push(bytes[i]);
            i += 1;
        }
        let mut params = Vec::new();
        let mut acc: Option<u16> = None;
        while i < bytes.len() && bytes[i] != b';' && !(0x40..=0x7e).contains(&bytes[i]) {
            let b = bytes[i];
            assert!(
                b.is_ascii_digit(),
                "expected CSI param digit, got {b:#x} at {i}"
            );
            let d = u16::from(b - b'0');
            acc = Some(acc.unwrap_or(0).saturating_mul(10).saturating_add(d));
            i += 1;
        }
        if let Some(n) = acc {
            params.push(n);
            acc = None;
        }
        while i < bytes.len() && bytes[i] == b';' {
            i += 1;
            while i < bytes.len() && bytes[i].is_ascii_digit() {
                let d = u16::from(bytes[i] - b'0');
                acc = Some(acc.unwrap_or(0).saturating_mul(10).saturating_add(d));
                i += 1;
            }
            params.push(acc.take().unwrap_or(0));
        }
        let final_byte = *bytes.get(i).expect("CSI final");
        assert!(
            (0x40..=0x7e).contains(&final_byte),
            "not a CSI final: {final_byte:#x}"
        );
        i += 1;
        out.push(Csi {
            intermediates,
            params,
            final_byte,
        });
    }
    out
}

/// T-CHIP1-REPLY — DA and DSR are answered.
///
/// Bug: SPEC-CHIP1 §3 required answering DA/DSR while the §2 API had no
/// channel for a reply, so a vim that probes would hang (SPIKE-VT Result 7).
///
/// Required mutation: `RILL_MUTATE=no_reply`.
#[test]
fn t_chip1_reply_da_and_dsr_are_answered() {
    let mut vt = engine(80, 24);
    vt.feed(b"\x1b[5;3H").expect("CUP");
    let grid = vt.snapshot().expect("snapshot after CUP");
    let want_row = grid.cursor_row.saturating_add(1);
    let want_col = grid.cursor_col.saturating_add(1);
    assert_eq!((want_row, want_col), (5, 3), "CUP 5;3 is the DSR fixture");

    vt.feed(b"\x1b[6n").expect("DSR");
    assert!(vt.has_replies(), "DSR must enqueue a reply");
    let dsr = vt.take_replies().expect("take DSR");
    let parsed = parse_csi_stream(&dsr);
    assert_eq!(parsed.len(), 1, "one DSR reply");
    assert_eq!(parsed[0].final_byte, b'R', "DSR CPR final is R");
    assert!(
        parsed[0].intermediates.is_empty(),
        "CPR has no private marker"
    );
    assert_eq!(
        parsed[0].params.as_slice(),
        [want_row, want_col],
        "DSR must report snapshot cursor as 1-based row;col"
    );
    let again = vt.take_replies().expect("second take");
    assert!(
        again.is_empty(),
        "take_replies drains; a second call is empty"
    );
    assert!(!vt.has_replies());

    vt.feed(b"\x1b[c").expect("primary DA");
    let da = vt.take_replies().expect("take DA");
    let parsed = parse_csi_stream(&da);
    assert_eq!(parsed.len(), 1, "one primary DA reply");
    assert_eq!(parsed[0].final_byte, b'c');
    assert_eq!(
        parsed[0].intermediates.as_slice(),
        b"?",
        "primary DA is CSI ? … c"
    );
    assert_eq!(
        parsed[0].params.as_slice(),
        [6],
        "v0 primary DA is VT102 class (CSI ? 6 c)"
    );

    vt.feed(b"\x1b[0c").expect("primary DA 0");
    let da0 = parse_csi_stream(&vt.take_replies().expect("DA 0"));
    assert_eq!(da0[0].intermediates.as_slice(), b"?");
    assert_eq!(da0[0].params.as_slice(), [6]);

    vt.feed(b"\x1b[>c").expect("secondary DA");
    let sec = parse_csi_stream(&vt.take_replies().expect("secondary DA"));
    assert_eq!(sec[0].intermediates.as_slice(), b">");
    assert_eq!(sec[0].final_byte, b'c');
    assert_eq!(
        sec[0].params.as_slice(),
        [0, 0, 0],
        "secondary DA is CSI > 0 ; 0 ; 0 c"
    );

    vt.feed(b"\x1b[5n").expect("DSR status");
    let status = parse_csi_stream(&vt.take_replies().expect("status"));
    assert_eq!(status[0].final_byte, b'n');
    assert_eq!(
        status[0].params.as_slice(),
        [0],
        "MAY answer CSI 5 n with 0 n"
    );

    vt.feed(b"\x1b[15n").expect("unknown DSR");
    assert!(
        vt.take_replies().expect("unknown").is_empty(),
        "unknown queries are consumed and not answered"
    );
}

/// T-CHIP1-REPLY — DSR matches snapshot on a deferred wrap (SPEC-VT-REPLY §3).
#[test]
fn t_chip1_reply_dsr_does_not_pre_resolve_pending_wrap() {
    let mut vt = engine(10, 6);
    vt.feed(b"0123456789").expect("fill last column");
    let grid = vt.snapshot().expect("snapshot");
    assert_eq!(grid.cursor_col, 9, "still on the last column");
    assert_eq!(grid.cursor_row, 0);
    vt.feed(b"\x1b[6n").expect("DSR");
    let parsed = parse_csi_stream(&vt.take_replies().expect("DSR"));
    assert_eq!(
        parsed[0].params.as_slice(),
        [grid.cursor_row + 1, grid.cursor_col + 1],
        "DSR must not wrap to 2;1 while snapshot still shows column 10"
    );
}

/// T-CHIP1-REPLY — overflow drops new replies and counts them.
///
/// Required mutation: `RILL_MUTATE=unbounded_replies`.
#[test]
fn t_chip1_reply_overflow_drops_and_counts() {
    let mut vt = engine(80, 24);
    // Home DSR is ESC [ 1 ; 1 R (6 bytes). 200 queries is > 1024 bytes.
    for _ in 0..200 {
        vt.feed(b"\x1b[6n").expect("spam DSR");
    }
    let grid = vt.snapshot().expect("snapshot");
    assert!(
        grid.replies_dropped > 0,
        "a full 1024-byte buffer must drop later replies (ADR 0022 D3)"
    );
    let drained = vt.take_replies().expect("drain");
    assert!(
        drained.len() <= 1024,
        "reply buffer must not grow past 1024, got {}",
        drained.len()
    );
    assert!(!drained.is_empty(), "already-buffered bytes stay");
    let first = &parse_csi_stream(&drained)[0];
    assert_eq!(first.final_byte, b'R');
    assert_eq!(
        first.params.as_slice(),
        [1, 1],
        "first buffered DSR is origin"
    );
}
