//! T-LOOK-GLASS — background-opacity must not make the window translucent.
//!
//! Bug (doc comment, not the name — ADR 0002 D6): windowed launch set
//! `NSWindow.alphaValue` from `background-opacity = 0.95`, so the Metal
//! surface was glass and the theme looked washed out.
//!
//! Required mutation: `RILL_MUTATE=window_alpha_from_opacity`.

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
    PathBuf::from(format!("/tmp/rill-look-glass-{n}.sock"))
}

struct Heartbeat {
    seq: u32,
    fullscreen: Option<u32>,
    opaque: Option<u32>,
    alpha: Option<u32>,
}

fn read_heartbeat(path: &PathBuf) -> Option<Heartbeat> {
    let s = fs::read_to_string(path).ok()?;
    let mut seq = None;
    let mut fullscreen = None;
    let mut opaque = None;
    let mut alpha = None;
    for part in s.split_whitespace() {
        if let Some(v) = part.strip_prefix("seq=") {
            seq = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("fullscreen=") {
            fullscreen = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("opaque=") {
            opaque = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("alpha=") {
            alpha = v.parse().ok();
        }
    }
    Some(Heartbeat {
        seq: seq?,
        fullscreen,
        opaque,
        alpha,
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

/// Packaged window stays opaque when the look file sets background-opacity.
#[test]
fn t_background_opacity_does_not_make_the_window_glass() {
    let gui_bin = gui_bin();
    assert!(
        gui_bin.is_file(),
        "T-LOOK-GLASS needs the packaged GUI at {}. Run: sh scripts/package-macos.sh",
        gui_bin.display()
    );

    let sock = unique_sock();
    let heartbeat = PathBuf::from(format!("{}.hb", sock.display()));
    let config = PathBuf::from(format!("{}.config", sock.display()));
    let _ = fs::remove_file(&heartbeat);
    fs::write(&config, "background-opacity = 0.95\n").expect("write look file");

    let mut gui_cmd = Command::new(&gui_bin);
    gui_cmd
        .env("RILL_SOCKET", &sock)
        .env("RILL_TEST_HEARTBEAT", &heartbeat)
        .env("RILL_CONFIG", &config)
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

    let windowed = wait_heartbeat(&heartbeat, pid, Duration::from_secs(6), |h| {
        h.fullscreen == Some(0) && h.seq >= 2
    });
    if windowed.is_none() || !alive(pid) {
        let still = alive(pid);
        let _ = gui.kill();
        let _ = gui.wait();
        let hb = fs::read_to_string(&heartbeat).ok();
        let _ = fs::remove_file(&heartbeat);
        let _ = fs::remove_file(&sock);
        let _ = fs::remove_file(&config);
        panic!("GUI died or entered fullscreen; heartbeat={hb:?} alive={still}");
    }

    let hb = windowed.unwrap();
    let opaque = hb.opaque;
    let alpha = hb.alpha;
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    let _ = gui.wait();
    let _ = fs::remove_file(&heartbeat);
    let _ = fs::remove_file(&sock);
    let _ = fs::remove_file(&config);

    assert_eq!(
        opaque,
        Some(1),
        "window/layer must stay opaque with background-opacity = 0.95; heartbeat opaque={opaque:?}"
    );
    assert_eq!(
        alpha,
        Some(100),
        "window.alphaValue must stay 1.0 (heartbeat alpha is percent); got {alpha:?} — glass washes the theme"
    );
}
