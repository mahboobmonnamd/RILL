//! T-NAV-IDENTITY — the default leaf's kernel identity is rendered as the host indicator.
//!
//! Bug (doc comment, not the name — ADR 0002 D6): chrome derived its host
//! label from `NSHomeDirectory`, so it could show a local account name instead
//! of the identity of the leaf the daemon owns and the GUI attaches to.
//!
//! Required mutation: `RILL_MUTATE=host_indicator_from_home`.

use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
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
    PathBuf::from(format!("/tmp/rill-host-identity-{n}.sock"))
}

struct Heartbeat {
    seq: u32,
    host: Option<String>,
}

fn read_heartbeat(path: &Path) -> Option<Heartbeat> {
    let text = fs::read_to_string(path).ok()?;
    let mut seq = None;
    let mut host = None;
    for part in text.split_whitespace() {
        if let Some(value) = part.strip_prefix("seq=") {
            seq = value.parse().ok();
        }
        if let Some(value) = part.strip_prefix("host=") {
            host = Some(value.to_string());
        }
    }
    Some(Heartbeat { seq: seq?, host })
}

fn alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

/// The default daemon leaf must attach by the 8-byte path and render its
/// independently-read kernel host identity, never the GUI account directory.
#[test]
fn t_default_leaf_host_indicator_renders_kernel_local_identity() {
    let gui_bin = gui_bin();
    assert!(
        gui_bin.is_file(),
        "T-NAV-IDENTITY needs the packaged GUI at {}. Run: sh scripts/package-macos.sh",
        gui_bin.display()
    );

    let sock = unique_sock();
    let heartbeat = PathBuf::from(format!("{}.hb", sock.display()));
    let poisoned_home = PathBuf::from(format!("/tmp/rill-not-a-host-{}", std::process::id()));
    let _ = fs::remove_file(&heartbeat);
    fs::create_dir_all(&poisoned_home).expect("create poisoned home");

    let mut command = Command::new(&gui_bin);
    command
        .env("RILL_SOCKET", &sock)
        .env("RILL_TEST_HEARTBEAT", &heartbeat)
        .env("HOME", &poisoned_home)
        .env("SHELL", "/bin/sh")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    if let Ok(mutation) = std::env::var("RILL_MUTATE") {
        command.env("RILL_MUTATE", mutation);
    }
    let mut gui = command.spawn().expect("spawn packaged Rill");
    let pid = gui.id();

    let start = Instant::now();
    let mut observed = None;
    while start.elapsed() < Duration::from_secs(8) {
        if !alive(pid) {
            break;
        }
        if let Some(heartbeat) = read_heartbeat(&heartbeat) {
            if heartbeat.seq >= 1 && heartbeat.host.is_some() {
                observed = heartbeat.host;
                break;
            }
        }
        thread::sleep(Duration::from_millis(20));
    }

    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    let _ = gui.wait();
    let _ = fs::remove_file(&heartbeat);
    let _ = fs::remove_file(&sock);
    let _ = fs::remove_dir(&poisoned_home);

    assert_eq!(
        observed.as_deref(),
        Some("local"),
        "host indicator must render the default leaf's cold kernel identity, not HOME; observed={observed:?}"
    );
}
