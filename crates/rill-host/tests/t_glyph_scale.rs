//! T-GLYPH-SCALE — atlas glyphs match backing-scale cell pixels.
//!
//! Bug (doc comment, not the name — ADR 0002 D6): CoreText rasterised at
//! font point size while Metal `cellPx` used backing pixels, so letters were
//! specks inside a full-size cursor on Retina.
//!
//! Required mutation: `RILL_MUTATE=skip_glyph_backing_scale`.

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
    let dir = PathBuf::from(format!("/tmp/ry{n:x}"));
    let _ = fs::create_dir_all(&dir);
    let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    dir.join("a")
}

struct Heartbeat {
    seq: u32,
    cell_px: Option<f64>,
    glyph_m: Option<f64>,
}

fn read_heartbeat(path: &PathBuf) -> Option<Heartbeat> {
    let s = fs::read_to_string(path).ok()?;
    let mut seq = None;
    let mut cell_px = None;
    let mut glyph_m = None;
    for part in s.split_whitespace() {
        if let Some(v) = part.strip_prefix("seq=") {
            seq = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("cell_px=") {
            cell_px = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("glyph_m=") {
            glyph_m = v.parse().ok();
        }
    }
    Some(Heartbeat {
        seq: seq?,
        cell_px,
        glyph_m,
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

/// Packaged Retina atlas `M` fills most of the cell, not a 1× speck.
#[test]
fn t_atlas_glyph_matches_backing_scale_cell() {
    let gui_bin = gui_bin();
    assert!(
        gui_bin.is_file(),
        "T-GLYPH-SCALE needs the packaged GUI at {}. Run: sh scripts/package-macos.sh",
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

    let shown = wait_heartbeat(&heartbeat, pid, Duration::from_secs(8), |h| {
        h.seq >= 2 && h.cell_px.unwrap_or(0.0) > 0.0 && h.glyph_m.unwrap_or(0.0) > 0.0
    });
    if shown.is_none() || !alive(pid) {
        let still = alive(pid);
        let _ = gui.kill();
        let _ = gui.wait();
        let hb = fs::read_to_string(&heartbeat).ok();
        let _ = fs::remove_file(&heartbeat);
        let _ = fs::remove_file(&sock);
        panic!("GUI died or never wrote glyph metrics; heartbeat={hb:?} alive={still}");
    }

    let hb = shown.unwrap();
    let cell_px = hb.cell_px.unwrap();
    let glyph_m = hb.glyph_m.unwrap();
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    let _ = gui.wait();
    let _ = fs::remove_file(&heartbeat);
    let _ = fs::remove_file(&sock);

    assert!(
        cell_px >= 24.0,
        "T-GLYPH-SCALE needs Retina cell_px (≥24px at 16pt); got {cell_px} — 1× displays cannot detect the bug"
    );
    let ratio = glyph_m / cell_px;
    assert!(
        ratio >= 0.7,
        "atlas M height {glyph_m} / cell_px {cell_px} = {ratio:.3} must be ≥ 0.7 (1× glyphs on 2× cells are ~0.5)"
    );
}
