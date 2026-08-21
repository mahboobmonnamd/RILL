//! T-LOOK file oracles on Chip 1. Authority: look files, not a catalog.

use rill_look::{
    apply_theme, chrome_surface_rgba, load_host_surface, look_file_candidates_for, overlay_look,
    parse_hex, parse_look_keys, resolve_theme, HostSurface, ThemeColors,
};
use rill_vt_types::{PodCell, PodGrid, Rgb, TerminalEmulation};
use std::path::{Path, PathBuf};
use vt_engine::{Palette, VtEngine};

fn fixture_look() -> &'static str {
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/look/rill.config"
    ))
}

fn fixture_themes() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/look/themes"
    ))
}

fn background_from_theme_file(name: &str) -> u32 {
    let text = std::fs::read_to_string(fixture_themes().join(name)).expect("theme file");
    parse_look_keys(&text, None)
        .expect("theme file parses")
        .background
        .expect("theme file has background =")
}

fn colors_from_theme_file(name: &str) -> ThemeColors {
    resolve_theme(name, Some(&fixture_themes())).expect("resolve fixture theme")
}

fn palette_from_theme_file(name: &str, index: usize) -> u32 {
    let text = std::fs::read_to_string(fixture_themes().join(name)).expect("theme file");
    let prefix = format!("palette = {index}=");
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix(&prefix) {
            return parse_hex(rest).unwrap_or_else(|| panic!("{name} palette {index}"));
        }
    }
    panic!("{name} has no palette = {index}=");
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

fn first_codepoint(grid: &PodGrid, cp: u32) -> PodCell {
    grid.cells
        .iter()
        .copied()
        .find(|c| c.codepoint == cp)
        .unwrap_or_else(|| panic!("no U+{cp:04X} in snapshot"))
}

fn base_surface() -> HostSurface {
    HostSurface {
        font_family: "Menlo".into(),
        font_size: 13.0,
        font_fallbacks: vec!["Courier".into()],
        cols: 80,
        rows: 24,
        theme: None,
        padding_x: 0.0,
        padding_y: 0.0,
        background_opacity: 1.0,
        macos_option_as_alt: false,
        colors: None,
    }
}

/// T-LOOK-FILE. Required mutation: `invent_theme_rgb`.
#[test]
fn t_look_theme_file_wins_over_hardcoded_rgb() {
    let dir = std::env::temp_dir().join(format!(
        "rill-look-file-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).expect("temp themes");
    let probe = 0xa1b2_c3ff;
    std::fs::write(
        dir.join("Catppuccin Latte"),
        "background = #a1b2c3\nforeground = #010203\ncursor-color = #040506\n",
    )
    .expect("write probe theme");
    let look = parse_look_keys("theme = Catppuccin Latte\n", Some(&dir)).expect("parse theme name");
    let _ = std::fs::remove_dir_all(&dir);
    let bg = look.background.expect("theme file must supply background");
    assert_eq!(bg, probe, "theme file background = #a1b2c3 must win");
    assert_ne!(bg, 0xeff1_f5ff);
}

/// T-LOOK-OVERLAY. Required mutation: `skip_look_overlay`.
#[test]
fn t_look_overlay_applies_latte_and_font_size() {
    let look = parse_look_keys(fixture_look(), Some(&fixture_themes())).expect("parse look keys");
    let resolved = overlay_look(base_surface(), &look);
    assert_eq!(resolved.font_size, 16.0);
    assert_eq!(resolved.font_family, "JetBrainsMono Nerd Font");
    let expected = background_from_theme_file("Catppuccin Latte");
    let bg = resolved.colors.expect("theme colours").background;
    assert_eq!(bg, expected);
    assert_ne!(bg, 0x1212_12ff);
    assert!(resolved.macos_option_as_alt);
}

#[test]
fn t_look_user_file_is_config_rill() {
    let paths = look_file_candidates_for(Some(Path::new("/Users/tester")), None);
    assert_eq!(
        paths,
        vec![PathBuf::from("/Users/tester/.config/rill/config")]
    );
    assert!(
        !paths.iter().any(|p| {
            let s = p.to_string_lossy();
            s.contains("ghostty") || s.contains("cmux")
        }),
        "must not live-read Ghostty or cmux config: {paths:?}"
    );
}

/// T-LOOK-UNKNOWN. Required mutation: `unknown_theme_wipes`.
#[test]
fn t_look_unknown_theme_does_not_replace_host_surface_colors() {
    let mut base = base_surface();
    base.colors = Some(colors_from_theme_file("Catppuccin Latte"));
    let look = parse_look_keys("theme = NotARealTheme\n", None).expect("parsed unknown theme");
    let resolved = overlay_look(base, &look);
    let expected = background_from_theme_file("Catppuccin Latte");
    assert_eq!(
        resolved.colors.expect("Latte survives").background,
        expected
    );
}

/// T-LOOK-CELL. Required mutation: `skip_file_palette`.
#[test]
fn t_look_themed_empty_cell_is_not_vt_default_dark() {
    let colors = colors_from_theme_file("Catppuccin Latte");
    let expected = background_from_theme_file("Catppuccin Latte");
    let mut chip = VtEngine::new(80, 24).expect("vt");
    chip.set_palette(palette_from_theme(&colors))
        .expect("palette");
    let grid = chip.snapshot().expect("snapshot");
    let after = grid.cell(0, 0).expect("cell").bg;
    assert_eq!(after, expected);
}

/// T-LOOK-ANSI. Required mutation: `skip_file_palette`.
#[test]
fn t_look_sgr_green_is_theme_file_palette() {
    let colors = colors_from_theme_file("Catppuccin Latte");
    let expected = palette_from_theme_file("Catppuccin Latte", 2);
    let bg = background_from_theme_file("Catppuccin Latte");
    let mut chip = VtEngine::new(80, 24).expect("vt");
    chip.set_palette(palette_from_theme(&colors))
        .expect("palette");
    chip.feed(b"\x1b[32mG").expect("feed SGR green");
    let grid = chip.snapshot().expect("snapshot");
    let cell = first_codepoint(&grid, b'G' as u32);
    assert_eq!(cell.fg, expected);
    let builtin_green = 0xb5bd_68ff;
    assert!(wcag_contrast(cell.fg, bg) > wcag_contrast(builtin_green, bg));
}

#[test]
fn t_look_unstyled_text_is_theme_file_foreground() {
    let colors = colors_from_theme_file("Catppuccin Latte");
    let expected = parse_look_keys(
        &std::fs::read_to_string(fixture_themes().join("Catppuccin Latte")).expect("file"),
        None,
    )
    .expect("parse")
    .foreground
    .expect("foreground =");
    let mut chip = VtEngine::new(80, 24).expect("vt");
    chip.set_palette(palette_from_theme(&colors))
        .expect("palette");
    chip.feed(b"A").expect("feed");
    let grid = chip.snapshot().expect("snapshot");
    let cell = first_codepoint(&grid, b'A' as u32);
    assert_eq!(cell.fg, expected);
}

#[test]
fn t_chrome_surface_darkens_latte_and_mocha_file_backgrounds() {
    let latte = background_from_theme_file("Catppuccin Latte");
    let mocha = background_from_theme_file("Catppuccin Mocha");
    assert_ne!(chrome_surface_rgba(latte), latte);
    assert_ne!(chrome_surface_rgba(mocha), mocha);
}

#[test]
fn unquoted_hash_hex_is_a_colour_not_a_comment() {
    let look = parse_look_keys("split-divider-color = #5c5f77\n", None).unwrap();
    assert_eq!(look.split_divider, parse_hex("#5c5f77"));
}

#[test]
fn bundled_host_surface_still_parses() {
    let cfg = load_host_surface("host-surface.toml")
        .or_else(|_| load_host_surface("../../host-surface.toml"))
        .expect("host-surface.toml");
    assert!(!cfg.font_family.is_empty());
    let bg = cfg.colors.expect("host-surface theme file").background;
    assert_eq!(bg, background_from_theme_file("Catppuccin Latte"));
}

#[test]
fn apply_theme_still_remaps_default_cells() {
    let mut chip = VtEngine::new(80, 24).expect("vt");
    let mut grid = chip.snapshot().expect("snap");
    let mut surface = base_surface();
    surface.colors = Some(colors_from_theme_file("Catppuccin Latte"));
    apply_theme(&mut grid, &surface);
    assert_eq!(
        grid.cell(0, 0).expect("cell").bg,
        background_from_theme_file("Catppuccin Latte")
    );
}
