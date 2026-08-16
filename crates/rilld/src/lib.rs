//! Kernel daemon: owns the session PTY, framed attach, cold-path resync.

use rill_attach::{Decoder, Frame};
use rill_chip0::Chip0;
use rill_kernel::{Session, Winsize};
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub struct Daemon {
    listener: UnixListener,
    socket_path: PathBuf,
    session: Session,
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
        if socket_path.exists() {
            if UnixStream::connect(&socket_path).is_ok() {
                return Err("already running".into());
            }
            let _ = std::fs::remove_file(&socket_path);
        }
        if let Some(dir) = socket_path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let listener = UnixListener::bind(&socket_path)?;
        listener.set_nonblocking(true)?;
        let session = Session::spawn(shell, args, size)?;
        if let Ok(path) = std::env::var("RILL_TEST_PIDFILE") {
            std::fs::write(path, format!("{}\n", session.child_pid()))?;
        }
        let chip = Chip0::new(size.cols, size.rows)?;
        Ok(Self {
            listener,
            socket_path,
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
        let client_idx = if self.client.is_some() {
            fds.push(libc::pollfd {
                fd: self.client.as_ref().map(|c| c.stream.as_raw_fd()).expect("client"),
                events: libc::POLLIN | libc::POLLOUT,
                revents: 0,
            });
            Some(1usize)
        } else {
            None
        };
        let pty_idx = if self.session.credit() > 0 && self.session.child_alive() {
            let i = fds.len();
            fds.push(libc::pollfd {
                fd: self.session.master_fd(),
                events: libc::POLLIN,
                revents: 0,
            });
            Some(i)
        } else {
            None
        };
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
        if let Some(i) = pty_idx {
            if fds[i].revents & libc::POLLIN != 0 {
                let _ = self.session.on_pty_readable()?;
            }
        }
        Ok(())
    }

    fn accept_client(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        match self.listener.accept() {
            Ok((stream, _)) => {
                stream.set_nonblocking(true)?;
                if self.client.is_some() && self.session.attached() {
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
        let Some(client) = self.client.as_mut() else {
            while self.session.pop_outbound().is_some() {}
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

#[cfg(test)]
mod tests {
    use super::*;
    use rill_attach::Frame;
    use rill_chip0::{Chip0, TerminalEmulation};
    use std::os::unix::net::UnixStream;
    use std::thread;
    use std::time::Instant;

    fn temp_sock() -> PathBuf {
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        PathBuf::from(format!("/tmp/rill-test-{n}.sock"))
    }

    fn send(stream: &mut UnixStream, frame: Frame) {
        let bytes = frame.encode().expect("enc");
        stream.write_all(&bytes).expect("write");
    }

    fn recv_until(stream: &mut UnixStream, decoder: &mut Decoder, timeout: Duration) -> Vec<Frame> {
        stream.set_nonblocking(true).ok();
        let mut all = Vec::new();
        let start = Instant::now();
        let mut buf = [0u8; 65536];
        while start.elapsed() < timeout {
            match stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => all.extend(decoder.push(&buf[..n]).expect("dec")),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        }
        all
    }

    #[test]
    fn t_resync_reopen_idle_shell_is_not_blank() {
        let sock = temp_sock();
        let mut daemon = Daemon::bind(&sock, "/bin/sh", &[], Winsize::default()).expect("bind");
        let mut gui = UnixStream::connect(&sock).expect("connect");
        pump(&mut daemon, Duration::from_millis(50)).ok();
        send(&mut gui, Frame::Attach { generation: 1 });
        send(&mut gui, Frame::Credit(u32::MAX));
        pump(&mut daemon, Duration::from_millis(80)).ok();
        send(&mut gui, Frame::Data(b"printf 'RILL-MARK-42'\n".to_vec()));
        pump(&mut daemon, Duration::from_millis(200)).ok();
        drop(gui);
        pump(&mut daemon, Duration::from_millis(50)).ok();

        let mut gui2 = UnixStream::connect(&sock).expect("reconnect");
        send(&mut gui2, Frame::Attach { generation: 2 });
        send(&mut gui2, Frame::Credit(u32::MAX));
        pump(&mut daemon, Duration::from_millis(200)).ok();
        let mut dec = Decoder::new();
        let frames = recv_until(&mut gui2, &mut dec, Duration::from_millis(300));
        let mut bytes = Vec::new();
        for f in frames {
            if let Frame::Data(b) = f {
                bytes.extend(b);
            }
        }
        let mut chip = Chip0::new(80, 24).expect("chip");
        chip.feed(&bytes).expect("feed resync");
        let grid = chip.snapshot().expect("snap");
        let text: String = grid
            .cells
            .iter()
            .filter_map(|c| char::from_u32(c.codepoint))
            .collect();
        assert!(
            text.contains("RILL-MARK-42"),
            "reopen was blank over a live process: {text:?}"
        );
    }

    #[test]
    fn t_attach_detach_attach_grids_do_not_diverge() {
        let sock = temp_sock();
        let mut daemon = Daemon::bind(&sock, "/bin/sh", &[], Winsize::default()).expect("bind");
        let mut gui = UnixStream::connect(&sock).expect("c1");
        send(&mut gui, Frame::Attach { generation: 1 });
        send(&mut gui, Frame::Credit(u32::MAX));
        pump(&mut daemon, Duration::from_millis(80)).ok();
        send(&mut gui, Frame::Data(b"printf 'GRID-A'\n".to_vec()));
        pump(&mut daemon, Duration::from_millis(200)).ok();
        let mut dec = Decoder::new();
        let first = recv_until(&mut gui, &mut dec, Duration::from_millis(200));
        drop(gui);
        pump(&mut daemon, Duration::from_millis(50)).ok();

        let mut gui2 = UnixStream::connect(&sock).expect("c2");
        send(&mut gui2, Frame::Attach { generation: 2 });
        send(&mut gui2, Frame::Credit(u32::MAX));
        pump(&mut daemon, Duration::from_millis(200)).ok();
        let mut dec2 = Decoder::new();
        let second = recv_until(&mut gui2, &mut dec2, Duration::from_millis(300));

        fn grid_of(frames: &[Frame]) -> String {
            let mut bytes = Vec::new();
            for f in frames {
                if let Frame::Data(b) = f {
                    bytes.extend_from_slice(b);
                }
            }
            let mut chip = Chip0::new(80, 24).expect("chip");
            chip.feed(&bytes).ok();
            chip.snapshot()
                .map(|g| {
                    g.cells
                        .iter()
                        .filter_map(|c| char::from_u32(c.codepoint))
                        .collect::<String>()
                })
                .unwrap_or_default()
        }
        let a = grid_of(&first);
        let b = grid_of(&second);
        assert!(a.contains("GRID-A") || b.contains("GRID-A"));
        assert!(
            b.contains("GRID-A"),
            "reattach grid diverged / lost content: first={a:?} second={b:?}"
        );
    }

    #[test]
    fn t_kill_gui_process_child_pid_unchanged() {
        let sock = temp_sock();
        let mut daemon = Daemon::bind(&sock, "/bin/sh", &["-c", "exec sleep 30"], Winsize::default())
            .expect("bind");
        let pid = daemon.child_pid();

        let gui = UnixStream::connect(&sock).expect("gui");
        drop(gui);
        pump(&mut daemon, Duration::from_millis(80)).ok();
        assert_eq!(daemon.child_pid(), pid);
        assert!(
            nix_still_alive(pid),
            "child pid changed or died after GUI drop"
        );
    }

    fn nix_still_alive(pid: u32) -> bool {
        let r = unsafe { libc::kill(pid as i32, 0) };
        r == 0
    }

    #[test]
    fn t_bind_does_not_steal_a_live_socket() {
        let sock = temp_sock();
        let first = Daemon::bind(&sock, "/bin/sh", &["-c", "exec sleep 30"], Winsize::default())
            .expect("first bind");
        let pid = first.child_pid();
        let err = Daemon::bind(&sock, "/bin/sh", &["-c", "exec sleep 30"], Winsize::default())
            .err()
            .expect("second bind must fail");
        assert!(
            err.to_string().contains("already running"),
            "live socket was stolen: {err}"
        );
        assert!(nix_still_alive(pid), "steal respawned the child");
        drop(first);
    }
}
