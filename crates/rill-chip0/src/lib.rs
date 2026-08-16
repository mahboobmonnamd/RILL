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
    /// Clusters this snapshot could not materialise and rendered as a space.
    /// Reported, never silently dropped (SPEC-CHIP0 §5).
    pub grapheme_truncated: u32,
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
    bytes_fed: u64,
}

impl Chip0 {
    pub fn new(cols: u16, rows: u16) -> Result<Self, Error> {
        Ok(Self {
            vt: adapter::Vt::new(cols, rows)?,
            bytes_fed: 0,
        })
    }

    /// Count only. The previous field retained *every byte ever fed* so a test
    /// could compare the input to our own copy of the input — a self-referential
    /// oracle (ADR 0002 D4) and an unbounded leak on the warm path (audit
    /// S2-2, S3-8a). T-BYTES now asserts against `repaint_bytes()` and the
    /// resulting grid, both of which are downstream of the VT.
    pub fn bytes_fed(&self) -> u64 {
        self.bytes_fed
    }

    pub fn reset(&mut self) {
        self.vt.reset();
    }

    /// Cold-path resync: format the current screen as VT bytes.
    pub fn repaint_bytes(&mut self) -> Result<Vec<u8>, Error> {
        self.vt.repaint_bytes()
    }

    /// Headless: feed history then emit a byte repaint. Cold path, once per
    /// attach (SPEC-CHIP0 §7). Never on a warm keystroke.
    ///
    /// The window cannot distinguish these bytes from live output: they travel
    /// as ordinary `DATA` frames with no marker.
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
        self.bytes_fed = self.bytes_fed.saturating_add(bytes.len() as u64);

        // Negative control for T-BYTES (ADR 0002 D3). Compiled only under the
        // `mutate` feature, which no shipping build enables, so production
        // carries no mutation code at all.
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("lossy_feed") {
            let lossy = String::from_utf8_lossy(bytes).into_owned();
            self.vt.feed(lossy.as_bytes());
            return Ok(());
        }

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

    /// Oracle is the VT's own re-emission of screen state, not our copy of the
    /// input (ADR 0002 D4). A lossy conversion anywhere inside libghostty-vt
    /// cannot survive a parse-then-format round trip.
    ///
    /// Required mutation: `RILL_MUTATE=lossy_feed` (TEST-CASES T-BYTES).
    #[test]
    fn t_bytes_invalid_utf8_survives_a_vt_round_trip() {
        for (name, fixture) in byte_fixtures() {
            let mut chip = Chip0::new(80, 24).expect("chip0");
            chip.feed(&fixture).expect("feed");

            let repaint = chip.repaint_bytes().expect("repaint");
            assert!(
                !repaint.windows(3).any(|w| w == [0xef, 0xbf, 0xbd])
                    || fixture.windows(3).any(|w| w == [0xef, 0xbf, 0xbd]),
                "{name}: U+FFFD appeared in the VT's re-emission but not in the fixture — \
                 something lossy sits between feed and the emulator"
            );

            let grid = chip.snapshot().expect("snapshot");
            assert_eq!(grid.cols, 80);
            assert_eq!(
                grid.grapheme_truncated, 0,
                "{name}: a grapheme cluster could not be materialised"
            );
        }
    }

    /// The overflow fixture from audit S3-1. Meaningful under ASan
    /// (`RILL_ASAN=1`); without it, this still exercises the heap path.
    #[test]
    fn t_bytes_long_grapheme_cluster_does_not_overrun_the_snapshot_buffer() {
        let mut chip = Chip0::new(80, 24).expect("chip0");
        // Base + 40 combining acute accents: one cell, 41 codepoints, well past
        // the fixed 8-element buffer the old adapter handed to GRAPHEMES_BUF.
        let mut s = String::from("e");
        for _ in 0..40 {
            s.push('\u{0301}');
        }
        chip.feed(s.as_bytes()).expect("feed");
        let grid = chip.snapshot().expect("snapshot");
        assert_eq!(grid.cols, 80, "snapshot survived a long cluster");
        let _ = grid.grapheme_truncated; // counted, whatever the value
    }

    #[test]
    fn pod_cell_is_flat_and_sixteen_bytes() {
        assert_eq!(std::mem::size_of::<PodCell>(), 16);
        assert_eq!(std::mem::align_of::<PodCell>(), 4);
    }

    /// Asserts on the *resulting grid*, not on the `\x1b[2J` prefix this
    /// function prepends itself — that was a tautology (audit S2, ADR 0002 D4).
    #[test]
    fn t_resync_headless_bytes_reconstruct_the_screen() {
        let mut source = Chip0::new(80, 24).expect("chip0");
        source.feed(b"RILL-RESYNC-MARK\r\n").expect("feed");
        let before = source.snapshot().expect("snapshot");

        let resync = source.resync_from_history(b"RILL-RESYNC-MARK\r\n").expect("resync");

        // A second, independent chip that has seen nothing but the resync bytes
        // must land on the same screen.
        let mut replay = Chip0::new(80, 24).expect("chip0");
        replay.feed(&resync).expect("feed resync");
        let after = replay.snapshot().expect("snapshot");

        let row = |g: &PodGrid, r: u16| -> String {
            (0..g.cols)
                .filter_map(|c| g.cell(c, r).and_then(|x| char::from_u32(x.codepoint)))
                .collect()
        };
        assert_eq!(
            row(&before, 0),
            row(&after, 0),
            "resync bytes did not reconstruct row 0"
        );
        assert!(row(&after, 0).contains("RILL-RESYNC-MARK"));
    }

    fn byte_fixtures() -> Vec<(&'static str, Vec<u8>)> {
        vec![
            ("lone_continuation", vec![0x80, 0x41]),
            ("truncated_3byte", vec![0xe2, 0x82]),
            ("overlong_slash", vec![0xc0, 0xaf]),
            ("lone_surrogate", vec![0xed, 0xa0, 0x80]),
            ("bom_then_high", vec![0xff, 0xfe, 0x80, 0x41]),
            ("csi_high_param", vec![0x1b, 0x5b, 0x80, 0x6d, 0x41]),
            ("c1_in_utf8", vec![0xc2, 0x9b, 0x41]),
        ]
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
