//! T-MOBILE-BACKGROUND-DETACH (#326).
//!
//! Required mutation: `RILL_MUTATE=background_terminates`.

use rill_attach::{Decoder, Frame};
use std::fs;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

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

fn unique_sock() -> PathBuf {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = PathBuf::from(format!("/tmp/rm{n:x}"));
    fs::create_dir_all(&dir).expect("dir");
    let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    dir.join("a")
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

fn wait_pidfile(path: &PathBuf, timeout: Duration) -> Option<u32> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Ok(s) = fs::read_to_string(path) {
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

fn alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

/// Backgrounding drops the attach lease, not the child. Desktop can reconnect.
#[test]
fn t_mobile_background_detach_does_not_terminate_the_session() {
    let gui_bin = gui_bin();
    assert!(
        gui_bin.is_file(),
        "T-MOBILE-BACKGROUND-DETACH needs the packaged GUI at {}. Run: sh scripts/package-macos.sh",
        gui_bin.display()
    );

    let sock = unique_sock();
    let pidfile = PathBuf::from(format!("{}.child", sock.display()));
    let mark = b"BG-KEEP-ALIVE";

    let mut gui_cmd = Command::new(&gui_bin);
    gui_cmd
        .env("RILL_SOCKET", &sock)
        .env("RILL_TEST_PIDFILE", &pidfile)
        .env("RILL_TEST_MOBILE_BACKGROUND", "1")
        .env("SHELL", "/bin/sh")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    if let Ok(m) = std::env::var("RILL_MUTATE") {
        gui_cmd.env("RILL_MUTATE", m);
    }
    let mut gui = gui_cmd.spawn().expect("spawn packaged Rill");
    let gui_pid = gui.id();

    assert!(
        wait_sock(&sock, Duration::from_secs(8)),
        "rilld did not bind {}",
        sock.display()
    );
    let child = wait_pidfile(&pidfile, Duration::from_secs(5)).expect("child pidfile");
    assert!(alive(child), "shell not running");
    // GUI holds the slot until the background hook shutdowns the attach fd.
    thread::sleep(Duration::from_millis(800));
    assert!(alive(child), "background callback terminated child {child}");
    assert!(alive(gui_pid), "GUI exited during background fixture");

    let mut stream = UnixStream::connect(&sock).expect("desktop reconnect");
    stream
        .write_all(&Frame::attach(2, None).encode().expect("a2"))
        .expect("reattach");
    stream
        .write_all(
            &Frame::Data(b"printf BG-KEEP-ALIVE\n".to_vec())
                .encode()
                .expect("d2"),
        )
        .expect("input");
    stream
        .write_all(&Frame::Credit(256 * 1024).encode().expect("c2"))
        .expect("credit");
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
    assert!(
        raw.windows(mark.len()).any(|w| w == mark),
        "desktop reconnect did not reach the original session"
    );
    assert!(alive(child), "desktop reconnect found a dead child {child}");

    unsafe {
        libc::kill(gui_pid as i32, libc::SIGTERM);
    }
    let _ = gui.wait();
    let _ = fs::remove_file(&pidfile);
}
