//! #313 worker offset, checkpoint store, and recovery export.
//!
//! Session is the worker failure boundary in this slice. Packaged daemon
//! process-split remains Red (SPEC-RUNTIME-SUPERVISION §7).

use rill_attach::Frame;
use rill_kernel::{
    Discipline, Error, Session, StoredCheckpoint, Winsize, CHECKPOINT_FORMAT_VERSION,
};
use std::time::{Duration, Instant};
use vt_engine::{TerminalEmulation, VtEngine};

fn stored_hash(blob: &[u8]) -> u64 {
    let mut raw = [0u8; 8];
    raw.copy_from_slice(&blob[14..22]);
    u64::from_le_bytes(raw)
}

fn pump_marker(session: &mut Session, marker: &[u8], timeout: Duration) -> Vec<u8> {
    let mut acc = Vec::new();
    let start = Instant::now();
    let _ = session.on_frame(Frame::Credit(64 * 1024));
    while start.elapsed() < timeout {
        let _ = session.poll_child();
        let consumed = session.on_pty_readable().unwrap_or(0);
        while let Some(f) = session.pop_outbound() {
            if let Frame::Data(b) = f {
                acc.extend_from_slice(&b);
            }
        }
        if consumed > 0 {
            let _ = session.on_frame(Frame::Credit(consumed as u32));
        }
        if acc.windows(marker.len()).any(|w| w == marker) {
            break;
        }
        if consumed == 0 {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    acc
}

fn row0(vt: &mut VtEngine) -> String {
    let g = vt.snapshot().expect("snapshot");
    (0..g.cols)
        .filter_map(|c| g.cell(c, 0).and_then(|x| char::from_u32(x.codepoint)))
        .collect()
}

/// T-RUNTIME-DAEMON-STATE — output while the control handle is absent stays
/// host-authoritative; recovery matches an independent continuous VT.
///
/// Required mutation: `RILL_MUTATE=blank_core`.
#[test]
fn t_runtime_daemon_state_recovery_matches_independent_vt() {
    let mut session = Session::spawn_with(
        "/bin/sh",
        &[
            "-c",
            "printf 'CK1-AAAA\\n'; sleep 0.2; printf 'CK1-BBBB\\n'",
        ],
        Winsize::default(),
        Discipline::Raw,
    )
    .expect("spawn");
    session.on_frame(Frame::attach(1, None)).expect("attach");
    let pid = session.child_pid();
    assert!(pid > 1);

    let first = pump_marker(&mut session, b"CK1-AAAA", Duration::from_secs(3));
    assert!(first.windows(8).any(|w| w == b"CK1-AAAA"));

    let mut host = VtEngine::new(80, 24).expect("host vt");
    host.feed(&session.history()).expect("feed first");
    let blob = host
        .export_checkpoint(session.end_offset())
        .expect("export");
    session
        .install_checkpoint(StoredCheckpoint::new(
            session.end_offset(),
            stored_hash(&blob),
            blob,
        ))
        .expect("install");

    let _ = pump_marker(&mut session, b"CK1-BBBB", Duration::from_secs(3));
    assert!(session.history().windows(8).any(|w| w == b"CK1-BBBB"));
    let mut want_vt = VtEngine::new(80, 24).expect("want");
    want_vt.feed(&session.history()).expect("want feed");
    let want = row0(&mut want_vt);

    assert_eq!(
        session.child_pid(),
        pid,
        "worker still owns the original child"
    );

    let (cp, deltas) = session.recovery().expect("recovery");
    let mut recovered = VtEngine::new(80, 24).expect("recovered");
    recovered.import_checkpoint(&cp.blob).expect("import");
    recovered.feed(&deltas).expect("deltas");
    assert_eq!(row0(&mut recovered), want);
    assert!(want.contains("CK1-BBBB") || row0(&mut recovered).contains("CK1-AAAA"));
}

/// T-RUNTIME-UPDATE-COMPAT — unknown checkpoint format is refused.
///
/// Required mutation: `RILL_MUTATE=accept_incompatible_checkpoint`.
#[test]
fn t_runtime_update_compat_refuses_unknown_format() {
    let mut session = Session::spawn_with(
        "/bin/sh",
        &["-c", "printf 'x\\n'"],
        Winsize::default(),
        Discipline::Raw,
    )
    .expect("spawn");
    let bad = StoredCheckpoint {
        format_version: CHECKPOINT_FORMAT_VERSION.saturating_add(9),
        ending_offset: 0,
        hash: 0,
        blob: vec![1, 2, 3],
    };
    let err = session
        .install_checkpoint(bad)
        .expect_err("incompatible format");
    assert!(matches!(err, Error::IncompatibleCheckpoint));
}
