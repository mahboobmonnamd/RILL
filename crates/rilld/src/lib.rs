//! Kernel daemon: owns the session PTY, framed attach, cold-path resync.

mod error;

use rill_attach::{Decoder, Frame};
use rill_chip0::Chip0;
use rill_kernel::{Kernel, Session, SessionId, Winsize};
use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub use error::Error;

pub struct Daemon {
    listener: UnixListener,
    socket_path: PathBuf,
    _lock: File,
    kernel: Kernel,
    default_id: SessionId,
    /// The connection currently holding the attach claim.
    client: Option<Client>,
    chip: Chip0,
}

struct Client {
    stream: UnixStream,
    decoder: Decoder,
    /// Partial writes. `write_all` on a non-blocking socket plus re-queue of
    /// the whole frame duplicated DATA (quality audit Q1).
    outbox: VecDeque<u8>,
}

impl Daemon {
    pub fn bind(
        socket_path: impl AsRef<Path>,
        shell: &str,
        args: &[&str],
        size: Winsize,
    ) -> Result<Self, Error> {
        let socket_path = socket_path.as_ref().to_path_buf();
        if let Some(dir) = socket_path.parent() {
            std::fs::create_dir_all(dir)?;
        }

        // An exclusive lock, taken before anything is unlinked.
        //
        // The previous sequence was exists() -> connect() -> remove_file() ->
        // bind(), which two daemons racing could both pass, with the second
        // unlinking the first's live socket (audit S3-7).
        let lock_path = socket_path.with_extension("lock");
        let lock = File::options()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)?;
        // SAFETY: lock is an open, owned file for the duration of the call.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(Error::AlreadyRunning);
        }

        // The lock is ours, so any socket at this path is stale unless someone
        // is answering on it.
        if socket_path.exists() {
            if UnixStream::connect(&socket_path).is_ok() {
                return Err(Error::AlreadyRunning);
            }
            std::fs::remove_file(&socket_path)?;
        }

        let listener = UnixListener::bind(&socket_path)?;
        listener.set_nonblocking(true)?;
        // The attach socket carries keystrokes and shell output. Do not leave
        // it world-writable (SPEC-ATTACH §1).
        std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))?;
        let mut kernel = Kernel::new();
        let default_id = kernel.spawn_leaf(shell, args, size)?;
        let child_pid = kernel
            .session(default_id)
            .ok_or(rill_kernel::Error::UnknownSession)?
            .child_pid();
        if let Ok(path) = std::env::var("RILL_TEST_PIDFILE") {
            std::fs::write(path, format!("{child_pid}\n"))?;
        }
        if let Ok(path) = std::env::var("RILL_TEST_DAEMON_PIDFILE") {
            std::fs::write(path, format!("{}\n", std::process::id()))?;
        }
        let chip = Chip0::new(size.cols, size.rows)?;
        Ok(Self {
            listener,
            socket_path,
            _lock: lock,
            kernel,
            default_id,
            client: None,
            chip,
        })
    }

    fn leaf(&self) -> Result<&Session, Error> {
        self.kernel
            .session(self.default_id)
            .ok_or(rill_kernel::Error::UnknownSession.into())
    }

    fn leaf_mut(&mut self) -> Result<&mut Session, Error> {
        self.kernel
            .session_mut(self.default_id)
            .ok_or(rill_kernel::Error::UnknownSession.into())
    }

    pub fn child_pid(&self) -> u32 {
        self.leaf().map(Session::child_pid).unwrap_or(0)
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Must stay at 1 however much the user types. Resync is a cold path
    /// (FR-RESYNC, SPEC-CHIP0 §7).
    pub fn resync_count(&self) -> u32 {
        self.leaf().map(Session::resync_count).unwrap_or(0)
    }

    pub fn stalled_reads(&self) -> u64 {
        self.leaf().map(Session::stalled_reads).unwrap_or(0)
    }

    pub fn child_alive(&self) -> bool {
        self.leaf().map(Session::child_alive).unwrap_or(false)
    }

    pub fn step(&mut self, timeout_ms: i32) -> Result<(), Error> {
        self.leaf_mut()?.poll_child()?;
        self.poll_io(timeout_ms)?;
        self.flush_outbound()?;
        self.maybe_resync()?;
        Ok(())
    }

    /// Timeout passed to `poll` by `run`.
    ///
    /// A live attach with credit must use `0`. Q5 applied a 50 ms sleep to
    /// that path; packaged hid p95 went from 7.011 ms to 12–13 ms because a
    /// sleeping daemon on battery misses vsync (cadence p95 16.67 ms). Idle
    /// (no client or no credit) still waits — that is the busy-loop fix.
    pub fn step_timeout_ms(&self) -> i32 {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("idle_poll_while_attached") {
            return 50;
        }
        let live = self
            .leaf()
            .ok()
            .is_some_and(|s| s.credit() > 0 && s.child_alive());
        if self.client.is_some() && live {
            0
        } else {
            50
        }
    }

    pub fn run(mut self) -> Result<(), Error> {
        loop {
            self.step(self.step_timeout_ms())?;
        }
    }

    fn maybe_resync(&mut self) -> Result<(), Error> {
        if let Some(history) = self.leaf_mut()?.take_resync_history() {
            let bytes = self.chip.resync_from_history(&history)?;
            if !bytes.is_empty() {
                self.leaf_mut()?.enqueue_outbound(Frame::Data(bytes));
            }
        }
        Ok(())
    }

    fn poll_io(&mut self, timeout_ms: i32) -> Result<(), Error> {
        let mut fds = Vec::new();
        fds.push(libc::pollfd {
            fd: self.listener.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        });
        let client_idx = match self.client.as_ref() {
            Some(c) => {
                let mut events = libc::POLLIN;
                if !c.outbox.is_empty() {
                    events |= libc::POLLOUT;
                }
                fds.push(libc::pollfd {
                    fd: c.stream.as_raw_fd(),
                    events,
                    revents: 0,
                });
                Some(1usize)
            }
            None => None,
        };
        let want_pty = {
            let s = self.leaf()?;
            s.credit() > 0 && s.child_alive()
        };
        let pty_ready = if want_pty {
            self.leaf()?.poll_with_extras(&mut fds, timeout_ms)?
        } else {
            // SAFETY: fds is a valid, non-empty slice of initialised pollfd.
            let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout_ms) };
            if n < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::Interrupted {
                    return Ok(());
                }
                return Err(err.into());
            }
            false
        };
        if fds[0].revents & libc::POLLIN != 0 {
            self.accept_client()?;
        }
        if let Some(i) = client_idx {
            if fds[i].revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0 {
                self.read_client()?;
            }
        }
        if pty_ready {
            loop {
                let s = self.leaf_mut()?;
                if !(s.credit() > 0 && s.on_pty_readable()? > 0) {
                    break;
                }
            }
        }
        Ok(())
    }

    fn accept_client(&mut self) -> Result<(), Error> {
        match self.listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(true)?;
                if std::env::var("RILL_TEST_TINY_SNDBUF").is_ok() {
                    // T-PARTIAL-WRITE: force short writes on the accepted
                    // socket. Integration tests link the non-cfg(test) lib.
                    let tiny: libc::c_int = 1024;
                    unsafe {
                        libc::setsockopt(
                            stream.as_raw_fd(),
                            libc::SOL_SOCKET,
                            libc::SO_SNDBUF,
                            &tiny as *const _ as *const libc::c_void,
                            std::mem::size_of_val(&tiny) as libc::socklen_t,
                        );
                    }
                }
                // A live connection holds its slot whether or not it has sent
                // ATTACH yet.
                //
                // The previous condition also required `session.attached()`, so
                // a client that connected and never attached could be silently
                // displaced by the next connection — FR-ONE bypassable by not
                // attaching (audit S3-6).
                let occupied = {
                    #[cfg(feature = "mutate")]
                    {
                        if std::env::var("RILL_MUTATE").as_deref() == Ok("accept_replaces_client") {
                            false
                        } else {
                            self.client.is_some()
                        }
                    }
                    #[cfg(not(feature = "mutate"))]
                    {
                        self.client.is_some()
                    }
                };
                if occupied {
                    let mut s = stream;
                    let frame = Frame::Refused {
                        reason: rill_attach::RefuseReason::AlreadyAttached,
                    };
                    let _ = s.write_all(&frame.encode()?);
                    return Ok(());
                }
                self.client = Some(Client {
                    stream,
                    decoder: Decoder::new(),
                    outbox: VecDeque::new(),
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.into()),
        }
        Ok(())
    }

    fn read_client(&mut self) -> Result<(), Error> {
        let Some(client) = self.client.as_mut() else {
            return Ok(());
        };
        let mut buf = [0u8; 8192];
        match client.stream.read(&mut buf) {
            Ok(0) => {
                if let Some(s) = self.kernel.session_mut(self.default_id) {
                    s.detach();
                }
                self.client = None;
                return Ok(());
            }
            Ok(n) => {
                let frames = client.decoder.push(&buf[..n])?;
                for frame in frames {
                    match self.kernel.on_frame(self.default_id, frame) {
                        Ok(()) => {}
                        Err(rill_kernel::Error::Dead) => {}
                        Err(e) => return Err(e.into()),
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => {
                if let Some(s) = self.kernel.session_mut(self.default_id) {
                    s.detach();
                }
                self.client = None;
            }
        }
        Ok(())
    }

    fn flush_outbound(&mut self) -> Result<(), Error> {
        // With no client, leave control frames queued. The previous version
        // drained and discarded everything here, so an EXIT that arrived while
        // the window was closed was destroyed and the reopened window painted a
        // live cursor over a dead process (audit S3-2).
        let Some(session) = self.kernel.session_mut(self.default_id) else {
            return Err(rill_kernel::Error::UnknownSession.into());
        };
        let Some(client) = self.client.as_mut() else {
            let mut keep = Vec::new();
            while let Some(f) = session.pop_outbound() {
                if !matches!(f, Frame::Data(_)) {
                    keep.push(f);
                }
            }
            for f in keep {
                session.enqueue_outbound(f);
            }
            return Ok(());
        };

        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("replay_full_frame") {
            while let Some(frame) = session.pop_outbound() {
                let bytes = frame.encode()?;
                match client.stream.write_all(&bytes) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        session.enqueue_outbound(frame);
                        break;
                    }
                    Err(_) => {
                        session.detach();
                        self.client = None;
                        break;
                    }
                }
            }
            return Ok(());
        }

        while let Some(frame) = session.pop_outbound() {
            client.outbox.extend(frame.encode()?);
        }
        let failed = write_outbox(&mut client.stream, &mut client.outbox).is_err();
        if failed {
            session.detach();
            self.client = None;
        }
        Ok(())
    }
}

/// Non-blocking drain of `outbox`. Partial progress stays queued (Q1).
fn write_outbox(stream: &mut UnixStream, outbox: &mut VecDeque<u8>) -> Result<(), Error> {
    while !outbox.is_empty() {
        let n = {
            let (front, back) = outbox.as_slices();
            let slice = if !front.is_empty() { front } else { back };
            match stream.write(slice) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e.into()),
            }
        };
        outbox.drain(..n);
    }
    Ok(())
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
    }
}

pub fn default_socket() -> PathBuf {
    if let Ok(p) = std::env::var("RILL_SOCKET") {
        return PathBuf::from(p);
    }
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("/tmp/rill-{uid}.sock"))
}

pub fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into())
}

/// Drive the daemon for a bounded time. Used by named tests.
pub fn pump(daemon: &mut Daemon, duration: Duration) -> Result<(), Error> {
    let start = std::time::Instant::now();
    while start.elapsed() < duration {
        daemon.step(20)?;
    }
    Ok(())
}

#[cfg(test)]
mod write_outbox_q1 {
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

    fn payload(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn t_write_outbox_does_not_replay_bytes_the_peer_already_has() {
        let (mut w, mut r) = UnixStream::pair().expect("pair");
        w.set_nonblocking(true).expect("w nb");
        r.set_nonblocking(true).expect("r nb");
        tiny_buf(w.as_raw_fd(), true);
        tiny_buf(r.as_raw_fd(), false);
        let src = payload(120_000);
        let mut outbox = VecDeque::from(src.clone());
        let mut got = Vec::new();
        let mut buf = [0u8; 4096];
        for _ in 0..50_000 {
            write_outbox(&mut w, &mut outbox).expect("flush");
            match r.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => got.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(e) => panic!("read: {e}"),
            }
            if got.len() == src.len() && outbox.is_empty() {
                break;
            }
        }
        assert_eq!(got, src, "outbox replayed or dropped bytes");
    }

    #[test]
    fn t_write_all_requeue_replays_a_prefix() {
        let (mut w, mut r) = UnixStream::pair().expect("pair");
        w.set_nonblocking(true).expect("w nb");
        r.set_nonblocking(true).expect("r nb");
        tiny_buf(w.as_raw_fd(), true);
        tiny_buf(r.as_raw_fd(), false);
        let src = payload(120_000);
        let mut pending = VecDeque::from(vec![src.clone()]);
        let mut got = Vec::new();
        let mut buf = [0u8; 4096];
        for _ in 0..50_000 {
            if let Some(frame) = pending.pop_front() {
                match w.write_all(&frame) {
                    Ok(()) => {}
                    Err(e) if e.kind() == ErrorKind::WouldBlock => {
                        pending.push_back(frame);
                    }
                    Err(e) => panic!("write: {e}"),
                }
            }
            match r.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => got.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(e) => panic!("read: {e}"),
            }
            if pending.is_empty() && got.len() >= src.len() {
                break;
            }
        }
        assert_ne!(
            got, src,
            "oracle is blind: write_all+requeue must differ from the source"
        );
    }
}
