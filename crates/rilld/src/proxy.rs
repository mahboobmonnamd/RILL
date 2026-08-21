//! Control-plane proxy: public attach socket to a surviving worker.

use crate::endpoint::{authorize_peer, ensure_protected_parent};
use crate::error::Error;
use rill_attach::{cold_identity_socket_path, worker_socket_path};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

pub fn worker_alive(worker_sock: &Path) -> bool {
    UnixStream::connect(worker_sock).is_ok()
}

pub fn spawn_worker(attach_sock: &Path, worker_sock: &Path) -> Result<std::process::Child, Error> {
    let exe = std::env::current_exe().map_err(Error::from)?;
    let mut cmd = Command::new(exe);
    cmd.env("RILL_WORKER", "1")
        .env("RILL_SOCKET", worker_sock)
        .stdin(Stdio::null())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if let Ok(v) = std::env::var("RILL_TEST_PIDFILE") {
        cmd.env("RILL_TEST_PIDFILE", v);
    }
    if let Ok(v) = std::env::var("RILL_TEST_SECOND_LEAF") {
        cmd.env("RILL_TEST_SECOND_LEAF", v);
    }
    if let Ok(v) = std::env::var("RILL_TEST_SECOND_PIDFILE") {
        cmd.env("RILL_TEST_SECOND_PIDFILE", v);
    }
    if let Ok(v) = std::env::var("RILL_ALLOW_NESTED") {
        cmd.env("RILL_ALLOW_NESTED", v);
    }
    if let Ok(v) = std::env::var("SHELL") {
        cmd.env("SHELL", v);
    }
    let child = cmd.spawn()?;
    let start = Instant::now();
    while start.elapsed() < Duration::from_secs(5) {
        if worker_alive(worker_sock) {
            return Ok(child);
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = attach_sock;
    Err(Error::WorkerMissing)
}

struct Pipe {
    a: UnixStream,
    b: UnixStream,
    a_to_b: VecDeque<u8>,
    b_to_a: VecDeque<u8>,
}

pub fn run_control(attach_sock: &Path) -> Result<(), Error> {
    ensure_protected_parent(attach_sock)?;
    let worker_sock = worker_socket_path(attach_sock);
    ensure_protected_parent(&worker_sock)?;

    if !worker_alive(&worker_sock) {
        let marker = attach_sock.with_file_name("shutdown");
        let recorded = std::env::var("RILL_TEST_PIDFILE")
            .ok()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|s| s.trim().parse().ok());
        let outcome = rill_kernel::reconcile_execution(recorded, false, marker.exists());
        if let Some(parent) = attach_sock.parent() {
            let _ = std::fs::write(parent.join("outcome"), format!("{outcome:?}\n"));
        }
        let child = spawn_worker(attach_sock, &worker_sock)?;
        std::mem::forget(child);
    }

    if let Ok(path) = std::env::var("RILL_TEST_DAEMON_PIDFILE") {
        std::fs::write(path, format!("{}\n", std::process::id()))?;
    }

    if attach_sock.exists() {
        if UnixStream::connect(attach_sock).is_ok() {
            return Err(Error::AlreadyRunning);
        }
        let _ = std::fs::remove_file(attach_sock);
    }
    let listener = UnixListener::bind(attach_sock)?;
    listener.set_nonblocking(true)?;
    std::fs::set_permissions(attach_sock, std::fs::Permissions::from_mode(0o600))?;

    let identity_path = cold_identity_socket_path(attach_sock);
    if identity_path.exists() {
        let _ = std::fs::remove_file(&identity_path);
    }
    let identity = UnixListener::bind(&identity_path)?;
    identity.set_nonblocking(true)?;
    std::fs::set_permissions(&identity_path, std::fs::Permissions::from_mode(0o600))?;
    let worker_identity = cold_identity_socket_path(&worker_sock);

    let mut pipes: Vec<Pipe> = Vec::new();
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                if authorize_peer(&stream).is_err() {
                    drop(stream);
                } else if let Ok(worker) = UnixStream::connect(&worker_sock) {
                    let _ = stream.set_nonblocking(true);
                    let _ = worker.set_nonblocking(true);
                    pipes.push(Pipe {
                        a: stream,
                        b: worker,
                        a_to_b: VecDeque::new(),
                        b_to_a: VecDeque::new(),
                    });
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.into()),
        }
        match identity.accept() {
            Ok((mut stream, _)) => {
                if authorize_peer(&stream).is_ok() {
                    if let Ok(mut w) = UnixStream::connect(&worker_identity) {
                        let mut buf = [0u8; 16];
                        if let Ok(n) = w.read(&mut buf) {
                            let _ = stream.write_all(&buf[..n]);
                        }
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.into()),
        }

        let mut i = 0;
        while i < pipes.len() {
            if splice(&mut pipes[i]).is_err() {
                pipes.remove(i);
            } else {
                i += 1;
            }
        }

        let mut fds = vec![
            libc::pollfd {
                fd: listener.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
            libc::pollfd {
                fd: identity.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            },
        ];
        for p in &pipes {
            let mut ev_a = libc::POLLIN;
            if !p.b_to_a.is_empty() {
                ev_a |= libc::POLLOUT;
            }
            let mut ev_b = libc::POLLIN;
            if !p.a_to_b.is_empty() {
                ev_b |= libc::POLLOUT;
            }
            fds.push(libc::pollfd {
                fd: p.a.as_raw_fd(),
                events: ev_a,
                revents: 0,
            });
            fds.push(libc::pollfd {
                fd: p.b.as_raw_fd(),
                events: ev_b,
                revents: 0,
            });
        }
        unsafe {
            libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, 50);
        }
    }
}

fn splice(p: &mut Pipe) -> Result<(), Error> {
    read_into(&mut p.a, &mut p.a_to_b)?;
    read_into(&mut p.b, &mut p.b_to_a)?;
    crate::write_outbox(&mut p.b, &mut p.a_to_b)?;
    crate::write_outbox(&mut p.a, &mut p.b_to_a)?;
    Ok(())
}

/// Read available bytes into `outbox`. Never discards a successful read
/// because the peer is not writable ([#334](https://github.com/mahboobmonnamd/RILL/issues/334)).
fn read_into(from: &mut UnixStream, outbox: &mut VecDeque<u8>) -> Result<(), Error> {
    let mut buf = [0u8; 8192];
    match from.read(&mut buf) {
        Ok(0) => Err(Error::PeerRefused),
        Ok(n) => {
            #[cfg(feature = "mutate")]
            if std::env::var("RILL_MUTATE").as_deref() == Ok("lossy_splice") {
                return Ok(());
            }
            outbox.extend(buf[..n].iter().copied());
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(()),
        Err(e) => Err(e.into()),
    }
}

#[cfg(test)]
mod splice_q1 {
    use super::*;
    use std::io::ErrorKind;
    use std::os::fd::AsRawFd;

    fn tiny_buf(fd: std::os::fd::RawFd, send: bool) {
        let n: libc::c_int = 2048;
        let opt = if send {
            libc::SO_SNDBUF
        } else {
            libc::SO_RCVBUF
        };
        unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                opt,
                &n as *const _ as *const libc::c_void,
                std::mem::size_of_val(&n) as libc::socklen_t,
            );
        }
    }

    /// T-PROXY-SPLICE — a blocked destination must not drop already-read bytes.
    ///
    /// Bug: `copy_nb` returned `Ok(())` on `WouldBlock` after `read` succeeded.
    /// Required mutation: `RILL_MUTATE=lossy_splice` (feature `mutate`).
    #[test]
    fn t_proxy_splice_does_not_drop_bytes_on_wouldblock() {
        let (mut from_w, mut from_r) = UnixStream::pair().expect("from");
        let (mut to_w, mut to_r) = UnixStream::pair().expect("to");
        from_r.set_nonblocking(true).expect("from nb");
        to_w.set_nonblocking(true).expect("to nb");
        to_r.set_nonblocking(true).expect("to r nb");
        tiny_buf(to_w.as_raw_fd(), true);
        tiny_buf(to_r.as_raw_fd(), false);
        let src: Vec<u8> = (0..80_000).map(|i| (i % 251) as u8).collect();
        let writer = std::thread::spawn({
            let src = src.clone();
            move || {
                from_w.write_all(&src).expect("fill");
                drop(from_w);
            }
        });
        let mut outbox = VecDeque::new();
        let mut got = Vec::new();
        let mut buf = [0u8; 4096];
        for _ in 0..50_000 {
            match read_into(&mut from_r, &mut outbox) {
                Ok(()) => {}
                Err(Error::PeerRefused) => {}
                Err(e) => panic!("read_into: {e}"),
            }
            crate::write_outbox(&mut to_w, &mut outbox).expect("flush");
            match to_r.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => got.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(e) => panic!("read: {e}"),
            }
            if got.len() == src.len() && outbox.is_empty() {
                break;
            }
        }
        writer.join().expect("writer");
        assert_eq!(got, src, "proxy splice dropped or desynced bytes");
    }
}
