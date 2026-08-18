//! T-SPLIT — window is three panes around Chip 0.
//!
//! Bug (doc comment, not the name — ADR 0002 D6): `contentView` was the
//! `MTKView` alone, so there was no navigation column and no inspector.
//!
//! Required mutation: `RILL_MUTATE=no_chrome`.

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
    PathBuf::from(format!("/tmp/rill-split-{n}.sock"))
}

struct Heartbeat {
    seq: u32,
    chrome: Option<u32>,
    left: Option<f64>,
    center: Option<f64>,
    right: Option<f64>,
    first: Option<String>,
}

fn read_heartbeat(path: &PathBuf) -> Option<Heartbeat> {
    let s = fs::read_to_string(path).ok()?;
    let mut seq = None;
    let mut chrome = None;
    let mut left = None;
    let mut center = None;
    let mut right = None;
    let mut first = None;
    for part in s.split_whitespace() {
        if let Some(v) = part.strip_prefix("seq=") {
            seq = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("chrome=") {
            chrome = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("left=") {
            left = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("center=") {
            center = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("right=") {
            right = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("first=") {
            first = Some(v.to_string());
        }
    }
    Some(Heartbeat {
        seq: seq?,
        chrome,
        left,
        center,
        right,
        first,
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

/// Default launch must be nav | Chip 0 | inspector, not a lone MTKView.
#[test]
fn t_window_is_three_pane_split_around_chip0() {
    let gui_bin = gui_bin();
    assert!(
        gui_bin.is_file(),
        "T-SPLIT needs the packaged GUI at {}. Run: sh scripts/package-macos.sh",
        gui_bin.display()
    );

    let sock = unique_sock();
    let heartbeat = PathBuf::from(format!("{}.hb", sock.display()));
    let _ = fs::remove_file(&heartbeat);

    let mut gui_cmd = Command::new(&gui_bin);
    gui_cmd
        .env("RILL_SOCKET", &sock)
        .env("RILL_TEST_HEARTBEAT", &heartbeat)
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

    let laid_out = wait_heartbeat(&heartbeat, pid, Duration::from_secs(8), |h| h.seq >= 1);
    if laid_out.is_none() || !alive(pid) {
        let _ = gui.kill();
        let _ = gui.wait();
        panic!(
            "GUI died or never wrote a heartbeat; heartbeat={:?}",
            fs::read_to_string(&heartbeat).ok()
        );
    }

    let split = wait_heartbeat(&heartbeat, pid, Duration::from_secs(4), |h| {
        h.chrome == Some(3)
            && h.left.unwrap_or(0.0) > 0.0
            && h.right.unwrap_or(0.0) > 0.0
            && h.center.unwrap_or(0.0) > 0.0
            && h.first.as_deref() == Some("terminal")
    });
    let raw = fs::read_to_string(&heartbeat).ok();
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    let _ = gui.wait();
    let _ = fs::remove_file(&heartbeat);
    let _ = fs::remove_file(&sock);

    let hb = split.unwrap_or_else(|| {
        panic!("window is not a three-pane split around Chip 0; heartbeat={raw:?}")
    });
    assert_eq!(hb.chrome, Some(3), "chrome columns; heartbeat={raw:?}");
    assert!(
        hb.left.unwrap_or(0.0) > 0.0 && hb.right.unwrap_or(0.0) > 0.0,
        "sidebars have width; heartbeat={raw:?}"
    );
    assert_eq!(
        hb.first.as_deref(),
        Some("terminal"),
        "first responder is Chip 0; heartbeat={raw:?}"
    );
}
