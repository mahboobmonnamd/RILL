//! Kernel daemon: owns the session PTY, framed attach, cold-path resync.

use rill_attach::{Decoder, Frame};
use rill_chip0::Chip0;
use rill_kernel::{Session, Winsize};
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct Daemon {
    listener: UnixListener,
    socket_path: PathBuf,
    _lock: File,
    session: Session,
    /// The connection currently holding the attach claim.
    client: Option<Client>,
    chip: Chip0,
}

struct Client {
    stream: UnixStream,
    decoder: Decoder,
}

impl Daemon {
    pub fn bind(
        socket_path: impl AsRef<Path>,
        shell: &str,
        args: &[&str],
        size: Winsize,
    ) -> Result<Self, Box<dyn std::error::Error>> {
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
            return Err("already running".into());
        }

        // The lock is ours, so any socket at this path is stale unless someone
        // is answering on it.
        if socket_path.exists() {
            if UnixStream::connect(&socket_path).is_ok() {
                return Err("already running".into());
            }
            std::fs::remove_file(&socket_path)?;
        }

        let listener = UnixListener::bind(&socket_path)?;
        listener.set_nonblocking(true)?;
        // The attach socket carries keystrokes and shell output. Do not leave
        // it world-writable (SPEC-ATTACH §1).
        std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))?;
        let session = Session::spawn(shell, args, size)?;
        if let Ok(path) = std::env::var("RILL_TEST_PIDFILE") {
            std::fs::write(path, format!("{}\n", session.child_pid()))?;
        }
        let chip = Chip0::new(size.cols, size.rows)?;
        Ok(Self {
            listener,
            socket_path,
            _lock: lock,
            session,
            client: None,
            chip,
        })
    }

    pub fn child_pid(&self) -> u32 {
        self.session.child_pid()
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    /// Must stay at 1 however much the user types. Resync is a cold path
    /// (FR-RESYNC, SPEC-CHIP0 §7).
    pub fn resync_count(&self) -> u32 {
        self.session.resync_count()
    }

    pub fn stalled_reads(&self) -> u64 {
        self.session.stalled_reads()
    }

    pub fn child_alive(&self) -> bool {
        self.session.child_alive()
    }

    pub fn step(&mut self, timeout_ms: i32) -> Result<(), Box<dyn std::error::Error>> {
        self.session.poll_child()?;
        self.poll_io(timeout_ms)?;
        self.flush_outbound()?;
        self.maybe_resync()?;
        Ok(())
    }

    pub fn run(mut self) -> Result<(), Box<dyn std::error::Error>> {
        loop {
            self.step(50)?;
        }
    }

    fn maybe_resync(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if let Some(history) = self.session.take_resync_history() {
            let bytes = self.chip.resync_from_history(&history)?;
            if !bytes.is_empty() {
                self.session.enqueue_outbound(Frame::Data(bytes));
            }
        }
        Ok(())
    }

    fn poll_io(&mut self, timeout_ms: i32) -> Result<(), Box<dyn std::error::Error>> {
        let mut fds = Vec::new();
        fds.push(libc::pollfd {
            fd: self.listener.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        });
        // No `expect` on a daemon path: an unwrap here would take the user's
        // shell with it (PRD NFR-FAIL).
        let client_idx = match self.client.as_ref() {
            Some(c) => {
                fds.push(libc::pollfd {
                    fd: c.stream.as_raw_fd(),
                    events: libc::POLLIN | libc::POLLOUT,
                    revents: 0,
                });
                Some(1usize)
            }
            None => None,
        };
        // The PTY is not in this fd set. `Session::wait_readable` is the only
        // readiness surface the kernel exposes; the master fd never leaves the
        // kernel crate (ADR 0001 §5, SPEC-KERNEL §1, audit S3-4). The socket
        // poll uses a short timeout so PTY readiness is checked promptly.
        let want_pty = self.session.credit() > 0 && self.session.child_alive();
        let timeout_ms = if want_pty { timeout_ms.min(2) } else { timeout_ms };

        // SAFETY: fds is a valid, non-empty slice of initialised pollfd.
        let n = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout_ms) };
        if n < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                return Ok(());
            }
            return Err(err.into());
        }
        if fds[0].revents & libc::POLLIN != 0 {
            self.accept_client()?;
        }
        if let Some(i) = client_idx {
            if fds[i].revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0 {
                self.read_client()?;
            }
        }
        if want_pty && self.session.wait_readable(Duration::from_millis(0))? {
            // Drain what credit allows in this turn rather than one chunk per
            // poll, so a flood does not need N wakeups to move N chunks.
            while self.session.credit() > 0 && self.session.on_pty_readable()? > 0 {}
        }
        Ok(())
    }

    fn accept_client(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        match self.listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(true)?;
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
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.into()),
        }
        Ok(())
    }

    fn read_client(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        let Some(client) = self.client.as_mut() else {
            return Ok(());
        };
        let mut buf = [0u8; 8192];
        match client.stream.read(&mut buf) {
            Ok(0) => {
                self.session.detach();
                self.client = None;
                return Ok(());
            }
            Ok(n) => {
                let frames = client.decoder.push(&buf[..n])?;
                for frame in frames {
                    match self.session.on_frame(frame) {
                        Ok(()) => {}
                        Err(rill_kernel::Error::Dead) => {}
                        Err(e) => return Err(e.into()),
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => {
                self.session.detach();
                self.client = None;
            }
        }
        Ok(())
    }

    fn flush_outbound(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // With no client, leave control frames queued. The previous version
        // drained and discarded everything here, so an EXIT that arrived while
        // the window was closed was destroyed and the reopened window painted a
        // live cursor over a dead process (audit S3-2).
        let Some(client) = self.client.as_mut() else {
            let mut keep = Vec::new();
            while let Some(f) = self.session.pop_outbound() {
                if !matches!(f, Frame::Data(_)) {
                    keep.push(f);
                }
            }
            for f in keep {
                self.session.enqueue_outbound(f);
            }
            return Ok(());
        };
        while let Some(frame) = self.session.pop_outbound() {
            let bytes = frame.encode()?;
            match client.stream.write_all(&bytes) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    self.session.enqueue_outbound(frame);
                    break;
                }
                Err(_) => {
                    self.session.detach();
                    self.client = None;
                    break;
                }
            }
        }
        Ok(())
    }
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
pub fn pump(daemon: &mut Daemon, duration: Duration) -> Result<(), Box<dyn std::error::Error>> {
    let start = std::time::Instant::now();
    while start.elapsed() < duration {
        daemon.step(20)?;
    }
    Ok(())
}

