//! T-RUNTIME-GUI-INDEPENDENT (#325).
//!
//! Production must not posix_spawn an unregistered daemon. Required mutation:
//! `RILL_MUTATE=posix_spawn_unregistered`.

use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn gui_binary() -> PathBuf {
    if let Ok(p) = std::env::var("RILL_GUI_BIN") {
        return PathBuf::from(p);
    }
    repo_root().join("dist/Rill.app/Contents/MacOS/Rill")
}

fn plist() -> PathBuf {
    repo_root().join("dist/Rill.app/Contents/Library/LaunchAgents/dev.rill.rilld.plist")
}

fn children_named(ppid: u32, name: &str) -> Vec<u32> {
    let out = Command::new("pgrep")
        .args(["-P", &ppid.to_string(), "-f", name])
        .output()
        .expect("pgrep");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter_map(|s| s.trim().parse().ok())
        .collect()
}

/// The packaged bundle carries the per-user agent plist.
#[test]
fn t_runtime_gui_independent_ships_launch_agent_plist() {
    let bin = gui_binary();
    assert!(
        bin.is_file(),
        "T-RUNTIME-GUI-INDEPENDENT needs {}. Run: sh scripts/package-macos.sh",
        bin.display()
    );
    assert!(
        plist().is_file(),
        "missing LaunchAgent plist at {}",
        plist().display()
    );
    let nm = Command::new("nm").arg("-u").arg(&bin).output().expect("nm");
    let text = String::from_utf8_lossy(&nm.stdout);
    assert!(
        text.contains("SMAppService"),
        "GUI does not import SMAppService: production would be posix_spawn only"
    );
}

/// Without RILL_SOCKET, an unregistered GUI must not spawn rilld as its child.
#[test]
fn t_runtime_gui_independent_does_not_spawn_unregistered_daemon() {
    let bin = gui_binary();
    assert!(
        bin.is_file(),
        "T-RUNTIME-GUI-INDEPENDENT needs {}. Run: sh scripts/package-macos.sh",
        bin.display()
    );
    let mutate = std::env::var("RILL_MUTATE").unwrap_or_default();
    let mut cmd = Command::new(&bin);
    cmd.env_remove("RILL_SOCKET")
        .env_remove("RILL_DEV_DIRECT_RILLD")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if !mutate.is_empty() {
        cmd.env("RILL_MUTATE", &mutate);
    }
    let mut gui = cmd.spawn().expect("spawn GUI");
    let pid = gui.id();
    let start = Instant::now();
    let mut spawned = Vec::new();
    while start.elapsed() < Duration::from_millis(800) {
        spawned = children_named(pid, "rilld");
        if !spawned.is_empty() {
            break;
        }
        std::thread::sleep(Duration::from_millis(40));
    }
    let _ = gui.kill();
    let _ = gui.wait();
    for c in &spawned {
        unsafe {
            libc::kill(*c as i32, libc::SIGKILL);
        }
    }
    if mutate == "posix_spawn_unregistered" {
        assert!(
            !spawned.is_empty(),
            "mutation did not posix_spawn rilld under the GUI"
        );
    } else {
        assert!(
            spawned.is_empty(),
            "unregistered GUI posix_spawned rilld {spawned:?}"
        );
    }
}
