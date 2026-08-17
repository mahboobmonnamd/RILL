//! Kernel-owned PTY. The master fd never leaves this module.
//!
//! SPEC-KERNEL §1, §2, §8.

use crate::error::Error;
use std::fs::File;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Child, Command, Stdio};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Winsize {
    pub cols: u16,
    pub rows: u16,
    pub px_w: u16,
    pub px_h: u16,
}

impl Default for Winsize {
    fn default() -> Self {
        Self {
            cols: 80,
            rows: 24,
            px_w: 80 * 8,
            px_h: 24 * 16,
        }
    }
}

/// Line-discipline mode for the slave.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Discipline {
    /// Normal interactive terminal.
    Interactive,
    /// `ISIG`, `ICANON`, `ECHO`, `OPOST`, `IXON`, `ISTRIP` cleared, so the
    /// line discipline cannot rewrite the byte stream. Required by T-BYTES:
    /// otherwise the "child output" under test is really the tty echo
    /// (SPEC-KERNEL §10, audit S2-2).
    Raw,
}

pub struct Pty {
    master: File,
    child: Child,
    reaped: Option<i32>,
}

impl Pty {
    pub fn spawn_with(
        shell: &str,
        args: &[&str],
        size: Winsize,
        discipline: Discipline,
    ) -> Result<Self, Error> {
        let (master_fd, slave_fd) = openpty()?;
        set_nonblocking(master_fd.as_raw_fd())?;
        set_winsize(master_fd.as_raw_fd(), size)?;
        if discipline == Discipline::Raw {
            set_raw(slave_fd.as_raw_fd())?;
        }

        let slave_in = slave_fd.try_clone().map_err(Error::Spawn)?;
        let slave_out = slave_fd.try_clone().map_err(Error::Spawn)?;
        let slave_err = slave_fd.try_clone().map_err(Error::Spawn)?;

        let mut cmd = Command::new(shell);
        cmd.args(args)
            .stdin(Stdio::from(slave_in))
            .stdout(Stdio::from(slave_out))
            .stderr(Stdio::from(slave_err))
            .env("TERM", "xterm-256color");
        // SAFETY: only async-signal-safe calls between fork and exec.
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                if libc::ioctl(0, libc::TIOCSCTTY as _, 0) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
        let child = cmd.spawn().map_err(Error::Spawn)?;
        drop(slave_fd);

        Ok(Self {
            master: File::from(master_fd),
            child,
            reaped: None,
        })
    }

    pub fn child_id(&self) -> u32 {
        self.child.id()
    }

    /// Raw `wait` status, so a signal death is distinguishable from an exit
    /// code. The previous `code().unwrap_or(1)` reported `1` for `SIGKILL`,
    /// which the GUI would have displayed as a normal failing exit
    /// (SPEC-KERNEL §2).
    pub fn try_wait(&mut self) -> Result<Option<i32>, Error> {
        if let Some(st) = self.reaped {
            return Ok(Some(st));
        }
        match self.child.try_wait()? {
            Some(st) => {
                let raw = st.into_raw();
                self.reaped = Some(raw);
                Ok(Some(raw))
            }
            None => Ok(None),
        }
    }

    pub fn write_all(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let mut written = 0;
        let start = std::time::Instant::now();
        while written < bytes.len() {
            if !wait_fd(self.master.as_raw_fd(), libc::POLLOUT, 20)? {
                if start.elapsed() > Duration::from_secs(2) {
                    return Err(Error::Pty("write timeout"));
                }
                continue;
            }
            // SAFETY: fd is owned; slice bounds are checked above.
            let n = unsafe {
                libc::write(
                    self.master.as_raw_fd(),
                    bytes[written..].as_ptr() as *const _,
                    bytes.len() - written,
                )
            };
            if n < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::WouldBlock
                    || err.raw_os_error() == Some(libc::EINTR)
                {
                    continue;
                }
                return Err(err.into());
            }
            written += n as usize;
        }
        Ok(())
    }

    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, Error> {
        if !wait_fd(self.master.as_raw_fd(), libc::POLLIN, 0)? {
            return Ok(0);
        }
        // SAFETY: fd is owned; buf is a valid mutable slice.
        let n = unsafe {
            libc::read(
                self.master.as_raw_fd(),
                buf.as_mut_ptr() as *mut _,
                buf.len(),
            )
        };
        if n < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::WouldBlock
                || err.raw_os_error() == Some(libc::EAGAIN)
                || err.raw_os_error() == Some(libc::EINTR)
            {
                return Ok(0);
            }
            return Err(err.into());
        }
        Ok(n as usize)
    }

    /// Readiness as a capability, not a descriptor. `rilld` drives the session
    /// through this; the master fd is never handed out (ADR 0001 §5,
    /// SPEC-KERNEL §1, audit S3-4).
    pub fn wait_readable(&self, timeout: Duration) -> Result<bool, Error> {
        let ms = timeout.as_millis().min(i32::MAX as u128) as i32;
        wait_fd(self.master.as_raw_fd(), libc::POLLIN, ms)
    }

    /// Poll the master together with caller sockets. The master fd never
    /// leaves this module (SPEC-KERNEL §1). `true` if the PTY is readable.
    pub fn poll_with_extras(
        &self,
        extras: &mut [libc::pollfd],
        timeout_ms: i32,
    ) -> Result<bool, Error> {
        let mut fds = Vec::with_capacity(1 + extras.len());
        fds.push(self.master_pollfd(libc::POLLIN));
        fds.extend_from_slice(extras);
        // SAFETY: fds is a valid pollfd slice we own for this call.
        let rc = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout_ms) };
        if rc < 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EINTR) {
                return Ok(false);
            }
            return Err(Error::Io(err));
        }
        for (dst, src) in extras.iter_mut().zip(fds.iter().skip(1)) {
            dst.revents = src.revents;
        }
        Ok(fds[0].revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0)
    }

    pub(crate) fn master_pollfd(&self, events: i16) -> libc::pollfd {
        libc::pollfd {
            fd: self.master.as_raw_fd(),
            events,
            revents: 0,
        }
    }

    pub fn set_winsize(&mut self, size: Winsize) -> Result<(), Error> {
        set_winsize(self.master.as_raw_fd(), size)
    }

    pub fn winsize(&self) -> Result<Winsize, Error> {
        get_winsize(self.master.as_raw_fd())
    }

    /// Intentional teardown. Nothing else may kill the child — in particular
    /// not `Drop` (SPEC-KERNEL §2, audit S3-3).
    ///
    /// Kill-wait is bounded (production bar). An unbounded `Child::wait` after
    /// `kill` hung T-RESIZE for minutes: the child had `setsid`, and a stopped
    /// or already-reaped pid made `wait` a parking lot. `try_wait` is WNOHANG.
    pub fn terminate(&mut self) -> Result<(), Error> {
        if self.reaped.is_some() {
            return Ok(());
        }
        let pid = self.child.id() as i32;
        // SAFETY: pid is the session leader we created in pre_exec; killing the
        // group reaps grandchildren (`sleep` in a `while` loop) as well.
        unsafe {
            libc::kill(-pid, libc::SIGKILL);
            libc::kill(pid, libc::SIGKILL);
        }
        // Hangup the slave. A child blocked on the PTY can sit in D-state
        // through SIGKILL until the master is gone.
        let _closed = std::mem::replace(
            &mut self.master,
            File::open("/dev/null").map_err(Error::Io)?,
        );
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            match self.child.try_wait()? {
                Some(st) => {
                    self.reaped = Some(st.into_raw());
                    return Ok(());
                }
                None => {
                    if std::time::Instant::now() >= deadline {
                        return Err(Error::Pty("terminate wait timed out"));
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        }
    }
}

impl Drop for Pty {
    /// Deliberately does not kill the child.
    ///
    /// The previous implementation did, which meant any error path that
    /// dropped a `Session` — a transient `poll` failure, an unwind, a `?` in
    /// `Daemon::run` — destroyed the user's shell. That is the one thing this
    /// product promises never to do, and it must not depend on nobody ever
    /// returning `Err` (audit S3-3).
    ///
    /// Callers that want the child gone call [`Pty::terminate`].
    fn drop(&mut self) {}
}

fn openpty() -> Result<(OwnedFd, File), Error> {
    // SAFETY: standard POSIX PTY handshake; every fd is wrapped or closed.
    let master = unsafe { libc::posix_openpt(libc::O_RDWR | libc::O_NOCTTY | libc::O_NONBLOCK) };
    if master < 0 {
        return Err(Error::Pty("posix_openpt"));
    }
    let master = unsafe { OwnedFd::from_raw_fd(master) };
    if unsafe { libc::grantpt(master.as_raw_fd()) } != 0 {
        return Err(Error::Pty("grantpt"));
    }
    if unsafe { libc::unlockpt(master.as_raw_fd()) } != 0 {
        return Err(Error::Pty("unlockpt"));
    }
    let name_ptr = unsafe { libc::ptsname(master.as_raw_fd()) };
    if name_ptr.is_null() {
        return Err(Error::Pty("ptsname"));
    }
    let slave = unsafe { libc::open(name_ptr, libc::O_RDWR | libc::O_NOCTTY) };
    if slave < 0 {
        return Err(Error::Pty("open slave"));
    }
    Ok((master, unsafe { File::from_raw_fd(slave) }))
}

/// `poll`, never `select`. `select`/`fd_set` is undefined behaviour for
/// fd >= `FD_SETSIZE` (1024), which a long-lived daemon can reach
/// (SPEC-KERNEL §8, audit S3-8c).
fn wait_fd(fd: RawFd, events: libc::c_short, timeout_ms: i32) -> Result<bool, Error> {
    let mut pfd = libc::pollfd {
        fd,
        events,
        revents: 0,
    };
    // SAFETY: single valid pollfd, count 1.
    let rc = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
    if rc < 0 {
        let err = std::io::Error::last_os_error();
        if err.raw_os_error() == Some(libc::EINTR) {
            return Ok(false);
        }
        return Err(Error::Io(err));
    }
    Ok(rc > 0 && pfd.revents & (events | libc::POLLHUP | libc::POLLERR) != 0)
}

fn set_nonblocking(fd: RawFd) -> Result<(), Error> {
    // SAFETY: fd is owned by the caller for the duration of this call.
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(Error::Pty("fcntl get"));
    }
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(Error::Pty("fcntl set"));
    }
    Ok(())
}

fn set_raw(fd: RawFd) -> Result<(), Error> {
    // SAFETY: termios is fully initialised by tcgetattr before use.
    unsafe {
        let mut t: libc::termios = std::mem::zeroed();
        if libc::tcgetattr(fd, &mut t) < 0 {
            return Err(Error::Pty("tcgetattr"));
        }
        t.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ECHOE | libc::ECHOK | libc::ISIG);
        t.c_iflag &= !(libc::IXON | libc::ICRNL | libc::INLCR | libc::ISTRIP | libc::IGNCR);
        t.c_oflag &= !libc::OPOST;
        t.c_cc[libc::VMIN] = 1;
        t.c_cc[libc::VTIME] = 0;
        if libc::tcsetattr(fd, libc::TCSANOW, &t) < 0 {
            return Err(Error::Pty("tcsetattr"));
        }
    }
    Ok(())
}

fn set_winsize(fd: RawFd, size: Winsize) -> Result<(), Error> {
    let ws = libc::winsize {
        ws_row: size.rows,
        ws_col: size.cols,
        ws_xpixel: size.px_w,
        ws_ypixel: size.px_h,
    };
    // SAFETY: ws is a fully initialised winsize.
    if unsafe { libc::ioctl(fd, libc::TIOCSWINSZ, &ws) } < 0 {
        return Err(Error::Pty("TIOCSWINSZ"));
    }
    Ok(())
}

fn get_winsize(fd: RawFd) -> Result<Winsize, Error> {
    let mut ws = libc::winsize {
        ws_row: 0,
        ws_col: 0,
        ws_xpixel: 0,
        ws_ypixel: 0,
    };
    // SAFETY: ws is a fully initialised winsize.
    if unsafe { libc::ioctl(fd, libc::TIOCGWINSZ, &mut ws) } < 0 {
        return Err(Error::Pty("TIOCGWINSZ"));
    }
    Ok(Winsize {
        cols: ws.ws_col,
        rows: ws.ws_row,
        px_w: ws.ws_xpixel,
        px_h: ws.ws_ypixel,
    })
}
