//! GUI attach client. Does not spawn the user shell. Does not own a PTY.
//!
//! SPEC-DISPLAY §3, SPEC-ATTACH §5, §8.
//!
//! The T-NFR *measurement* is not here. It cannot be: the segment being timed
//! starts at an `NSEvent` and ends at a `CAMetalDrawable` presentation
//! (ADR 0003 D5), neither of which exists in Rust. This module exposes the
//! oracle primitives the host needs — cursor cell, cell contents, warm-path
//! frame accounting — and the host does the timing. The old `nfr_key` here
//! measured to a POD snapshot and never left the client, which is why it
//! reported 32 microseconds (docs/SPIKE-0-AUDIT.md S1-2).

use rill_attach::{cold_identity_socket_path, Decoder, Frame};
use rill_look::{load_resolved_surface, HostSurface, ThemeColors};
use rill_vt_types::{PodGrid, Rgb, TerminalEmulation, TerminalModeState, ATTR_WIDE_TAIL};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use vt_engine::{Palette, VtEngine};

mod error;
pub use error::Error;
pub use rill_look::chrome_surface_rgba;

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
    modes: TerminalModeState,
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
            outbox: VecDeque::new(),
            outstanding_credit: 0,
            warm_path_violations: 0,
            auditing: false,
            cached: None,
            modes: TerminalModeState::default(),
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
                                let replies = self.chip.take_replies()?;
                                if !replies.is_empty() {
                                    self.send(Frame::Data(replies))?;
                                }
                                self.modes = self.chip.mode_state();
                                self.cached = None;
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

    pub fn snapshot(&mut self) -> Result<&PodGrid, rill_vt_types::Error> {
        if self.cached.is_none() {
            // Chip 1 materialises the look-file palette in snapshot().
            // apply_theme is a second full-grid walk and is not needed.
            let grid = self.chip.snapshot()?;
            self.cached = Some(grid);
        }
        self.cached
            .as_ref()
            .ok_or(rill_vt_types::Error::Vt("snapshot cache"))
    }

    pub fn cell_codepoint(&mut self, col: u16, row: u16) -> u32 {
        self.snapshot()
            .ok()
            .and_then(|g| g.cell(col, row).map(|x| x.codepoint))
            .unwrap_or(0)
    }

    pub fn cursor_cell(&mut self) -> Result<(u16, u16), rill_vt_types::Error> {
        let g = self.snapshot()?;
        Ok((g.cursor_col, g.cursor_row))
    }

    pub fn mode_state(&self) -> TerminalModeState {
        self.modes
    }

    /// Cursor keys from Chip 1 DECCKM, not a host CSI parser (ADR 0037 D5).
    pub fn encode_arrow(&self, letter: u8) -> [u8; 3] {
        encode_arrow(self.modes.application_cursor_keys, letter)
    }

    pub fn wrap_paste(&self, body: &[u8]) -> Vec<u8> {
        wrap_paste(self.modes.bracketed_paste, body)
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
    rill_attach_default_runtime()
}

fn rill_attach_default_runtime() -> PathBuf {
    if let Ok(p) = std::env::var("RILL_RUNTIME_DIR") {
        return PathBuf::from(p).join("attach.sock");
    }
    let home = std::env::var("HOME").unwrap_or_else(|_| "/var/empty".into());
    #[cfg(target_os = "macos")]
    {
        PathBuf::from(home).join("Library/Application Support/RILL/run/attach.sock")
    }
    #[cfg(not(target_os = "macos"))]
    {
        if let Ok(xdg) = std::env::var("XDG_RUNTIME_DIR") {
            return PathBuf::from(xdg).join("rill/attach.sock");
        }
        PathBuf::from(home).join(".local/share/rill/run/attach.sock")
    }
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

pub fn load_surface() -> Result<HostSurface, rill_vt_types::Error> {
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

fn u32_to_rgb(v: u32) -> Rgb {
    Rgb {
        r: ((v >> 24) & 0xff) as u8,
        g: ((v >> 16) & 0xff) as u8,
        b: ((v >> 8) & 0xff) as u8,
    }
}

fn palette_from_theme(colors: &ThemeColors) -> Palette {
    let mut p = Palette::vt_default();
    p.foreground = u32_to_rgb(colors.foreground);
    p.background = u32_to_rgb(colors.background);
    p.cursor = u32_to_rgb(colors.cursor);
    if let Some(ansi) = colors.ansi {
        for (i, v) in ansi.iter().enumerate() {
            p.ansi[i] = u32_to_rgb(*v);
        }
    }
    p
}

/// Metal must not place a glyph on a wide tail (ADR 0035 D7).
pub fn should_paint_cell(attrs: u16) -> bool {
    #[cfg(feature = "mutate")]
    if std::env::var("RILL_MUTATE").as_deref() == Ok("ignore_wide_bits") {
        return true;
    }
    attrs & ATTR_WIDE_TAIL == 0
}

/// `letter` is `b'A'`..`b'D'` (up/down/right/left).
pub fn encode_arrow(application_cursor_keys: bool, letter: u8) -> [u8; 3] {
    #[cfg(feature = "mutate")]
    if std::env::var("RILL_MUTATE").as_deref() == Ok("ignore_decckm") {
        return [0x1b, b'[', letter];
    }
    if application_cursor_keys {
        [0x1b, b'O', letter]
    } else {
        [0x1b, b'[', letter]
    }
}

pub fn wrap_paste(bracketed: bool, body: &[u8]) -> Vec<u8> {
    #[cfg(feature = "mutate")]
    if std::env::var("RILL_MUTATE").as_deref() == Ok("skip_bracketed_paste") {
        return body.to_vec();
    }
    if !bracketed {
        return body.to_vec();
    }
    let mut out = b"\x1b[200~".to_vec();
    out.extend_from_slice(body);
    out.extend_from_slice(b"\x1b[201~");
    out
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

    #[test]
    fn t_host_encodes_application_cursor_keys_from_mode() {
        assert_eq!(encode_arrow(false, b'A'), [0x1b, b'[', b'A']);
        assert_eq!(encode_arrow(true, b'A'), [0x1b, b'O', b'A']);
    }

    #[test]
    fn t_host_wraps_bracketed_paste_from_mode() {
        assert_eq!(wrap_paste(false, b"hi"), b"hi");
        assert_eq!(wrap_paste(true, b"hi"), b"\x1b[200~hi\x1b[201~".to_vec());
    }
}
