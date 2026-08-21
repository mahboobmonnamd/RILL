//! Look paint oracles that need Chip 0 (T-LOOK).

use super::*;
use std::path::{Path, PathBuf};

fn fixture_ghostty() -> &'static str {
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

/// Oracle: parse `background =` from the theme file, not a Rust constant.
fn background_from_theme_file(name: &str) -> u32 {
    let text = std::fs::read_to_string(fixture_themes().join(name)).expect("theme file");
    parse_look_keys(&text, None)
        .expect("theme file is Ghostty grammar")
        .background
        .expect("theme file has background =")
}

fn colors_from_theme_file(name: &str) -> ThemeColors {
    resolve_theme(name, Some(&fixture_themes())).expect("resolve fixture theme")
}

/// Oracle: `palette = N=` from the theme file, not ThemeColors we also fed in.
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

fn first_codepoint(grid: &PodGrid, cp: u32) -> crate::PodCell {
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
fn t_ghostty_look_theme_file_wins_over_hardcoded_rgb() {
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
    assert_eq!(
        bg, probe,
        "theme file background = #a1b2c3 must win; hardcoded Latte is {bg:#08x}"
    );
    assert_ne!(
        bg, 0xeff1_f5ff,
        "oracle is the file, not official Latte cream"
    );
}

/// T-LOOK-OVERLAY. Required mutation: `skip_ghostty_overlay`.
#[test]
fn t_ghostty_look_overlay_applies_latte_and_font_size() {
    let look = parse_look_keys(fixture_ghostty(), Some(&fixture_themes()))
        .expect("parse system-setup look keys");
    let resolved = overlay_look(base_surface(), &look);
    assert_eq!(
        resolved.font_size, 16.0,
        "Ghostty font-size=16 must win over host-surface 13"
    );
    assert_eq!(
        resolved.font_family, "JetBrainsMono Nerd Font",
        "Ghostty font-family must win"
    );
    assert_eq!(resolved.padding_x, 8.0);
    assert_eq!(resolved.padding_y, 8.0);
    let expected = background_from_theme_file("Catppuccin Latte");
    let bg = resolved.colors.expect("theme colours").background;
    assert_eq!(
        bg, expected,
        "theme = Catppuccin Latte must resolve to the theme file background, not Chip0 dark ({bg:#08x})"
    );
    assert_ne!(bg, 0x1212_12ff);
    assert!(resolved.macos_option_as_alt);
}

/// User look file is ~/.config/rill/config, not Ghostty or cmux.
#[test]
fn t_ghostty_look_user_file_is_config_rill() {
    let paths = look_file_candidates_for(Some(Path::new("/Users/tester")), None);
    assert_eq!(
        paths,
        vec![PathBuf::from("/Users/tester/.config/rill/config")]
    );
    let override_paths = look_file_candidates_for(
        Some(Path::new("/Users/tester")),
        Some("/tmp/rill-test-config".into()),
    );
    assert_eq!(
        override_paths[0],
        PathBuf::from("/tmp/rill-test-config"),
        "RILL_CONFIG must win"
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
fn t_ghostty_look_unknown_theme_does_not_replace_host_surface_colors() {
    let mut base = base_surface();
    base.colors = Some(colors_from_theme_file("Catppuccin Latte"));
    let look = parse_look_keys("theme = NotARealTheme\n", None).expect("parsed unknown theme");
    assert!(
        look.background.is_none(),
        "unknown theme must not invent colours at parse"
    );
    let resolved = overlay_look(base, &look);
    let expected = background_from_theme_file("Catppuccin Latte");
    let bg = resolved
        .colors
        .expect("host-surface Latte must survive")
        .background;
    assert_eq!(bg, expected);
}

/// T-LOOK-CELL. Required mutation: `skip_theme_apply`.
#[test]
fn t_ghostty_look_themed_empty_cell_is_not_chip0_default_dark() {
    let mut chip = Chip0::new(80, 24).expect("chip0");
    let mut grid = chip.snapshot().expect("snapshot");
    let before = grid.cell(0, 0).expect("cell").bg;
    let expected = background_from_theme_file("Catppuccin Latte");
    assert_eq!(
        before, grid.default_bg,
        "empty cell must be the VT default so remap has something to catch"
    );
    assert_ne!(
        before, expected,
        "precondition: Chip0 default is not already the theme file background"
    );

    let mut surface = base_surface();
    surface.colors = Some(colors_from_theme_file("Catppuccin Latte"));
    apply_theme(&mut grid, &surface);
    let after = grid.cell(0, 0).expect("cell").bg;
    assert_eq!(
        after, expected,
        "empty cell bg must be the theme file background, not Chip0 {before:#08x}"
    );
    assert_ne!(after, before);
}

/// T-LOOK-ANSI. Bug: Ghostty/cmux paint Latte SGR green from the theme
/// file; Rill left libghostty-vt's dark default palette, so `killall` was
/// pale yellow-green on `#eff1f5`.
/// Required mutation: `skip_vt_look_colors`.
#[test]
fn t_ghostty_look_sgr_green_is_theme_file_palette() {
    let colors = colors_from_theme_file("Catppuccin Latte");
    let expected = palette_from_theme_file("Catppuccin Latte", 2);
    let bg = background_from_theme_file("Catppuccin Latte");
    assert_eq!(
        colors.ansi.expect("Latte file has palette 0-15")[2],
        expected,
        "precondition: resolve_theme palette 2 matches the file"
    );

    let mut chip = Chip0::new(80, 24).expect("chip0");
    chip.apply_look(&colors).expect("apply_look");
    chip.feed(b"\x1b[32mG").expect("feed SGR green");
    let grid = chip.snapshot().expect("snapshot");
    let cell = first_codepoint(&grid, b'G' as u32);
    assert_eq!(
        cell.fg, expected,
        "SGR 32 must be Latte file palette 2, not Chip0 default green; got {:#08x}",
        cell.fg
    );
    let builtin_green = 0xb5bd_68ff;
    assert!(
        wcag_contrast(cell.fg, bg) > wcag_contrast(builtin_green, bg),
        "file palette 2 must beat Chip0 default green on Latte bg; file {:.2} vs builtin {:.2}",
        wcag_contrast(cell.fg, bg),
        wcag_contrast(builtin_green, bg)
    );
}

/// Unstyled glyphs must be the file foreground, not `#cccccc` on cream.
#[test]
fn t_ghostty_look_unstyled_text_is_theme_file_foreground() {
    let colors = colors_from_theme_file("Catppuccin Latte");
    let expected = parse_look_keys(
        &std::fs::read_to_string(fixture_themes().join("Catppuccin Latte")).expect("file"),
        None,
    )
    .expect("parse")
    .foreground
    .expect("foreground =");
    let bg = background_from_theme_file("Catppuccin Latte");

    let mut chip = Chip0::new(80, 24).expect("chip0");
    chip.apply_look(&colors).expect("apply_look");
    chip.feed(b"A").expect("feed");
    let grid = chip.snapshot().expect("snapshot");
    let cell = first_codepoint(&grid, b'A' as u32);
    assert_eq!(
        cell.fg, expected,
        "unstyled text must be Latte file foreground, not Chip0 default; got {:#08x}",
        cell.fg
    );
    assert!(
        wcag_contrast(cell.fg, bg) >= 4.5,
        "unstyled fg on Latte bg must be readable; contrast {:.2}",
        wcag_contrast(cell.fg, bg)
    );
}

#[test]
fn t_chrome_surface_darkens_latte_and_mocha_file_backgrounds() {
    let latte = background_from_theme_file("Catppuccin Latte");
    let mocha = background_from_theme_file("Catppuccin Mocha");
    let latte_s = chrome_surface_rgba(latte);
    let mocha_s = chrome_surface_rgba(mocha);
    assert_ne!(latte_s, latte, "chrome must not match Chip 0 Latte base");
    assert_ne!(mocha_s, mocha, "chrome must not match Chip 0 Mocha base");
    assert_ne!(
        latte_s, mocha_s,
        "two files must not share one cream constant"
    );
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
    assert_ne!(cfg.font_family, "SF Mono");
    let bg = cfg.colors.expect("host-surface theme file").background;
    assert_eq!(bg, background_from_theme_file("Catppuccin Latte"));
}
