//! Ghostty-grammar look keys and host-surface.toml (ADR 0017, ADR 0025).
//! Shared by the GUI attach client and Chip 0 measurement gates.

mod look;
mod surface;

pub use look::{
    apply_theme, chrome_surface_rgba, load_look_overlay, look_file_candidates_for, overlay_look,
    parse_look_keys, palette_from_theme, resolve_theme, TerminalLook, ThemeColors,
};
pub use surface::{
    discover_host_surface, load_host_surface, load_resolved_surface, HostSurface,
};
