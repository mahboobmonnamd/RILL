//! Look helpers live in `rill-look`; Chip 0 gates that need libghostty stay here.

pub use rill_look::*;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{load_host_surface, Chip0, TerminalEmulation};
    use std::path::PathBuf;

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

    fn background_from_theme_file(name: &str) -> u32 {
        let text = std::fs::read_to_string(fixture_themes().join(name)).expect("theme file");
        parse_look_keys(&text, None)
            .expect("theme file is Ghostty grammar")
            .background
            .expect("theme file has background =")
    }

    fn colors_from_theme_file(name: &str) -> ThemeColors {
        resolve_theme(name, Some(fixture_themes().as_path())).expect("theme file")
    }

    fn palette_from_theme_file(name: &str, index: usize) -> u32 {
        let prefix = format!("palette = {index}=");
        for line in std::fs::read_to_string(fixture_themes().join(name))
            .expect("theme file")
            .lines()
        {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix(&prefix) {
                return parse_hex(rest).expect("palette hex");
            }
        }
        panic!("palette = {index}= missing in {name}");
    }

    fn wcag_contrast(fg: u32, bg: u32) -> f64 {
        let l = |c: u32| {
            let v = (c >> 8) as f64 / 255.0;
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        let (l1, l2) = (l(fg), l(bg));
        let (hi, lo) = if l1 > l2 { (l1, l2) } else { (l2, l1) };
        (hi + 0.05) / (lo + 0.05)
    }

    #[test]
    fn t_ghostty_look_theme_file_wins_over_hardcoded_rgb() {
        let look = parse_look_keys(fixture_ghostty(), Some(fixture_themes().as_path()))
            .expect("fixture config");
        let surface = overlay_look(
            load_host_surface("../../host-surface.toml").expect("host-surface"),
            &look,
        );
        let bg = surface.colors.expect("theme").background;
        let expected = background_from_theme_file("Catppuccin Latte");
        assert_eq!(
            bg, expected,
            "theme = Catppuccin Latte must resolve to the theme file background, not Chip0 dark ({bg:#08x})"
        );
    }

    #[test]
    fn t_ghostty_look_overlay_applies_latte_and_font_size() {
        let look = parse_look_keys(fixture_ghostty(), Some(fixture_themes().as_path()))
            .expect("fixture config");
        let surface = overlay_look(
            load_host_surface("../../host-surface.toml").expect("host-surface"),
            &look,
        );
        assert_eq!(surface.font_size, 16.0);
        assert_eq!(surface.theme.as_deref(), Some("Catppuccin Latte"));
    }

    #[test]
    fn t_ghostty_look_user_file_is_config_rill() {
        let candidates = look_file_candidates_for(
            Some(std::path::Path::new("/home/test")),
            Some("fixtures/look/rill.config".into()),
        );
        assert_eq!(candidates[0], PathBuf::from("fixtures/look/rill.config"));
    }

    #[test]
    fn t_ghostty_look_unknown_theme_does_not_replace_host_surface_colors() {
        let mut look = TerminalLook::default();
        look.theme_name = Some("NoSuchTheme".into());
        let base = load_host_surface("../../host-surface.toml").expect("host-surface");
        let before = base.colors.clone();
        let after = overlay_look(base, &look);
        assert_eq!(after.colors, before);
    }

    #[test]
    fn t_ghostty_look_themed_empty_cell_is_not_chip0_default_dark() {
        let mut chip = Chip0::new(80, 24).expect("chip0");
        let colors = colors_from_theme_file("Catppuccin Latte");
        let expected = colors.background;
        let before = chip.snapshot().expect("snap").cell(0, 0).expect("cell").bg;
        assert_ne!(
            before, expected,
            "precondition: Chip0 default is not already the theme file background"
        );
        chip.feed(b" ").expect("feed");
        let mut grid = chip.snapshot().expect("snap");
        let mut surface = load_host_surface("../../host-surface.toml").expect("host-surface");
        surface.colors = Some(colors);
        apply_theme(&mut grid, &surface);
        assert_eq!(
            grid.cell(0, 0).expect("cell").bg,
            expected,
            "empty cell bg must be the theme file background, not Chip0 {before:#08x}"
        );
    }

    #[test]
    fn t_ghostty_look_sgr_green_is_theme_file_palette() {
        let colors = colors_from_theme_file("Catppuccin Latte");
        let expected = palette_from_theme_file("Catppuccin Latte", 2);
        let bg = background_from_theme_file("Catppuccin Latte");
        let mut chip = Chip0::new(80, 24).expect("chip0");
        chip.apply_look(&colors).expect("apply_look");
        chip.feed(b"\x1b[32mG").expect("sgr");
        let fg = chip.snapshot().expect("snap").cell(0, 0).expect("cell").fg;
        assert_eq!(
            fg, expected,
            "SGR 32 must be Latte file palette 2, not Chip0 default green; got {:#08x}",
            fg
        );
        assert!(
            wcag_contrast(fg, bg) >= 4.5,
            "file palette 2 must beat Chip0 default green on Latte bg; file {:.2} vs builtin {:.2}",
            wcag_contrast(fg, bg),
            wcag_contrast(0x00ff00ff, bg)
        );
    }

    #[test]
    fn t_ghostty_look_unstyled_text_is_theme_file_foreground() {
        let colors = colors_from_theme_file("Catppuccin Latte");
        let expected = colors.foreground;
        let bg = background_from_theme_file("Catppuccin Latte");
        let mut chip = Chip0::new(80, 24).expect("chip0");
        chip.apply_look(&colors).expect("apply_look");
        chip.feed(b"A").expect("plain");
        let fg = chip.snapshot().expect("snap").cell(0, 0).expect("cell").fg;
        assert_eq!(
            fg, expected,
            "unstyled text must be Latte file foreground, not Chip0 default; got {:#08x}",
            fg
        );
        assert!(wcag_contrast(fg, bg) >= 4.5);
    }

    #[test]
    fn t_chrome_surface_darkens_latte_and_mocha_file_backgrounds() {
        let latte = background_from_theme_file("Catppuccin Latte");
        let mocha = background_from_theme_file("Catppuccin Mocha");
        let cl = chrome_surface_rgba(latte);
        let cm = chrome_surface_rgba(mocha);
        assert_ne!(cl, latte);
        assert_ne!(cm, mocha);
        assert_ne!(cl, cm);
    }
}
