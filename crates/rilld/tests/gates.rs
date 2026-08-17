//! Daemon-plane Spike 0 gates: T-ATTACH, T-RESYNC, T-EXIT across detach.
//!
//! Definitions and required mutations: docs/TEST-CASES.md.
//!
//! These drive a real `Daemon` over a real `AF_UNIX` socket. They do **not**
//! close T-KILL, T-SPAWN, or T-NFR — socket-only tests never do (AGENTS.md §8).

use rill_attach::{Decoder, Frame, RefuseReason};
use rill_chip0::{Chip0, PodGrid, TerminalEmulation};
use rill_kernel::Winsize;
use rilld::{pump, Daemon};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn temp_sock(tag: &str) -> PathBuf {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    PathBuf::from(format!("/tmp/rill-{tag}-{n}.sock"))
}

fn send(stream: &mut UnixStream, frame: Frame) {
    let bytes = frame.encode().expect("encode");
    stream.write_all(&bytes).expect("write");
}

fn recv_frames(stream: &mut UnixStream, decoder: &mut Decoder, timeout: Duration) -> Vec<Frame> {
    stream.set_nonblocking(true).ok();
    let mut all = Vec::new();
    let start = Instant::now();
    let mut buf = [0u8; 65536];
    while start.elapsed() < timeout {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => all.extend(decoder.push(&buf[..n]).expect("decode")),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err(_) => break,
        }
    }
    all
}

fn grid_from(frames: &[Frame]) -> PodGrid {
    let mut bytes = Vec::new();
    for f in frames {
        if let Frame::Data(b) = f {
            bytes.extend_from_slice(b);
        }
    }
    let mut chip = Chip0::new(80, 24).expect("chip");
    chip.feed(&bytes).expect("feed");
    chip.snapshot().expect("snapshot")
}

/// Cell by cell, not a substring search over concatenated codepoints. The old
/// comparison would have accepted two grids with the same characters in
/// different places, different colours, or a different cursor.
fn assert_grids_match(a: &PodGrid, b: &PodGrid) {
    assert_eq!((a.cols, a.rows), (b.cols, b.rows), "geometry diverged");
    for r in 0..a.rows {
        for c in 0..a.cols {
            let (x, y) = (a.cell(c, r), b.cell(c, r));
            assert_eq!(
                (
                    x.map(|v| v.codepoint),
                    x.map(|v| v.fg),
                    x.map(|v| v.bg),
                    x.map(|v| v.attrs)
                ),
                (
                    y.map(|v| v.codepoint),
                    y.map(|v| v.fg),
                    y.map(|v| v.bg),
                    y.map(|v| v.attrs)
                ),
                "cell ({c},{r}) diverged after reattach"
            );
        }
    }
    assert_eq!(
        (a.cursor_col, a.cursor_row, a.cursor_visible),
        (b.cursor_col, b.cursor_row, b.cursor_visible),
        "cursor diverged after reattach"
    );
}

fn text_of(g: &PodGrid) -> String {
    (0..g.rows)
        .flat_map(|r| {
            (0..g.cols).filter_map(move |c| g.cell(c, r).and_then(|x| char::from_u32(x.codepoint)))
        })
        .collect()
}

// --------------------------------------------------------------------- T-RESYNC

#[test]
fn t_resync_reopen_over_a_live_shell_restores_the_screen() {
    let sock = temp_sock("resync");
    let mut daemon = Daemon::bind(&sock, "/bin/sh", &[], Winsize::default()).expect("bind");

    let mut gui = UnixStream::connect(&sock).expect("connect");
    pump(&mut daemon, Duration::from_millis(50)).ok();
    send(&mut gui, Frame::Attach { generation: 1 });
    send(&mut gui, Frame::Credit(256 * 1024));
    pump(&mut daemon, Duration::from_millis(80)).ok();
    send(
        &mut gui,
        Frame::Data(b"printf 'RILL-MARK-42\\n'\n".to_vec()),
    );
    pump(&mut daemon, Duration::from_millis(300)).ok();

    let mut d1 = Decoder::new();
    let first = recv_frames(&mut gui, &mut d1, Duration::from_millis(300));
    let before = grid_from(&first);
    drop(gui);
    pump(&mut daemon, Duration::from_millis(80)).ok();

    let mut gui2 = UnixStream::connect(&sock).expect("reconnect");
    send(&mut gui2, Frame::Attach { generation: 2 });
    send(&mut gui2, Frame::Credit(256 * 1024));
    pump(&mut daemon, Duration::from_millis(300)).ok();
    let mut d2 = Decoder::new();
    let second = recv_frames(&mut gui2, &mut d2, Duration::from_millis(400));
    let after = grid_from(&second);

    assert!(
        text_of(&after).contains("RILL-MARK-42"),
        "reopen was blank over a live process: {:?}",
        text_of(&after)
    );
    assert!(
        before.cursor_row == after.cursor_row,
        "cursor row moved across resync: {} -> {}",
        before.cursor_row,
        after.cursor_row
    );

    // Resync is a cold path: keystrokes after reattach must not run it again.
    // Attach→detach→attach is two resyncs; asserting `== 1` here was unsatisfiable.
    let after_reattach = daemon.resync_count();
    assert!(
        after_reattach >= 1,
        "reattach did not resync: count={after_reattach}"
    );
    for _ in 0..100 {
        send(&mut gui2, Frame::Data(b"x".to_vec()));
    }
    pump(&mut daemon, Duration::from_millis(200)).ok();
    assert_eq!(
        daemon.resync_count(),
        after_reattach,
        "resync ran on a keystroke — it is on the warm path (FR-RESYNC)"
    );

    // The window must not be able to tell resync bytes from live bytes.
    assert!(
        second.iter().all(|f| matches!(f, Frame::Data(_))),
        "resync arrived with a distinguishing frame type"
    );
}

// --------------------------------------------------------------------- T-ATTACH

#[test]
fn t_attach_detach_attach_grids_do_not_diverge() {
    let sock = temp_sock("attach");
    let mut daemon = Daemon::bind(&sock, "/bin/sh", &[], Winsize::default()).expect("bind");

    let mut gui = UnixStream::connect(&sock).expect("c1");
    send(&mut gui, Frame::Attach { generation: 1 });
    send(&mut gui, Frame::Credit(256 * 1024));
    pump(&mut daemon, Duration::from_millis(80)).ok();
    send(&mut gui, Frame::Data(b"printf 'GRID-A\\n'\n".to_vec()));
    pump(&mut daemon, Duration::from_millis(300)).ok();
    let mut d1 = Decoder::new();
    let first = grid_from(&recv_frames(&mut gui, &mut d1, Duration::from_millis(300)));
    drop(gui);
    pump(&mut daemon, Duration::from_millis(80)).ok();

    let mut gui2 = UnixStream::connect(&sock).expect("c2");
    send(&mut gui2, Frame::Attach { generation: 2 });
    send(&mut gui2, Frame::Credit(256 * 1024));
    pump(&mut daemon, Duration::from_millis(300)).ok();
    let mut d2 = Decoder::new();
    let second = grid_from(&recv_frames(&mut gui2, &mut d2, Duration::from_millis(400)));

    assert!(text_of(&first).contains("GRID-A"));
    assert_grids_match(&first, &second);
}

#[test]
fn t_attach_second_attach_is_refused() {
    let sock = temp_sock("refuse");
    let mut daemon = Daemon::bind(
        &sock,
        "/bin/sh",
        &["-c", "exec sleep 30"],
        Winsize::default(),
    )
    .expect("bind");

    let mut a = UnixStream::connect(&sock).expect("a");
    send(&mut a, Frame::Attach { generation: 1 });
    pump(&mut daemon, Duration::from_millis(80)).ok();

    let mut b = UnixStream::connect(&sock).expect("b");
    pump(&mut daemon, Duration::from_millis(80)).ok();
    let mut db = Decoder::new();
    let frames = recv_frames(&mut b, &mut db, Duration::from_millis(200));
    assert!(
        frames.iter().any(|f| matches!(
            f,
            Frame::Refused {
                reason: RefuseReason::AlreadyAttached
            }
        )),
        "second connection was not refused (FR-ONE), got {frames:?}"
    );
}

/// **Fails against `main` with no mutation applied.**
///
/// A connection that never sends ATTACH used to be displaceable, because the
/// refusal also required `session.attached()`. FR-ONE was bypassable by simply
/// not attaching (audit S3-6).
///
/// Required mutation: `RILL_MUTATE=accept_replaces_client`.
#[test]
fn t_attach_a_bare_connection_cannot_displace_the_attached_client() {
    let sock = temp_sock("displace");
    let mut daemon = Daemon::bind(&sock, "/bin/sh", &[], Winsize::default()).expect("bind");

    let mut a = UnixStream::connect(&sock).expect("a");
    send(&mut a, Frame::Attach { generation: 1 });
    send(&mut a, Frame::Credit(256 * 1024));
    pump(&mut daemon, Duration::from_millis(150)).ok();

    // Connects, says nothing.
    let _intruder = UnixStream::connect(&sock).expect("intruder");
    pump(&mut daemon, Duration::from_millis(150)).ok();

    // The original client must still be the one being served.
    send(&mut a, Frame::Data(b"printf 'STILL-MINE\\n'\n".to_vec()));
    pump(&mut daemon, Duration::from_millis(400)).ok();
    let mut da = Decoder::new();
    let got = grid_from(&recv_frames(&mut a, &mut da, Duration::from_millis(400)));
    assert!(
        text_of(&got).contains("STILL-MINE"),
        "a connection that never attached displaced the attached client (FR-ONE)"
    );
}

// ----------------------------------------------------------------------- T-EXIT

/// **Fails against `main` with no mutation applied.** See
/// docs/SPIKE-0-AUDIT.md S3-2.
///
/// Required mutation: `RILL_MUTATE=clear_outbound_on_detach`.
#[test]
fn t_exit_across_detach_is_delivered_to_the_reattaching_client() {
    let sock = temp_sock("exit-detach");
    let mut daemon = Daemon::bind(
        &sock,
        "/bin/sh",
        &["-c", "sleep 0.4; exit 5"],
        Winsize::default(),
    )
    .expect("bind");

    let gui = UnixStream::connect(&sock).expect("c1");
    pump(&mut daemon, Duration::from_millis(50)).ok();
    let mut gui = gui;
    send(&mut gui, Frame::Attach { generation: 1 });
    pump(&mut daemon, Duration::from_millis(50)).ok();
    drop(gui);

    // Child dies with nobody watching.
    pump(&mut daemon, Duration::from_millis(1200)).ok();

    let mut gui2 = UnixStream::connect(&sock).expect("c2");
    send(&mut gui2, Frame::Attach { generation: 2 });
    send(&mut gui2, Frame::Credit(64 * 1024));
    pump(&mut daemon, Duration::from_millis(300)).ok();
    let mut d = Decoder::new();
    let frames = recv_frames(&mut gui2, &mut d, Duration::from_millis(400));

    let exit = frames.iter().find_map(|f| match f {
        Frame::Exit { status } => Some(*status),
        _ => None,
    });
    let status = exit.expect(
        "reattaching client was never told the child is dead — the reopened \
         window paints a live cursor over a corpse (FR-EXIT)",
    );
    assert_eq!((status >> 8) & 0xff, 5, "exit status lost: {status:#x}");
}

// ------------------------------------------------------------------------ bind

#[test]
fn t_bind_does_not_steal_a_live_socket() {
    let sock = temp_sock("steal");
    let first = Daemon::bind(
        &sock,
        "/bin/sh",
        &["-c", "exec sleep 30"],
        Winsize::default(),
    )
    .expect("first bind");
    let pid = first.child_pid();

    let err = Daemon::bind(
        &sock,
        "/bin/sh",
        &["-c", "exec sleep 30"],
        Winsize::default(),
    )
    .err()
    .expect("second bind must fail");
    assert!(
        err.to_string().contains("already running"),
        "live socket was stolen: {err}"
    );
    assert!(
        unsafe { libc::kill(pid as i32, 0) } == 0,
        "the rejected bind killed the first daemon's child"
    );
    drop(first);
}

/// T-NFR regression lock: a live attach must not `poll` with a sleep.
///
/// Required mutation: `RILL_MUTATE=idle_poll_while_attached` (feature `mutate`).
/// Packaged hid p95 is the vsync oracle; this asserts the timeout `run` uses.
#[test]
fn t_attached_session_poll_does_not_sleep() {
    let sock = temp_sock("nfr-poll");
    let mut daemon = Daemon::bind(
        &sock,
        "/bin/sh",
        &["-c", "exec sleep 30"],
        Winsize::default(),
    )
    .expect("bind");
    assert!(
        daemon.step_timeout_ms() > 0,
        "idle daemon must not busy-loop (Q5)"
    );

    let mut gui = UnixStream::connect(&sock).expect("connect");
    send(&mut gui, Frame::Attach { generation: 1 });
    send(&mut gui, Frame::Credit(256 * 1024));
    for _ in 0..30 {
        let _ = daemon.step(5);
        if daemon.step_timeout_ms() == 0 {
            break;
        }
    }
    assert_eq!(
        daemon.step_timeout_ms(),
        0,
        "attached poll slept; T-NFR hid p95 missed vsync after Q5"
    );
}

/// T-PARTIAL-WRITE (quality Q1): a blocked attach socket must not replay a
/// DATA frame that was already partially written.
///
/// Oracle: child-emitted `RILL-UNIQ-<n>-END` tokens in decoded DATA. A 4s
/// cutoff can bisect `RILL-UNIQ-16073` into a line that looks like `160`;
/// only complete `-END` tokens count. Required mutation:
/// `RILL_MUTATE=replay_full_frame` (feature `mutate`).
#[test]
fn t_outbound_partial_write_does_not_replay_a_frame() {
    use std::os::fd::AsRawFd;

    std::env::set_var("RILL_TEST_TINY_SNDBUF", "1");
    struct ClearTiny;
    impl Drop for ClearTiny {
        fn drop(&mut self) {
            std::env::remove_var("RILL_TEST_TINY_SNDBUF");
        }
    }
    let _clear = ClearTiny;
    let sock = temp_sock("partial-write");
    let mut daemon = Daemon::bind(
        &sock,
        "/bin/sh",
        &[
            "-c",
            "sleep 0.2; seq 0 20000 | awk '{printf \"RILL-UNIQ-%d-END\\n\", $1}'",
        ],
        Winsize::default(),
    )
    .expect("bind");
    let mut gui = UnixStream::connect(&sock).expect("connect");
    let small: libc::c_int = 2048;
    unsafe {
        libc::setsockopt(
            gui.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_RCVBUF,
            &small as *const _ as *const libc::c_void,
            std::mem::size_of_val(&small) as libc::socklen_t,
        );
    }
    send(&mut gui, Frame::Attach { generation: 1 });
    send(&mut gui, Frame::Credit(256 * 1024));

    // Do not drain while the child is flooding. A test that reads every step
    // never fills the socket, so `write_all` never WouldBlock and the
    // required mutation is a no-op.
    let flood_until = Instant::now() + Duration::from_millis(800);
    while Instant::now() < flood_until {
        let _ = daemon.step(5);
    }

    let mut decoder = Decoder::new();
    let mut bytes = Vec::new();
    let start = Instant::now();
    let mut buf = [0u8; 65536];
    gui.set_nonblocking(true).ok();
    while start.elapsed() < Duration::from_secs(4) {
        let _ = daemon.step(5);
        match gui.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                for f in decoder.push(&buf[..n]).expect("decode") {
                    if let Frame::Data(b) = f {
                        bytes.extend_from_slice(&b);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => break,
        }
    }

    let text = String::from_utf8_lossy(&bytes);
    let mut seen = std::collections::BTreeSet::new();
    let mut dup = None;
    for line in text.lines() {
        let Some(rest) = line.strip_prefix("RILL-UNIQ-") else {
            continue;
        };
        let Some(n) = rest.strip_suffix("-END") else {
            continue;
        };
        if n.is_empty() || !n.bytes().all(|b| b.is_ascii_digit()) {
            continue;
        }
        if !seen.insert(n.to_string()) {
            dup = Some(n.to_string());
            break;
        }
    }
    assert!(
        dup.is_none(),
        "DATA frame was replayed; marker RILL-UNIQ-{}-END appeared twice",
        dup.unwrap_or_default()
    );
    assert!(
        seen.len() > 50,
        "too few unique markers to exercise backpressure ({})",
        seen.len()
    );
}
