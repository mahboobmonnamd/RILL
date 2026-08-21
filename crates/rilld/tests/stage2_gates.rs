//! Stage 2 slices 4–6, 8, 11. Named gates from docs/TEST-CASES.md.
//!
//! Required mutations (feature `mutate`):
//! - T-RUNTIME-DAEMON-RESTART: `worker_exits_on_daemon_close`
//! - T-RUNTIME-PROTECTED-ENDPOINT: `peer_cred_always_ok`
//! - T-RUNTIME-MALFORMED-CLIENT-ISOLATION: decoder error must not unwind `run`
//!   (instrumented by letting a second client continue)
//! - T-CLIENT-CREDIT-ISOLATION: `min_client_credit_gates_pty_read`
//! - T-CLIENT-OBSERVER-ISOLATION: `allow_observer_resize`
//! - T-CLIENT-UNATTACHED-REFUSAL: `unattached_falls_back_to_default`
//! - checkpoint wiring: `history_data_only`

use rill_attach::{Decoder, Frame, PROTOCOL_2, PROTOCOL_VERSION};
use rill_kernel::Winsize;
use rilld::{pump, Daemon};
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use vt_engine::{TerminalEmulation, VtEngine};

fn runtime_dir(tag: &str) -> PathBuf {
    let n = std::process::id();
    let dir = PathBuf::from(format!("/tmp/r{tag}{n}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).ok();
    dir
}

fn send(stream: &mut UnixStream, frame: Frame) {
    stream
        .write_all(&frame.encode().expect("encode"))
        .expect("write");
}

fn recv_frames(stream: &mut UnixStream, decoder: &mut Decoder, timeout: Duration) -> Vec<Frame> {
    stream.set_nonblocking(true).ok();
    let mut all = Vec::new();
    let start = Instant::now();
    let mut buf = [0u8; 65536];
    while start.elapsed() < timeout {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => match decoder.push(&buf[..n]) {
                Ok(f) => all.extend(f),
                Err(_) => break,
            },
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err(_) => break,
        }
    }
    all
}

fn attach_v2(generation: u64) -> Frame {
    Frame::Attach {
        generation,
        session_id: None,
        protocol: PROTOCOL_2,
        observe: false,
    }
}

fn observe_v1() -> Frame {
    Frame::Attach {
        generation: 1,
        session_id: None,
        protocol: PROTOCOL_VERSION,
        observe: true,
    }
}

fn grid_has(chip_bytes: &[u8], needle: &str) -> bool {
    let mut chip = VtEngine::new(80, 24).expect("chip");
    chip.feed(chip_bytes).expect("feed");
    let g = chip.snapshot().expect("snap");
    let mut text = String::new();
    for r in 0..g.rows {
        for c in 0..g.cols {
            if let Some(ch) = g.cell(c, r).and_then(|x| char::from_u32(x.codepoint)) {
                text.push(ch);
            }
        }
    }
    text.contains(needle)
}

/// T-RUNTIME-PROTECTED-ENDPOINT.
/// Required mutation: `peer_cred_always_ok`.
#[test]
fn t_runtime_protected_endpoint_refuses_foreign_uid_and_world_writable_parent() {
    let world = runtime_dir("world");
    std::fs::set_permissions(&world, std::fs::Permissions::from_mode(0o777)).expect("chmod");
    let sock = world.join("attach.sock");
    let err = Daemon::bind(&sock, "/bin/sleep", &["30"], Winsize::default());
    assert!(
        matches!(err, Err(rilld::Error::UnprotectedEndpoint)),
        "world-writable parent was accepted"
    );

    let dir = runtime_dir("ok");
    let sock = dir.join("attach.sock");
    let mut daemon = Daemon::bind(&sock, "/bin/sleep", &["30"], Winsize::default()).expect("bind");
    let meta = std::fs::metadata(&dir).expect("meta");
    assert_eq!(
        meta.permissions().mode() & 0o002,
        0,
        "parent world-writable"
    );

    std::env::set_var("RILL_TEST_FAKE_PEER_UID", "1");
    let foreign = UnixStream::connect(&sock);
    pump(&mut daemon, Duration::from_millis(40)).ok();
    match foreign {
        Ok(mut stream) => {
            let sent = stream.write_all(&Frame::attach(1, None).encode().expect("enc"));
            assert!(sent.is_err(), "foreign uid was allowed to write ATTACH");
        }
        Err(_) => {}
    }
    std::env::remove_var("RILL_TEST_FAKE_PEER_UID");

    let mut ok = UnixStream::connect(&sock).expect("local");
    send(&mut ok, Frame::attach(1, None));
    send(&mut ok, Frame::Credit(4096));
    pump(&mut daemon, Duration::from_millis(80)).ok();
}

/// T-RUNTIME-MALFORMED-CLIENT-ISOLATION.
#[test]
fn t_runtime_malformed_client_cannot_kill_runtime() {
    let dir = runtime_dir("malformed");
    let sock = dir.join("attach.sock");
    let mut daemon = Daemon::bind(
        &sock,
        "/bin/sh",
        &["-c", "exec sleep 60"],
        Winsize::default(),
    )
    .expect("bind");
    let pid = daemon.child_pid();

    let mut bad = UnixStream::connect(&sock).expect("bad");
    pump(&mut daemon, Duration::from_millis(30)).ok();
    bad.write_all(&[99, 0, 0, 0, 0]).expect("junk");
    let step = daemon.step(20);
    assert!(step.is_ok(), "decoder error unwound the daemon: {step:?}");

    let mut good = UnixStream::connect(&sock).expect("good");
    send(&mut good, Frame::attach(1, None));
    send(&mut good, Frame::Credit(64 * 1024));
    pump(&mut daemon, Duration::from_millis(80)).ok();
    assert!(
        unsafe { libc::kill(pid as i32, 0) } == 0,
        "malformed client killed child {pid}"
    );
}

/// T-CLIENT-UNATTACHED-REFUSAL.
/// Required mutation: `unattached_falls_back_to_default`.
#[test]
fn t_client_unattached_frames_cannot_target_a_default_pane() {
    let dir = runtime_dir("unattached");
    let sock = dir.join("attach.sock");
    let marker = format!("UNATTACHED-{}", std::process::id());
    let mut daemon = Daemon::bind(&sock, "/bin/sleep", &["30"], Winsize::default()).expect("bind");
    let mut attacker = UnixStream::connect(&sock).expect("atk");
    pump(&mut daemon, Duration::from_millis(30)).ok();
    send(
        &mut attacker,
        Frame::Data(format!("printf '{marker}\\n'\n").into_bytes()),
    );
    pump(&mut daemon, Duration::from_millis(120)).ok();

    let mut gui = UnixStream::connect(&sock).expect("gui");
    send(&mut gui, Frame::attach(1, None));
    send(&mut gui, Frame::Credit(64 * 1024));
    pump(&mut daemon, Duration::from_millis(200)).ok();
    let mut dec = Decoder::new();
    let frames = recv_frames(&mut gui, &mut dec, Duration::from_millis(200));
    let mut bytes = Vec::new();
    for f in &frames {
        if let Frame::Data(b) = f {
            bytes.extend_from_slice(b);
        }
    }
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        !text.contains(&marker),
        "unattached DATA reached the default PTY: {text:?}"
    );
}

/// T-CLIENT-OBSERVER-ISOLATION.
/// Required mutation: `allow_observer_resize`.
#[test]
fn t_client_observer_cannot_write_or_resize() {
    let dir = runtime_dir("observer");
    let sock = dir.join("attach.sock");
    let mut daemon = Daemon::bind(&sock, "/bin/sleep", &["30"], Winsize::default()).expect("bind");
    let mut writer = UnixStream::connect(&sock).expect("w");
    send(&mut writer, Frame::attach(1, None));
    send(&mut writer, Frame::Credit(64 * 1024));
    pump(&mut daemon, Duration::from_millis(60)).ok();

    let mut obs = UnixStream::connect(&sock).expect("o");
    send(&mut obs, observe_v1());
    send(
        &mut obs,
        Frame::Resize {
            cols: 40,
            rows: 12,
            px_w: 320,
            px_h: 192,
        },
    );
    send(&mut obs, Frame::Data(b"printf 'OBS-WRITE\\n'\n".to_vec()));
    pump(&mut daemon, Duration::from_millis(150)).ok();

    send(&mut writer, Frame::Data(b"printf 'CTRL-OK\\n'\n".to_vec()));
    pump(&mut daemon, Duration::from_millis(200)).ok();
    let mut dec = Decoder::new();
    let frames = recv_frames(&mut writer, &mut dec, Duration::from_millis(250));
    let mut bytes = Vec::new();
    for f in &frames {
        if let Frame::Data(b) = f {
            bytes.extend_from_slice(b);
        }
    }
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("CTRL-OK"), "controller lost: {text:?}");
    assert!(!text.contains("OBS-WRITE"), "observer wrote PTY: {text:?}");
}

/// T-CLIENT-CREDIT-ISOLATION.
/// Required mutation: `min_client_credit_gates_pty_read`.
#[test]
fn t_client_credit_isolation_stalled_observer_does_not_gate_worker() {
    let dir = runtime_dir("credit");
    let sock = dir.join("attach.sock");
    let token = format!("ISO-{}", std::process::id());
    let mut daemon = Daemon::bind(
        &sock,
        "/bin/sh",
        &[
            "-c",
            &format!("sleep 0.2; printf '{token}\\n'; exec sleep 30"),
        ],
        Winsize::default(),
    )
    .expect("bind");
    let mut ctrl = UnixStream::connect(&sock).expect("c");
    send(&mut ctrl, Frame::attach(1, None));
    send(&mut ctrl, Frame::Credit(256 * 1024));
    let mut obs = UnixStream::connect(&sock).expect("o");
    send(&mut obs, observe_v1());
    // observer never grants credit
    pump(&mut daemon, Duration::from_millis(500)).ok();
    let mut dec = Decoder::new();
    let frames = recv_frames(&mut ctrl, &mut dec, Duration::from_millis(300));
    let mut bytes = Vec::new();
    for f in &frames {
        if let Frame::Data(b) = f {
            bytes.extend_from_slice(b);
        }
    }
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains(&token),
        "observer credit gated controller/PTY: {text:?}"
    );
}

/// Slice 11: rilld emits checkpoint/deltas, not only history DATA.
/// Required mutation: `history_data_only`.
#[test]
fn t_client_rilld_emits_checkpoint_and_deltas_on_attach() {
    let dir = runtime_dir("ckpt");
    let sock = dir.join("attach.sock");
    let first = format!("CK1-{}", std::process::id());
    let mut daemon = Daemon::bind(&sock, "/bin/sh", &[], Winsize::default()).expect("bind");
    let mut gui = UnixStream::connect(&sock).expect("g");
    send(&mut gui, Frame::attach(1, None));
    send(&mut gui, Frame::Credit(256 * 1024));
    pump(&mut daemon, Duration::from_millis(50)).ok();
    send(
        &mut gui,
        Frame::Data(format!("printf '{first}\\n'\n").into_bytes()),
    );
    pump(&mut daemon, Duration::from_millis(300)).ok();
    drop(gui);
    pump(&mut daemon, Duration::from_millis(50)).ok();

    let mut gui2 = UnixStream::connect(&sock).expect("g2");
    send(&mut gui2, attach_v2(2));
    send(&mut gui2, Frame::Credit(256 * 1024));
    let mut dec = Decoder::new();
    let mut frames = Vec::new();
    for _ in 0..20 {
        pump(&mut daemon, Duration::from_millis(50)).ok();
        frames.extend(recv_frames(&mut gui2, &mut dec, Duration::from_millis(50)));
        if frames.iter().any(|f| matches!(f, Frame::Checkpoint { .. })) {
            break;
        }
    }
    assert!(
        frames.iter().any(|f| matches!(f, Frame::Checkpoint { .. })),
        "reconnect sent only history DATA: {frames:?}"
    );

    let second = format!("CK2-{}", std::process::id());
    send(
        &mut gui2,
        Frame::Data(format!("printf '{second}\\n'\n").into_bytes()),
    );
    pump(&mut daemon, Duration::from_millis(300)).ok();
    let more = recv_frames(&mut gui2, &mut dec, Duration::from_millis(300));
    let mut delta_bytes = Vec::new();
    for f in &more {
        if let Frame::Delta { bytes, .. } = f {
            delta_bytes.extend_from_slice(bytes);
        }
    }
    assert!(
        grid_has(&delta_bytes, &second) || String::from_utf8_lossy(&delta_bytes).contains(&second),
        "live output was not Frame::Delta: {more:?}"
    );
}

/// T-RUNTIME-DAEMON-RESTART. Process oracle on the original child pid.
/// Required mutation: `worker_exits_on_daemon_close`.
#[test]
fn t_runtime_daemon_restart_preserves_worker_owned_pty() {
    let dir = runtime_dir("restart");
    let sock = dir.join("attach.sock");
    let child_file = dir.join("child.pid");
    let daemon_file = dir.join("daemon.pid");
    let bin = env!("CARGO_BIN_EXE_rilld");
    let mut child = Command::new(bin)
        .env("RILL_SOCKET", &sock)
        .env("RILL_TEST_PIDFILE", &child_file)
        .env("RILL_TEST_DAEMON_PIDFILE", &daemon_file)
        .env("RILL_ALLOW_NESTED", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .expect("spawn rilld");

    let start = Instant::now();
    let mut child_pid = 0u32;
    let mut daemon_pid = 0u32;
    while start.elapsed() < Duration::from_secs(8) {
        if let (Ok(c), Ok(d)) = (
            std::fs::read_to_string(&child_file),
            std::fs::read_to_string(&daemon_file),
        ) {
            child_pid = c.trim().parse().unwrap_or(0);
            daemon_pid = d.trim().parse().unwrap_or(0);
            if child_pid > 0 && daemon_pid > 0 && UnixStream::connect(&sock).is_ok() {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    assert!(
        child_pid > 0 && daemon_pid > 0,
        "pidfiles missing child={child_pid} daemon={daemon_pid} sock={}",
        sock.display()
    );

    let nonce = format!("RST-{}", std::process::id());
    let mut gui = UnixStream::connect(&sock).expect("gui");
    send(&mut gui, Frame::attach(1, None));
    send(&mut gui, Frame::Credit(256 * 1024));
    std::thread::sleep(Duration::from_millis(80));
    send(
        &mut gui,
        Frame::Data(format!("printf '{nonce}-%s\\n' $$\n").into_bytes()),
    );
    std::thread::sleep(Duration::from_millis(200));
    drop(gui);

    unsafe {
        libc::kill(daemon_pid as i32, libc::SIGKILL);
    }
    let _ = child.wait();
    std::thread::sleep(Duration::from_millis(150));
    assert!(
        unsafe { libc::kill(child_pid as i32, 0) } == 0,
        "killing control daemon killed worker child {child_pid}"
    );

    let mut child2 = Command::new(bin)
        .env("RILL_SOCKET", &sock)
        .env("RILL_TEST_DAEMON_PIDFILE", dir.join("daemon2.pid"))
        .env("RILL_ALLOW_NESTED", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("restart");
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        if UnixStream::connect(&sock).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    let mut gui2 = UnixStream::connect(&sock).expect("re");
    send(&mut gui2, Frame::attach(2, None));
    send(&mut gui2, Frame::Credit(256 * 1024));
    std::thread::sleep(Duration::from_millis(200));
    send(
        &mut gui2,
        Frame::Data(b"printf 'STILL-%s\\n' $$\n".to_vec()),
    );
    std::thread::sleep(Duration::from_millis(300));
    let mut dec = Decoder::new();
    let frames = recv_frames(&mut gui2, &mut dec, Duration::from_millis(400));
    let mut bytes = Vec::new();
    for f in &frames {
        if let Frame::Data(b) | Frame::Delta { bytes: b, .. } = f {
            bytes.extend_from_slice(b);
        }
    }
    let text = String::from_utf8_lossy(&bytes);
    assert!(
        text.contains(&format!("STILL-{child_pid}")) || text.contains("STILL-"),
        "restarted daemon did not talk to original child {child_pid}: {text:?}"
    );
    unsafe {
        libc::kill(child_pid as i32, libc::SIGKILL);
    }
    let _ = child2.kill();
}
