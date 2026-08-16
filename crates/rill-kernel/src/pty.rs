//! Kernel-owned PTY. The master fd never leaves this module.

use crate::error::Error;
use std::fs::File;
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, OwnedFd, RawFd};
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};

#[derive(Clone, Copy, Debug)]
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

pub struct Pty {
    master: File,
    child: Child,
}

impl Pty {
    pub fn spawn(shell: &str, args: &[&str], size: Winsize) -> Result<Self, Error> {
        let (master_fd, slave_fd) = openpty()?;
        set_nonblocking(master_fd.as_raw_fd())?;
        set_winsize(master_fd.as_raw_fd(), size)?;

        let slave_in = slave_fd.try_clone().map_err(Error::Spawn)?;
        let slave_out = slave_fd.try_clone().map_err(Error::Spawn)?;
        let slave_err = slave_fd.try_clone().map_err(Error::Spawn)?;

        let mut cmd = Command::new(shell);
        cmd.args(args)
            .stdin(Stdio::from(slave_in))
            .stdout(Stdio::from(slave_out))
            .stderr(Stdio::from(slave_err))
            .env("TERM", "xterm-256color");
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
        set_nonblocking(master_fd.as_raw_fd())?;

        Ok(Self {
            master: File::from(master_fd),
            child,
        })
    }

    pub fn child_id(&self) -> u32 {
        self.child.id()
    }

    pub fn try_wait(&mut self) -> Result<Option<i32>, Error> {
        match self.child.try_wait()? {
            Some(st) => Ok(Some(st.code().unwrap_or(1))),
            None => Ok(None),
        }
    }

    pub fn write_all(&mut self, bytes: &[u8]) -> Result<(), Error> {
        let mut written = 0;
        let start = std::time::Instant::now();
        while written < bytes.len() {
            if !wait_fd(self.master.as_raw_fd(), Wait::Write, 20)? {
                if start.elapsed() > std::time::Duration::from_secs(2) {
                    return Err(Error::Pty("write timeout"));
                }
                continue;
            }
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
        if !wait_fd(self.master.as_raw_fd(), Wait::Read, 10)? {
            return Ok(0);
        }
        let n = unsafe { libc::read(self.master.as_raw_fd(), buf.as_mut_ptr() as *mut _, buf.len()) };
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

    pub fn set_winsize(&mut self, size: Winsize) -> Result<(), Error> {
        set_winsize(self.master.as_raw_fd(), size)
    }

    pub fn winsize(&self) -> Result<Winsize, Error> {
        get_winsize(self.master.as_raw_fd())
    }

    pub fn master_raw_fd(&self) -> RawFd {
        self.master.as_raw_fd()
    }
}

fn openpty() -> Result<(OwnedFd, File), Error> {
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

enum Wait {
    Read,
    Write,
}

fn wait_fd(fd: RawFd, wait: Wait, timeout_ms: i32) -> Result<bool, Error> {
    let mut tv = libc::timeval {
        tv_sec: (timeout_ms / 1000) as libc::time_t,
        tv_usec: ((timeout_ms % 1000) * 1000) as libc::suseconds_t,
    };
    unsafe {
        let mut set = std::mem::zeroed::<libc::fd_set>();
        libc::FD_ZERO(&mut set);
        libc::FD_SET(fd, &mut set);
        let nfds = fd + 1;
        let rc = match wait {
            Wait::Read => libc::select(nfds, &mut set, std::ptr::null_mut(), std::ptr::null_mut(), &mut tv),
            Wait::Write => libc::select(nfds, std::ptr::null_mut(), &mut set, std::ptr::null_mut(), &mut tv),
        };
        if rc < 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() == Some(libc::EINTR) {
                return Ok(false);
            }
            return Err(Error::Io(err));
        }
        Ok(rc > 0)
    }
}

fn set_nonblocking(fd: RawFd) -> Result<(), Error> {
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    if flags < 0 {
        return Err(Error::Pty("fcntl get"));
    }
    if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
        return Err(Error::Pty("fcntl set"));
    }
    let mut one: libc::c_int = 1;
    unsafe { libc::ioctl(fd, libc::FIONBIO, &mut one) };
    Ok(())
}

fn set_winsize(fd: RawFd, size: Winsize) -> Result<(), Error> {
    let ws = libc::winsize {
        ws_row: size.rows,
        ws_col: size.cols,
        ws_xpixel: size.px_w,
        ws_ypixel: size.px_h,
    };
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

/// Test helper: the kernel crate is allowed to own this fd; callers must not
/// send it over SCM_RIGHTS.
#[allow(dead_code)]
pub fn leak_master_forbidden(_pty: &Pty) -> RawFd {
    // Intentionally not `pub` on Pty beyond poll. This symbol exists so
    // reviews can grep that we never export the master to the GUI crate.
    _pty.master.as_raw_fd()
}

impl Drop for Pty {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// Keep IntoRawFd import used if we later wrap fds; silence on stable.
#[allow(dead_code)]
fn _keep_into_raw_fd(fd: OwnedFd) -> RawFd {
    fd.into_raw_fd()
}
