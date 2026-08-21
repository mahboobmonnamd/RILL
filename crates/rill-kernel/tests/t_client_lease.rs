//! T-CLIENT-LEASE-ATOMIC and T-CLIENT-LEASE-EXPIRY-DETACH (#322).
//!
//! Required mutations: `both_leases_valid`, `expiry_terminates`.

use rill_kernel::{Discipline, Session, Winsize};
use std::time::Duration;

fn alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

/// Racing takeovers keep one generation; only that nonce reaches the PTY.
#[test]
fn t_client_lease_atomic_one_generation_one_nonce() {
    let mut session = Session::spawn_with(
        "/bin/sh",
        &["-c", "exec cat"],
        Winsize::default(),
        Discipline::Raw,
    )
    .expect("spawn");
    let g1 = session.take_input_lease(0xA1A1);
    let g2 = session.take_input_lease(0xB2B2);
    assert_ne!(g1, g2, "takeover did not advance generation");
    assert_eq!(session.lease_generation(), Some(g2));

    session
        .input_with_lease(0xA1A1, b"LOSER-NONCE\n".to_vec())
        .expect("loser");
    session
        .input_with_lease(0xB2B2, b"WINNER-NONCE\n".to_vec())
        .expect("winner");
    let _ = session.on_frame(rill_attach::Frame::Credit(64 * 1024));
    std::thread::sleep(Duration::from_millis(80));
    let _ = session.on_pty_readable();
    let hist = session.history();
    let text = String::from_utf8_lossy(&hist);
    assert!(
        text.contains("WINNER-NONCE"),
        "winning nonce never reached PTY: {text:?}"
    );
    assert!(
        !text.contains("LOSER-NONCE"),
        "losing nonce was written through: {text:?}"
    );
}

/// Expiry drops input, not the process. Another client can take the lease.
#[test]
fn t_client_lease_expiry_detaches_input_not_the_process() {
    std::env::set_var("RILL_LEASE_GRACE_MS", "30");
    let mut session = Session::spawn_with(
        "/bin/sh",
        &["-c", "exec cat"],
        Winsize::default(),
        Discipline::Raw,
    )
    .expect("spawn");
    let pid = session.child_pid();
    let _ = session.take_input_lease(7);
    std::thread::sleep(Duration::from_millis(80));
    session.expire_input_lease_if_due();
    session
        .input_with_lease(7, b"AFTER-EXPIRY\n".to_vec())
        .expect("expired write");
    let _ = session.on_frame(rill_attach::Frame::Credit(64 * 1024));
    std::thread::sleep(Duration::from_millis(40));
    let _ = session.on_pty_readable();
    assert!(alive(pid), "expiry terminated child {pid}");
    let hist = session.history();
    assert!(
        !hist.windows(12).any(|w| w == b"AFTER-EXPIRY"),
        "expired lease still wrote PTY"
    );
    let g = session.take_input_lease(9);
    assert!(g > 0);
    session
        .input_with_lease(9, b"AFTER-RETAKE\n".to_vec())
        .expect("retake");
    let _ = session.on_frame(rill_attach::Frame::Credit(64 * 1024));
    std::thread::sleep(Duration::from_millis(80));
    let _ = session.on_pty_readable();
    let hist = session.history();
    let text = String::from_utf8_lossy(&hist);
    assert!(
        text.contains("AFTER-RETAKE"),
        "new lease holder did not reach PTY: {text:?}"
    );
    assert!(alive(pid), "retake terminated child {pid}");
    std::env::remove_var("RILL_LEASE_GRACE_MS");
}
