//! Chip 1 isolated VT engine. Bytes in, POD snapshots out.
//!
//! Not the live chip ([ADR 0012] D1). Parser is in-tree ([ADR 0020] D1);
//! `vte` and `unicode-width` must not appear in `[dependencies]`. Cites S-VT #21
//! and SPIKE-WIDTH / ADR 0035.

#![forbid(unsafe_code)]

mod color;
mod east_asian_width;
mod parser;
mod screen;

#[cfg(test)]
mod diff;

pub use rill_vt_types::{
    Color, Error, Palette, PodCell, PodGrid, Rgb, TerminalEmulation, TerminalModeState,
};

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

    /// Colour identity before `snapshot()` materialises (ADR 0021 D1).
    pub fn color_at(&self, col: u16, row: u16) -> Option<(Color, Color)> {
        self.screen.color_at(col, row)
    }

    /// Host-supplied palette. Not a look-file parse (ADR 0021 D2–D3).
    pub fn set_palette(&mut self, palette: Palette) -> Result<(), Error> {
        self.screen.set_palette(palette)
    }

    /// Bytes the program is owed (ADR 0022 D1). Drains.
    pub fn take_replies(&mut self) -> Result<Vec<u8>, Error> {
        self.screen.take_replies()
    }

    pub fn has_replies(&self) -> bool {
        self.screen.has_replies()
    }

    /// Mode flags for the host key/mouse encoder (ADR 0036 D2).
    pub fn mode_state(&self) -> TerminalModeState {
        self.screen.mode_state()
    }

    /// Clear parser and grid. Size is unchanged.
    pub fn reset(&mut self) -> Result<(), Error> {
        let cols = self.screen.cols();
        let rows = self.screen.rows();
        self.parser = Parser::new();
        self.screen = Screen::new(cols, rows)?;
        Ok(())
    }

    /// Visible grid as VT bytes (ADR 0012 D4). Does not prepend ED.
    pub fn repaint_bytes(&mut self) -> Result<Vec<u8>, Error> {
        Ok(self.screen.repaint_bytes())
    }

    /// Cold resync: replay history, emit a byte repaint. Kernel does not call
    /// this in M4. Replies from history are discarded and counted.
    pub fn resync_from_history(&mut self, history: &[u8]) -> Result<Vec<u8>, Error> {
        self.reset()?;
        self.screen.set_discard_replies(true);
        if !history.is_empty() {
            self.feed(history)?;
        }
        self.screen.set_discard_replies(false);
        if crate::mutate("empty_resync") {
            return Ok(b"\x1b[2J\x1b[H".to_vec());
        }
        let mut out = b"\x1b[2J\x1b[H".to_vec();
        out.extend(self.screen.repaint_bytes());
        Ok(out)
    }

    /// Compact versioned checkpoint (SPEC-VT-CHECKPOINT, #312).
    pub fn export_checkpoint(&self, ending_offset: u64) -> Result<Vec<u8>, Error> {
        if !self.parser.is_idle() {
            return Err(Error::Vt("incomplete parser"));
        }
        self.screen.export_checkpoint(ending_offset)
    }

    /// Restore from a checkpoint blob. Returns the encoded ending offset.
    pub fn import_checkpoint(&mut self, bytes: &[u8]) -> Result<u64, Error> {
        if mutate("import_is_resync_bytes") {
            self.parser = Parser::new();
            self.parser.feed(bytes, &mut self.screen);
            return Ok(0);
        }
        let offset = self.screen.import_checkpoint(bytes)?;
        self.parser = Parser::new();
        Ok(offset)
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
