//! Control-plane proxy: public attach socket to a surviving worker.

use crate::endpoint::{authorize_peer, ensure_protected_parent};
use crate::error::Error;
use rill_attach::{cold_identity_socket_path, worker_socket_path};
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
}

pub fn run_control(attach_sock: &Path) -> Result<(), Error> {
    ensure_protected_parent(attach_sock)?;
    let worker_sock = worker_socket_path(attach_sock);
    ensure_protected_parent(&worker_sock)?;

    if !worker_alive(&worker_sock) {
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
            fds.push(libc::pollfd {
                fd: p.a.as_raw_fd(),
                events: libc::POLLIN | libc::POLLOUT,
                revents: 0,
            });
            fds.push(libc::pollfd {
                fd: p.b.as_raw_fd(),
                events: libc::POLLIN | libc::POLLOUT,
                revents: 0,
            });
        }
        unsafe {
            libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, 50);
        }
    }
}

fn splice(p: &mut Pipe) -> Result<(), Error> {
    copy_nb(&mut p.a, &mut p.b)?;
    copy_nb(&mut p.b, &mut p.a)?;
    Ok(())
}

fn copy_nb(from: &mut UnixStream, to: &mut UnixStream) -> Result<(), Error> {
    let mut buf = [0u8; 8192];
    match from.read(&mut buf) {
        Ok(0) => return Err(Error::PeerRefused),
        Ok(n) => match to.write(&buf[..n]) {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(()),
            Err(e) => Err(e.into()),
        },
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => Ok(()),
        Err(e) => Err(e.into()),
    }
}
