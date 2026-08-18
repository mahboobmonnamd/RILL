//! Chip 1 isolated VT engine. Bytes in, POD snapshots out.
//!
//! Not the live chip ([ADR 0012] D1). Parser work is Slice 2 (#283).
//! `vte` must not appear in this crate's `[dependencies]`.

#![forbid(unsafe_code)]

pub use rill_vt_types::{Color, Error, Palette, PodCell, PodGrid, Rgb, TerminalEmulation};

/// Isolated Chip 1 emulator. Empty until Slice 2 lands the parser and screen.
pub struct VtEngine {
    _private: (),
}

impl VtEngine {
    pub fn new(_cols: u16, _rows: u16) -> Result<Self, Error> {
        Ok(Self { _private: () })
    }
}
