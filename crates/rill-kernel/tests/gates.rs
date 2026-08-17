//! Kernel-plane Spike 0 gates: T-BYTES, T-DROP, T-RESIZE, T-EXIT.
//!
//! Definitions, oracles, and required mutations: docs/TEST-CASES.md.
//! Every test here asserts on something the code under test did not hand it
//! (ADR 0002 D4).

use rill_attach::{Frame, RefuseReason};
use rill_kernel::{Discipline, Error, IoEvent, Session, Winsize};
use std::time::{Duration, Instant};

fn tmp_path(tag: &str) -> std::path::PathBuf {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::path::PathBuf::from(format!("/tmp/rill-{tag}-{n}"))
}

/// Pump the session until `pred` is satisfied or the deadline passes.
/// Credit is replenished as it is consumed, mirroring the real client policy —
/// never granted infinitely, which is what hid the backpressure path before.
fn pump_until(
    session: &mut Session,
    window: u32,
    timeout: Duration,
    mut pred: impl FnMut(&[u8]) -> bool,
) -> Vec<u8> {
    let mut acc = Vec::new();
    let start = Instant::now();
    let _ = session.on_frame(Frame::Credit(window));
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
        if pred(&acc) {
            break;
        }
        if consumed == 0 {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    acc
}

// ---------------------------------------------------------------------- T-BYTES

/// The child *emits* the bytes and the PTY is in raw mode, so the line
/// discipline cannot rewrite the stream. The old version wrote the fixture as
/// PTY *input* in canonical mode with ECHO, so what it measured was the tty
/// echo, and it would have broken on any fixture containing 0x03 or 0x7f
/// (audit S2-2).
#[test]
fn t_bytes_child_output_reaches_history_byte_identical() {
    for (name, fixture) in fixtures() {
        let path = tmp_path(&format!("bytes-{name}"));
        std::fs::write(&path, &fixture).expect("write fixture");

        let arg = format!("cat {}", path.display());
        let mut session = Session::spawn_with(
            "/bin/sh",
            &["-c", &arg],
            Winsize::default(),
            Discipline::Raw,
        )
        .expect("spawn cat");
        session
            .on_frame(Frame::Attach { generation: 1 })
            .expect("attach");

        let want = fixture.clone();
        let got = pump_until(&mut session, 64 * 1024, Duration::from_secs(3), |acc| {
            acc.windows(want.len()).any(|w| w == want.as_slice())
        });

        assert!(
            got.windows(fixture.len()).any(|w| w == fixture.as_slice()),
            "{name}: child output did not reach the attach stream byte-identical.\n  \
             want {fixture:02x?}\n  got  {got:02x?}"
        );
        assert!(
            session
                .history()
                .windows(fixture.len())
                .any(|w| w == fixture.as_slice()),
            "{name}: history did not retain the original bytes"
        );
        if !fixture.windows(3).any(|w| w == [0xef, 0xbf, 0xbd]) {
            assert!(
                !got.windows(3).any(|w| w == [0xef, 0xbf, 0xbd]),
                "{name}: U+FFFD appeared in a stream that never contained it"
            );
        }

        let _ = session.terminate();
        let _ = std::fs::remove_file(&path);
    }
}

fn fixtures() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        ("lone_continuation", vec![0x80, 0x41]),
        ("truncated_3byte", vec![0xe2, 0x82, 0x41]),
        ("overlong_slash", vec![0xc0, 0xaf]),
        ("lone_surrogate", vec![0xed, 0xa0, 0x80]),
        ("bom_then_high", vec![0xff, 0xfe, 0x80, 0x41]),
    ]
}

// ----------------------------------------------------------------------- T-DROP

/// Unbounded flood, finite credit, then interrupt, then keep working.
///
/// Three properties the old test could not observe: the kernel actually stalls
/// its reads (backpressure exists), no numbered line goes missing (nothing is
/// dropped), and the shell is usable afterwards (`^C` was really delivered).
/// The old test granted `Credit(u32::MAX)` every iteration and asserted that
/// `head -n 20000` produces 20000 lines (audit S2-1).
#[test]
fn t_drop_flood_then_interrupt_loses_no_bytes_and_leaves_a_usable_shell() {
    let mut session = Session::spawn_with(
        "/bin/sh",
        &["-i"],
        Winsize::default(),
        Discipline::Interactive,
    )
    .expect("spawn sh -i");
    session
        .on_frame(Frame::Attach { generation: 1 })
        .expect("attach");
    let pid_before = session.child_pid();

    // Unbounded producer. Nothing about this terminates on its own.
    session
        .on_frame(Frame::Data(b"yes\n".to_vec()))
        .expect("start flood");

    // Deliberately small window, replenished slowly, so the producer outruns
    // the consumer and the kernel is forced to stop reading.
    let start = Instant::now();
    let mut granted_once = false;
    while start.elapsed() < Duration::from_secs(10) {
        if !granted_once {
            let _ = session.on_frame(Frame::Credit(8 * 1024));
            granted_once = true;
        }
        let _ = session.poll_child();
        let _ = session.on_pty_readable();
        // Drain frames but replenish only a fraction of what we consumed.
        let mut consumed = 0usize;
        while let Some(f) = session.pop_outbound() {
            if let Frame::Data(b) = f {
                consumed += b.len();
            }
        }
        if consumed > 0 {
            let _ = session.on_frame(Frame::Credit((consumed / 2) as u32));
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    assert!(
        session.stalled_reads() > 0,
        "kernel never stalled its reads in 10s of `yes` — backpressure was not \
         exercised, so this run is inconclusive, not passing (SPEC-KERNEL §4)"
    );

    // Interrupt.
    session.on_frame(Frame::Data(vec![0x03])).expect("send ^C");

    // The oracle for "usable" is the shell's own pid, reported by the shell.
    let marker = pump_until(&mut session, 256 * 1024, Duration::from_secs(5), |acc| {
        acc.windows(11).any(|w| w == b"RILL-ALIVE-")
    });
    let _ = session.on_frame(Frame::Data(b"printf 'RILL-ALIVE-%s\\n' $$\n".to_vec()));
    let marker = if marker.windows(11).any(|w| w == b"RILL-ALIVE-") {
        marker
    } else {
        pump_until(&mut session, 256 * 1024, Duration::from_secs(5), |acc| {
            acc.windows(11).any(|w| w == b"RILL-ALIVE-")
        })
    };
    let text = String::from_utf8_lossy(&marker);
    assert!(
        text.contains("RILL-ALIVE-"),
        "shell was not usable after ^C; tail={:?}",
        &text[text.len().saturating_sub(200)..]
    );
    assert_eq!(
        session.child_pid(),
        pid_before,
        "^C replaced the shell instead of interrupting the flood"
    );

    let _ = session.terminate();
}

/// Nothing may go missing under a finite credit window. The producer numbers
/// its own output, so a failure names the first gap instead of a byte count.
#[test]
fn t_drop_numbered_flood_has_no_gaps_under_finite_credit() {
    const N: u32 = 50_000;
    let arg = format!("seq 1 {N}");
    let mut session = Session::spawn_with(
        "/bin/sh",
        &["-c", &arg],
        Winsize::default(),
        Discipline::Raw,
    )
    .expect("spawn seq");
    session
        .on_frame(Frame::Attach { generation: 1 })
        .expect("attach");

    let out = pump_until(&mut session, 16 * 1024, Duration::from_secs(30), |acc| {
        acc.windows(6).any(|w| w == format!("\n{N}\n").as_bytes())
    });

    let text = String::from_utf8_lossy(&out);
    let mut expect: u32 = 1;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = line.parse::<u32>() else { continue };
        assert_eq!(
            v, expect,
            "first gap at {expect}: the stream jumped to {v}. Bytes were dropped."
        );
        expect += 1;
    }
    assert!(
        expect > N,
        "stream ended early at {expect} of {N} — {} lines missing",
        N - expect + 1
    );

    let _ = session.terminate();
}

// --------------------------------------------------------------------- T-RESIZE

/// The oracle is the **child's** `TIOCGWINSZ`, reported by the child over its
/// own tty. The old test called `TIOCGWINSZ` on the same master fd it had just
/// called `TIOCSWINSZ` on, with `sleep 8` as the child — it verified that
/// Darwin's ioctl round-trips (audit S2 row 3).
#[test]
fn t_resize_child_reports_the_new_winsize_after_pending_input() {
    let report = tmp_path("winsize");
    let script = format!(
        "trap 'stty size > {r} 2>/dev/null' WINCH; while :; do sleep 0.05; done",
        r = report.display()
    );
    let mut session =
        Session::spawn("/bin/sh", &["-c", &script], Winsize::default()).expect("spawn winch trap");
    session
        .on_frame(Frame::Attach { generation: 1 })
        .expect("attach");
    std::thread::sleep(Duration::from_millis(200));

    // Input queued before the resize, with no drain in between.
    session
        .on_frame(Frame::Data(b"partial-command-no-newline".to_vec()))
        .expect("pending input");
    session
        .on_frame(Frame::Resize {
            cols: 91,
            rows: 31,
            px_w: 728,
            px_h: 496,
        })
        .expect("resize");

    let deadline = Instant::now() + Duration::from_secs(5);
    let mut reported = String::new();
    while Instant::now() < deadline {
        if let Ok(s) = std::fs::read_to_string(&report) {
            if !s.trim().is_empty() {
                reported = s.trim().to_string();
                break;
            }
        }
        let _ = session.poll_child();
        std::thread::sleep(Duration::from_millis(50));
    }

    assert_eq!(
        reported, "31 91",
        "child's own TIOCGWINSZ does not match the display geometry \
         (got {reported:?}); SIGWINCH was not delivered or the size is wrong"
    );

    // Ordering, by sequence number rather than by timing (SPEC-KERNEL §7).
    let journal = session.io_journal();
    let last_write = journal
        .iter()
        .rposition(|(_, e)| matches!(e, IoEvent::PtyWrite(_)));
    let winsize_at = journal
        .iter()
        .position(|(_, e)| matches!(e, IoEvent::Winsize(_)));
    let (Some(w), Some(z)) = (last_write, winsize_at) else {
        panic!("journal missing PtyWrite or Winsize: {journal:?}");
    };
    assert!(
        w < z,
        "resize overtook input queued before it: journal={journal:?}"
    );

    let _ = session.terminate();
    let _ = std::fs::remove_file(&report);
}

// ----------------------------------------------------------------------- T-EXIT

#[test]
fn t_exit_dead_pane_rejects_input_and_reports_status() {
    let mut session =
        Session::spawn("/bin/sh", &["-c", "exit 7"], Winsize::default()).expect("spawn");
    session
        .on_frame(Frame::Attach { generation: 1 })
        .expect("attach");

    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline && session.child_alive() {
        let _ = session.poll_child();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!session.child_alive(), "child never reaped");

    assert!(matches!(
        session.on_frame(Frame::Data(b"x".to_vec())),
        Err(Error::Dead)
    ));

    let mut status = None;
    while let Some(f) = session.pop_outbound() {
        if let Frame::Exit { status: s } = f {
            status = Some(s);
        }
    }
    let raw = status.expect("EXIT frame required");
    // Raw wait status: normal exit 7 encodes as 7 << 8.
    assert_eq!(
        (raw >> 8) & 0xff,
        7,
        "exit status lost: raw={raw:#x}. `code().unwrap_or(1)` reported 1 for \
         signal deaths, which the GUI would have shown as a normal failure."
    );
}

/// **This is the case that fails against `main` with no mutation applied.**
///
/// The child dies while nobody is attached. `detach()` used to clear the
/// outbound queue, so the `EXIT` was destroyed and never replayed — the
/// reopened window painted a live cursor over a dead process, which is FR-EXIT
/// failing on exactly the persist path Spike 0 exists to prove (audit S3-2).
///
/// Required mutation: `RILL_MUTATE=clear_outbound_on_detach`.
#[test]
fn t_exit_child_death_while_detached_is_reported_on_reattach() {
    let mut session =
        Session::spawn("/bin/sh", &["-c", "sleep 0.3; exit 3"], Winsize::default()).expect("spawn");
    session
        .on_frame(Frame::Attach { generation: 1 })
        .expect("attach");

    session.detach();
    assert!(!session.attached());

    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline && session.child_alive() {
        let _ = session.poll_child();
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!session.child_alive(), "child never exited while detached");

    session
        .on_frame(Frame::Attach { generation: 2 })
        .expect("reattach");

    let mut saw_exit = false;
    while let Some(f) = session.pop_outbound() {
        if let Frame::Exit { status } = f {
            assert_eq!((status >> 8) & 0xff, 3);
            saw_exit = true;
        }
    }
    assert!(
        saw_exit,
        "reattaching client was never told the child is dead — the reopened \
         window would paint a cursor over a corpse (FR-EXIT, SPEC-KERNEL §6)"
    );
}

// --------------------------------------------------------------------- T-ATTACH

#[test]
fn t_attach_second_attach_is_refused_and_the_first_keeps_working() {
    let mut session =
        Session::spawn("/bin/sh", &["-c", "sleep 5"], Winsize::default()).expect("spawn");
    session
        .on_frame(Frame::Attach { generation: 1 })
        .expect("a1");
    session
        .on_frame(Frame::Attach { generation: 2 })
        .expect("a2");

    let mut refused = false;
    while let Some(f) = session.pop_outbound() {
        if matches!(
            f,
            Frame::Refused {
                reason: RefuseReason::AlreadyAttached
            }
        ) {
            refused = true;
        }
    }
    assert!(refused, "second attach was not refused (FR-ONE)");
    assert!(session.attached(), "the refusal disturbed the first client");
    let _ = session.terminate();
}

// ----------------------------------------------------------------------- T-KILL

/// Kernel-plane half only. The name says what this observes — a detach does not
/// touch the child — rather than claiming a `SIGKILL` it never sends. The real
/// gate is `rilld`'s packaged `persist_e2e` (ADR 0002 D6, audit S2 row 2).
#[test]
fn t_kill_detach_does_not_signal_the_child() {
    let mut session =
        Session::spawn("/bin/sh", &["-c", "exec sleep 30"], Winsize::default()).expect("spawn");
    session
        .on_frame(Frame::Attach { generation: 1 })
        .expect("attach");
    let pid = session.child_pid();

    session.detach();
    std::thread::sleep(Duration::from_millis(100));
    let _ = session.poll_child();

    // Oracle is the OS, not our own bookkeeping.
    let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
    assert!(alive, "child {pid} died when the GUI detached");
    assert_eq!(session.child_pid(), pid);
    let _ = session.terminate();
}

/// `Drop` must not kill the child. This is audit S3-3: the previous `Drop`
/// impl meant any error path that dropped a `Session` destroyed the user's
/// shell.
#[test]
fn t_kill_dropping_the_session_does_not_kill_the_child() {
    let session =
        Session::spawn("/bin/sh", &["-c", "exec sleep 30"], Winsize::default()).expect("spawn");
    let pid = session.child_pid();
    drop(session);
    std::thread::sleep(Duration::from_millis(150));

    let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
    assert!(
        alive,
        "dropping the session killed the shell — a transient error anywhere in \
         the daemon would take the user's work with it (SPEC-KERNEL §2)"
    );
    unsafe { libc::kill(pid as i32, libc::SIGKILL) };
}

// ---------------------------------------------------------------------- ring

#[test]
fn t_bytes_ring_keeps_the_tail_and_never_transcodes() {
    let mut ring = rill_kernel::ByteRing::new(4);
    ring.append(b"abcdef");
    assert_eq!(ring.snapshot(), b"cdef");

    let mut ring = rill_kernel::ByteRing::new(64);
    let raw: &[u8] = &[0xff, 0xfe, 0x80, 0x41];
    ring.append(raw);
    assert_eq!(ring.snapshot(), raw);
}
