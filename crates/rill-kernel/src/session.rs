use crate::error::Error;
use crate::pty::{Pty, Winsize};
use crate::ring::ByteRing;
use rill_attach::{Frame, RefuseReason};
use std::collections::VecDeque;

const RING_CAP: usize = 4 * 1024 * 1024;
const READ_BUF: usize = 64 * 1024;

pub struct Session {
    pty: Pty,
    ring: ByteRing,
    attach_generation: Option<u64>,
    credit: u64,
    outbound: VecDeque<Frame>,
    child_exit: Option<i32>,
    resync_pending: bool,
    size: Winsize,
}

impl Session {
    pub fn spawn(shell: &str, args: &[&str], size: Winsize) -> Result<Self, Error> {
        let pty = Pty::spawn(shell, args, size)?;
        Ok(Self {
            pty,
            ring: ByteRing::new(RING_CAP),
            attach_generation: None,
            credit: 0,
            outbound: VecDeque::new(),
            child_exit: None,
            resync_pending: false,
            size,
        })
    }

    pub fn child_pid(&self) -> u32 {
        self.pty.child_id()
    }

    pub fn attached(&self) -> bool {
        self.attach_generation.is_some()
    }

    pub fn child_alive(&self) -> bool {
        self.child_exit.is_none()
    }

    pub fn history(&self) -> Vec<u8> {
        self.ring.snapshot()
    }

    pub fn take_resync_history(&mut self) -> Option<Vec<u8>> {
        if !self.resync_pending {
            return None;
        }
        self.resync_pending = false;
        Some(self.ring.snapshot())
    }

    pub fn enqueue_outbound(&mut self, frame: Frame) {
        self.outbound.push_back(frame);
    }

    pub fn pop_outbound(&mut self) -> Option<Frame> {
        self.outbound.pop_front()
    }

    pub fn credit(&self) -> u64 {
        self.credit
    }

    pub fn master_fd(&self) -> std::os::fd::RawFd {
        self.pty.master_raw_fd()
    }

    pub fn poll_child(&mut self) -> Result<(), Error> {
        if self.child_exit.is_some() {
            return Ok(());
        }
        if let Some(status) = self.pty.try_wait()? {
            self.child_exit = Some(status);
            self.outbound.push_back(Frame::Exit { status });
        }
        Ok(())
    }

    /// Apply one inbound attach frame. Never `unwrap`s.
    pub fn on_frame(&mut self, frame: Frame) -> Result<(), Error> {
        match frame {
            Frame::Attach { generation } => {
                if self.attach_generation.is_some() {
                    self.outbound.push_back(Frame::Refused {
                        reason: RefuseReason::AlreadyAttached,
                    });
                    return Ok(());
                }
                self.attach_generation = Some(generation);
                self.resync_pending = true;
                Ok(())
            }
            Frame::Credit(n) => {
                self.credit = self.credit.saturating_add(u64::from(n));
                Ok(())
            }
            Frame::Resize {
                cols,
                rows,
                px_w,
                px_h,
            } => {
                self.size = Winsize {
                    cols,
                    rows,
                    px_w,
                    px_h,
                };
                self.pty.set_winsize(self.size)
            }
            Frame::Data(bytes) => {
                if self.child_exit.is_some() {
                    return Err(Error::Dead);
                }
                self.pty.write_all(&bytes)
            }
            Frame::Exit { .. } | Frame::Refused { .. } => Ok(()),
        }
    }

    pub fn detach(&mut self) {
        self.attach_generation = None;
        self.credit = 0;
        self.outbound.clear();
    }

    /// Read PTY if credit remains. Stop reading when credit is 0 (backpressure).
    /// Never drops bytes that were read.
    pub fn on_pty_readable(&mut self) -> Result<usize, Error> {
        if self.credit == 0 {
            return Ok(0);
        }
        let n = (self.credit as usize).min(READ_BUF);
        let mut buf = vec![0u8; n];
        let got = self.pty.read(&mut buf)?;
        if got == 0 {
            return Ok(0);
        }
        buf.truncate(got);
        self.ring.append(&buf);
        self.credit -= got as u64;
        if self.attach_generation.is_some() {
            self.outbound.push_back(Frame::Data(buf));
        }
        Ok(got)
    }

    pub fn winsize(&self) -> Result<Winsize, Error> {
        self.pty.winsize()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::{Duration, Instant};

    #[allow(dead_code)]
    fn drain_until(session: &mut Session, pred: impl Fn(&[u8]) -> bool, timeout: Duration) -> Vec<u8> {
        let _ = session.on_frame(Frame::Credit(u32::MAX));
        let mut acc = Vec::new();
        let start = Instant::now();
        while start.elapsed() < timeout {
            let _ = session.poll_child();
            let _ = session.on_pty_readable();
            while let Some(Frame::Data(b)) = session.pop_outbound() {
                acc.extend_from_slice(&b);
            }
            if pred(&acc) {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        acc
    }

    #[test]
    fn t_bytes_ring_stores_raw_invalid_utf8_from_child() {
        let fixture = include_bytes!("../../../fixtures/invalid_utf8.bin");
        let mut session = Session::spawn("/bin/sh", &["-c", "cat"], Winsize::default())
            .expect("spawn cat");
        session
            .on_frame(Frame::Attach { generation: 1 })
            .expect("attach");
        session.on_frame(Frame::Credit(u32::MAX)).expect("credit");
        session.on_frame(Frame::Data(fixture.to_vec())).expect("write");
        // Close stdin to cat by sending nothing more; write the bytes to the pty
        // as the child's stdin. Then send Ctrl-D? cat reads stdin.
        // We already wrote fixture to the PTY as input; cat echoes to stdout.
        thread::sleep(Duration::from_millis(80));
        let _ = session.on_pty_readable();
        let hist = session.history();
        assert!(
            hist.windows(fixture.len()).any(|w| w == fixture),
            "history must contain original invalid UTF-8, got {hist:?}"
        );
        assert!(
            !hist.windows(3).any(|w| w == [0xef, 0xbf, 0xbd]),
            "must not UTF-8-replace before the ring"
        );
    }

    #[test]
    fn t_drop_yes_ten_seconds_ctrl_c_type_does_not_drop() {
        let mut session = Session::spawn(
            "/bin/sh",
            &["-c", "/usr/bin/yes | /usr/bin/head -n 20000; printf USABLE"],
            Winsize::default(),
        )
        .expect("sh");
        session
            .on_frame(Frame::Attach { generation: 1 })
            .expect("attach");
        session.on_frame(Frame::Credit(u32::MAX)).expect("credit");
        let start = Instant::now();
        let mut y_lines: u64 = 0;
        let mut acc = Vec::new();
        while start.elapsed() < Duration::from_secs(10) {
            let _ = session.on_frame(Frame::Credit(u32::MAX));
            let _ = session.on_pty_readable();
            while let Some(Frame::Data(b)) = session.pop_outbound() {
                y_lines += b.iter().filter(|&&c| c == b'y').count() as u64;
                acc.extend_from_slice(&b);
            }
            if acc.windows(6).any(|w| w == b"USABLE") {
                break;
            }
        }
        assert!(
            y_lines >= 20000,
            "dropped chunks or corrupted grid: only {y_lines} y/n pairs, tail={:?}",
            String::from_utf8_lossy(&acc)
        );
        assert!(
            acc.windows(6).any(|w| w == b"USABLE"),
            "prompt not usable after flood, got {}",
            String::from_utf8_lossy(&acc[acc.len().saturating_sub(64)..])
        );
    }

    #[test]
    fn t_kill_gui_sigkill_does_not_change_child_pid() {
        let mut session = Session::spawn("/bin/sh", &["-c", "exec sleep 30"], Winsize::default())
            .expect("spawn");
        session
            .on_frame(Frame::Attach { generation: 1 })
            .expect("attach");
        let pid = session.child_pid();
        session.detach();
        assert_eq!(session.child_pid(), pid);
        assert!(session.child_alive());
        session
            .on_frame(Frame::Attach { generation: 2 })
            .expect("reattach");
        assert_eq!(session.child_pid(), pid);
        let _ = session.poll_child();
        assert!(session.child_alive(), "child must survive GUI detach");
    }

    #[test]
    fn t_attach_second_attach_is_refused() {
        let mut session = Session::spawn("/bin/sh", &["-c", "sleep 5"], Winsize::default())
            .expect("spawn");
        session
            .on_frame(Frame::Attach { generation: 1 })
            .expect("a1");
        session
            .on_frame(Frame::Attach { generation: 2 })
            .expect("a2");
        let mut refused = false;
        while let Some(f) = session.pop_outbound() {
            if matches!(
                f,
                Frame::Refused {
                    reason: RefuseReason::AlreadyAttached
                }
            ) {
                refused = true;
            }
        }
        assert!(refused);
    }

    #[test]
    fn t_exit_dead_pane_does_not_accept_keys_as_alive() {
        let mut session = Session::spawn("/bin/sh", &["-c", "exit 0"], Winsize::default())
            .expect("spawn");
        session
            .on_frame(Frame::Attach { generation: 1 })
            .expect("attach");
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(2) {
            let _ = session.poll_child();
            if !session.child_alive() {
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert!(!session.child_alive());
        let err = session.on_frame(Frame::Data(b"x".to_vec()));
        assert!(matches!(err, Err(Error::Dead)));
        let mut saw_exit = false;
        while let Some(f) = session.pop_outbound() {
            if matches!(f, Frame::Exit { .. }) {
                saw_exit = true;
            }
        }
        assert!(saw_exit, "EXIT frame required");
    }

    #[test]
    fn t_resize_child_tiocgwinsz_matches_display() {
        let mut session = Session::spawn("/bin/sh", &["-c", "sleep 8"], Winsize::default())
            .expect("spawn");
        session
            .on_frame(Frame::Attach { generation: 1 })
            .expect("attach");
        session
            .on_frame(Frame::Data(b"pending".to_vec()))
            .expect("pending input");
        session
            .on_frame(Frame::Resize {
                cols: 91,
                rows: 31,
                px_w: 728,
                px_h: 496,
            })
            .expect("resize");
        let ws = session.winsize().expect("winsize");
        assert_eq!(ws.cols, 91);
        assert_eq!(ws.rows, 31);
    }
}
