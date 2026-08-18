//! T-SPLIT-LOOK / T-CHROME-INSET / T-CHROME-FONT — chrome surface, top
//! inset, and sidebar type size.
//!
//! Bugs (doc comment, not the name — ADR 0002 D6):
//! - sidebars used `colorWithCalibratedWhite:0.09` while Chip 0 remapped to
//!   Catppuccin Latte, then painted the file `background` so chrome and the
//!   grid were the same cream ([#269](https://github.com/mahboobmonnamd/RILL/issues/269),
//!   [#270](https://github.com/mahboobmonnamd/RILL/issues/270)).
//! - Workspaces sat in leftover space because labels were framed for a 680pt
//!   pane (`y = 680 - 36`) while Chip 0 used `padding-y`.
//! - Section labels used 11pt caption size next to a 16pt grid.
//!
//! Required mutations: `hardcoded_chrome_gray`, `hardcoded_chrome_y`,
//! `tiny_chrome_font`.

use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

fn gui_bin() -> PathBuf {
    if let Ok(p) = std::env::var("RILL_GUI_BIN") {
        return PathBuf::from(p);
    }
    let app = std::env::var("RILL_GUI_APP").unwrap_or_else(|_| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../dist/Rill.app")
            .display()
            .to_string()
    });
    PathBuf::from(app).join("Contents/MacOS/Rill")
}

fn fixture_theme(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/look/themes")
        .join(name)
}

/// Oracle: `background =` from the theme file, not a Rust constant.
fn background_hex_from_theme_file(name: &str) -> String {
    let text = fs::read_to_string(fixture_theme(name))
        .unwrap_or_else(|e| panic!("T-SPLIT-LOOK needs {}: {e}", fixture_theme(name).display()));
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("background") {
            let rest = rest.trim_start();
            if let Some(val) = rest.strip_prefix('=') {
                return val.trim().trim_start_matches('#').to_ascii_lowercase();
            }
        }
    }
    panic!("{name} has no background =");
}

/// SPEC-CHROME §4a: each 8-bit channel saturating-minus 9. Independent of
/// `rill_chrome_surface_rgba` so a copied FFI result cannot fake the gate.
fn chrome_surface_hex_from_theme_file(name: &str) -> String {
    let bg = u32::from_str_radix(&background_hex_from_theme_file(name), 16)
        .expect("theme background is RRGGBB");
    let r = ((bg >> 16) & 0xff).saturating_sub(9);
    let g = ((bg >> 8) & 0xff).saturating_sub(9);
    let b = (bg & 0xff).saturating_sub(9);
    format!("{r:02x}{g:02x}{b:02x}")
}

fn unique_sock(tag: &str) -> PathBuf {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    PathBuf::from(format!("/tmp/rill-split-look-{tag}-{n}.sock"))
}

struct Heartbeat {
    seq: u32,
    chrome: Option<u32>,
    nav_bg: Option<String>,
    nav_top: Option<f64>,
    pad_y: Option<f64>,
    chrome_font: Option<f64>,
}

fn read_heartbeat(path: &Path) -> Option<Heartbeat> {
    let s = fs::read_to_string(path).ok()?;
    let mut seq = None;
    let mut chrome = None;
    let mut nav_bg = None;
    let mut nav_top = None;
    let mut pad_y = None;
    let mut chrome_font = None;
    for part in s.split_whitespace() {
        if let Some(v) = part.strip_prefix("seq=") {
            seq = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("chrome=") {
            chrome = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("nav_bg=") {
            nav_bg = Some(v.to_ascii_lowercase());
        }
        if let Some(v) = part.strip_prefix("nav_top=") {
            nav_top = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("pad_y=") {
            pad_y = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("chrome_font=") {
            chrome_font = v.parse().ok();
        }
    }
    Some(Heartbeat {
        seq: seq?,
        chrome,
        nav_bg,
        nav_top,
        pad_y,
        chrome_font,
    })
}

fn alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

fn wait_heartbeat(
    path: &Path,
    pid: u32,
    timeout: Duration,
    pred: impl Fn(&Heartbeat) -> bool,
) -> Option<Heartbeat> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if !alive(pid) {
            return None;
        }
        if let Some(hb) = read_heartbeat(path) {
            if pred(&hb) {
                return Some(hb);
            }
        }
        thread::sleep(Duration::from_millis(20));
    }
    None
}

fn launch_theme(theme: &str) -> (Option<Heartbeat>, Option<String>) {
    let gui_bin = gui_bin();
    assert!(
        gui_bin.is_file(),
        "T-SPLIT-LOOK needs the packaged GUI at {}. Run: sh scripts/package-macos.sh",
        gui_bin.display()
    );

    let sock = unique_sock(theme.split_whitespace().next().unwrap_or("theme"));
    let heartbeat = PathBuf::from(format!("{}.hb", sock.display()));
    let config = PathBuf::from(format!("{}.config", sock.display()));
    let _ = fs::remove_file(&heartbeat);
    fs::write(&config, format!("theme = {theme}\n")).expect("write look file");

    let mut gui_cmd = Command::new(&gui_bin);
    gui_cmd
        .env("RILL_SOCKET", &sock)
        .env("RILL_TEST_HEARTBEAT", &heartbeat)
        .env("RILL_CONFIG", &config)
        .env("SHELL", "/bin/sh")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0);
    if let Ok(m) = std::env::var("RILL_MUTATE") {
        gui_cmd.env("RILL_MUTATE", m);
    }
    let mut gui = gui_cmd.spawn().expect("spawn packaged Rill");
    let pid = gui.id();

    let hb = wait_heartbeat(&heartbeat, pid, Duration::from_secs(8), |h| {
        h.seq >= 1 && h.chrome == Some(3) && h.nav_bg.is_some()
    });
    let raw = fs::read_to_string(&heartbeat).ok();
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    let _ = gui.wait();
    let _ = fs::remove_file(&heartbeat);
    let _ = fs::remove_file(&sock);
    let _ = fs::remove_file(&config);
    (hb, raw)
}

fn assert_nav_is_chrome_surface(theme: &str) {
    let file_bg = background_hex_from_theme_file(theme);
    let expected = chrome_surface_hex_from_theme_file(theme);
    assert_ne!(
        expected, file_bg,
        "precondition: chrome surface must differ from {theme} file background"
    );
    let (hb, raw) = launch_theme(theme);
    let hb = hb.unwrap_or_else(|| panic!("no three-pane heartbeat for {theme}; heartbeat={raw:?}"));
    let got = hb.nav_bg.as_deref().unwrap_or("");
    assert_eq!(
        got, expected,
        "{theme}: nav CALayer must be file background #{file_bg} saturating-minus 9 = #{expected}, got #{got}; heartbeat={raw:?}"
    );
    assert_ne!(
        got, file_bg,
        "{theme}: chrome must not match Chip 0 file background #{file_bg}"
    );
    assert_ne!(
        got, "121212",
        "{theme}: chrome must not be Chip 0 default dark"
    );
}

/// Latte chrome must be the derived surface, not file cream or hardcoded gray.
#[test]
fn t_chrome_nav_background_matches_latte_theme_file() {
    assert_nav_is_chrome_surface("Catppuccin Latte");
}

/// Mocha must paint a different file-backed surface so Latte cream cannot fake it.
#[test]
fn t_chrome_nav_background_matches_mocha_theme_file() {
    let latte = chrome_surface_hex_from_theme_file("Catppuccin Latte");
    let mocha = chrome_surface_hex_from_theme_file("Catppuccin Mocha");
    assert_ne!(
        latte, mocha,
        "precondition: the two derived surfaces differ"
    );
    assert_nav_is_chrome_surface("Catppuccin Mocha");
}

/// Workspaces must share Chip 0's look padding-y, not a 680pt template gap.
#[test]
fn t_chrome_heading_top_inset_matches_look_padding_y() {
    let (hb, raw) = launch_theme("Catppuccin Latte");
    let hb = hb.unwrap_or_else(|| panic!("no three-pane heartbeat for inset; heartbeat={raw:?}"));
    let nav_top = hb.nav_top.unwrap_or(-1.0);
    let pad_y = hb.pad_y.unwrap_or(-1.0);
    assert!(
        pad_y >= 0.0,
        "pad_y must come from Chip 0 look padding, got {pad_y}; heartbeat={raw:?}"
    );
    assert!(
        (nav_top - pad_y).abs() <= 1.0,
        "nav_top={nav_top} must match look pad_y={pad_y} (680pt template leaves ~20); heartbeat={raw:?}"
    );
}

/// Section labels must be macOS control size, not 11pt caption size.
#[test]
fn t_chrome_section_font_is_sidebar_size() {
    let (hb, raw) = launch_theme("Catppuccin Latte");
    let hb = hb.unwrap_or_else(|| panic!("no three-pane heartbeat for font; heartbeat={raw:?}"));
    let font = hb.chrome_font.unwrap_or(0.0);
    assert!(
        font >= 13.0,
        "chrome_font={font} must be NSFont.systemFontSize (≥13), not 11pt caption; heartbeat={raw:?}"
    );
}
