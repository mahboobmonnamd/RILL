//! Daemon-plane Spike 0 gates: T-ATTACH, T-RESYNC, T-EXIT across detach.
//!
//! Definitions and required mutations: docs/TEST-CASES.md.
//!
//! These drive a real `Daemon` over a real `AF_UNIX` socket. They do **not**
//! close T-KILL, T-SPAWN, or T-NFR — socket-only tests never do (AGENTS.md §8).

use rill_attach::{Decoder, Frame, RefuseReason};
use rill_kernel::Winsize;
use rill_vt_types::{PodGrid, TerminalEmulation};
use rilld::{pump, Daemon};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use vt_engine::VtEngine;

fn temp_sock(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let dir = PathBuf::from(format!(
        "/tmp/r{}{}{}",
        std::process::id() % 10000,
        tag.chars().next().unwrap_or('x'),
        n
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("runtime dir");
    std::fs::set_permissions(&dir, std::os::unix::fs::PermissionsExt::from_mode(0o700)).ok();
    dir.join("a")
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
    let mut chip = VtEngine::new(80, 24).expect("chip");
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
    send(&mut gui, Frame::attach(1, None));
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
    send(&mut gui2, Frame::attach(2, None));
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
    send(&mut gui, Frame::attach(1, None));
    send(&mut gui, Frame::Credit(256 * 1024));
    pump(&mut daemon, Duration::from_millis(80)).ok();
    send(&mut gui, Frame::Data(b"printf 'GRID-A\\n'\n".to_vec()));
    pump(&mut daemon, Duration::from_millis(300)).ok();
    let mut d1 = Decoder::new();
    let first = grid_from(&recv_frames(&mut gui, &mut d1, Duration::from_millis(300)));
    drop(gui);
    pump(&mut daemon, Duration::from_millis(80)).ok();

    let mut gui2 = UnixStream::connect(&sock).expect("c2");
    send(&mut gui2, Frame::attach(2, None));
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
    send(&mut a, Frame::attach(1, None));
    pump(&mut daemon, Duration::from_millis(80)).ok();

    let mut b = UnixStream::connect(&sock).expect("b");
    send(&mut b, Frame::attach(2, None));
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
    send(&mut a, Frame::attach(1, None));
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
    send(&mut gui, Frame::attach(1, None));
    pump(&mut daemon, Duration::from_millis(50)).ok();
    drop(gui);

    // Child dies with nobody watching.
    pump(&mut daemon, Duration::from_millis(1200)).ok();

    let mut gui2 = UnixStream::connect(&sock).expect("c2");
    send(&mut gui2, Frame::attach(2, None));
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

/// T-ATTACHED-POLL: attached with an empty outbox must not busy-loop (`poll` timeout 0).
///
/// Required mutation: `RILL_MUTATE=spin_poll_while_attached` (feature `mutate`).
#[test]
fn t_attached_session_poll_does_not_busy_loop() {
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
    send(&mut gui, Frame::attach(1, None));
    send(&mut gui, Frame::Credit(256 * 1024));
    for _ in 0..30 {
        let _ = daemon.step(5);
    }
    assert!(
        daemon.step_timeout_ms() > 0,
        "attached poll with empty outbox busy-looped (timeout 0); burns a core at idle"
    );
}

/// T-PARTIAL-WRITE (quality Q1): a blocked attach socket must not replay a
/// DATA frame that was already partially written.
///
/// Oracle: child-emitted `RILL-UNIQ-<n>-END` tokens in decoded DATA. A 4s
/// cutoff can bisect `RILL-UNIQ-16073` into a line that looks like `160`;
/// only complete `-END` tokens count. Required mutation:
/// `RILL_MUTATE=replay_full_frame` (feature `mutate`) re-queues the whole
/// frame after `write_all`, including when `write_all` succeeded.
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
    send(&mut gui, Frame::attach(1, None));
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

// ----------------------------------------------------- T-GRAPH attach follow-on

/// Required mutation: `RILL_MUTATE=ignore_session_id`.
#[test]
fn t_attach_named_id_reaches_that_leaf() {
    let sock = temp_sock("named-leaf");
    let mut daemon = Daemon::bind(
        &sock,
        "/bin/sh",
        &["-c", "printf 'LEAF-DEFAULT\\n'; exec sleep 30"],
        Winsize::default(),
    )
    .expect("bind");
    let b = daemon
        .spawn_leaf(
            "/bin/sh",
            &["-c", "printf 'LEAF-B\\n'; exec sleep 30"],
            Winsize::default(),
        )
        .expect("spawn B");

    let mut gui = UnixStream::connect(&sock).expect("connect");
    send(&mut gui, Frame::attach(1, Some(b.as_u64())));
    send(&mut gui, Frame::Credit(64 * 1024));
    pump(&mut daemon, Duration::from_millis(400)).ok();
    let mut d = Decoder::new();
    let frames = recv_frames(&mut gui, &mut d, Duration::from_millis(400));
    let mut bytes = Vec::new();
    for f in &frames {
        if let Frame::Data(b) = f {
            bytes.extend_from_slice(b);
        }
    }
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains("LEAF-B"),
        "named attach did not receive that leaf's output, got {text:?}"
    );
    assert!(
        !text.contains("LEAF-DEFAULT"),
        "named attach mixed the default leaf into the stream, got {text:?}"
    );
}

#[test]
fn t_attach_unknown_id_is_refused() {
    let sock = temp_sock("unknown-id");
    let mut daemon = Daemon::bind(
        &sock,
        "/bin/sh",
        &["-c", "exec sleep 30"],
        Winsize::default(),
    )
    .expect("bind");
    let mut gui = UnixStream::connect(&sock).expect("connect");
    send(&mut gui, Frame::attach(1, Some(0xDEAD_BEEF)));
    pump(&mut daemon, Duration::from_millis(80)).ok();
    let mut d = Decoder::new();
    let frames = recv_frames(&mut gui, &mut d, Duration::from_millis(200));
    assert!(
        frames.iter().any(|f| matches!(
            f,
            Frame::Refused {
                reason: RefuseReason::Invalid
            }
        )),
        "unknown session id was not refused, got {frames:?}"
    );
}

/// Required mutation: `RILL_MUTATE=ignore_session_id`.
#[test]
fn t_attach_second_connection_to_other_id_is_accepted() {
    let sock = temp_sock("two-ids");
    let mut daemon = Daemon::bind(
        &sock,
        "/bin/sh",
        &["-c", "exec sleep 30"],
        Winsize::default(),
    )
    .expect("bind");
    let b = daemon
        .spawn_leaf("/bin/sh", &["-c", "exec sleep 30"], Winsize::default())
        .expect("spawn B");

    let mut a = UnixStream::connect(&sock).expect("a");
    send(&mut a, Frame::attach(1, None));
    pump(&mut daemon, Duration::from_millis(80)).ok();

    let mut gui_b = UnixStream::connect(&sock).expect("b");
    send(&mut gui_b, Frame::attach(1, Some(b.as_u64())));
    pump(&mut daemon, Duration::from_millis(80)).ok();
    let mut db = Decoder::new();
    let frames = recv_frames(&mut gui_b, &mut db, Duration::from_millis(200));
    assert!(
        !frames.iter().any(|f| matches!(f, Frame::Refused { .. })),
        "attach to a second id was refused, got {frames:?}"
    );
}

/// T-GRAPH-FLOOD. Oracle is B's child marker on B's socket while A is a
/// stalled `yes` flood. Required mutation: `RILL_MUTATE=starve_other_leaves`.
#[test]
fn t_graph_flood_on_one_leaf_does_not_drop_bytes_on_the_other() {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let marker = format!("RILL-GRAPH-FLOOD-B-{n}");
    let path = PathBuf::from(format!("/tmp/rill-graph-flood-b-{n}"));
    std::fs::write(&path, format!("{marker}\n")).expect("write B fixture");

    let sock = temp_sock("graph-flood");
    let mut daemon =
        Daemon::bind(&sock, "/bin/sh", &["-c", "exec yes"], Winsize::default()).expect("bind A");
    let cat = format!("cat {}; exec sleep 30", path.display());
    let b = daemon
        .spawn_leaf("/bin/sh", &["-c", &cat], Winsize::default())
        .expect("spawn B");

    let mut gui_a = UnixStream::connect(&sock).expect("a");
    send(&mut gui_a, Frame::attach(1, None));
    send(&mut gui_a, Frame::Credit(8 * 1024));

    let mut gui_b = UnixStream::connect(&sock).expect("b");
    send(&mut gui_b, Frame::attach(1, Some(b.as_u64())));
    send(&mut gui_b, Frame::Credit(64 * 1024));

    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        pump(&mut daemon, Duration::from_millis(50)).ok();
    }

    let mut da = Decoder::new();
    let a_frames = recv_frames(&mut gui_a, &mut da, Duration::from_millis(400));
    let mut a_bytes = Vec::new();
    for f in &a_frames {
        if let Frame::Data(b) = f {
            a_bytes.extend_from_slice(b);
        }
    }
    let y_count = a_bytes.iter().filter(|b| **b == b'y').count();
    assert!(
        y_count > 100,
        "leaf A never received `yes` output ({y_count} y's) — the flood did \
         not run, so this run is inconclusive (ADR 0011 D4)"
    );

    let mut db = Decoder::new();
    let frames = recv_frames(&mut gui_b, &mut db, Duration::from_millis(400));
    let mut bytes = Vec::new();
    for f in &frames {
        if let Frame::Data(b) = f {
            bytes.extend_from_slice(b);
        }
    }
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains(&marker),
        "flood on A dropped B's child marker, got {text:?}"
    );

    let _ = std::fs::remove_file(&path);
}

/// T-ATTACH-PROTO. Required mutation: `ignore_protocol_version`.
#[test]
fn t_attach_protocol_mismatch_is_refused() {
    let sock = temp_sock("proto");
    let mut daemon = Daemon::bind(&sock, "/bin/sleep", &["30"], Winsize::default()).expect("bind");
    let mut gui = UnixStream::connect(&sock).expect("connect");
    gui.set_nonblocking(true).ok();
    send(
        &mut gui,
        Frame::Attach {
            generation: 1,
            session_id: None,
            protocol: 99,
            observe: false,
        },
    );
    let _ = pump(&mut daemon, Duration::from_millis(80));
    let mut dec = Decoder::new();
    let frames = recv_frames(&mut gui, &mut dec, Duration::from_millis(200));
    assert!(
        frames.iter().any(|f| matches!(
            f,
            Frame::Refused {
                reason: RefuseReason::ProtocolMismatch
            }
        )),
        "mismatch was not refused, got {frames:?}"
    );
}

/// T-GRAPH-NESTED. Required mutation: `skip_nested_guard`.
#[test]
fn t_nested_rilld_bind_is_refused() {
    std::env::set_var("RILL_INSIDE", "1");
    let sock = temp_sock("nested");
    let err = Daemon::bind(&sock, "/bin/sleep", &["30"], Winsize::default());
    std::env::remove_var("RILL_INSIDE");
    assert!(
        matches!(err, Err(rilld::Error::NestedLaunch)),
        "nested bind succeeded"
    );
}

/// T-GRAPH-OBSERVE. Observer gets DATA; must not write the PTY.
/// Required mutation: `allow_observer_write`.
#[test]
fn t_observe_attach_receives_data_and_cannot_write() {
    let sock = temp_sock("observe");
    let marker = format!("RILL-OBS-{}", std::process::id());
    let path = PathBuf::from(format!("/tmp/{marker}"));
    std::fs::write(&path, &marker).expect("fixture");
    let mut daemon = Daemon::bind(
        &sock,
        "/bin/sh",
        &["-c", &format!("cat {}; exec cat", path.display())],
        Winsize::default(),
    )
    .expect("bind");
    let id = daemon.default_id().as_u64();

    let mut writer = UnixStream::connect(&sock).expect("w");
    writer.set_nonblocking(true).ok();
    send(&mut writer, Frame::attach(1, Some(id)));
    send(&mut writer, Frame::Credit(64 * 1024));
    let _ = pump(&mut daemon, Duration::from_millis(200));

    let mut observer = UnixStream::connect(&sock).expect("o");
    observer.set_nonblocking(true).ok();
    send(
        &mut observer,
        Frame::Attach {
            generation: 2,
            session_id: Some(id),
            protocol: rill_attach::PROTOCOL_VERSION,
            observe: true,
        },
    );
    send(&mut observer, Frame::Credit(64 * 1024));
    let _ = pump(&mut daemon, Duration::from_millis(200));

    send(
        &mut observer,
        Frame::Data(b"SHOULD-NOT-REACH-PTY\n".to_vec()),
    );
    let _ = pump(&mut daemon, Duration::from_millis(80));

    let mut dec = Decoder::new();
    let frames = recv_frames(&mut observer, &mut dec, Duration::from_millis(300));
    let mut bytes = Vec::new();
    for f in &frames {
        if let Frame::Data(b) = f {
            bytes.extend_from_slice(b);
        }
    }
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains(&marker),
        "observer did not see child output: {text:?}"
    );
    assert!(
        !text.contains("SHOULD-NOT-REACH-PTY"),
        "observer DATA was written through to the PTY"
    );
    let _ = std::fs::remove_file(&path);
}

/// T-GRAPH-KILL-N. Dropping the daemon must not kill either child.
#[test]
fn t_graph_dropping_the_daemon_does_not_kill_either_child() {
    let sock = temp_sock("drop-n");
    let mut daemon = Daemon::bind(
        &sock,
        "/bin/sh",
        &["-c", "exec sleep 60"],
        Winsize::default(),
    )
    .expect("bind");
    let a = daemon.child_pid();
    let b = daemon
        .spawn_leaf("/bin/sh", &["-c", "exec sleep 60"], Winsize::default())
        .expect("B");
    let pid_b = daemon.child_pid_of(b).expect("B pid");
    drop(daemon);
    std::thread::sleep(Duration::from_millis(80));
    assert!(
        unsafe { libc::kill(a as i32, 0) } == 0,
        "dropping daemon killed default child {a}"
    );
    assert!(
        unsafe { libc::kill(pid_b as i32, 0) } == 0,
        "dropping daemon killed leaf B {pid_b}"
    );
    unsafe {
        libc::kill(a as i32, libc::SIGKILL);
        libc::kill(pid_b as i32, libc::SIGKILL);
    }
}
