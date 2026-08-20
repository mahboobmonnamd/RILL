//! T-KILL packaged spawn path.
//!
//! Bug (doc comment, not the name — ADR 0002 D6): *"quit app and reload does
//! not persist the session."* The previous body launched `rilld` from a `sh`
//! helper and SIGKILL'd that helper. That never exercised `posix_spawn` +
//! `POSIX_SPAWN_SETSID` in `main.m`. Socket-only tests do not close T-KILL.

use rill_attach::{Decoder, Frame};
use rill_vt_types::TerminalEmulation;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};
use vt_engine::VtEngine;

fn unique_sock() -> PathBuf {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    PathBuf::from(format!("/tmp/rill-persist-{n}.sock"))
}

fn wait_sock(path: &PathBuf, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if UnixStream::connect(path).is_ok() {
            return true;
        }
        thread::sleep(Duration::from_millis(20));
    }
    false
}

fn alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

fn wait_pidfile(path: &PathBuf, timeout: Duration) -> Option<u32> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(s) = std::fs::read_to_string(path) {
            if let Ok(pid) = s.trim().parse::<u32>() {
                if pid > 0 {
                    return Some(pid);
                }
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
    None
}

fn gui_bin() -> PathBuf {
    if let Ok(p) = std::env::var("RILL_GUI_BIN") {
        return PathBuf::from(p);
    }
    let app = std::env::var("RILL_GUI_APP").unwrap_or_else(|_| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../dist/Rill.app")
            .display()
            .to_string()
    });
    PathBuf::from(app).join("Contents/MacOS/Rill")
}

/// Quit/SIGKILL of the GUI process group must not kill rilld or the shell.
#[test]
fn t_kill_gui_process_group_child_pid_survives_and_reattach_shows_prior_output() {
    let gui_bin = gui_bin();
    assert!(
        gui_bin.is_file(),
        "T-KILL needs the packaged GUI at {}. Run: sh scripts/package-macos.sh",
        gui_bin.display()
    );

    let sock = unique_sock();
    let pidfile = PathBuf::from(format!("{}.child", sock.display()));
    let rilld_pidfile = PathBuf::from(format!("{}.rilld", sock.display()));
    let second_pidfile = PathBuf::from(format!("{}.child2", sock.display()));

    let mut gui_cmd = Command::new(&gui_bin);
    gui_cmd
        .env("RILL_SOCKET", &sock)
        .env("RILL_TEST_PIDFILE", &pidfile)
        .env("RILL_TEST_DAEMON_PIDFILE", &rilld_pidfile)
        .env("RILL_TEST_SECOND_LEAF", "1")
        .env("RILL_TEST_SECOND_PIDFILE", &second_pidfile)
        .env("SHELL", "/bin/sh")
        .env("PS1", "PERSIST-MARK")
        .env("PROMPT", "PERSIST-MARK")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // Become a process-group leader so kill(-gui_pid) actually signals
        // the GUI group. Without this, the GUI stays in cargo's group,
        // killpg is ESRCH, and drop_POSIX_SPAWN_SETSID cannot go red.
        .process_group(0);
    if let Ok(m) = std::env::var("RILL_MUTATE") {
        gui_cmd.env("RILL_MUTATE", m);
    }
    let mut gui = gui_cmd.spawn().expect("spawn packaged Rill");
    let gui_pid = gui.id();

    assert!(
        wait_sock(&sock, Duration::from_secs(8)),
        "packaged rilld did not bind {}",
        sock.display()
    );
    let child = wait_pidfile(&pidfile, Duration::from_secs(5)).expect("child pidfile");
    let child2 = wait_pidfile(&second_pidfile, Duration::from_secs(5)).expect("second pidfile");
    let daemon = wait_pidfile(&rilld_pidfile, Duration::from_secs(5)).expect("rilld pidfile");
    assert!(alive(child), "shell not running");
    assert!(alive(child2), "second leaf not running");
    assert_ne!(child, child2, "second leaf reused the default pid");
    assert!(alive(daemon), "rilld not running");
    assert_ne!(daemon, gui_pid, "rilld must not be the GUI process");
    // Give the interactive shell time to paint its prompt (PS1=PERSIST-MARK).
    // Do not attach from this process while the GUI holds the slot (FR-ONE).
    thread::sleep(Duration::from_millis(400));

    // SIGKILL the GUI process group. rilld was posix_spawn'd with
    // POSIX_SPAWN_SETSID, so this must not take the daemon or the shell.
    unsafe {
        libc::kill(-(gui_pid as i32), libc::SIGKILL);
        libc::kill(gui_pid as i32, libc::SIGKILL);
    }
    let _ = gui.wait();
    thread::sleep(Duration::from_millis(200));

    assert!(
        alive(daemon),
        "SIGKILL of the GUI killed rilld — session does not persist"
    );
    assert!(
        alive(child),
        "SIGKILL of the GUI killed the shell (child pid {})",
        child
    );
    assert!(
        alive(child2),
        "SIGKILL of the GUI killed the second leaf (child pid {})",
        child2
    );
    assert_eq!(
        child,
        std::fs::read_to_string(&pidfile)
            .expect("pidfile after quit")
            .trim()
            .parse::<u32>()
            .expect("pid"),
        "child PID changed"
    );

    let mut stream = UnixStream::connect(&sock).expect("reload connect");
    stream
        .write_all(&Frame::attach(2, None).encode().expect("a2"))
        .expect("reattach");
    stream
        .write_all(&Frame::Credit(256 * 1024).encode().expect("c2"))
        .expect("credit2");
    thread::sleep(Duration::from_millis(300));
    let mut dec = Decoder::new();
    let mut buf = [0u8; 65536];
    stream.set_nonblocking(true).ok();
    let mut raw = Vec::new();
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(400) {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                for f in dec.push(&buf[..n]).expect("dec") {
                    if let Frame::Data(b) = f {
                        raw.extend(b);
                    }
                }
            }
            Err(_) => thread::sleep(Duration::from_millis(10)),
        }
    }
    let mut chip = VtEngine::new(80, 24).expect("chip");
    chip.feed(&raw).ok();
    let text: String = chip
        .snapshot()
        .map(|g| {
            g.cells
                .iter()
                .filter_map(|c| char::from_u32(c.codepoint))
                .collect()
        })
        .unwrap_or_default();
    assert!(
        text.contains("PERSIST-MARK") || raw.windows(12).any(|w| w == b"PERSIST-MARK"),
        "reload was blank over a live process: {text:?}"
    );

    unsafe {
        libc::kill(daemon as i32, libc::SIGTERM);
    }
    let _ = std::fs::remove_file(&sock);
    let _ = std::fs::remove_file(&pidfile);
    let _ = std::fs::remove_file(&rilld_pidfile);
    let _ = std::fs::remove_file(&second_pidfile);
}
