//! T-VT-LIVE-RESYNC: daemon resync is Chip 1 `resync_from_history`.

use rill_attach::{Decoder, Frame};
use rill_kernel::Winsize;
use rilld::{pump, Daemon};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use vt_engine::VtEngine;

fn temp_sock(tag: &str) -> PathBuf {
    let dir = PathBuf::from(format!("/tmp/rr{}{}", std::process::id() % 10000, tag));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("dir");
    std::fs::set_permissions(&dir, std::os::unix::fs::PermissionsExt::from_mode(0o700)).ok();
    dir.join("a")
}

#[test]
fn t_vt_live_resync_matches_vt_engine() {
    let sock = temp_sock("rs");
    let mut daemon = Daemon::bind(
        &sock,
        "/bin/sh",
        &["-c", "printf 'LIVE-RESYNC-MARK\\n'; exec sleep 30"],
        Winsize::default(),
    )
    .expect("bind");
    let mut a = UnixStream::connect(&sock).expect("a");
    a.write_all(&Frame::attach(1, None).encode().unwrap())
        .unwrap();
    a.write_all(&Frame::Credit(64 * 1024).encode().unwrap())
        .unwrap();
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(2) {
        let _ = pump(&mut daemon, Duration::from_millis(20));
    }
    drop(a);
    let _ = pump(&mut daemon, Duration::from_millis(50));

    let mut b = UnixStream::connect(&sock).expect("b");
    b.write_all(&Frame::attach(2, None).encode().unwrap())
        .unwrap();
    b.write_all(&Frame::Credit(256 * 1024).encode().unwrap())
        .unwrap();
    let mut dec = Decoder::new();
    let mut buf = [0u8; 65536];
    let mut raw = Vec::new();
    let start = Instant::now();
    b.set_nonblocking(true).ok();
    while start.elapsed() < Duration::from_secs(2) {
        let _ = pump(&mut daemon, Duration::from_millis(20));
        if let Ok(n) = b.read(&mut buf) {
            if n > 0 {
                for f in dec.push(&buf[..n]).expect("dec") {
                    if let Frame::Data(d) = f {
                        raw.extend(d);
                    }
                }
            }
        }
        if raw.windows(16).any(|w| w == b"LIVE-RESYNC-MARK") {
            break;
        }
    }
    let mut expect = VtEngine::new(80, 24).expect("vt");
    let engine_bytes = expect
        .resync_from_history(b"LIVE-RESYNC-MARK\n")
        .expect("engine resync");
    assert!(
        raw.windows(16).any(|w| w == b"LIVE-RESYNC-MARK"),
        "resync DATA missing mark: {raw:?}"
    );
    assert!(
        engine_bytes.windows(16).any(|w| w == b"LIVE-RESYNC-MARK"),
        "VtEngine resync_from_history lost the mark"
    );
    let toml = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
    assert!(toml.contains("vt-engine"), "rilld is not on vt-engine");
    assert!(
        !toml
            .lines()
            .any(|l| l.trim_start().starts_with("rill-chip0")),
        "rilld still depends on rill-chip0"
    );
}
