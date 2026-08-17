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
    /// S2-2, S3-8a). T-BYTES now asserts on the snapshot grid, downstream of
    /// the VT.
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
        //
        // `from_utf8_lossy` is a no-op against this VT: libghostty-vt already
        // emits U+FFFD for illegal UTF-8, so that mutation could not turn the
        // gate red. Dropping high bytes can: the emulator never sees them.
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("drop_high_bytes") {
            let ascii: Vec<u8> = bytes.iter().copied().filter(|b| *b < 0x80).collect();
            self.vt.feed(&ascii);
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

    /// Invalid bytes must reach the VT, not be stripped before `vt_write`.
    /// libghostty-vt is allowed to substitute U+FFFD — that is decoding, not
    /// dropping. A formatter round-trip of illegal UTF-8 is not the oracle
    /// (PRD NFR-BYTES is the kernel ring; SPEC-CHIP0 §3 is unmodified feed).
    ///
    /// Required mutation: `RILL_MUTATE=drop_high_bytes` (TEST-CASES T-BYTES).
    #[test]
    fn t_bytes_invalid_utf8_survives_a_vt_round_trip() {
        for (name, fixture) in byte_fixtures() {
            let mut chip = Chip0::new(80, 24).expect("chip0");
            chip.feed(&fixture).expect("feed");
            let grid = chip.snapshot().expect("snapshot");
            assert_eq!(grid.cols, 80);

            let row0: Vec<u32> = (0..grid.cols)
                .filter_map(|c| grid.cell(c, 0).map(|x| x.codepoint))
                .take_while(|cp| *cp != 32)
                .collect();

            if fixture.contains(&0x41) {
                assert!(
                    row0.contains(&u32::from(b'A')),
                    "{name}: ASCII 'A' from the fixture did not land in the grid: {row0:x?}"
                );
            }
            // CSI with a high parameter may consume the high byte without a
            // cell. Every other high-byte fixture must produce a non-ASCII
            // cell (U+FFFD or C1). Dropping high bytes before feed cannot.
            if fixture.first() != Some(&0x1b) && fixture.iter().any(|b| *b >= 0x80) {
                assert!(
                    row0.iter().any(|cp| *cp > 127),
                    "{name}: high bytes never reached the VT (no non-ASCII cell): {row0:x?}"
                );
            }
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

        let resync = source
            .resync_from_history(b"RILL-RESYNC-MARK\r\n")
            .expect("resync");

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

    fn byte_fixtures() -> Vec<(String, Vec<u8>)> {
        let mut out = vec![
            ("lone_continuation".into(), vec![0x80, 0x41]),
            ("truncated_3byte".into(), vec![0xe2, 0x82, 0x41]),
            ("overlong_slash".into(), vec![0xc0, 0xaf]),
            ("lone_surrogate".into(), vec![0xed, 0xa0, 0x80]),
            ("bom_then_high".into(), vec![0xff, 0xfe, 0x80, 0x41]),
            ("csi_high_param".into(), vec![0x1b, 0x5b, 0x80, 0x6d, 0x41]),
            ("c1_in_utf8".into(), vec![0xc2, 0x9b, 0x41]),
        ];
        let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/bytes");
        assert!(
            dir.is_dir(),
            "fixtures/bytes/ is required (SPEC-CHIP0 §5, ADR 0002 D5)"
        );
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("bin"))
            .collect();
        files.sort();
        assert!(
            !files.is_empty(),
            "fixtures/bytes/ has no .bin files (SPEC-CHIP0 §5)"
        );
        for path in files {
            let name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("bin")
                .to_string();
            let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("read {name}: {e}"));
            out.push((name, bytes));
        }
        out
    }

    #[test]
    fn feed_ascii_lands_in_pod_grid() {
        let mut chip = Chip0::new(40, 5).expect("chip0");
        chip.feed(b"Hello").expect("feed");
        let grid = chip.snapshot().expect("snap");
        let row0: String = (0..5)
            .filter_map(|c| {
                grid.cell(c, 0)
                    .map(|cell| char::from_u32(cell.codepoint).unwrap_or('?'))
            })
            .collect();
        assert_eq!(row0, "Hello");
    }
}
