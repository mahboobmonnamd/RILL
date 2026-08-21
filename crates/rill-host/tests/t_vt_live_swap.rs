//! T-VT-LIVE-REPLIES and wide-tail paint (SPEC-VT-LIVE-SWAP).
//!
//! Required mutation: `RILL_MUTATE=ignore_wide_bits`.

use rill_attach::{cold_identity_socket_path, Decoder, Frame};
use rill_chip0::HostSurface;
use rill_host::{should_paint_cell, Client};
use rill_vt_types::ATTR_WIDE_TAIL;
use std::io::{Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

fn surface() -> HostSurface {
    HostSurface {
        font_family: "Menlo".into(),
        font_size: 13.0,
        font_fallbacks: Vec::new(),
        cols: 80,
        rows: 24,
        theme: None,
        padding_x: 0.0,
        padding_y: 0.0,
        background_opacity: 1.0,
        macos_option_as_alt: false,
        colors: None,
    }
}

fn runtime_sock() -> PathBuf {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    let dir = PathBuf::from(format!("/tmp/rl{:x}", n));
    std::fs::create_dir_all(&dir).expect("dir");
    let _ = std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700));
    dir.join("a")
}

/// DSR from PTY-shaped DATA must come back as attach DATA toward the child.
#[test]
fn t_vt_live_replies_dsr_is_written_as_data() {
    let sock = runtime_sock();
    let ident = cold_identity_socket_path(&sock);
    let _ = std::fs::remove_file(&ident);
    let id_l = UnixListener::bind(&ident).expect("identity bind");
    let attach_l = UnixListener::bind(&sock).expect("attach bind");
    attach_l.set_nonblocking(true).ok();

    let ident_h = thread::spawn(move || {
        let (mut s, _) = id_l.accept().expect("id accept");
        let _ = s.write_all(b"local\n");
    });

    let attach_h = thread::spawn(move || {
        let start = std::time::Instant::now();
        let mut peer = loop {
            match attach_l.accept() {
                Ok((s, _)) => break s,
                Err(_) if start.elapsed() < Duration::from_secs(2) => {
                    thread::sleep(Duration::from_millis(10));
                }
                Err(e) => panic!("attach accept: {e}"),
            }
        };
        peer.set_nonblocking(true).ok();
        let mut dec = Decoder::new();
        let mut buf = [0u8; 4096];
        let mut got_attach = false;
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::time::Instant::now() < deadline && !got_attach {
            match peer.read(&mut buf) {
                Ok(n) if n > 0 => {
                    for f in dec.push(&buf[..n]).expect("dec") {
                        if matches!(f, Frame::Attach { .. }) {
                            got_attach = true;
                        }
                    }
                }
                _ => thread::sleep(Duration::from_millis(5)),
            }
        }
        assert!(got_attach, "client never attached");
        let dsr = Frame::Data(b"\x1b[6n".to_vec()).encode().expect("dsr");
        peer.write_all(&dsr).expect("write dsr");
        let mut replies = Vec::new();
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while std::time::Instant::now() < deadline {
            match peer.read(&mut buf) {
                Ok(n) if n > 0 => {
                    for f in dec.push(&buf[..n]).expect("dec2") {
                        if let Frame::Data(b) = f {
                            replies.extend(b);
                        }
                    }
                    if replies.windows(3).any(|w| w == b"\x1b[" || w[0] == b'\x1b') {
                        break;
                    }
                }
                _ => thread::sleep(Duration::from_millis(10)),
            }
        }
        replies
    });

    thread::sleep(Duration::from_millis(30));
    let mut client = Client::connect(&sock, surface()).expect("connect");
    for _ in 0..40 {
        let _ = client.pump();
        thread::sleep(Duration::from_millis(10));
    }
    ident_h.join().ok();
    let replies = attach_h.join().expect("attach thread");
    assert!(
        replies.starts_with(&[0x1b, b'[']) || replies.iter().any(|&b| b == b'R'),
        "DSR was not written back as DATA: {replies:?}"
    );
}

#[test]
fn t_vt_live_wide_tail_is_not_painted() {
    assert!(should_paint_cell(0));
    assert!(!should_paint_cell(ATTR_WIDE_TAIL));
}
