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

use rill_attach::{cold_identity_socket_path, cold_nav_socket_path, Decoder, Frame};
use rill_look::{load_resolved_surface, HostSurface, ThemeColors};
use rill_vt_types::{PodGrid, Rgb, TerminalEmulation, TerminalModeState, ATTR_WIDE_TAIL};
use std::collections::VecDeque;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use vt_engine::{Palette, VtEngine};

mod error;
mod nav;
mod scroll;
pub use error::Error;
pub use nav::{nav_new_tab, nav_snapshot, parse_nav_line, NavSnapshot};
pub use rill_look::chrome_surface_rgba;
use scroll::{
    clamp_offset, compose_viewport, follow_live_after_input, ingest_scrolled,
    mark_full_if_viewport_jumped,
};

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
    live_cached: Option<PodGrid>,
    history_rows: Vec<Vec<rill_vt_types::PodCell>>,
    scroll_offset: u32,
    viewport_jumped: bool,
    workspace_id: Option<u64>,
    modes: TerminalModeState,
    bytes_sent: u64,
    #[allow(dead_code)]
    attach_path: PathBuf,
}

impl Client {
    pub fn connect(socket: impl AsRef<Path>, surface: HostSurface) -> Result<Self, Error> {
        Self::connect_session(socket, surface, None)
    }

    pub fn connect_session(
        socket: impl AsRef<Path>,
        surface: HostSurface,
        session_id: Option<u64>,
    ) -> Result<Self, Error> {
        let attach_path = socket.as_ref().to_path_buf();
        let host_identity = cold_host_identity(&attach_path)?;
        let workspace_id = cold_workspace_id(&attach_path);
        let stream = UnixStream::connect(&attach_path)?;
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
            live_cached: None,
            history_rows: Vec::new(),
            scroll_offset: 0,
            viewport_jumped: true,
            workspace_id,
            modes: TerminalModeState::default(),
            bytes_sent: 0,
            attach_path,
        };
        client.send(Frame::attach(1, session_id))?;
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
        self.follow_live();
        self.bytes_sent = self.bytes_sent.saturating_add(bytes.len() as u64);
        self.send(Frame::Data(bytes.to_vec()))
    }

    pub fn follow_live(&mut self) {
        self.scroll_offset = follow_live_after_input(self.scroll_offset);
        self.viewport_jumped = true;
        self.cached = None;
        self.live_cached = None;
    }

    pub fn bytes_sent(&self) -> u64 {
        self.bytes_sent
    }

    pub fn workspace_id(&self) -> Option<u64> {
        self.workspace_id
    }

    pub fn scroll_offset(&self) -> u32 {
        self.scroll_offset
    }

    pub fn history_row_count(&self) -> u32 {
        u32::try_from(self.history_rows.len()).unwrap_or(u32::MAX)
    }

    pub fn scroll_lines(&mut self, delta: i32) {
        if std::env::var("RILL_MUTATE").as_deref() == Ok("ignore_wheel") {
            return;
        }
        let max = self.history_rows.len() as i64;
        let next = (self.scroll_offset as i64 + i64::from(delta)).clamp(0, max);
        self.scroll_offset = next as u32;
        self.viewport_jumped = true;
        self.cached = None;
        self.live_cached = None;
    }

    pub fn live_codepoint(&mut self, col: u16, row: u16) -> u32 {
        self.ensure_snapshots()
            .ok()
            .and_then(|_| {
                self.live_cached
                    .as_ref()
                    .and_then(|g| g.cell(col, row).map(|x| x.codepoint))
            })
            .unwrap_or(0)
    }

    pub fn resize(&mut self, cols: u16, rows: u16, px_w: u16, px_h: u16) -> Result<(), Error> {
        self.cached = None;
        self.live_cached = None;
        self.viewport_jumped = true;
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
                                ingest_scrolled(
                                    &mut self.history_rows,
                                    self.chip.take_scrolled_off(),
                                );
                                self.scroll_offset = clamp_offset(
                                    self.history_rows.len(),
                                    self.surface.rows,
                                    self.scroll_offset,
                                );
                                let replies = self.chip.take_replies()?;
                                if !replies.is_empty() {
                                    self.send(Frame::Data(replies))?;
                                }
                                self.modes = self.chip.mode_state();
                                self.cached = None;
                                self.live_cached = None;
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

    fn ensure_snapshots(&mut self) -> Result<(), rill_vt_types::Error> {
        if self.cached.is_some() && self.live_cached.is_some() {
            return Ok(());
        }
        ingest_scrolled(&mut self.history_rows, self.chip.take_scrolled_off());
        let live = self.chip.snapshot()?;
        let mut view = compose_viewport(&self.history_rows, &live, self.scroll_offset);
        mark_full_if_viewport_jumped(&mut view, self.viewport_jumped);
        self.viewport_jumped = false;
        self.live_cached = Some(live);
        self.cached = Some(view);
        Ok(())
    }

    pub fn snapshot(&mut self) -> Result<&PodGrid, rill_vt_types::Error> {
        self.ensure_snapshots()?;
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

    pub fn send_pointer(&mut self, kind: u8, button: u8, col: u16, row: u16) -> Result<bool, Error> {
        let bytes = encode_pointer(self.modes, kind, button, col, row);
        if bytes.is_empty() {
            return Ok(false);
        }
        self.send_input(&bytes)?;
        Ok(true)
    }

    pub fn paste(&mut self, body: &[u8]) -> Result<(), Error> {
        let wrapped = self.wrap_paste(body);
        self.send_input(&wrapped)
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

fn cold_workspace_id(attach_socket: &Path) -> Option<u64> {
    #[cfg(feature = "mutate")]
    if std::env::var("RILL_MUTATE").as_deref() == Ok("chrome_invents_workspace_row") {
        return None;
    }
    let mut stream = UnixStream::connect(cold_nav_socket_path(attach_socket)).ok()?;
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(1)))
        .ok()?;
    let mut buf = [0_u8; 512];
    let n = stream.read(&mut buf).ok()?;
    let text = std::str::from_utf8(&buf[..n]).ok()?;
    crate::parse_nav_line(text).map(|s| s.workspace_id)
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

/// Host translation of macOS edit chords into bytes the shell line editor
/// already understands. Not a second editor (ADR 0051 D1).
pub const EDIT_LINE_START: u8 = 1;
pub const EDIT_LINE_END: u8 = 2;
pub const EDIT_WORD_LEFT: u8 = 3;
pub const EDIT_WORD_RIGHT: u8 = 4;
pub const EDIT_DELETE_WORD: u8 = 5;
pub const EDIT_DELETE_TO_START: u8 = 6;
pub const EDIT_DELETE_TO_END: u8 = 7;
pub const EDIT_HOME: u8 = 8;
pub const EDIT_END: u8 = 9;

pub fn encode_edit(op: u8) -> &'static [u8] {
    if std::env::var("RILL_MUTATE").as_deref() == Ok("skip_host_edit_keys") {
        return &[];
    }
    match op {
        EDIT_LINE_START => &[0x01],
        EDIT_LINE_END => &[0x05],
        EDIT_WORD_LEFT => b"\x1bb",
        EDIT_WORD_RIGHT => b"\x1bf",
        EDIT_DELETE_WORD => &[0x17],
        EDIT_DELETE_TO_START => &[0x15],
        EDIT_DELETE_TO_END => &[0x0b],
        EDIT_HOME => b"\x1b[H",
        EDIT_END => b"\x1b[F",
        _ => &[],
    }
}

pub const POINTER_PRESS: u8 = 0;
pub const POINTER_RELEASE: u8 = 1;
pub const POINTER_WHEEL_UP: u8 = 2;
pub const POINTER_WHEEL_DOWN: u8 = 3;

pub fn pointer_reporting(modes: TerminalModeState) -> bool {
    modes.mouse_x10 || modes.mouse_button || modes.mouse_any || modes.mouse_sgr
}

/// Chip 1 modes in, attach DATA out (ADR 0055, SPEC-HOST-POINTER).
pub fn encode_pointer(
    modes: TerminalModeState,
    kind: u8,
    button: u8,
    col: u16,
    row: u16,
) -> Vec<u8> {
    if std::env::var("RILL_MUTATE").as_deref() == Ok("skip_mouse_encode") {
        return Vec::new();
    }
    if !pointer_reporting(modes) {
        return Vec::new();
    }
    let col = col.max(1);
    let row = row.max(1);
    let (cb, release) = match kind {
        POINTER_PRESS => (button, false),
        POINTER_RELEASE => (button, true),
        POINTER_WHEEL_UP => (64, false),
        POINTER_WHEEL_DOWN => (65, false),
        _ => return Vec::new(),
    };
    if modes.mouse_sgr {
        let end = if release { 'm' } else { 'M' };
        return format!("\x1b[<{cb};{col};{row}{end}").into_bytes();
    }
    let cx = 32u16.saturating_add(col.min(223));
    let cy = 32u16.saturating_add(row.min(223));
    let b = 32u16.saturating_add(u16::from(cb.min(223)));
    vec![0x1b, b'[', b'M', b.min(255) as u8, cx.min(255) as u8, cy.min(255) as u8]
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

    /// Bug: ⌘/⌥ line and word motions never became PTY bytes, so zsh/readline
    /// editing felt dead. Required mutation: `skip_host_edit_keys`.
    #[test]
    fn t_cmd_option_edit_keys_encode_readline_bytes() {
        assert_eq!(encode_edit(EDIT_LINE_START), [0x01]);
        assert_eq!(encode_edit(EDIT_LINE_END), [0x05]);
        assert_eq!(encode_edit(EDIT_WORD_LEFT), b"\x1bb");
        assert_eq!(encode_edit(EDIT_WORD_RIGHT), b"\x1bf");
        assert_eq!(encode_edit(EDIT_DELETE_WORD), [0x17]);
        assert_eq!(encode_edit(EDIT_DELETE_TO_START), [0x15]);
        assert_eq!(encode_edit(EDIT_DELETE_TO_END), [0x0b]);
        assert_eq!(encode_edit(EDIT_HOME), b"\x1b[H");
        assert_eq!(encode_edit(EDIT_END), b"\x1b[F");
        assert!(encode_edit(255).is_empty());
    }

    /// Bug: clicks never became PTY CSI. Required mutation: `skip_mouse_encode`.
    #[test]
    fn t_host_encodes_sgr_mouse_when_mode_is_on() {
        let mut on = TerminalModeState::fresh();
        on.mouse_sgr = true;
        on.mouse_x10 = true;
        assert_eq!(
            encode_pointer(on, POINTER_PRESS, 0, 1, 1),
            b"\x1b[<0;1;1M"
        );
        assert_eq!(
            encode_pointer(on, POINTER_RELEASE, 0, 2, 3),
            b"\x1b[<0;2;3m"
        );
        assert_eq!(
            encode_pointer(on, POINTER_WHEEL_UP, 0, 1, 1),
            b"\x1b[<64;1;1M"
        );
        let off = TerminalModeState::fresh();
        assert!(encode_pointer(off, POINTER_PRESS, 0, 1, 1).is_empty());
        let mut x10 = TerminalModeState::fresh();
        x10.mouse_x10 = true;
        assert_eq!(
            encode_pointer(x10, POINTER_PRESS, 0, 1, 1),
            vec![0x1b, b'[', b'M', 32, 33, 33]
        );
    }

    #[test]
    fn t_parse_nav_line_reads_tab_and_leaf_ids() {
        let s = parse_nav_line("ws=1 tabs=2,4 leaves=7,9").expect("parse");
        assert_eq!(s.workspace_id, 1);
        assert_eq!(s.tabs, vec![2, 4]);
        assert_eq!(s.leaves, vec![7, 9]);
    }
}
