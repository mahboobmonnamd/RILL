//! Ghostty / cmux look-key subset (ADR 0017 D2). Not TOML. Not a theme store.
//! Theme RGB comes from Ghostty-grammar files, never a compiled-in catalog.

use crate::surface::HostSurface;
use rill_vt_types::{Palette, PodGrid, Rgb};
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

pub fn look_file_candidates_for(home: Option<&Path>, rill_config: Option<String>) -> Vec<PathBuf> {
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

fn rgba_to_rgb(v: u32) -> Rgb {
    Rgb {
        r: ((v >> 24) & 0xff) as u8,
        g: ((v >> 16) & 0xff) as u8,
        b: ((v >> 8) & 0xff) as u8,
    }
}

/// Build a Chip 1 `Palette` from resolved theme colours (ADR 0037 D4).
pub fn palette_from_theme(colors: &ThemeColors) -> Palette {
    let default = Palette::vt_default();
    let ansi = colors
        .ansi
        .map(|slots| std::array::from_fn(|i| rgba_to_rgb(slots[i])))
        .unwrap_or(default.ansi);
    Palette {
        ansi,
        foreground: rgba_to_rgb(colors.foreground),
        background: rgba_to_rgb(colors.background),
        cursor: rgba_to_rgb(colors.cursor),
    }
}
