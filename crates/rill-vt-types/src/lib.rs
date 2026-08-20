//! Shared VT types for Chip 0 and Chip 1. No Ghostty, no Zig, no engine.
//!
//! [SPEC-VT-TYPES]. `PodCell` is the layout lock T-CHIP1-POD observes.

#![forbid(unsafe_code)]

use std::fmt;

/// One visible cell. `#[repr(C)]`, exactly 16 bytes, alignment 4.
///
/// Snapshot cells are already materialised RGB. Colour identity lives inside
/// the engine ([ADR 0021] D1).
///
/// `attrs`: bit0 bold, bit1 underline, bit2 inverse, bit3 wide-lead, bit4
/// wide-tail ([ADR 0035] D5). Empty cell is space `32`. A wide tail stores the
/// lead's base scalar; it MUST NOT be `codepoint == 0`.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PodCell {
    pub codepoint: u32,
    pub fg: u32,
    pub bg: u32,
    pub attrs: u16,
    pub _pad: u16,
}

/// `PodCell.attrs` bits (SPEC-VT-TYPES §2, ADR 0035 D5).
pub const ATTR_BOLD: u16 = 1 << 0;
pub const ATTR_UNDERLINE: u16 = 1 << 1;
pub const ATTR_INVERSE: u16 = 1 << 2;
pub const ATTR_WIDE_LEAD: u16 = 1 << 3;
pub const ATTR_WIDE_TAIL: u16 = 1 << 4;

/// Visible grid. `cells.len()` is exactly `cols * rows` (ADR 0012 D3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PodGrid {
    pub cols: u16,
    pub rows: u16,
    pub cursor_col: u16,
    pub cursor_row: u16,
    pub cursor_visible: bool,
    pub full_damage: bool,
    pub damage_row0: u16,
    pub damage_row1: u16,
    /// VT default colours, RGBA8888 as `PodCell.fg` / `bg`. Not a theme.
    pub default_fg: u32,
    pub default_bg: u32,
    pub grapheme_truncated: u32,
    /// Replies lost to a full buffer ([ADR 0022] D3). Chip 0 reports 0.
    pub replies_dropped: u32,
    pub cells: Vec<PodCell>,
}

impl PodGrid {
    /// Returns `None` for any out-of-range coordinate. Must not panic.
    pub fn cell(&self, col: u16, row: u16) -> Option<&PodCell> {
        let cols = self.cols as usize;
        let i = (row as usize)
            .checked_mul(cols)?
            .checked_add(col as usize)?;
        self.cells.get(i)
    }
}

/// 24-bit triplet. Alpha is always `0xff` at materialisation ([ADR 0021] D5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Colour identity kept until `snapshot()` materialises against a `Palette`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Color {
    Default,
    Indexed(u8),
    Rgb(u8, u8, u8),
}

/// Host-supplied palette. This crate does not parse look files ([ADR 0017] D2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Palette {
    pub ansi: [Rgb; 16],
    pub foreground: Rgb,
    pub background: Rgb,
    pub cursor: Rgb,
}

impl Palette {
    /// VT default, not a theme. Values: [SPEC-VT-COLOR] §4.
    ///
    /// These sixteen ANSI colours plus `#cccccc` / `#121212` are the
    /// `no-theme-rgb-in-rust` exemption (SPEC-VT-CONFORMANCE §5).
    pub fn vt_default() -> Self {
        const fn rgb(r: u8, g: u8, b: u8) -> Rgb {
            Rgb { r, g, b }
        }
        Self {
            ansi: [
                rgb(0x1d, 0x1f, 0x21),
                rgb(0xcc, 0x66, 0x66),
                rgb(0xb5, 0xbd, 0x68),
                rgb(0xf0, 0xc6, 0x74),
                rgb(0x81, 0xa2, 0xbe),
                rgb(0xb2, 0x94, 0xbb),
                rgb(0x8a, 0xbe, 0xb7),
                rgb(0xc5, 0xc8, 0xc6),
                rgb(0x66, 0x66, 0x66),
                rgb(0xd5, 0x4e, 0x53),
                rgb(0xb9, 0xca, 0x4a),
                rgb(0xe7, 0xc5, 0x47),
                rgb(0x7a, 0xa6, 0xda),
                rgb(0xc3, 0x97, 0xd8),
                rgb(0x70, 0xc0, 0xb1),
                rgb(0xea, 0xea, 0xea),
            ],
            foreground: rgb(0xcc, 0xcc, 0xcc),
            background: rgb(0x12, 0x12, 0x12),
            cursor: rgb(0xcc, 0xcc, 0xcc),
        }
    }
}

pub trait TerminalEmulation {
    fn feed(&mut self, bytes: &[u8]) -> Result<(), Error>;
    fn resize(&mut self, cols: u16, rows: u16, cell_w: u32, cell_h: u32) -> Result<(), Error>;
    fn snapshot(&mut self) -> Result<PodGrid, Error>;
}

#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    Vt(&'static str),
    Config(String),
    Io(std::io::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Vt(s) => write!(f, "vt: {s}"),
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
