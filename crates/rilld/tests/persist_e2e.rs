//! T-KILL packaged spawn path.
//! User-reported: quit app and reload does not persist the session.

use rill_attach::{Decoder, Frame};
use rill_chip0::{Chip0, TerminalEmulation};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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

/// Quit/SIGKILL of the GUI process group must not kill rilld or the shell.
/// Fails while rilld is an NSTask/child in the GUI's process group (the bug).
#[test]
fn t_quit_app_and_reload_does_not_persist_the_session() {
    let rilld = std::env::var("RILL_RILLD_BIN").unwrap_or_else(|_| env!("CARGO_BIN_EXE_rilld").into());
    let sock = unique_sock();
    let pidfile = PathBuf::from(format!("{}.child", sock.display()));
    let rilld_pidfile = PathBuf::from(format!("{}.rilld", sock.display()));

    let script = format!(
        "export RILL_SOCKET='{sock}'; export RILL_TEST_PIDFILE='{pidfile}'; '{rilld}' & echo $! > '{rilld_pidfile}'; sleep 3600",
        sock = sock.display(),
        pidfile = pidfile.display(),
        rilld = rilld,
        rilld_pidfile = rilld_pidfile.display(),
    );
    let mut gui = Command::new("/bin/sh")
        .arg("-c")
        .arg(&script)
        .process_group(0)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("gui helper");
    let gui_pgid = gui.id();

    assert!(
        wait_sock(&sock, Duration::from_secs(5)),
        "rilld did not bind {}",
        sock.display()
    );
    thread::sleep(Duration::from_millis(100));
    let child: u32 = std::fs::read_to_string(&pidfile)
        .expect("child pidfile")
        .trim()
        .parse()
        .expect("child pid");
    let daemon: u32 = std::fs::read_to_string(&rilld_pidfile)
        .expect("rilld pidfile")
        .trim()
        .parse()
        .expect("rilld pid");
    assert!(alive(child), "shell not running");
    assert!(alive(daemon), "rilld not running");

    let mut stream = UnixStream::connect(&sock).expect("attach");
    let bytes = Frame::Attach { generation: 1 }
        .encode()
        .expect("enc");
    stream.write_all(&bytes).expect("attach write");
    stream
        .write_all(&Frame::Credit(u32::MAX).encode().expect("credit"))
        .expect("credit");
    stream
        .write_all(
            &Frame::Data(b"printf 'PERSIST-MARK'\n".to_vec())
                .encode()
                .expect("data"),
        )
        .expect("printf");
    thread::sleep(Duration::from_millis(250));
    drop(stream);

    // AppKit quit / GUI SIGKILL of the process group (the user-reported path).
    unsafe {
        libc::kill(-(gui_pgid as i32), libc::SIGHUP);
        libc::kill(-(gui_pgid as i32), libc::SIGKILL);
    }
    let _ = gui.wait();
    thread::sleep(Duration::from_millis(200));

    assert!(
        alive(daemon),
        "quit app killed rilld — session does not persist"
    );
    assert!(
        alive(child),
        "quit app killed zsh — session does not persist (child pid {})",
        child
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
        .write_all(&Frame::Attach { generation: 2 }.encode().expect("a2"))
        .expect("reattach");
    stream
        .write_all(&Frame::Credit(u32::MAX).encode().expect("c2"))
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
    let mut chip = Chip0::new(80, 24).expect("chip");
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
}
