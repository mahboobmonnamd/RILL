//! Chip 0 display: `libghostty-vt` behind an adapter + POD cells.
//!
//! Domain types do not name Ghostty FFI. Adapter C files only.

mod adapter;
mod surface;

pub use surface::{load_host_surface, HostSurface};

use std::fmt;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PodCell {
    pub codepoint: u32,
    pub fg: u32,
    pub bg: u32,
    pub attrs: u16,
    pub _pad: u16,
}

#[derive(Clone, Debug)]
pub struct PodGrid {
    pub cols: u16,
    pub rows: u16,
    pub cursor_col: u16,
    pub cursor_row: u16,
    pub cursor_visible: bool,
    pub full_damage: bool,
    pub damage_row0: u16,
    pub damage_row1: u16,
    pub cells: Vec<PodCell>,
}

impl PodGrid {
    pub fn cell(&self, col: u16, row: u16) -> Option<&PodCell> {
        let i = (row as usize) * (self.cols as usize) + (col as usize);
        self.cells.get(i)
    }
}

pub trait TerminalEmulation {
    fn feed(&mut self, bytes: &[u8]) -> Result<(), Error>;
    fn resize(&mut self, cols: u16, rows: u16, cell_w: u32, cell_h: u32) -> Result<(), Error>;
    fn snapshot(&mut self) -> Result<PodGrid, Error>;
}

pub struct Chip0 {
    vt: adapter::Vt,
    fed: Vec<u8>,
}

impl Chip0 {
    pub fn new(cols: u16, rows: u16) -> Result<Self, Error> {
        Ok(Self {
            vt: adapter::Vt::new(cols, rows)?,
            fed: Vec::new(),
        })
    }

    pub fn bytes_fed(&self) -> &[u8] {
        &self.fed
    }

    pub fn reset(&mut self) {
        self.vt.reset();
        self.fed.clear();
    }

    /// Cold-path resync: format the current screen as VT bytes.
    pub fn repaint_bytes(&mut self) -> Result<Vec<u8>, Error> {
        self.vt.repaint_bytes()
    }

    /// Headless: feed history then emit a byte repaint. Not for warm keys.
    pub fn resync_from_history(&mut self, history: &[u8]) -> Result<Vec<u8>, Error> {
        self.reset();
        if !history.is_empty() {
            self.feed(history)?;
        }
        let mut out = b"\x1b[2J\x1b[H".to_vec();
        out.extend(self.repaint_bytes()?);
        Ok(out)
    }
}

impl TerminalEmulation for Chip0 {
    fn feed(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.fed.extend_from_slice(bytes);
        self.vt.feed(bytes);
        Ok(())
    }

    fn resize(&mut self, cols: u16, rows: u16, cell_w: u32, cell_h: u32) -> Result<(), Error> {
        self.vt.resize(cols, rows, cell_w, cell_h)
    }

    fn snapshot(&mut self) -> Result<PodGrid, Error> {
        self.vt.snapshot()
    }
}

#[derive(Debug)]
pub enum Error {
    Vt(&'static str),
    Config(String),
    Io(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vt(s) => write!(f, "chip0 vt: {s}"),
            Self::Config(s) => write!(f, "host-surface: {s}"),
            Self::Io(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_bytes_invalid_utf8_reaches_emulator_byte_identical() {
        let fixture = include_bytes!("../../../fixtures/invalid_utf8.bin");
        let mut chip = Chip0::new(80, 24).expect("chip0");
        chip.feed(fixture).expect("feed");
        assert_eq!(
            chip.bytes_fed(),
            fixture,
            "emulator must see original bytes, not UTF-8 replacement"
        );
        assert!(
            !chip.bytes_fed().windows(3).any(|w| w == [0xef, 0xbf, 0xbd]),
            "lossy UTF-8 conversion happened before feed"
        );
        let grid = chip.snapshot().expect("snapshot");
        assert_eq!(grid.cols, 80);
        assert!(!grid.cells.is_empty());
        assert_eq!(
            std::mem::size_of::<PodCell>(),
            16,
            "POD cell, not per-cell String"
        );
    }

    #[test]
    fn t_resync_headless_emits_bytes_not_cells() {
        let mut chip = Chip0::new(80, 24).expect("chip0");
        chip.feed(b"hello-resync").expect("feed");
        let bytes = chip.resync_from_history(b"hello-resync").expect("resync");
        assert!(bytes.starts_with(b"\x1b[2J"));
        let as_text = String::from_utf8_lossy(&bytes);
        assert!(
            as_text.contains("hello-resync") || bytes.windows(12).any(|w| w == b"hello-resync"),
            "resync must carry screen bytes, got {}",
            as_text
        );
    }

    #[test]
    fn feed_ascii_lands_in_pod_grid() {
        let mut chip = Chip0::new(40, 5).expect("chip0");
        chip.feed(b"Hello").expect("feed");
        let grid = chip.snapshot().expect("snap");
        let row0: String = (0..5)
            .filter_map(|c| grid.cell(c, 0).map(|cell| char::from_u32(cell.codepoint).unwrap_or('?')))
            .collect();
        assert_eq!(row0, "Hello");
    }
}
