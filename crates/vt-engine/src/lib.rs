//! Chip 1 isolated VT engine. Bytes in, POD snapshots out.
//!
//! Not the live chip ([ADR 0012] D1). Parser is in-tree ([ADR 0020] D1);
//! `vte` must not appear in `[dependencies]`. Cites S-VT #21.

#![forbid(unsafe_code)]

mod parser;
mod screen;

pub use rill_vt_types::{Color, Error, Palette, PodCell, PodGrid, Rgb, TerminalEmulation};

use crate::parser::Parser;
use crate::screen::Screen;

/// Isolated Chip 1 emulator. Not linked into the window until M7.
pub struct VtEngine {
    parser: Parser,
    screen: Screen,
}

impl VtEngine {
    pub fn new(cols: u16, rows: u16) -> Result<Self, Error> {
        Ok(Self {
            parser: Parser::new(),
            screen: Screen::new(cols, rows)?,
        })
    }
}

pub(crate) fn mutate(name: &str) -> bool {
    #[cfg(feature = "mutate")]
    {
        return std::env::var("RILL_MUTATE").as_deref() == Ok(name);
    }
    #[cfg(not(feature = "mutate"))]
    {
        let _ = name;
        false
    }
}

impl TerminalEmulation for VtEngine {
    fn feed(&mut self, bytes: &[u8]) -> Result<(), Error> {
        if mutate("drop_high_bytes") {
            let ascii: Vec<u8> = bytes.iter().copied().filter(|b| *b < 0x80).collect();
            self.parser.feed(&ascii, &mut self.screen);
            return Ok(());
        }
        self.parser.feed(bytes, &mut self.screen);
        Ok(())
    }

    fn resize(&mut self, cols: u16, rows: u16, cell_w: u32, cell_h: u32) -> Result<(), Error> {
        let _ = (cell_w, cell_h);
        self.screen.resize(cols, rows)
    }

    fn snapshot(&mut self) -> Result<PodGrid, Error> {
        Ok(self.screen.snapshot())
    }
}
