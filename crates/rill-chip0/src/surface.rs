use crate::Error;
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
}

fn default_cols() -> u16 {
    80
}
fn default_rows() -> u16 {
    24
}

pub fn load_host_surface(path: impl AsRef<Path>) -> Result<HostSurface, Error> {
    let text = std::fs::read_to_string(path.as_ref())?;
    let cfg: HostSurface = toml::from_str(&text).map_err(|e| Error::Config(e.to_string()))?;
    if cfg.font_family.is_empty() {
        return Err(Error::Config("font-family required".into()));
    }
    Ok(cfg)
}

#[allow(dead_code)]
pub fn discover_host_surface() -> PathBuf {
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
