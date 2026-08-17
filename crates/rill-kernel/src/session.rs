//! One session: PTY, byte history, credit, attach state.
//!
//! SPEC-KERNEL. Never paints, never ships cells, never exports the master fd.

use crate::error::Error;
use crate::pty::{Discipline, Pty, Winsize};
use crate::ring::ByteRing;
use rill_attach::{Frame, RefuseReason};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::time::Duration;

const RING_CAP: usize = 4 * 1024 * 1024;
const READ_BUF: usize = 64 * 1024;

/// Ordered record of what the kernel did, so gates can assert ordering by
/// sequence number instead of by timing (SPEC-KERNEL §7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoEvent {
    PtyWrite(usize),
    PtyRead(usize),
    Winsize(Winsize),
    ChildExit(i32),
}

const JOURNAL_CAP: usize = 4096;

pub struct Session {
    pty: Pty,
    ring: ByteRing,
    attach_generation: Option<u64>,
    credit: u64,
    outbound: VecDeque<Frame>,
    pending_input: VecDeque<Vec<u8>>,
    child_exit: Option<i32>,
    exit_delivered: bool,
    resync_pending: bool,
    resync_count: u32,
    stalled_reads: u64,
    bytes_delivered: u64,
    journal: VecDeque<(u64, IoEvent)>,
    seq: u64,
    size: Winsize,
    last_cwd: Option<PathBuf>,
}

impl Session {
    pub fn spawn(shell: &str, args: &[&str], size: Winsize) -> Result<Self, Error> {
        Self::spawn_with(shell, args, size, Discipline::Interactive)
    }

    /// `Discipline::Raw` is required by T-BYTES: with the normal line
    /// discipline, what reaches the ring is the tty echo, not child output
    /// (SPEC-KERNEL §10).
    pub fn spawn_with(
        shell: &str,
        args: &[&str],
        size: Winsize,
        discipline: Discipline,
    ) -> Result<Self, Error> {
        let pty = Pty::spawn_with(shell, args, size, discipline)?;
        Ok(Self {
            pty,
            ring: ByteRing::new(RING_CAP),
            attach_generation: None,
            credit: 0,
            outbound: VecDeque::new(),
            pending_input: VecDeque::new(),
            child_exit: None,
            exit_delivered: false,
            resync_pending: false,
            resync_count: 0,
            stalled_reads: 0,
            bytes_delivered: 0,
            journal: VecDeque::new(),
            seq: 0,
            size,
            last_cwd: None,
        })
    }

    // ---------------------------------------------------------------- observers

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

    pub fn credit(&self) -> u64 {
        self.credit
    }

    /// Times a PTY read was skipped because credit was exhausted. T-DROP
    /// asserts this is non-zero: a flood that never stalls the kernel has not
    /// exercised backpressure, and is inconclusive rather than passing
    /// (SPEC-KERNEL §4).
    pub fn stalled_reads(&self) -> u64 {
        self.stalled_reads
    }

    pub fn bytes_delivered(&self) -> u64 {
        self.bytes_delivered
    }

    /// Must be 1 after any number of keystrokes. Resync is a cold path.
    pub fn resync_count(&self) -> u32 {
        self.resync_count
    }

    pub fn io_journal(&self) -> Vec<(u64, IoEvent)> {
        self.journal.iter().copied().collect()
    }

    pub fn winsize(&self) -> Result<Winsize, Error> {
        self.pty.winsize()
    }

    /// Readiness without handing out the descriptor (SPEC-KERNEL §1).
    pub fn wait_readable(&self, timeout: Duration) -> Result<bool, Error> {
        self.pty.wait_readable(timeout)
    }

    /// Combined wait: PTY master plus caller sockets. Master fd stays here.
    pub fn poll_with_extras(
        &self,
        extras: &mut [libc::pollfd],
        timeout_ms: i32,
    ) -> Result<bool, Error> {
        self.pty.poll_with_extras(extras, timeout_ms)
    }

    pub(crate) fn master_pollfd(&self, events: i16) -> libc::pollfd {
        self.pty.master_pollfd(events)
    }

    pub fn terminate(&mut self) -> Result<(), Error> {
        self.pty.terminate()
    }

    /// Foreground process cwd (ADR 0013). Cold. Not an attach frame.
    pub fn cwd(&mut self) -> Result<PathBuf, Error> {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("cwd_fail_open") {
            if self.pty.fg_cwd().is_err() {
                return Ok(PathBuf::new());
            }
        }
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("osc7_only") {
            let hist = self.ring.snapshot();
            if !hist.windows(3).any(|w| w == b"\x1b]7") {
                if let Some(path) = &self.last_cwd {
                    return Ok(path.clone());
                }
                let path = self.pty.fg_cwd()?;
                self.last_cwd = Some(path.clone());
                return Ok(path);
            }
        }
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("leader_cwd") {
            let path = self.pty.leader_cwd()?;
            self.last_cwd = Some(path.clone());
            return Ok(path);
        }

        let path = self.pty.fg_cwd()?;
        self.last_cwd = Some(path.clone());
        Ok(path)
    }

    pub fn last_cwd(&self) -> Option<&Path> {
        self.last_cwd.as_deref()
    }

    // ---------------------------------------------------------------- outbound

    pub fn enqueue_outbound(&mut self, frame: Frame) {
        self.outbound.push_back(frame);
    }

    pub fn pop_outbound(&mut self) -> Option<Frame> {
        self.outbound.pop_front()
    }

    pub fn take_resync_history(&mut self) -> Option<Vec<u8>> {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("no_resync") {
            return None;
        }
        if !self.resync_pending {
            return None;
        }
        self.resync_pending = false;
        self.resync_count = self.resync_count.saturating_add(1);
        Some(self.ring.snapshot())
    }

    // ---------------------------------------------------------------- lifecycle

    pub fn poll_child(&mut self) -> Result<(), Error> {
        if self.child_exit.is_some() {
            return Ok(());
        }
        if let Some(status) = self.pty.try_wait()? {
            self.child_exit = Some(status);
            self.record(IoEvent::ChildExit(status));
            // Only queue for delivery if someone is listening. If not, the
            // status is retained and replayed on the next ATTACH (§6).
            if self.attach_generation.is_some() {
                self.outbound.push_back(Frame::Exit { status });
                self.exit_delivered = true;
            }
        }
        Ok(())
    }

    /// Detach must not destroy a pending `EXIT`.
    ///
    /// The previous implementation cleared all outbound frames here, so a child
    /// that died while the window was closed was never reported, and the
    /// reopened window painted a live cursor over a dead process — FR-EXIT
    /// failing on precisely the persist path Spike 0 exists to prove
    /// (audit S3-2, SPEC-KERNEL §6).
    pub fn detach(&mut self) {
        self.attach_generation = None;
        self.credit = 0;

        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("clear_outbound_on_detach") {
            self.outbound.clear();
            return;
        }

        // Pending DATA is dropped — the reattaching client resyncs from
        // history instead. Control frames are retained.
        self.outbound.retain(|f| !matches!(f, Frame::Data(_)));
    }

    // ------------------------------------------------------------------ frames

    /// Apply one inbound attach frame. Total; never panics.
    pub fn on_frame(&mut self, frame: Frame) -> Result<(), Error> {
        match frame {
            Frame::Attach {
                generation,
                session_id: _,
            } => {
                if self.attach_generation.is_some() {
                    self.outbound.push_back(Frame::Refused {
                        reason: RefuseReason::AlreadyAttached,
                    });
                    return Ok(());
                }
                self.attach_generation = Some(generation);
                self.resync_pending = true;

                // Replay a death the previous client never learned about, so
                // the reopened window does not paint a cursor over a corpse.
                if let Some(status) = self.child_exit {
                    #[cfg(feature = "mutate")]
                    if std::env::var("RILL_MUTATE").as_deref() == Ok("clear_outbound_on_detach") {
                        // S3-2: detach dropped EXIT and reattach did not replay it.
                    } else {
                        self.outbound.push_back(Frame::Exit { status });
                        self.exit_delivered = true;
                    }
                    #[cfg(not(feature = "mutate"))]
                    {
                        self.outbound.push_back(Frame::Exit { status });
                        self.exit_delivered = true;
                    }
                }
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
                // Input queued before this frame must reach the child before
                // the new geometry does (FR-RESIZE, SPEC-KERNEL §5).
                let resize_first = {
                    #[cfg(feature = "mutate")]
                    {
                        std::env::var("RILL_MUTATE").as_deref() == Ok("resize_before_data")
                    }
                    #[cfg(not(feature = "mutate"))]
                    {
                        false
                    }
                };
                if !resize_first {
                    self.flush_input()?;
                }
                self.size = Winsize {
                    cols,
                    rows,
                    px_w,
                    px_h,
                };
                self.pty.set_winsize(self.size)?;
                self.record(IoEvent::Winsize(self.size));
                if resize_first {
                    self.flush_input()?;
                }
                Ok(())
            }

            Frame::Data(bytes) => {
                if self.child_exit.is_some() {
                    return Err(Error::Dead);
                }
                self.pending_input.push_back(bytes);
                // Queue without writing so Resize can overtake (T-RESIZE mutation).
                #[cfg(feature = "mutate")]
                if std::env::var("RILL_MUTATE").as_deref() == Ok("resize_before_data") {
                    return Ok(());
                }
                self.flush_input()
            }

            Frame::Exit { .. } | Frame::Refused { .. } => Ok(()),
        }
    }

    fn flush_input(&mut self) -> Result<(), Error> {
        while let Some(buf) = self.pending_input.pop_front() {
            self.pty.write_all(&buf)?;
            self.record(IoEvent::PtyWrite(buf.len()));
        }
        Ok(())
    }

    /// Read the master only while credit remains. When credit is exhausted the
    /// kernel stops reading — real backpressure, not a dropping channel
    /// (SPEC-KERNEL §4).
    pub fn on_pty_readable(&mut self) -> Result<usize, Error> {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("drop_on_full") {
            // The failure this gate exists to catch: read a fixed chunk
            // regardless of credit and discard whatever does not fit.
            let mut buf = vec![0u8; 4096];
            let got = self.pty.read(&mut buf)?;
            if got == 0 {
                return Ok(0);
            }
            buf.truncate(got);
            let keep = (self.credit as usize).min(got);
            self.credit -= keep as u64;
            self.ring.append(&buf[..keep]);
            if self.attach_generation.is_some() && keep > 0 {
                self.outbound.push_back(Frame::Data(buf[..keep].to_vec()));
            }
            return Ok(got);
        }

        if self.credit == 0 {
            self.stalled_reads = self.stalled_reads.saturating_add(1);
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
        self.bytes_delivered = self.bytes_delivered.saturating_add(got as u64);
        self.record(IoEvent::PtyRead(got));
        if self.attach_generation.is_some() {
            self.outbound.push_back(Frame::Data(buf));
        }
        Ok(got)
    }

    fn record(&mut self, event: IoEvent) {
        self.seq += 1;
        if self.journal.len() == JOURNAL_CAP {
            self.journal.pop_front();
        }
        let seq = self.seq;
        self.journal.push_back((seq, event));
    }
}
