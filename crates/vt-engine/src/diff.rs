//! T-CHIP1-DIFF: in-tree parser vs `vte` on the same `Screen` (ADR 0020 D2).
//!
//! Mutations hit the in-tree front only. Divergence 1 (C1 `execute` vs U+FFFD)
//! is remapped in the `vte` Perform adapter, not by dropping fixtures.

#![cfg(test)]

use crate::parser::Actions;
use crate::screen::Screen;
use crate::VtEngine;
use rill_vt_types::{PodGrid, TerminalEmulation};
use std::path::PathBuf;

struct VteDrive<'a> {
    screen: &'a mut Screen,
}

impl vte::Perform for VteDrive<'_> {
    fn print(&mut self, c: char) {
        Actions::print(self.screen, c);
    }

    fn execute(&mut self, byte: u8) {
        // Divergence 1 (SPEC-VT-CONFORMANCE §4): vte treats 0x80..=0x9f as
        // C1 execute; we paint U+FFFD for an invalid 8-bit byte.
        if (0x80..=0x9f).contains(&byte) {
            Actions::print(self.screen, '\u{FFFD}');
            return;
        }
        Actions::execute(self.screen, byte);
    }

    fn csi_dispatch(
        &mut self,
        params: &vte::Params,
        intermediates: &[u8],
        ignore: bool,
        action: char,
    ) {
        let mut flat = Vec::new();
        for part in params.iter() {
            for n in part {
                flat.push(*n);
            }
        }
        Actions::csi(self.screen, &flat, intermediates, ignore, action);
    }

    fn esc_dispatch(&mut self, intermediates: &[u8], ignore: bool, byte: u8) {
        if ignore {
            return;
        }
        Actions::esc(self.screen, intermediates, byte);
    }
}

fn ours(bytes: &[u8]) -> PodGrid {
    let mut vt = VtEngine::new(80, 24).expect("ours");
    vt.feed(bytes).expect("feed ours");
    vt.snapshot().expect("snap ours")
}

fn via_vte(bytes: &[u8]) -> PodGrid {
    let mut screen = Screen::new(80, 24).expect("vte screen");
    let mut driver = VteDrive {
        screen: &mut screen,
    };
    let mut parser = vte::Parser::new();
    parser.advance(&mut driver, bytes);
    screen.snapshot()
}

fn apply_registered_remaps(ours: &PodGrid, vte: &mut PodGrid) {
    // Divergence 1: decoded C1. We print U+0080..=U+009F; vte often yields U+FFFD.
    for i in 0..ours.cells.len() {
        let o = ours.cells[i].codepoint;
        if (0x80..=0x9f).contains(&o) && vte.cells[i].codepoint == 0xfffd {
            vte.cells[i].codepoint = o;
        }
    }
    // Invalid UTF-8: we emit one U+FFFD per sequence (SPEC-VT-PARSER §2);
    // vte 0.15 emits one per byte. If the extra cells are only U+FFFD, drop them.
    if vte.cursor_row == ours.cursor_row && vte.cursor_col > ours.cursor_col {
        let cols = usize::from(ours.cols);
        let base = usize::from(ours.cursor_row) * cols;
        let from = usize::from(ours.cursor_col);
        let to = usize::from(vte.cursor_col);
        if (from..to).all(|c| vte.cells[base + c].codepoint == 0xfffd) {
            for c in from..to {
                vte.cells[base + c].codepoint = 32;
            }
            vte.cursor_col = ours.cursor_col;
        }
    }
}

fn assert_same(name: &str, bytes: &[u8]) {
    let a = ours(bytes);
    let mut b = via_vte(bytes);
    apply_registered_remaps(&a, &mut b);
    assert_eq!(a.cols, b.cols, "{name} cols");
    assert_eq!(a.rows, b.rows, "{name} rows");
    assert_eq!(
        (a.cursor_col, a.cursor_row),
        (b.cursor_col, b.cursor_row),
        "{name} cursor"
    );
    for i in 0..a.cells.len() {
        assert_eq!(
            a.cells[i].codepoint, b.cells[i].codepoint,
            "{name} codepoint at {i}"
        );
        assert_eq!(a.cells[i].attrs, b.cells[i].attrs, "{name} attrs at {i}");
    }
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
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/bytes");
    assert!(dir.is_dir(), "{} missing (ADR 0002 D5)", dir.display());
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", dir.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("bin"))
        .collect();
    files.sort();
    assert!(!files.is_empty(), "fixtures/bytes/ has no .bin files");
    for path in files {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("bin")
            .to_string();
        out.push((name, std::fs::read(&path).expect("read bin")));
    }
    let invalid = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/invalid_utf8.bin");
    assert!(invalid.is_file(), "invalid_utf8.bin required");
    out.push((
        "invalid_utf8.bin".into(),
        std::fs::read(&invalid).expect("read invalid"),
    ));
    out
}

/// T-CHIP1-DIFF — an independent parser agrees over the corpus.
///
/// Required mutation (parser front only): `RILL_MUTATE=drop_high_bytes` or
/// `RILL_MUTATE=c1_as_control`.
#[test]
fn t_chip1_diff_in_tree_agrees_with_vte_over_the_corpus() {
    for (name, bytes) in byte_fixtures() {
        assert_same(&name, &bytes);
    }
    for (name, bytes) in [
        ("ascii", b"Hello".as_slice()),
        ("crlf", b"A\r\nB".as_slice()),
        ("cup", b"\x1b[5;10H".as_slice()),
        ("ed", b"ABC\x1b[2J".as_slice()),
        ("sgr", b"\x1b[1mX".as_slice()),
        ("cjk", "日本X".as_bytes()),
        ("da", b"\x1b[c".as_slice()),
        ("dsr", b"\x1b[6n".as_slice()),
        ("combining", "e\u{0301}".as_bytes()),
    ] {
        assert_same(name, bytes);
    }
}
