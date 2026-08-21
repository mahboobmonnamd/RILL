//! T-FS-EXIT — leaving fullscreen must not hang the window.
//!
//! Bug (doc comment, not the name — ADR 0002 D6): clicking the button to
//! return to a normal window hangs; force quit is required. Default launch is
//! windowed (ADR 0017); this test enters a Space via
//! `RILL_TEST_EXIT_FULLSCREEN=1` then leaves.
//!
//! Required mutation: `RILL_MUTATE=wait_forever_on_inflight`.

use std::fs;
use std::os::unix::fs::PermissionsExt;
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
    let dir = PathBuf::from(format!("/tmp/rf{n:x}"));
    let _ = fs::create_dir_all(&dir);
    let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    dir.join("a")
}

struct Heartbeat {
    seq: u32,
    fullscreen: Option<u32>,
}

fn read_heartbeat(path: &PathBuf) -> Option<Heartbeat> {
    let s = fs::read_to_string(path).ok()?;
    let mut seq = None;
    let mut fullscreen = None;
    for part in s.split_whitespace() {
        if let Some(v) = part.strip_prefix("seq=") {
            seq = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("fullscreen=") {
            fullscreen = v.parse().ok();
        }
    }
    Some(Heartbeat {
        seq: seq?,
        fullscreen,
    })
}

fn alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

fn wait_heartbeat(
    path: &PathBuf,
    pid: u32,
    timeout: Duration,
    pred: impl Fn(&Heartbeat) -> bool,
) -> Option<Heartbeat> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if !alive(pid) {
            return None;
        }
        if let Some(hb) = read_heartbeat(path) {
            if pred(&hb) {
                return Some(hb);
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
    None
}

/// Clicking out of fullscreen must not hang (force quit).
#[test]
fn t_exit_fullscreen_does_not_hang_the_window() {
    let gui_bin = gui_bin();
    assert!(
        gui_bin.is_file(),
        "T-FS-EXIT needs the packaged GUI at {}. Run: sh scripts/package-macos.sh",
        gui_bin.display()
    );

    let sock = unique_sock();
    let heartbeat = PathBuf::from(format!("{}.hb", sock.display()));
    let _ = fs::remove_file(&heartbeat);

    let mut gui_cmd = Command::new(&gui_bin);
    gui_cmd
        .env("RILL_SOCKET", &sock)
        .env("RILL_TEST_HEARTBEAT", &heartbeat)
        .env("RILL_TEST_EXIT_FULLSCREEN", "1")
        .env("SHELL", "/bin/sh")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    if let Ok(m) = std::env::var("RILL_MUTATE") {
        gui_cmd.env("RILL_MUTATE", m);
    }
    let mut gui = gui_cmd.spawn().expect("spawn packaged Rill");
    let pid = gui.id();

    let entered = wait_heartbeat(&heartbeat, pid, Duration::from_secs(6), |h| {
        h.fullscreen == Some(1)
    });
    if entered.is_none() || !alive(pid) {
        let _ = gui.kill();
        let _ = gui.wait();
        panic!(
            "GUI died or never entered fullscreen; heartbeat={:?}",
            fs::read_to_string(&heartbeat).ok()
        );
    }

    let left = wait_heartbeat(&heartbeat, pid, Duration::from_secs(5), |h| {
        h.fullscreen == Some(0)
    });
    if left.is_none() {
        let still = alive(pid);
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
        let _ = gui.wait();
        let _ = fs::remove_file(&heartbeat);
        let _ = fs::remove_file(&sock);
        panic!(
            "leaving fullscreen hung or never completed; heartbeat={:?} alive={}",
            fs::read_to_string(&heartbeat).ok(),
            still
        );
    }

    let seq0 = left.unwrap().seq;
    thread::sleep(Duration::from_millis(400));
    assert!(alive(pid), "GUI died after leaving fullscreen");
    let after = read_heartbeat(&heartbeat).expect("heartbeat after leave");
    assert!(
        after.seq > seq0,
        "main thread stopped after leave (seq {seq0} then {}); force quit was required",
        after.seq
    );

    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    let _ = gui.wait();
    let _ = fs::remove_file(&heartbeat);
    let _ = fs::remove_file(&sock);
}
