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
    /// One connection per attach claim. A second connection MAY attach a
    /// different id (ADR 0011 D3). FR-ONE is per leaf, not per daemon.
    clients: Vec<Client>,
    chip: Chip0,
}

struct Client {
    stream: UnixStream,
    decoder: Decoder,
    /// Partial writes. `write_all` on a non-blocking socket plus re-queue of
    /// the whole frame duplicated DATA (quality audit Q1).
    outbox: VecDeque<u8>,
    leaf: Option<SessionId>,
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
            clients: Vec::new(),
            chip,
        })
    }

    pub fn default_id(&self) -> SessionId {
        self.default_id
    }

    pub fn spawn_leaf(
        &mut self,
        shell: &str,
        args: &[&str],
        size: Winsize,
    ) -> Result<SessionId, Error> {
        Ok(self.kernel.spawn_leaf(shell, args, size)?)
    }

    /// Cold destroy of one leaf. MUST NOT kill any other live child.
    pub fn terminate_leaf(&mut self, id: SessionId) -> Result<(), Error> {
        Ok(self.kernel.terminate(id)?)
    }

    fn leaf(&self) -> Result<&Session, Error> {
        self.kernel
            .session(self.default_id)
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
        for id in self.kernel.ids() {
            if let Some(s) = self.kernel.session_mut(id) {
                s.poll_child()?;
            }
        }
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
        let live = self.clients.iter().any(|c| {
            c.leaf
                .and_then(|id| self.kernel.session(id))
                .is_some_and(|s| s.credit() > 0 && s.child_alive())
        });
        if live {
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
        for id in self.kernel.ids() {
            let history = self
                .kernel
                .session_mut(id)
                .and_then(Session::take_resync_history);
            if let Some(history) = history {
                let bytes = self.chip.resync_from_history(&history)?;
                if !bytes.is_empty() {
                    if let Some(s) = self.kernel.session_mut(id) {
                        s.enqueue_outbound(Frame::Data(bytes));
                    }
                }
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
        for c in &self.clients {
            let mut events = libc::POLLIN;
            if !c.outbox.is_empty() {
                events |= libc::POLLOUT;
            }
            fds.push(libc::pollfd {
                fd: c.stream.as_raw_fd(),
                events,
                revents: 0,
            });
        }
        let n_clients = self.clients.len();
        let readable = self.kernel.poll_with_extras(&mut fds, timeout_ms)?;
        if fds[0].revents & libc::POLLIN != 0 {
            self.accept_client()?;
        }
        let mut i = 0;
        while i < self.clients.len() && i < n_clients {
            let idx = 1 + i;
            if idx < fds.len()
                && fds[idx].revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0
            {
                self.read_client(i)?;
                if i < self.clients.len() {
                    i += 1;
                }
            } else {
                i += 1;
            }
        }
        for id in readable {
            while let Some(s) = self.kernel.session_mut(id) {
                if !(s.credit() > 0 && s.on_pty_readable()? > 0) {
                    break;
                }
            }
        }
        Ok(())
    }

    fn resolve_attach(&self, session_id: Option<u64>) -> Result<SessionId, Error> {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("ignore_session_id") {
            return Ok(self.default_id);
        }
        match session_id {
            None => Ok(self.default_id),
            Some(raw) => {
                let id = SessionId::from_u64(raw);
                if self.kernel.session(id).is_some() {
                    Ok(id)
                } else {
                    Err(rill_kernel::Error::UnknownSession.into())
                }
            }
        }
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
                #[cfg(feature = "mutate")]
                if std::env::var("RILL_MUTATE").as_deref() == Ok("accept_replaces_client") {
                    let leaves: Vec<SessionId> =
                        self.clients.iter().filter_map(|c| c.leaf).collect();
                    for id in leaves {
                        if let Some(s) = self.kernel.session_mut(id) {
                            s.detach();
                        }
                    }
                    self.clients.clear();
                }
                self.clients.push(Client {
                    stream,
                    decoder: Decoder::new(),
                    outbox: VecDeque::new(),
                    leaf: None,
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.into()),
        }
        Ok(())
    }

    fn read_client(&mut self, idx: usize) -> Result<(), Error> {
        let mut buf = [0u8; 8192];
        let n = {
            let Some(client) = self.clients.get_mut(idx) else {
                return Ok(());
            };
            match client.stream.read(&mut buf) {
                Ok(0) => None,
                Ok(n) => Some(n),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
                Err(_) => None,
            }
        };
        let Some(n) = n else {
            self.drop_client(idx);
            return Ok(());
        };
        let frames = {
            let Some(client) = self.clients.get_mut(idx) else {
                return Ok(());
            };
            client.decoder.push(&buf[..n])?
        };
        for frame in frames {
            self.dispatch_frame(idx, frame)?;
        }
        Ok(())
    }

    fn dispatch_frame(&mut self, idx: usize, frame: Frame) -> Result<(), Error> {
        match frame {
            Frame::Attach {
                generation,
                session_id,
            } => {
                let id = match self.resolve_attach(session_id) {
                    Ok(id) => id,
                    Err(_) => {
                        if let Some(c) = self.clients.get_mut(idx) {
                            c.outbox.extend(
                                Frame::Refused {
                                    reason: rill_attach::RefuseReason::Invalid,
                                }
                                .encode()?,
                            );
                        }
                        return Ok(());
                    }
                };
                let already = self.kernel.session(id).is_some_and(Session::attached);
                let mine = self.clients.get(idx).and_then(|c| c.leaf) == Some(id);
                if already && !mine {
                    if let Some(c) = self.clients.get_mut(idx) {
                        c.outbox.extend(
                            Frame::Refused {
                                reason: rill_attach::RefuseReason::AlreadyAttached,
                            }
                            .encode()?,
                        );
                    }
                    return Ok(());
                }
                match self.kernel.on_frame(
                    id,
                    Frame::Attach {
                        generation,
                        session_id,
                    },
                ) {
                    Ok(()) => {}
                    Err(rill_kernel::Error::Dead) => {}
                    Err(e) => return Err(e.into()),
                }
                if self.kernel.session(id).is_some_and(Session::attached) {
                    if let Some(c) = self.clients.get_mut(idx) {
                        c.leaf = Some(id);
                    }
                }
            }
            other => {
                let id = self
                    .clients
                    .get(idx)
                    .and_then(|c| c.leaf)
                    .unwrap_or(self.default_id);
                match self.kernel.on_frame(id, other) {
                    Ok(()) => {}
                    Err(rill_kernel::Error::Dead) => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }
        Ok(())
    }

    fn drop_client(&mut self, idx: usize) {
        if idx >= self.clients.len() {
            return;
        }
        let leaf = self.clients[idx].leaf;
        if let Some(id) = leaf {
            if let Some(s) = self.kernel.session_mut(id) {
                s.detach();
            }
        }
        self.clients.remove(idx);
    }

    fn flush_outbound(&mut self) -> Result<(), Error> {
        // With no client for a leaf, leave control frames queued (audit S3-2).
        let claimed: Vec<SessionId> = self.clients.iter().filter_map(|c| c.leaf).collect();
        for id in self.kernel.ids() {
            if claimed.contains(&id) {
                continue;
            }
            let Some(session) = self.kernel.session_mut(id) else {
                continue;
            };
            let mut keep = Vec::new();
            while let Some(f) = session.pop_outbound() {
                if !matches!(f, Frame::Data(_)) {
                    keep.push(f);
                }
            }
            for f in keep {
                session.enqueue_outbound(f);
            }
        }

        let mut drop_idx = None;
        for i in 0..self.clients.len() {
            if let Some(id) = self.clients[i].leaf {
                let frames = if let Some(session) = self.kernel.session_mut(id) {
                    let mut frames = Vec::new();
                    while let Some(f) = session.pop_outbound() {
                        frames.push(f);
                    }
                    frames
                } else {
                    Vec::new()
                };
                #[cfg(feature = "mutate")]
                if std::env::var("RILL_MUTATE").as_deref() == Ok("replay_full_frame") {
                    let client = &mut self.clients[i];
                    for frame in frames {
                        let bytes = frame.encode()?;
                        match client.stream.write_all(&bytes) {
                            Ok(()) => {}
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                                if let Some(session) = self.kernel.session_mut(id) {
                                    session.enqueue_outbound(frame);
                                }
                                break;
                            }
                            Err(_) => {
                                drop_idx = Some(i);
                                break;
                            }
                        }
                    }
                    if drop_idx.is_some() {
                        break;
                    }
                    continue;
                }
                let client = &mut self.clients[i];
                for frame in frames {
                    client.outbox.extend(frame.encode()?);
                }
            }
            let client = &mut self.clients[i];
            if write_outbox(&mut client.stream, &mut client.outbox).is_err() {
                drop_idx = Some(i);
                break;
            }
        }
        if let Some(i) = drop_idx {
            self.drop_client(i);
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
