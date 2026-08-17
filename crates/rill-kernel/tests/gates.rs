//! Kernel-plane Spike 0 gates: T-BYTES, T-DROP, T-RESIZE, T-EXIT.
//!
//! Definitions, oracles, and required mutations: docs/TEST-CASES.md.
//! Every test here asserts on something the code under test did not hand it
//! (ADR 0002 D4).

use rill_attach::{Frame, RefuseReason};
use rill_kernel::{Discipline, Error, IoEvent, Kernel, Session, Winsize};
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
            .on_frame(Frame::Attach {
                generation: 1,
                session_id: None,
            })
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
        .on_frame(Frame::Attach {
            generation: 1,
            session_id: None,
        })
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
        .on_frame(Frame::Attach {
            generation: 1,
            session_id: None,
        })
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
        .on_frame(Frame::Attach {
            generation: 1,
            session_id: None,
        })
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
        .on_frame(Frame::Attach {
            generation: 1,
            session_id: None,
        })
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
        .on_frame(Frame::Attach {
            generation: 1,
            session_id: None,
        })
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
        .on_frame(Frame::Attach {
            generation: 2,
            session_id: None,
        })
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
        .on_frame(Frame::Attach {
            generation: 1,
            session_id: None,
        })
        .expect("a1");
    session
        .on_frame(Frame::Attach {
            generation: 2,
            session_id: None,
        })
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
        .on_frame(Frame::Attach {
            generation: 1,
            session_id: None,
        })
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

// -------------------------------------------------------------- T-GRAPH (M1)

fn graph_tmp(tag: &str) -> std::path::PathBuf {
    tmp_path(tag)
}

fn pump_leaf(
    kernel: &mut Kernel,
    id: rill_kernel::SessionId,
    window: u32,
    timeout: Duration,
    mut pred: impl FnMut(&[u8]) -> bool,
) -> Vec<u8> {
    let mut acc = Vec::new();
    let start = Instant::now();
    let _ = kernel.on_frame(id, Frame::Credit(window));
    while start.elapsed() < timeout {
        let consumed = {
            let Some(session) = kernel.session_mut(id) else {
                break;
            };
            let _ = session.poll_child();
            let consumed = session.on_pty_readable().unwrap_or(0);
            while let Some(f) = session.pop_outbound() {
                if let Frame::Data(b) = f {
                    acc.extend_from_slice(&b);
                }
            }
            consumed
        };
        if consumed > 0 {
            let _ = kernel.on_frame(id, Frame::Credit(consumed as u32));
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

fn terminate_leaf(kernel: &mut Kernel, id: rill_kernel::SessionId) {
    let _ = kernel.terminate(id);
}

/// T-GRAPH-SPAWN. Oracle is two live `posix_spawn` children, not a counter
/// the test wrote. Required mutation: `RILL_MUTATE=single_session`.
#[test]
fn t_graph_two_sessions_have_distinct_child_pids() {
    let mut kernel = Kernel::new();
    let a = kernel
        .spawn_leaf("/bin/sh", &["-c", "exec sleep 60"], Winsize::default())
        .expect("spawn A");
    let b = kernel
        .spawn_leaf("/bin/sh", &["-c", "exec sleep 60"], Winsize::default())
        .expect("spawn B");

    let pid_a = kernel.session(a).expect("A").child_pid();
    let pid_b = kernel.session(b).expect("B").child_pid();
    assert_ne!(
        pid_a, pid_b,
        "second spawn_leaf did not create a distinct child (SPEC-GRAPH §3)"
    );
    assert!(
        kernel.session(a).expect("A").child_alive() && kernel.session(b).expect("B").child_alive(),
        "a spawned leaf is already dead"
    );
    let alive_a = unsafe { libc::kill(pid_a as i32, 0) } == 0;
    let alive_b = unsafe { libc::kill(pid_b as i32, 0) } == 0;
    assert!(alive_a, "child A pid {pid_a} is not a live process");
    assert!(alive_b, "child B pid {pid_b} is not a live process");

    terminate_leaf(&mut kernel, a);
    terminate_leaf(&mut kernel, b);
}

/// T-GRAPH-TERMINATE. Oracle is the OS: after `Kernel::terminate(A)`,
/// `kill(pid_A, 0)` fails and `kill(pid_B, 0)` succeeds. Not `child_alive()`.
/// Required mutation: `RILL_MUTATE=terminate_all_leaves`.
#[test]
fn t_graph_terminate_one_leaf_leaves_the_other_alive() {
    let mut kernel = Kernel::new();
    let a = kernel
        .spawn_leaf("/bin/sh", &["-c", "exec sleep 60"], Winsize::default())
        .expect("spawn A");
    let b = kernel
        .spawn_leaf("/bin/sh", &["-c", "exec sleep 60"], Winsize::default())
        .expect("spawn B");

    let pid_a = kernel.session(a).expect("A").child_pid();
    let pid_b = kernel.session(b).expect("B").child_pid();
    assert_ne!(pid_a, pid_b, "need two distinct children");
    assert!(
        unsafe { libc::kill(pid_a as i32, 0) } == 0,
        "child A pid {pid_a} is not live before terminate"
    );
    assert!(
        unsafe { libc::kill(pid_b as i32, 0) } == 0,
        "child B pid {pid_b} is not live before terminate"
    );

    kernel.terminate(a).expect("terminate A");

    let alive_a = unsafe { libc::kill(pid_a as i32, 0) } == 0;
    let alive_b = unsafe { libc::kill(pid_b as i32, 0) } == 0;
    assert!(
        !alive_a,
        "child A pid {pid_a} is still a live process after terminate(A)"
    );
    assert!(
        alive_b,
        "child B pid {pid_b} died when terminating A (ADR 0011 D2)"
    );

    terminate_leaf(&mut kernel, b);
}

/// T-GRAPH-ISOLATE. Markers come from the children (`cat` of distinct files),
/// not from a buffer the kernel copied for the test. Required mutation:
/// `RILL_MUTATE=single_session`.
#[test]
fn t_graph_histories_do_not_mix() {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let marker_a = format!("RILL-GRAPH-A-{n}");
    let marker_b = format!("RILL-GRAPH-B-{n}");
    let path_a = graph_tmp("graph-a");
    let path_b = graph_tmp("graph-b");
    std::fs::write(&path_a, &marker_a).expect("write A");
    std::fs::write(&path_b, &marker_b).expect("write B");

    let mut kernel = Kernel::new();
    let arg_a = format!("cat {}", path_a.display());
    let arg_b = format!("cat {}", path_b.display());
    let a = kernel
        .spawn_leaf_with(
            "/bin/sh",
            &["-c", &arg_a],
            Winsize::default(),
            Discipline::Raw,
        )
        .expect("spawn A");
    let b = kernel
        .spawn_leaf_with(
            "/bin/sh",
            &["-c", &arg_b],
            Winsize::default(),
            Discipline::Raw,
        )
        .expect("spawn B");

    kernel
        .on_frame(
            a,
            Frame::Attach {
                generation: 1,
                session_id: None,
            },
        )
        .expect("attach A");
    kernel
        .on_frame(
            b,
            Frame::Attach {
                generation: 1,
                session_id: None,
            },
        )
        .expect("attach B");

    let _ = pump_leaf(&mut kernel, a, 64 * 1024, Duration::from_secs(3), |acc| {
        acc.windows(marker_a.len())
            .any(|w| w == marker_a.as_bytes())
    });
    let _ = pump_leaf(&mut kernel, b, 64 * 1024, Duration::from_secs(3), |acc| {
        acc.windows(marker_b.len())
            .any(|w| w == marker_b.as_bytes())
    });

    let hist_a = kernel.session(a).expect("A").history();
    let hist_b = kernel.session(b).expect("B").history();
    assert!(
        hist_a
            .windows(marker_a.len())
            .any(|w| w == marker_a.as_bytes()),
        "A's child marker did not reach A's history"
    );
    assert!(
        hist_b
            .windows(marker_b.len())
            .any(|w| w == marker_b.as_bytes()),
        "B's child marker did not reach B's history"
    );
    assert!(
        !hist_a
            .windows(marker_b.len())
            .any(|w| w == marker_b.as_bytes()),
        "B's marker leaked into A's history (SPEC-GRAPH §3)"
    );
    assert!(
        !hist_b
            .windows(marker_a.len())
            .any(|w| w == marker_a.as_bytes()),
        "A's marker leaked into B's history (SPEC-GRAPH §3)"
    );

    terminate_leaf(&mut kernel, a);
    terminate_leaf(&mut kernel, b);
    let _ = std::fs::remove_file(&path_a);
    let _ = std::fs::remove_file(&path_b);
}

/// T-GRAPH-ATTACH (same id). Second attach is REFUSED; the first claim still
/// receives later DATA.
#[test]
fn t_graph_second_attach_to_same_id_is_refused() {
    let mut kernel = Kernel::new();
    let a = kernel
        .spawn_leaf(
            "/bin/sh",
            &[
                "-c",
                "printf FIRST-A\\n; sleep 0.4; printf SECOND-A\\n; sleep 5",
            ],
            Winsize::default(),
        )
        .expect("spawn A");
    kernel
        .on_frame(
            a,
            Frame::Attach {
                generation: 1,
                session_id: None,
            },
        )
        .expect("a1");

    let first = pump_leaf(&mut kernel, a, 64 * 1024, Duration::from_secs(3), |acc| {
        acc.windows(7).any(|w| w == b"FIRST-A")
    });
    assert!(
        first.windows(7).any(|w| w == b"FIRST-A"),
        "first client never saw FIRST-A from the child"
    );

    kernel
        .on_frame(
            a,
            Frame::Attach {
                generation: 2,
                session_id: None,
            },
        )
        .expect("a2");
    let mut refused = false;
    if let Some(session) = kernel.session_mut(a) {
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
        assert!(session.attached(), "the refusal disturbed the first client");
    } else {
        panic!("leaf A vanished");
    }
    assert!(
        refused,
        "second attach to the same id was not refused (FR-ONE)"
    );

    let second = pump_leaf(&mut kernel, a, 64 * 1024, Duration::from_secs(3), |acc| {
        acc.windows(8).any(|w| w == b"SECOND-A")
    });
    assert!(
        second.windows(8).any(|w| w == b"SECOND-A"),
        "first client stopped receiving DATA after the refused attach"
    );

    terminate_leaf(&mut kernel, a);
}

/// T-GRAPH-ATTACH (other id). Attach to B succeeds while A stays attached.
/// Required mutation: `RILL_MUTATE=single_session` turns this red.
#[test]
fn t_graph_attach_to_a_second_id_is_accepted() {
    let mut kernel = Kernel::new();
    let a = kernel
        .spawn_leaf("/bin/sh", &["-c", "exec sleep 30"], Winsize::default())
        .expect("spawn A");
    let b = kernel
        .spawn_leaf("/bin/sh", &["-c", "exec sleep 30"], Winsize::default())
        .expect("spawn B");

    kernel
        .on_frame(
            a,
            Frame::Attach {
                generation: 1,
                session_id: None,
            },
        )
        .expect("attach A");
    kernel
        .on_frame(
            b,
            Frame::Attach {
                generation: 1,
                session_id: None,
            },
        )
        .expect("attach B");

    let mut refused_b = false;
    if let Some(session) = kernel.session_mut(b) {
        while let Some(f) = session.pop_outbound() {
            if matches!(
                f,
                Frame::Refused {
                    reason: RefuseReason::AlreadyAttached
                }
            ) {
                refused_b = true;
            }
        }
        assert!(
            session.attached(),
            "attach to a second id did not take the claim"
        );
    } else {
        panic!("leaf B vanished");
    }
    assert!(
        !refused_b,
        "attach to id B was refused as if it were id A (SPEC-GRAPH §2)"
    );
    assert!(
        kernel.session(a).expect("A").attached(),
        "attaching B disturbed A's claim"
    );

    terminate_leaf(&mut kernel, a);
    terminate_leaf(&mut kernel, b);
}

// ------------------------------------------------------------------ T-CWD (M6)

#[cfg(target_os = "macos")]
fn os_vnode_cwd(pid: u32) -> std::path::PathBuf {
    use std::os::unix::ffi::OsStrExt;
    let mut info = std::mem::MaybeUninit::<libc::proc_vnodepathinfo>::zeroed();
    let n = unsafe {
        libc::proc_pidinfo(
            pid as i32,
            libc::PROC_PIDVNODEPATHINFO,
            0,
            info.as_mut_ptr() as *mut libc::c_void,
            std::mem::size_of::<libc::proc_vnodepathinfo>() as libc::c_int,
        )
    };
    assert!(n > 0, "proc_pidinfo({pid}) failed; oracle is the OS");
    let info = unsafe { info.assume_init() };
    let flat =
        unsafe { std::slice::from_raw_parts(info.pvi_cdir.vip_path.as_ptr() as *const u8, 1024) };
    let end = flat.iter().position(|&b| b == 0).expect("cwd path");
    std::path::PathBuf::from(std::ffi::OsStr::from_bytes(&flat[..end]))
}

#[cfg(target_os = "macos")]
fn write_chdir_fixture(go: &std::path::Path) -> std::path::PathBuf {
    let script = tmp_path("cwd-chdir");
    std::fs::write(
        &script,
        format!(
            "import os, sys, time\nprint('BEFORE', os.getcwd(), flush=True)\nwhile not os.path.exists({go:?}):\n    time.sleep(0.05)\nos.chdir('/private/tmp')\nprint('AFTER', os.getcwd(), flush=True)\ntime.sleep(30)\n"
        ),
    )
    .expect("write fixture");
    script
}

/// T-CWD-FG. Oracle is OS vnode of the fg python vs zsh leader, not a
/// string the test copied into the kernel. Required mutation: `leader_cwd`.
#[cfg(target_os = "macos")]
#[test]
fn t_cwd_foreground_job_chdir_is_visible() {
    let go = tmp_path("cwd-fg-go");
    let script = write_chdir_fixture(&go);
    let size = Winsize {
        cols: 200,
        ..Winsize::default()
    };
    let mut session = Session::spawn("/bin/zsh", &["-f", "-i"], size).expect("zsh");
    session
        .on_frame(Frame::Attach {
            generation: 1,
            session_id: None,
        })
        .expect("attach");
    let leader = session.child_pid();
    let start_dir = os_vnode_cwd(leader);

    let cmd = format!("/usr/bin/python3 {}\n", script.display());
    pump_until(&mut session, 64 * 1024, Duration::from_millis(400), |_| {
        true
    });
    session
        .on_frame(Frame::Data(cmd.into_bytes()))
        .expect("send");
    let before = pump_until(&mut session, 64 * 1024, Duration::from_secs(5), |acc| {
        acc.windows(6).any(|w| w == b"BEFORE")
    });
    assert!(
        before.windows(6).any(|w| w == b"BEFORE"),
        "zsh never started the fg job; tail={:?} alive={}",
        String::from_utf8_lossy(&before[before.len().saturating_sub(400)..]),
        session.child_alive()
    );
    let first = session.cwd().expect("cwd before chdir");
    assert_eq!(first, start_dir, "seed cwd should be the shell's directory");

    std::fs::write(&go, b"go").expect("release chdir");
    let after = pump_until(&mut session, 64 * 1024, Duration::from_secs(5), |acc| {
        acc.windows(5).any(|w| w == b"AFTER")
    });
    assert!(
        after.windows(5).any(|w| w == b"AFTER"),
        "fg job never chdir'd"
    );

    let tapped = session.cwd().expect("cwd after fg chdir");
    let leader_now = os_vnode_cwd(leader);
    assert_eq!(
        leader_now, start_dir,
        "zsh leader cwd moved; this test needs a stopped shell"
    );
    assert_eq!(
        tapped,
        std::path::PathBuf::from("/private/tmp"),
        "cwd tap followed the shell, not the fg TUI (ADR 0013 D2)"
    );

    let _ = session.terminate();
    let _ = std::fs::remove_file(&script);
    let _ = std::fs::remove_file(&go);
}

/// T-CWD-NO-OSC7. Leaf chdir's without writing OSC 7. Required mutation:
/// `osc7_only`.
#[cfg(target_os = "macos")]
#[test]
fn t_cwd_tui_chdir_without_osc7_is_visible() {
    let go = tmp_path("cwd-osc-go");
    let script = write_chdir_fixture(&go);
    let mut session = Session::spawn_with(
        "/usr/bin/python3",
        &[script.to_str().expect("utf8")],
        Winsize::default(),
        Discipline::Interactive,
    )
    .expect("python leaf");
    session
        .on_frame(Frame::Attach {
            generation: 1,
            session_id: None,
        })
        .expect("attach");
    let before = pump_until(&mut session, 64 * 1024, Duration::from_secs(5), |acc| {
        acc.windows(6).any(|w| w == b"BEFORE")
    });
    assert!(
        before.windows(6).any(|w| w == b"BEFORE"),
        "python leaf never printed BEFORE; tail={:?} alive={}",
        String::from_utf8_lossy(&before[before.len().saturating_sub(400)..]),
        session.child_alive()
    );
    let start = session.cwd().expect("cwd before");
    assert_ne!(start, std::path::PathBuf::from("/private/tmp"));

    std::fs::write(&go, b"go").expect("release");
    let after = pump_until(&mut session, 64 * 1024, Duration::from_secs(5), |acc| {
        acc.windows(5).any(|w| w == b"AFTER")
    });
    assert!(after.windows(5).any(|w| w == b"AFTER"));
    let hist = session.history();
    assert!(
        !hist.windows(3).any(|w| w == b"\x1b]7"),
        "fixture emitted OSC 7; oracle would be the classifier, not the tap"
    );
    let tapped = session.cwd().expect("cwd after chdir");
    assert_eq!(
        tapped,
        std::path::PathBuf::from("/private/tmp"),
        "cwd required OSC 7 (ADR 0013 D3)"
    );

    let _ = session.terminate();
    let _ = std::fs::remove_file(&script);
    let _ = std::fs::remove_file(&go);
}

/// T-CWD-FAIL-CLOSED. Dead child → Err, last known kept. Required mutation:
/// `cwd_fail_open`.
#[cfg(target_os = "macos")]
#[test]
fn t_cwd_unreadable_does_not_invent_a_path() {
    let mut session = Session::spawn_with(
        "/usr/bin/python3",
        &["-c", "import time; time.sleep(30)"],
        Winsize::default(),
        Discipline::Raw,
    )
    .expect("python");
    let known = session.cwd().expect("live cwd");
    assert!(
        !known.as_os_str().is_empty(),
        "live cwd was empty — not a path"
    );
    session.terminate().expect("kill");
    let after = session.cwd();
    assert!(
        after.is_err(),
        "unreadable cwd was Ok({after:?}); fail-open (ADR 0013 D5)"
    );
    assert_eq!(
        session.last_cwd(),
        Some(known.as_path()),
        "last known was dropped or replaced"
    );
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
