//! Ghostty-grammar look keys and host-surface.toml. Not a VT.

mod look;
mod surface;

pub use look::{
    apply_theme, chrome_surface_rgba, load_look_overlay, look_file_candidates_for, overlay_look,
    parse_hex, parse_look_keys, resolve_theme, TerminalLook, ThemeColors,
};
pub use rill_vt_types::Error;
pub use surface::{discover_host_surface, load_host_surface, load_resolved_surface, HostSurface};
