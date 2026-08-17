//! Ghostty / cmux look-key subset (ADR 0017 D2). Not TOML. Not a theme store.
//! Theme RGB comes from Ghostty-grammar files, never a compiled-in catalog.

use crate::surface::HostSurface;
use crate::PodGrid;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct ThemeColors {
    pub background: u32,
    pub foreground: u32,
    pub cursor: u32,
    pub ansi: Option<[u32; 16]>,
    pub split_divider: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct TerminalLook {
    pub font_family: Option<String>,
    pub font_size: Option<f32>,
    pub font_fallbacks: Vec<String>,
    pub background: Option<u32>,
    pub foreground: Option<u32>,
    pub cursor: Option<u32>,
    pub ansi: Option<[u32; 16]>,
    pub padding_x: Option<f32>,
    pub padding_y: Option<f32>,
    pub background_opacity: Option<f32>,
    pub split_divider: Option<u32>,
    pub theme_name: Option<String>,
    pub macos_option_as_alt: Option<bool>,
}

pub fn parse_look_keys(text: &str, theme_directory: Option<&Path>) -> Option<TerminalLook> {
    let mut look = TerminalLook::default();
    let mut ansi = [0u32; 16];
    let mut ansi_set = [false; 16];
    let mut saw = false;
    let mut explicit_bg = None;
    let mut explicit_fg = None;
    let mut explicit_cursor = None;
    let mut explicit_split = None;

    for raw in text.lines() {
        let mut line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some(idx) = line.find(" #") {
            let rest = line[idx + 2..].trim();
            if parse_hex(rest).is_none() && parse_hex(&format!("#{rest}")).is_none() {
                line = line[..idx].trim();
            }
        }
        let Some(eq) = line.find('=') else { continue };
        let key = line[..eq].trim().to_ascii_lowercase();
        let mut value = line[eq + 1..].trim().to_string();
        if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
            value = value[1..value.len() - 1].to_string();
        }
        match key.as_str() {
            "font-family" if !value.is_empty() => {
                look.font_family = Some(value);
                saw = true;
            }
            "font-size" => {
                if let Ok(n) = value.parse::<f32>() {
                    look.font_size = Some(n);
                    saw = true;
                }
            }
            "font-family-fallback" if !value.is_empty() => {
                look.font_fallbacks.push(value);
                saw = true;
            }
            "background" => {
                if let Some(h) = parse_hex(&value) {
                    explicit_bg = Some(h);
                    saw = true;
                }
            }
            "foreground" => {
                if let Some(h) = parse_hex(&value) {
                    explicit_fg = Some(h);
                    saw = true;
                }
            }
            "cursor" | "cursor-color" => {
                if let Some(h) = parse_hex(&value) {
                    explicit_cursor = Some(h);
                    saw = true;
                }
            }
            "window-padding-x" => {
                if let Some(n) = first_number(&value) {
                    look.padding_x = Some(n);
                    saw = true;
                }
            }
            "window-padding-y" => {
                if let Some(n) = first_number(&value) {
                    look.padding_y = Some(n);
                    saw = true;
                }
            }
            "background-opacity" => {
                if let Ok(n) = value.parse::<f32>() {
                    look.background_opacity = Some(n.clamp(0.0, 1.0));
                    saw = true;
                }
            }
            "split-divider-color" => {
                if let Some(h) = parse_hex(&value) {
                    explicit_split = Some(h);
                    saw = true;
                }
            }
            "theme" => {
                look.theme_name = Some(light_theme_name(&value));
                saw = true;
            }
            "macos-option-as-alt" => {
                look.macos_option_as_alt = parse_bool(&value);
                if look.macos_option_as_alt.is_some() {
                    saw = true;
                }
            }
            "palette" => {
                let parts: Vec<&str> = value.splitn(2, '=').map(str::trim).collect();
                if parts.len() == 2 {
                    if let Ok(i) = parts[0].parse::<usize>() {
                        if i <= 15 {
                            if let Some(h) = parse_hex(parts[1]) {
                                ansi[i] = h;
                                ansi_set[i] = true;
                                saw = true;
                            }
                        }
                    }
                }
            }
            _ => {
                if let Some(rest) = key.strip_prefix("palette.") {
                    if let Ok(i) = rest.parse::<usize>() {
                        if i <= 15 {
                            if let Some(h) = parse_hex(&value) {
                                ansi[i] = h;
                                ansi_set[i] = true;
                                saw = true;
                            }
                        }
                    }
                }
            }
        }
    }
    if !saw {
        return None;
    }
    if let Some(name) = look.theme_name.as_deref() {
        if let Some(theme) = resolve_theme(name, theme_directory) {
            look.background = Some(theme.background);
            look.foreground = Some(theme.foreground);
            look.cursor = Some(theme.cursor);
            look.split_divider = theme.split_divider;
            look.ansi = theme.ansi;
        }
    }
    if let Some(h) = explicit_bg {
        look.background = Some(h);
    }
    if let Some(h) = explicit_fg {
        look.foreground = Some(h);
    }
    if let Some(h) = explicit_cursor {
        look.cursor = Some(h);
    }
    if let Some(h) = explicit_split {
        look.split_divider = Some(h);
    }
    if ansi_set.iter().any(|s| *s) {
        if let Some(mut slots) = look.ansi {
            for (i, set) in ansi_set.iter().enumerate() {
                if *set {
                    slots[i] = ansi[i];
                }
            }
            look.ansi = Some(slots);
        } else if ansi_set.iter().all(|s| *s) {
            look.ansi = Some(ansi);
        }
    }
    Some(look)
}

pub fn overlay_look(mut base: HostSurface, look: &TerminalLook) -> HostSurface {
    #[cfg(feature = "mutate")]
    if std::env::var("RILL_MUTATE").as_deref() == Ok("skip_ghostty_overlay") {
        return base;
    }

    if let Some(f) = look.font_family.clone() {
        base.font_family = f;
    }
    if let Some(s) = look.font_size {
        base.font_size = s;
    }
    if !look.font_fallbacks.is_empty() {
        base.font_fallbacks = look.font_fallbacks.clone();
    }
    if let Some(n) = look.padding_x {
        base.padding_x = n;
    }
    if let Some(n) = look.padding_y {
        base.padding_y = n;
    }
    if let Some(n) = look.background_opacity {
        base.background_opacity = n;
    }
    if let Some(b) = look.macos_option_as_alt {
        base.macos_option_as_alt = b;
    }

    #[cfg(feature = "mutate")]
    if std::env::var("RILL_MUTATE").as_deref() == Ok("unknown_theme_wipes")
        && look.theme_name.is_some()
        && look.background.is_none()
    {
        base.colors = None;
        return base;
    }

    if look.background.is_some()
        || look.foreground.is_some()
        || look.cursor.is_some()
        || look.ansi.is_some()
    {
        let mut colors = base.colors.clone().unwrap_or_else(|| ThemeColors {
            background: look.background.unwrap_or(0x1212_12ff),
            foreground: look.foreground.unwrap_or(0xcccc_ccff),
            cursor: look.cursor.or(look.foreground).unwrap_or(0xd9d9_d9ff),
            ansi: look.ansi,
            split_divider: look.split_divider,
        });
        if let Some(c) = look.background {
            colors.background = c;
        }
        if let Some(c) = look.foreground {
            colors.foreground = c;
        }
        if let Some(c) = look.cursor {
            colors.cursor = c;
        }
        if let Some(a) = look.ansi {
            colors.ansi = Some(a);
        }
        if look.split_divider.is_some() {
            colors.split_divider = look.split_divider;
        }
        base.colors = Some(colors);
        if let Some(name) = look.theme_name.clone() {
            base.theme = Some(name);
        }
    }
    base
}

pub fn apply_theme(grid: &mut PodGrid, surface: &HostSurface) {
    #[cfg(feature = "mutate")]
    if std::env::var("RILL_MUTATE").as_deref() == Ok("skip_theme_apply") {
        return;
    }
    let Some(theme) = surface.colors.as_ref() else {
        return;
    };
    let def_fg = grid.default_fg;
    let def_bg = grid.default_bg;
    for cell in &mut grid.cells {
        if cell.fg == def_fg {
            cell.fg = theme.foreground;
        }
        if cell.bg == def_bg {
            cell.bg = theme.background;
        }
    }
}

pub fn load_look_overlay() -> Option<TerminalLook> {
    #[cfg(feature = "mutate")]
    if std::env::var("RILL_MUTATE").as_deref() == Ok("skip_ghostty_overlay") {
        return None;
    }
    for path in look_file_candidates() {
        if let Ok(text) = std::fs::read_to_string(&path) {
            let themes = path.parent().map(|p| p.join("themes"));
            if let Some(look) = parse_look_keys(&text, themes.as_deref()) {
                return Some(look);
            }
        }
    }
    None
}

fn look_file_candidates() -> Vec<PathBuf> {
    look_file_candidates_for(
        std::env::var_os("HOME").map(PathBuf::from).as_deref(),
        std::env::var("RILL_CONFIG").ok().filter(|p| !p.is_empty()),
    )
}

pub(crate) fn look_file_candidates_for(
    home: Option<&Path>,
    rill_config: Option<String>,
) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(p) = rill_config {
        out.push(PathBuf::from(p));
    }
    if let Some(home) = home {
        out.push(home.join(".config/rill/config"));
    }
    out
}

pub fn resolve_theme(name: &str, directory: Option<&Path>) -> Option<ThemeColors> {
    #[cfg(feature = "mutate")]
    if std::env::var("RILL_MUTATE").as_deref() == Ok("invent_theme_rgb") {
        return Some(ThemeColors {
            background: 0xeff1_f5ff,
            foreground: 0x4c4f_69ff,
            cursor: 0xdc8a_78ff,
            ansi: None,
            split_divider: None,
        });
    }

    let key = normalize_theme(name);
    for dir in theme_search_dirs(directory) {
        let slug = key.replace(' ', "-");
        let candidates = [
            dir.join(name),
            dir.join(format!("{name}.conf")),
            dir.join(&slug),
            dir.join(format!("{slug}.conf")),
        ];
        for url in candidates {
            let Ok(text) = std::fs::read_to_string(&url) else {
                continue;
            };
            let Some(look) = parse_look_keys(&text, None) else {
                continue;
            };
            let Some(background) = look.background else {
                continue;
            };
            let Some(foreground) = look.foreground else {
                continue;
            };
            return Some(ThemeColors {
                background,
                foreground,
                cursor: look.cursor.unwrap_or(foreground),
                ansi: look.ansi,
                split_divider: look.split_divider,
            });
        }
    }
    None
}

fn theme_search_dirs(primary: Option<&Path>) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let mut push = |p: PathBuf| {
        if !p.as_os_str().is_empty() && !dirs.iter().any(|d| d == &p) {
            dirs.push(p);
        }
    };
    if let Some(p) = primary {
        push(p.to_path_buf());
        push(p.join("themes"));
        push(p.join("fixtures/look/themes"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        push(PathBuf::from(home).join(".config/rill/themes"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            push(dir.join("../Resources/themes"));
            push(dir.join("themes"));
        }
    }
    for rel in [
        "fixtures/look/themes",
        "../fixtures/look/themes",
        "../../fixtures/look/themes",
    ] {
        let p = PathBuf::from(rel);
        if p.is_dir() {
            push(p);
        }
    }
    dirs
}

fn normalize_theme(name: &str) -> String {
    let mut text = name.trim().to_string();
    if let Some(stripped) = text.strip_suffix(".conf") {
        text = stripped.to_string();
    }
    text.replace(['-', '_'], " ").to_ascii_lowercase()
}

fn light_theme_name(value: &str) -> String {
    let parts: Vec<&str> = value.split(',').map(str::trim).collect();
    for part in &parts {
        if part.len() > 6 && part[..6].eq_ignore_ascii_case("light:") {
            return part[6..].trim().to_string();
        }
    }
    if parts.len() == 1 && parts[0].len() > 5 && parts[0][..5].eq_ignore_ascii_case("dark:") {
        return parts[0][5..].trim().to_string();
    }
    parts
        .iter()
        .find(|p| p.len() < 5 || !p[..5].eq_ignore_ascii_case("dark:"))
        .map(|s| (*s).to_string())
        .unwrap_or_else(|| value.to_string())
}

fn first_number(value: &str) -> Option<f32> {
    let token = value
        .split(|c: char| c == ',' || c.is_whitespace())
        .next()
        .unwrap_or(value);
    token.parse().ok()
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" => Some(true),
        "false" | "0" | "no" => Some(false),
        _ => None,
    }
}

/// Chrome pane fill: look `background` with each 8-bit RGB channel
/// saturating-minus 9 (SPEC-CHROME §4a). Not a compiled theme table.
pub fn chrome_surface_rgba(background: u32) -> u32 {
    let r = ((background >> 24) & 0xff).saturating_sub(9);
    let g = ((background >> 16) & 0xff).saturating_sub(9);
    let b = ((background >> 8) & 0xff).saturating_sub(9);
    (r << 24) | (g << 16) | (b << 8) | (background & 0xff)
}

pub fn parse_hex(value: &str) -> Option<u32> {
    let mut hex = value.trim().trim_start_matches('#');
    if !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let owned;
    if hex.len() == 3 {
        owned = hex.chars().flat_map(|c| [c, c]).collect::<String>();
        hex = &owned;
    }
    if hex.len() != 6 {
        return None;
    }
    let n = u32::from_str_radix(hex, 16).ok()?;
    Some((n << 8) | 0xff)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{load_host_surface, Chip0, TerminalEmulation};

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
        let look =
            parse_look_keys("theme = Catppuccin Latte\n", Some(&dir)).expect("parse theme name");
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
        assert_ne!(latte_s, mocha_s, "two files must not share one cream constant");
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
}
