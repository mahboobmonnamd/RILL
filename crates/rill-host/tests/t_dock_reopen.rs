//! T-DOCK-REOPEN — Dock click shows the window.
//!
//! Bug (doc comment, not the name — ADR 0002 D6): after `make run`, switching
//! to another app and clicking Rill in the Dock does not show the window;
//! quit and `make run` again is required.
//!
//! Required mutation: `RILL_MUTATE=skip_dock_reopen`.

use std::fs;
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
    PathBuf::from(format!("/tmp/rill-dock-reopen-{n}.sock"))
}

struct Heartbeat {
    seq: u32,
    visible: Option<u32>,
    key: Option<u32>,
}

fn read_heartbeat(path: &PathBuf) -> Option<Heartbeat> {
    let s = fs::read_to_string(path).ok()?;
    let mut seq = None;
    let mut visible = None;
    let mut key = None;
    for part in s.split_whitespace() {
        if let Some(v) = part.strip_prefix("seq=") {
            seq = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("visible=") {
            visible = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("key=") {
            key = v.parse().ok();
        }
    }
    Some(Heartbeat {
        seq: seq?,
        visible,
        key,
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

/// Dock click of a running app must make the existing window key and visible.
#[test]
fn t_dock_reopen_makes_the_window_key_and_visible() {
    let gui_bin = gui_bin();
    assert!(
        gui_bin.is_file(),
        "T-DOCK-REOPEN needs the packaged GUI at {}. Run: sh scripts/package-macos.sh",
        gui_bin.display()
    );

    let sock = unique_sock();
    let heartbeat = PathBuf::from(format!("{}.hb", sock.display()));
    let _ = fs::remove_file(&heartbeat);

    let mut gui_cmd = Command::new(&gui_bin);
    gui_cmd
        .env("RILL_SOCKET", &sock)
        .env("RILL_TEST_HEARTBEAT", &heartbeat)
        .env("RILL_TEST_DOCK_REOPEN", "1")
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

    let shown = wait_heartbeat(&heartbeat, pid, Duration::from_secs(6), |h| {
        h.visible == Some(1) && h.seq >= 1
    });
    if shown.is_none() || !alive(pid) {
        let _ = gui.kill();
        let _ = gui.wait();
        panic!(
            "GUI died or never showed a window; heartbeat={:?}",
            fs::read_to_string(&heartbeat).ok()
        );
    }

    let hidden = wait_heartbeat(&heartbeat, pid, Duration::from_secs(4), |h| {
        h.visible == Some(0)
    });
    if hidden.is_none() || !alive(pid) {
        let still = alive(pid);
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
        let _ = gui.wait();
        let hb = fs::read_to_string(&heartbeat).ok();
        let _ = fs::remove_file(&heartbeat);
        let _ = fs::remove_file(&sock);
        panic!("window never ordered out before reopen; heartbeat={hb:?} alive={still}");
    }

    let restored = wait_heartbeat(&heartbeat, pid, Duration::from_secs(4), |h| {
        h.visible == Some(1) && h.key == Some(1)
    });
    if restored.is_none() {
        let still = alive(pid);
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
        let _ = gui.wait();
        let hb = fs::read_to_string(&heartbeat).ok();
        let _ = fs::remove_file(&heartbeat);
        let _ = fs::remove_file(&sock);
        panic!("dock reopen did not show the window; heartbeat={hb:?} alive={still}");
    }

    let seq0 = restored.unwrap().seq;
    thread::sleep(Duration::from_millis(300));
    assert!(alive(pid), "GUI died after dock reopen");
    let after = read_heartbeat(&heartbeat).expect("heartbeat after reopen");
    assert!(
        after.seq > seq0,
        "main thread stopped after dock reopen (seq {seq0} then {})",
        after.seq
    );
    assert_eq!(after.visible, Some(1), "window not visible after reopen");
    assert_eq!(after.key, Some(1), "window not key after reopen");

    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    let _ = gui.wait();
    let _ = fs::remove_file(&heartbeat);
    let _ = fs::remove_file(&sock);
}
