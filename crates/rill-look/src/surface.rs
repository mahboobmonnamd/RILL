use crate::look::{resolve_theme, ThemeColors};
use rill_vt_types::Error;
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
pub struct HostSurface {
    #[serde(rename = "font-family")]
    pub font_family: String,
    #[serde(rename = "font-size")]
    pub font_size: f32,
    #[serde(rename = "font-fallbacks", default)]
    pub font_fallbacks: Vec<String>,
    #[serde(default = "default_cols")]
    pub cols: u16,
    #[serde(default = "default_rows")]
    pub rows: u16,
    #[serde(default)]
    pub theme: Option<String>,
    #[serde(rename = "padding-x", default)]
    pub padding_x: f32,
    #[serde(rename = "padding-y", default)]
    pub padding_y: f32,
    #[serde(rename = "background-opacity", default = "default_opaque")]
    pub background_opacity: f32,
    #[serde(rename = "macos-option-as-alt", default)]
    pub macos_option_as_alt: bool,
    #[serde(skip)]
    pub colors: Option<ThemeColors>,
}

fn default_cols() -> u16 {
    80
}
fn default_rows() -> u16 {
    24
}
fn default_opaque() -> f32 {
    1.0
}

pub fn load_host_surface(path: impl AsRef<Path>) -> Result<HostSurface, Error> {
    let text = std::fs::read_to_string(path.as_ref())?;
    let mut cfg: HostSurface = toml::from_str(&text).map_err(|e| Error::Config(e.to_string()))?;
    if cfg.font_family.is_empty() {
        return Err(Error::Config("font-family required".into()));
    }
    if let Some(name) = cfg.theme.clone() {
        cfg.colors = resolve_theme(&name, path.as_ref().parent());
    }
    Ok(cfg)
}

pub fn load_resolved_surface(path: impl AsRef<Path>) -> Result<HostSurface, Error> {
    let base = load_host_surface(path)?;
    match crate::look::load_look_overlay() {
        Some(look) => Ok(crate::look::overlay_look(base, &look)),
        None => Ok(base),
    }
}

#[allow(dead_code)]
pub fn discover_host_surface() -> PathBuf {
    if let Ok(p) = std::env::var("RILL_HOST_SURFACE") {
        if !p.is_empty() {
            return PathBuf::from(p);
        }
    }
    let candidates = [
        PathBuf::from("host-surface.toml"),
        PathBuf::from("../host-surface.toml"),
        PathBuf::from("../../host-surface.toml"),
    ];
    for p in candidates {
        if p.exists() {
            return p;
        }
    }
    PathBuf::from("host-surface.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_surface_does_not_hardcode_system_mono() {
        let cfg = load_host_surface("host-surface.toml")
            .or_else(|_| load_host_surface("../../host-surface.toml"));
        let cfg = cfg.expect("host-surface.toml");
        assert_ne!(cfg.font_family, "SF Mono");
        assert!(!cfg.font_family.is_empty());
    }
}
