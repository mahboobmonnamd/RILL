//! Slice 5 T-CHIP1-SGR / T-CHIP1-COLOR-IDENTITY / T-CHIP1-LOOK-ANSI.
//!
//! Authority: ADR 0021, SPEC-VT-COLOR. Theme RGB is parsed from fixtures, not
//! written as Rust constants (ADR 0021 D3). Chip 0 stays live.

use rill_vt_types::{Color, Palette, Rgb, TerminalEmulation};
use std::path::PathBuf;
use vt_engine::VtEngine;

fn engine(cols: u16, rows: u16) -> VtEngine {
    VtEngine::new(cols, rows).expect("vt-engine")
}

fn themes_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/look/themes")
}

fn parse_hex_rgb(value: &str) -> Rgb {
    let hex = value.trim().trim_start_matches('#');
    assert_eq!(hex.len(), 6, "need RRGGBB from the theme file, got {value}");
    let n = u32::from_str_radix(hex, 16).expect("hex");
    Rgb {
        r: ((n >> 16) & 0xff) as u8,
        g: ((n >> 8) & 0xff) as u8,
        b: (n & 0xff) as u8,
    }
}

fn pack(rgb: Rgb) -> u32 {
    (u32::from(rgb.r) << 24) | (u32::from(rgb.g) << 16) | (u32::from(rgb.b) << 8) | 0xff
}

fn theme_file(name: &str) -> String {
    let path = themes_dir().join(name);
    assert!(
        path.is_file(),
        "{} is required (ADR 0021 D3, ADR 0002 D5)",
        path.display()
    );
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn palette_from_theme_file(name: &str) -> Palette {
    let text = theme_file(name);
    let mut ansi = [Rgb { r: 0, g: 0, b: 0 }; 16];
    let mut foreground = None;
    let mut background = None;
    let mut cursor = None;
    let mut seen = [false; 16];
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("palette = ") {
            let (idx, hex) = rest.split_once('=').expect("palette = N=#hex");
            let i: usize = idx.parse().expect("palette index");
            assert!(i < 16, "ansi index");
            ansi[i] = parse_hex_rgb(hex);
            seen[i] = true;
        } else if let Some(hex) = line.strip_prefix("foreground = ") {
            foreground = Some(parse_hex_rgb(hex));
        } else if let Some(hex) = line.strip_prefix("background = ") {
            background = Some(parse_hex_rgb(hex));
        } else if let Some(hex) = line.strip_prefix("cursor-color = ") {
            cursor = Some(parse_hex_rgb(hex));
        }
    }
    assert!(seen.iter().all(|s| *s), "{name} missing an ANSI 0–15 entry");
    Palette {
        ansi,
        foreground: foreground.expect("{name} foreground ="),
        background: background.expect("{name} background ="),
        cursor: cursor.expect("{name} cursor-color ="),
    }
}

fn file_palette_entry(name: &str, index: usize) -> u32 {
    let prefix = format!("palette = {index}=");
    for line in theme_file(name).lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(&prefix) {
            return pack(parse_hex_rgb(rest));
        }
    }
    panic!("{name} has no palette = {index}=");
}

fn file_key(name: &str, key: &str) -> u32 {
    let prefix = format!("{key} = ");
    for line in theme_file(name).lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(&prefix) {
            return pack(parse_hex_rgb(rest));
        }
    }
    panic!("{name} has no {key} =");
}

fn wcag_contrast(a: u32, b: u32) -> f64 {
    fn luma(rgba: u32) -> f64 {
        let chan = |shift: u32| {
            let s = ((rgba >> shift) & 0xff) as f64 / 255.0;
            if s <= 0.04045 {
                s / 12.92
            } else {
                ((s + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * chan(24) + 0.7152 * chan(16) + 0.0722 * chan(8)
    }
    let (x, y) = (luma(a), luma(b));
    let (hi, lo) = if x > y { (x, y) } else { (y, x) };
    (hi + 0.05) / (lo + 0.05)
}

/// T-CHIP1-SGR — bold sets attrs bit 0.
///
/// Required mutation: `RILL_MUTATE=ignore_sgr`.
#[test]
fn t_chip1_sgr_bold_sets_attrs_bit_0() {
    let mut vt = engine(8, 4);
    vt.feed(b"\x1b[1mX").expect("feed");
    let grid = vt.snapshot().expect("snapshot");
    let cell = grid.cell(0, 0).expect("cell");
    assert_eq!(cell.codepoint, u32::from(b'X'));
    assert_ne!(cell.attrs & 1, 0, "CSI 1 m must set attrs bit 0 (bold)");

    let mut vt = engine(8, 4);
    vt.feed(b"\x1b[1;99;4mY").expect("feed unknown skipped");
    let cell = vt
        .snapshot()
        .expect("snapshot")
        .cell(0, 0)
        .copied()
        .unwrap();
    assert_ne!(cell.attrs & 1, 0, "unknown 99 must not abort bold");
    assert_ne!(cell.attrs & 2, 0, "unknown 99 must not abort underline");
}

/// T-CHIP1-COLOR-IDENTITY — SGR keeps its palette index until materialisation.
///
/// Required mutation: `RILL_MUTATE=sgr_rgb_at_parse`.
#[test]
fn t_chip1_color_identity_sgr_keeps_palette_index() {
    let mut vt = engine(8, 4);
    vt.feed(b"\x1b[32mG").expect("feed");
    assert_eq!(
        vt.color_at(0, 0).expect("cell").0,
        Color::Indexed(2),
        "CSI 32 m is Indexed(2) before snapshot (ADR 0021 D1)"
    );

    vt.set_palette(palette_from_theme_file("Catppuccin Latte"))
        .expect("latte");
    let latte_fg = vt.snapshot().expect("latte snap").cell(0, 0).unwrap().fg;

    vt.set_palette(palette_from_theme_file("Catppuccin Mocha"))
        .expect("mocha");
    let mocha_fg = vt.snapshot().expect("mocha snap").cell(0, 0).unwrap().fg;

    assert_ne!(
        latte_fg, mocha_fg,
        "the same Indexed(2) cell must materialise differently on Latte vs Mocha"
    );
    assert_eq!(
        latte_fg,
        file_palette_entry("Catppuccin Latte", 2),
        "Latte materialisation must equal the file's palette = 2="
    );
    assert_eq!(
        mocha_fg,
        file_palette_entry("Catppuccin Mocha", 2),
        "Mocha materialisation must equal the file's palette = 2="
    );
}

/// T-CHIP1-LOOK-ANSI — SGR colours come from the theme file (Latte and Mocha).
///
/// Required mutation: `RILL_MUTATE=skip_file_palette`.
#[test]
fn t_chip1_look_ansi_sgr_colours_come_from_the_theme_file() {
    for name in ["Catppuccin Latte", "Catppuccin Mocha"] {
        let pal = palette_from_theme_file(name);
        let mut sgr = engine(8, 4);
        sgr.set_palette(pal).expect("palette");
        sgr.feed(b"\x1b[32mG").expect("sgr");
        let g = sgr.snapshot().expect("snap").cell(0, 0).unwrap().fg;
        assert_eq!(
            g,
            file_palette_entry(name, 2),
            "{name}: CSI 32 m must equal palette = 2= from the file"
        );

        let mut plain = engine(8, 4);
        plain.set_palette(pal).expect("palette");
        plain.feed(b"A").expect("plain");
        let grid = plain.snapshot().expect("snap");
        let a = grid.cell(0, 0).unwrap().fg;
        let fg = file_key(name, "foreground");
        let bg = file_key(name, "background");
        assert_eq!(
            a, fg,
            "{name}: unstyled A must equal foreground = from the file"
        );
        assert!(
            wcag_contrast(a, bg) >= 4.5,
            "{name}: unstyled fg/bg contrast {} must be ≥ 4.5",
            wcag_contrast(a, bg)
        );
    }
}

/// xterm-256 cube and greyscale ramp are arithmetic, not a theme table.
#[test]
fn t_chip1_indexed_cube_and_ramp_are_arithmetic() {
    let mut vt = engine(8, 4);
    vt.feed(b"\x1b[38;5;16mA").expect("cube origin");
    let fg = vt.snapshot().expect("snap").cell(0, 0).unwrap().fg;
    assert_eq!(fg, 0x000000ff, "Indexed(16) is cube 0,0,0");

    let mut vt = engine(8, 4);
    vt.feed(b"\x1b[38;5;232mA").expect("grey");
    let fg = vt.snapshot().expect("snap").cell(0, 0).unwrap().fg;
    assert_eq!(fg, 0x080808ff, "Indexed(232) is 8+10*0");
}
