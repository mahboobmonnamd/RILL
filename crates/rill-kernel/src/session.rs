//! One session: PTY, byte history, credit, attach state.
//!
//! SPEC-KERNEL. Never paints, never ships cells, never exports the master fd.

use crate::checkpoint::StoredCheckpoint;
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

/// ADR 0015 D3. Downstream of the PTY write, not of a copied input buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputDelivery {
    Pending,
    Dispatched,
    Unknown,
}

/// Verified kernel identity for one leaf. M2 has one local kernel only
/// (ADR 0020 D6); remote identity is not represented until its own ADR.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostIdentity {
    Local,
}

impl HostIdentity {
    pub fn as_wire(self) -> &'static [u8] {
        match self {
            Self::Local => b"local\n",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
        }
    }
}

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
    last_delivery: InputDelivery,
    observers: u32,
    terminated: bool,
    checkpoint: Option<StoredCheckpoint>,
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
            last_delivery: InputDelivery::Unknown,
            observers: 0,
            terminated: false,
            checkpoint: None,
        })
    }

    // ---------------------------------------------------------------- observers

    pub fn child_pid(&self) -> u32 {
        self.pty.child_id()
    }

    /// Cold kernel-owned identity. This is not a title or a GUI label.
    pub fn host_identity(&self) -> HostIdentity {
        HostIdentity::Local
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

    pub fn end_offset(&self) -> u64 {
        self.ring.end_offset()
    }

    pub fn retained_base(&self) -> u64 {
        self.ring.retained_base()
    }

    /// Install a host-captured VT checkpoint at a monotonic PTY offset.
    pub fn install_checkpoint(&mut self, rec: StoredCheckpoint) -> Result<(), Error> {
        rec.check_compatible()?;
        self.checkpoint = Some(rec);
        Ok(())
    }

    /// Worker recovery export: last checkpoint plus retained deltas after it.
    ///
    /// This Session is the worker failure boundary for #313. A later slice
    /// splits the control daemon into a separate process; packaged T-RUNTIME
    /// GUI-INDEPENDENT remains Red until then.
    pub fn recovery(&self) -> Result<(StoredCheckpoint, Vec<u8>), Error> {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("blank_core") {
            return Ok((
                StoredCheckpoint::new(self.ring.end_offset(), 0, Vec::new()),
                Vec::new(),
            ));
        }
        let cp = self.checkpoint.clone().ok_or(Error::NoCheckpoint)?;
        let deltas = self.ring.deltas_after(cp.ending_offset)?;
        Ok((cp, deltas))
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
        if self.terminated {
            return Ok(());
        }
        self.terminated = true;
        self.pty.terminate()
    }

    pub fn is_terminated(&self) -> bool {
        self.terminated
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

    pub fn last_delivery(&self) -> InputDelivery {
        self.last_delivery
    }

    pub fn size(&self) -> Winsize {
        self.size
    }

    pub fn release_observer(&mut self) {
        self.observers = self.observers.saturating_sub(1);
    }

    fn has_listeners(&self) -> bool {
        self.attach_generation.is_some() || self.observers > 0
    }

    fn ephemeral() -> bool {
        std::env::var("RILL_EPHEMERAL").as_deref() == Ok("1")
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

    /// Returns `true` when the child is observed to have exited on this call.
    pub fn poll_child(&mut self) -> Result<bool, Error> {
        if self.child_exit.is_some() {
            return Ok(false);
        }
        if let Some(status) = self.pty.try_wait()? {
            self.child_exit = Some(status);
            self.record(IoEvent::ChildExit(status));
            // Only queue for delivery if someone is listening. If not, the
            // status is retained and replayed on the next ATTACH (§6).
            // Observers count (ADR 0015 D7).
            if self.has_listeners() {
                self.outbound.push_back(Frame::Exit { status });
                self.exit_delivered = true;
            }
            return Ok(true);
        }
        Ok(false)
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

        if Self::ephemeral() {
            let _ = self.pty.terminate();
        }

        if !self.pending_input.is_empty() {
            self.last_delivery = InputDelivery::Unknown;
            self.pending_input.clear();
        }

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
                protocol: _,
                observe,
            } => {
                if observe {
                    self.observers = self.observers.saturating_add(1);
                    return Ok(());
                }
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
                self.last_delivery = InputDelivery::Pending;
                self.pending_input.push_back(bytes);
                // Queue without writing so Resize can overtake (T-RESIZE mutation).
                #[cfg(feature = "mutate")]
                if std::env::var("RILL_MUTATE").as_deref() == Ok("always_pending") {
                    return Ok(());
                }
                #[cfg(feature = "mutate")]
                if std::env::var("RILL_MUTATE").as_deref() == Ok("resize_before_data") {
                    return Ok(());
                }
                self.flush_input()
            }

            Frame::Exit { .. }
            | Frame::Refused { .. }
            | Frame::Checkpoint { .. }
            | Frame::Delta { .. }
            | Frame::ResyncRequest => Ok(()),
        }
    }

    fn flush_input(&mut self) -> Result<(), Error> {
        while let Some(buf) = self.pending_input.pop_front() {
            self.pty.write_all(&buf)?;
            self.record(IoEvent::PtyWrite(buf.len()));
            self.last_delivery = InputDelivery::Dispatched;
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
        if self.has_listeners() {
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

impl Drop for Session {
    fn drop(&mut self) {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("ignore_ephemeral") {
            return;
        }
        if Self::ephemeral() {
            let _ = self.pty.terminate();
        }
    }
}
