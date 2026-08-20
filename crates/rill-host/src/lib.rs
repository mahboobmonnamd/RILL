//! GUI attach client. Does not spawn the user shell. Does not own a PTY.
//!
//! SPEC-DISPLAY §3, SPEC-ATTACH §5, §8, SPEC-VT-LIVE-SWAP §2.

use rill_attach::{cold_identity_socket_path, Decoder, Frame};
use rill_look::{load_resolved_surface, palette_from_theme, HostSurface};
use rill_vt_types::{Error as VtError, PodGrid, TerminalEmulation, TerminalModeState};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use vt_engine::VtEngine;

mod error;
pub use error::Error;

/// Initial credit window. Not `u32::MAX`: the client replenishes as it feeds,
/// so the kernel can actually apply backpressure (SPEC-ATTACH §5, audit S3-5).
const CREDIT_WINDOW: u32 = 256 * 1024;

pub struct Client {
    stream: UnixStream,
    decoder: Decoder,
    chip: VtEngine,
    surface: HostSurface,
    host_identity: String,
    alive: bool,
    exit_status: Option<i32>,
    /// Host encoder flags polled after each feed (ADR 0036, SPEC-VT-LIVE-SWAP §2).
    modes: TerminalModeState,
    /// Bytes written but not yet accepted by the socket. Keeps `send` from
    /// blocking the UI thread (audit S3-8d).
    outbox: VecDeque<u8>,
    /// Credit granted but not yet consumed by our own feed.
    outstanding_credit: u64,
    /// Frames we sent that are not `DATA`/`CREDIT`, and non-`DATA` frames we
    /// received, since the last `reset_warm_path_audit`. The real oracle for
    /// "zero control RPCs on the warm path" (SPEC-ATTACH §8, ADR 0003 D9).
    warm_path_violations: u32,
    auditing: bool,
    /// Last VT extract. `cell_codepoint` / `cursor` must not calloc a new grid
    /// on every probe (quality audit Q4).
    cached: Option<PodGrid>,
}

impl Client {
    pub fn connect(socket: impl AsRef<Path>, surface: HostSurface) -> Result<Self, Error> {
        let host_identity = cold_host_identity(socket.as_ref())?;
        let stream = UnixStream::connect(socket.as_ref())?;
        stream.set_nonblocking(true)?;
        let mut chip = VtEngine::new(surface.cols, surface.rows)?;
        if let Some(ref colors) = surface.colors {
            chip.set_palette(palette_from_theme(colors))?;
        }
        let mut client = Self {
            stream,
            decoder: Decoder::new(),
            chip,
            surface,
            host_identity,
            alive: true,
            exit_status: None,
            modes: TerminalModeState::fresh(),
            outbox: VecDeque::new(),
            outstanding_credit: 0,
            warm_path_violations: 0,
            auditing: false,
            cached: None,
        };
        client.send(Frame::attach(1, None))?;
        client.grant_credit(CREDIT_WINDOW)?;
        Ok(client)
    }

    pub fn font_family(&self) -> &str {
        &self.surface.font_family
    }

    /// Read from the daemon's separate cold identity socket during connection.
    /// The returned label is kernel-owned and never inferred from the GUI.
    pub fn host_identity(&self) -> &str {
        &self.host_identity
    }

    pub fn font_size(&self) -> f32 {
        self.surface.font_size
    }

    pub fn font_fallbacks(&self) -> &[String] {
        &self.surface.font_fallbacks
    }

    pub fn padding_x(&self) -> f32 {
        self.surface.padding_x
    }

    pub fn padding_y(&self) -> f32 {
        self.surface.padding_y
    }

    pub fn background_opacity(&self) -> f32 {
        self.surface.background_opacity
    }

    pub fn macos_option_as_alt(&self) -> bool {
        self.surface.macos_option_as_alt
    }

    pub fn background_rgba(&self) -> u32 {
        self.surface
            .colors
            .as_ref()
            .map(|c| c.background)
            .unwrap_or(0x1212_12ff)
    }

    pub fn foreground_rgba(&self) -> u32 {
        self.surface
            .colors
            .as_ref()
            .map(|c| c.foreground)
            .unwrap_or(0xcccc_ccff)
    }

    pub fn cursor_rgba(&self) -> u32 {
        self.surface
            .colors
            .as_ref()
            .map(|c| c.cursor)
            .unwrap_or(0xd9d9_d9ff)
    }

    pub fn alive(&self) -> bool {
        self.alive
    }

    pub fn exit_status(&self) -> Option<i32> {
        self.exit_status
    }

    /// The attach socket, so the host can arm a `dispatch_source` on it.
    /// Event-driven, not polled — the 60 Hz `NSTimer` was the largest single
    /// term in the old latency budget (ADR 0003 D2).
    pub fn socket_fd(&self) -> std::os::fd::RawFd {
        use std::os::fd::AsRawFd;
        self.stream.as_raw_fd()
    }

    // ------------------------------------------------------------- warm path

    pub fn begin_warm_path_audit(&mut self) {
        self.warm_path_violations = 0;
        self.auditing = true;
    }

    pub fn end_warm_path_audit(&mut self) -> u32 {
        self.auditing = false;
        self.warm_path_violations
    }

    pub fn warm_path_violations(&self) -> u32 {
        self.warm_path_violations
    }

    pub fn send_input(&mut self, bytes: &[u8]) -> Result<(), Error> {
        if !self.alive {
            return Err(Error::Dead);
        }
        self.send(Frame::Data(bytes.to_vec()))
    }

    pub fn resize(&mut self, cols: u16, rows: u16, px_w: u16, px_h: u16) -> Result<(), Error> {
        self.cached = None;
        let cell_w = u32::from(px_w) / u32::from(cols.max(1));
        let cell_h = u32::from(px_h) / u32::from(rows.max(1));
        self.chip.resize(cols, rows, cell_w, cell_h)?;
        self.send(Frame::Resize {
            cols,
            rows,
            px_w,
            px_h,
        })
    }

    /// Drain everything readable, feed it, replenish exactly what we consumed.
    /// Returns bytes fed this turn.
    pub fn pump(&mut self) -> Result<usize, Error> {
        self.flush_outbox()?;

        let mut buf = [0u8; 65536];
        let mut fed = 0usize;
        loop {
            match self.stream.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    for frame in self.decoder.push(&buf[..n])? {
                        match frame {
                            Frame::Data(bytes) => {
                                fed += bytes.len();
                                self.chip.feed(&bytes)?;
                                self.after_feed()?;
                            }
                            Frame::Exit { status } => {
                                self.alive = false;
                                self.exit_status = Some(status);
                                if self.auditing {
                                    self.warm_path_violations += 1;
                                }
                            }
                            other => {
                                if self.auditing {
                                    self.warm_path_violations += 1;
                                }
                                if matches!(other, Frame::Refused { .. }) {
                                    return Err(Error::Refused);
                                }
                            }
                        }
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e.into()),
            }
        }

        // Replenish only what we actually consumed. Granting a fixed amount per
        // pump — or `u32::MAX` at connect — makes credit outrun delivery and
        // turns backpressure into decoration (audit S3-5).
        if fed > 0 {
            let give = u32::try_from(fed).unwrap_or(u32::MAX);
            self.grant_credit(give)?;
        }
        Ok(fed)
    }

    fn grant_credit(&mut self, n: u32) -> Result<(), Error> {
        self.outstanding_credit = self.outstanding_credit.saturating_add(u64::from(n));
        self.send(Frame::Credit(n))
    }

    pub fn snapshot(&mut self) -> Result<&PodGrid, VtError> {
        if self.cached.is_none() {
            self.cached = Some(self.chip.snapshot()?);
        }
        self.cached
            .as_ref()
            .ok_or(VtError::Vt("snapshot cache"))
    }

    pub fn mode_state(&self) -> TerminalModeState {
        self.modes
    }

    fn after_feed(&mut self) -> Result<(), Error> {
        self.drain_replies()?;
        if !Self::skip_mode_poll() {
            self.modes = self.chip.mode_state();
        }
        self.cached = None;
        Ok(())
    }

    fn drain_replies(&mut self) -> Result<(), Error> {
        if Self::skip_reply_drain() {
            loop {
                let reply = self.chip.take_replies()?;
                if reply.is_empty() {
                    break;
                }
            }
            return Ok(());
        }
        loop {
            let reply = self.chip.take_replies()?;
            if reply.is_empty() {
                break;
            }
            self.send(Frame::Data(reply))?;
        }
        Ok(())
    }

    fn skip_reply_drain() -> bool {
        skip_reply_drain_mutate()
    }

    fn skip_mode_poll() -> bool {
        skip_mode_poll_mutate()
    }

    pub fn cell_codepoint(&mut self, col: u16, row: u16) -> u32 {
        self.snapshot()
            .ok()
            .and_then(|g| g.cell(col, row).map(|x| x.codepoint))
            .unwrap_or(0)
    }

    pub fn cursor_cell(&mut self) -> Result<(u16, u16), VtError> {
        let g = self.snapshot()?;
        Ok((g.cursor_col, g.cursor_row))
    }

    /// Non-blocking. A partial write is queued and completed on the next pump;
    /// the socket's blocking mode is never toggled (audit S3-8d).
    fn send(&mut self, frame: Frame) -> Result<(), Error> {
        if self.auditing && !frame.is_warm_path() {
            self.warm_path_violations += 1;
        }
        let bytes = frame.encode()?;
        self.outbox.extend(bytes);
        self.flush_outbox()
    }

    fn flush_outbox(&mut self) -> Result<(), Error> {
        while !self.outbox.is_empty() {
            let (front, _) = self.outbox.as_slices();
            let chunk: Vec<u8> = if front.is_empty() {
                self.outbox.iter().copied().collect()
            } else {
                front.to_vec()
            };
            match self.stream.write(&chunk) {
                Ok(0) => break,
                Ok(n) => {
                    self.outbox.drain(..n);
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
}

pub fn default_socket() -> PathBuf {
    if let Ok(p) = std::env::var("RILL_SOCKET") {
        return PathBuf::from(p);
    }
    // SAFETY: getuid is always safe.
    let uid = unsafe { libc::getuid() };
    PathBuf::from(format!("/tmp/rill-{uid}.sock"))
}

fn cold_host_identity(attach_socket: &Path) -> Result<String, Error> {
    let mut stream = UnixStream::connect(cold_identity_socket_path(attach_socket))?;
    stream.set_read_timeout(Some(std::time::Duration::from_secs(1)))?;
    let mut identity = [0_u8; 6];
    stream.read_exact(&mut identity)?;
    match identity.as_slice() {
        b"local\n" => Ok("local".into()),
        _ => Err(Error::InvalidHostIdentity),
    }
}

pub fn load_surface() -> Result<HostSurface, VtError> {
    let mut paths = vec![PathBuf::from("host-surface.toml")];
    if let Ok(configured) = std::env::var("RILL_HOST_SURFACE") {
        if !configured.is_empty() {
            paths.insert(0, PathBuf::from(configured));
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            paths.push(dir.join("../Resources/host-surface.toml"));
            paths.push(dir.join("host-surface.toml"));
        }
    }
    paths.push(PathBuf::from("../../host-surface.toml"));
    for p in &paths {
        if p.exists() {
            return load_resolved_surface(p);
        }
    }
    load_resolved_surface("host-surface.toml")
}

/// Percentile over an already-sorted slice, 0-indexed and without the
/// off-by-one the previous `ceil()` introduced (audit S3-8e).
pub fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return f64::NAN;
    }
    let rank = q * (sorted.len() - 1) as f64;
    let lo = rank.floor() as usize;
    let hi = rank.ceil() as usize;
    if lo == hi {
        sorted[lo]
    } else {
        let frac = rank - lo as f64;
        sorted[lo] * (1.0 - frac) + sorted[hi] * frac
    }
}

#[doc(hidden)]
pub fn skip_reply_drain_mutate() -> bool {
    #[cfg(feature = "mutate")]
    {
        return std::env::var("RILL_MUTATE").as_deref() == Ok("skip_reply_drain");
    }
    #[cfg(not(feature = "mutate"))]
    false
}

#[doc(hidden)]
pub fn skip_mode_poll_mutate() -> bool {
    #[cfg(feature = "mutate")]
    {
        return std::env::var("RILL_MUTATE").as_deref() == Ok("skip_mode_poll");
    }
    #[cfg(not(feature = "mutate"))]
    false
}

mod ffi;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentile_is_zero_indexed_and_interpolates() {
        let v: Vec<f64> = (1..=100).map(|i| i as f64).collect();
        assert_eq!(percentile(&v, 0.0), 1.0);
        assert_eq!(percentile(&v, 1.0), 100.0);
        // 95th percentile of 1..=100 sits between 95 and 96, not at 96.
        let p95 = percentile(&v, 0.95);
        assert!((94.9..=96.1).contains(&p95), "p95 = {p95}");
    }

    #[test]
    fn percentile_of_one_sample_is_that_sample() {
        assert_eq!(percentile(&[7.0], 0.95), 7.0);
    }
}
