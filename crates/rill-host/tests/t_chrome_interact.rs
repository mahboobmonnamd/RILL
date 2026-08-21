//! Packaged chrome interaction: scroll, hide chrome, workspace id, empty agents.
//!
//! Required mutations: `ignore_wheel`, `hide_sidebar_detaches`,
//! `chrome_invents_workspace_row`, `always_forward_chord`, `fabricate_agent_row`.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
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

fn unique_sock() -> PathBuf {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("../../target/rill-ui-{n:x}"));
    let _ = fs::create_dir_all(&dir);
    let _ = fs::set_permissions(&dir, fs::Permissions::from_mode(0o700));
    dir.join("a")
}

#[derive(Debug)]
struct Heartbeat {
    seq: u32,
    visible: Option<u32>,
    left: Option<f64>,
    right: Option<f64>,
    first: Option<String>,
    host: Option<String>,
    paint0: Option<u32>,
    live0: Option<u32>,
    hist: Option<u32>,
    scroll: Option<u32>,
    mark: Option<u32>,
    ws_id: Option<u64>,
    ws_label: Option<String>,
    agents: Option<u32>,
    hidden: Option<u32>,
    insp: Option<u32>,
    sent: Option<u64>,
    quit: Option<u32>,
    close: Option<u32>,
    tabs: Option<u32>,
    kern_tabs: Option<u32>,
}

fn read_heartbeat(path: &PathBuf) -> Option<Heartbeat> {
    let s = fs::read_to_string(path).ok()?;
    let mut seq = None;
    let mut visible = None;
    let mut left = None;
    let mut right = None;
    let mut first = None;
    let mut host = None;
    let mut paint0 = None;
    let mut live0 = None;
    let mut hist = None;
    let mut scroll = None;
    let mut mark = None;
    let mut ws_id = None;
    let mut ws_label = None;
    let mut agents = None;
    let mut hidden = None;
    let mut insp = None;
    let mut sent = None;
    let mut quit = None;
    let mut close = None;
    let mut tabs = None;
    let mut kern_tabs = None;
    for part in s.split_whitespace() {
        if let Some(v) = part.strip_prefix("seq=") {
            seq = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("visible=") {
            visible = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("left=") {
            left = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("right=") {
            right = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("first=") {
            first = Some(v.to_string());
        }
        if let Some(v) = part.strip_prefix("host=") {
            host = Some(v.to_string());
        }
        if let Some(v) = part.strip_prefix("paint0=") {
            paint0 = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("live0=") {
            live0 = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("hist=") {
            hist = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("scroll=") {
            scroll = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("mark=") {
            mark = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("ws_id=") {
            ws_id = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("ws_label=") {
            ws_label = Some(v.to_string());
        }
        if let Some(v) = part.strip_prefix("agents=") {
            agents = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("hidden=") {
            hidden = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("insp=") {
            insp = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("sent=") {
            sent = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("quit=") {
            quit = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("close=") {
            close = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("tabs=") {
            tabs = v.parse().ok();
        }
        if let Some(v) = part.strip_prefix("kern_tabs=") {
            kern_tabs = v.parse().ok();
        }
    }
    Some(Heartbeat {
        seq: seq?,
        visible,
        left,
        right,
        first,
        host,
        paint0,
        live0,
        hist,
        scroll,
        mark,
        ws_id,
        ws_label,
        agents,
        hidden,
        insp,
        sent,
        quit,
        close,
        tabs,
        kern_tabs,
    })
}

fn alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

fn wait_heartbeat(
    path: &PathBuf,
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

fn spawn_gui(sock: &PathBuf, heartbeat: &PathBuf, extra: &[(&str, &str)]) -> std::process::Child {
    let gui_bin = gui_bin();
    assert!(
        gui_bin.is_file(),
        "packaged GUI missing at {}. Run: sh scripts/package-macos.sh",
        gui_bin.display()
    );
    let mut gui_cmd = Command::new(&gui_bin);
    gui_cmd
        .env("RILL_SOCKET", sock)
        .env("RILL_TEST_HEARTBEAT", heartbeat)
        .env("RILL_DEV_DIRECT_RILLD", "1")
        .env("SHELL", "/bin/sh")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(fs::File::create(format!("{}.err", heartbeat.display())).expect("stderr log"))
        .process_group(0);
    if let Ok(m) = std::env::var("RILL_MUTATE") {
        gui_cmd.env("RILL_MUTATE", m);
    }
    for (k, v) in extra {
        gui_cmd.env(k, v);
    }
    gui_cmd.spawn().expect("spawn packaged Rill")
}

/// Bug: long output left the Chip 1 grid with no wheel/offset paint.
/// Required mutation: `RILL_MUTATE=ignore_wheel`.
#[test]
fn t_scroll_wheel_reveals_lines_that_left_the_grid() {
    let sock = unique_sock();
    let heartbeat = PathBuf::from(format!("{}.hb", sock.display()));
    let _ = fs::remove_file(&heartbeat);
    let mut gui = spawn_gui(&sock, &heartbeat, &[("RILL_TEST_SCROLL", "40")]);
    let pid = gui.id();
    let hb = wait_heartbeat(&heartbeat, pid, Duration::from_secs(12), |h| {
        h.hist.unwrap_or(0) > 0 && h.scroll.unwrap_or(0) > 0 && h.mark == Some(1)
    });
    let raw = fs::read_to_string(&heartbeat).ok();
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    let _ = gui.wait();
    let hb = hb.unwrap_or_else(|| {
        let err = fs::read_to_string(format!("{}.err", heartbeat.display())).ok();
        panic!("no scrolled heartbeat; heartbeat={raw:?} stderr={err:?}")
    });
    assert_ne!(
        hb.paint0, hb.live0,
        "painted row 0 must differ from live grid row 0; heartbeat={raw:?}"
    );
    assert_eq!(
        hb.mark,
        Some(1),
        "RILLMARK must be in the scrolled viewport; heartbeat={raw:?}"
    );
}

/// T-NAV-VIEWSTATE. Hide chrome must not kill the GUI (mutation closes it).
/// Required mutation: `RILL_MUTATE=hide_sidebar_detaches`.
#[test]
fn t_nav_viewstate_hide_chrome_keeps_the_window() {
    let sock = unique_sock();
    let heartbeat = PathBuf::from(format!("{}.hb", sock.display()));
    let _ = fs::remove_file(&heartbeat);
    let mut gui = spawn_gui(&sock, &heartbeat, &[("RILL_TEST_HIDE_CHROME", "1")]);
    let pid = gui.id();
    let hb = wait_heartbeat(&heartbeat, pid, Duration::from_secs(8), |h| {
        h.hidden == Some(1) && h.left.unwrap_or(99.0) < 1.0
    });
    let still = alive(pid);
    let raw = fs::read_to_string(&heartbeat).ok();
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    let _ = gui.wait();
    assert!(still, "GUI must stay alive while chrome is hidden; heartbeat={raw:?}");
    let hb = hb.unwrap_or_else(|| panic!("sidebars never hid; heartbeat={raw:?}"));
    assert_eq!(hb.first.as_deref(), Some("terminal"));
    assert_ne!(
        hb.insp,
        Some(1),
        "hiding Workspaces must not empty the inspector; heartbeat={raw:?}"
    );
    assert!(
        hb.right.unwrap_or(0.0) > 40.0,
        "inspector must keep width when nav is hidden; heartbeat={raw:?}"
    );
    assert_eq!(
        hb.host.as_deref(),
        Some("local"),
        "inspector rows must remain; heartbeat={raw:?}"
    );
}

/// Host chord must not write to the PTY (`sent` stays at the pre-chord value
/// except the mutation `always_forward_chord`).
#[test]
fn t_host_chord_does_not_write_the_pty() {
    let sock = unique_sock();
    let heartbeat = PathBuf::from(format!("{}.hb", sock.display()));
    let _ = fs::remove_file(&heartbeat);
    let mut gui = spawn_gui(&sock, &heartbeat, &[("RILL_TEST_INJECT_HOST_CHORD", "1")]);
    let pid = gui.id();
    let hb = wait_heartbeat(&heartbeat, pid, Duration::from_secs(6), |h| {
        h.hidden == Some(1) || h.sent.unwrap_or(0) > 0
    });
    let raw = fs::read_to_string(&heartbeat).ok();
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    let _ = gui.wait();
    let hb = hb.unwrap_or_else(|| panic!("chord produced no heartbeat; heartbeat={raw:?}"));
    if std::env::var("RILL_MUTATE").as_deref() == Ok("always_forward_chord") {
        assert!(
            hb.sent.unwrap_or(0) > 0,
            "mutation must forward the chord; heartbeat={raw:?}"
        );
        assert_ne!(hb.hidden, Some(1), "mutation must not hide chrome; heartbeat={raw:?}");
        return;
    }
    assert_eq!(hb.hidden, Some(1), "chord must hide chrome; heartbeat={raw:?}");
    assert_ne!(
        hb.insp,
        Some(1),
        "⌥⌘1 must not hide the inspector; heartbeat={raw:?}"
    );
    assert_eq!(
        hb.sent.unwrap_or(0),
        0,
        "host chord must not send PTY bytes; heartbeat={raw:?}"
    );
}

/// Bug: both chords hid Workspaces and left an empty inspector strip.
#[test]
fn t_control_cmd_1_toggles_inspector_not_workspaces() {
    let sock = unique_sock();
    let heartbeat = PathBuf::from(format!("{}.hb", sock.display()));
    let _ = fs::remove_file(&heartbeat);
    let mut gui = spawn_gui(&sock, &heartbeat, &[("RILL_TEST_INJECT_INSPECTOR_CHORD", "1")]);
    let pid = gui.id();
    let hb = wait_heartbeat(&heartbeat, pid, Duration::from_secs(6), |h| {
        h.insp == Some(1) || h.sent.unwrap_or(0) > 0
    });
    let raw = fs::read_to_string(&heartbeat).ok();
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    let _ = gui.wait();
    let hb = hb.unwrap_or_else(|| panic!("inspector chord produced no heartbeat; heartbeat={raw:?}"));
    assert_eq!(hb.insp, Some(1), "⌃⌘1 must hide the inspector; heartbeat={raw:?}");
    assert_ne!(
        hb.hidden,
        Some(1),
        "⌃⌘1 must not hide Workspaces; heartbeat={raw:?}"
    );
    assert!(
        hb.left.unwrap_or(0.0) > 40.0,
        "nav must keep width; heartbeat={raw:?}"
    );
}

/// Left pane must show the kernel Workspace id, not a host-invented home name.
/// Required mutation: `RILL_MUTATE=chrome_invents_workspace_row`.
#[test]
fn t_nav_workspace_projection_uses_kernel_id() {
    let sock = unique_sock();
    let heartbeat = PathBuf::from(format!("{}.hb", sock.display()));
    let _ = fs::remove_file(&heartbeat);
    let mut gui = spawn_gui(&sock, &heartbeat, &[]);
    let pid = gui.id();
    let hb = wait_heartbeat(&heartbeat, pid, Duration::from_secs(8), |h| h.seq >= 1);
    let raw = fs::read_to_string(&heartbeat).ok();
    let err = fs::read_to_string(format!("{}.err", heartbeat.display())).ok();
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    let _ = gui.wait();
    let hb = hb.unwrap_or_else(|| {
        panic!("no heartbeat; sock={} heartbeat={raw:?} stderr={err:?}", sock.display())
    });
    let id = hb.ws_id.unwrap_or(0);
    assert!(id > 0, "kernel workspace id missing; heartbeat={raw:?}");
    let label = hb.ws_label.clone().unwrap_or_default();
    assert_eq!(
        label,
        id.to_string(),
        "chrome label must be the kernel NodeId; heartbeat={raw:?}"
    );
}

/// Agent inventory stays empty until Task exists (SPEC-NAV §7).
/// Required mutation: `RILL_MUTATE=fabricate_agent_row`.
#[test]
fn t_agent_inventory_is_empty_until_task_exists() {
    let sock = unique_sock();
    let heartbeat = PathBuf::from(format!("{}.hb", sock.display()));
    let _ = fs::remove_file(&heartbeat);
    let mut gui = spawn_gui(&sock, &heartbeat, &[]);
    let pid = gui.id();
    let hb = wait_heartbeat(&heartbeat, pid, Duration::from_secs(8), |h| h.seq >= 1);
    let raw = fs::read_to_string(&heartbeat).ok();
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    let _ = gui.wait();
    let hb = hb.unwrap_or_else(|| panic!("no heartbeat; heartbeat={raw:?}"));
    assert_eq!(hb.agents, Some(0), "fabricated agent row; heartbeat={raw:?}");
}

/// Bug: ⌘A / line-start never reached the PTY.
#[test]
fn t_cmd_line_start_reaches_the_pty() {
    let sock = unique_sock();
    let heartbeat = PathBuf::from(format!("{}.hb", sock.display()));
    let _ = fs::remove_file(&heartbeat);
    let mut gui = spawn_gui(&sock, &heartbeat, &[("RILL_TEST_INJECT_EDIT", "1")]);
    let pid = gui.id();
    let hb = wait_heartbeat(&heartbeat, pid, Duration::from_secs(6), |h| {
        h.sent.unwrap_or(0) > 0
    });
    let raw = fs::read_to_string(&heartbeat).ok();
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    let _ = gui.wait();
    let hb = hb.unwrap_or_else(|| panic!("edit chord sent nothing; heartbeat={raw:?}"));
    assert!(
        hb.sent.unwrap_or(0) > 0,
        "line-start must write the PTY; heartbeat={raw:?}"
    );
}

/// Bug: ⌘V did not paste into the attach stream.
#[test]
fn t_cmd_v_pastes_onto_the_pty() {
    let sock = unique_sock();
    let heartbeat = PathBuf::from(format!("{}.hb", sock.display()));
    let _ = fs::remove_file(&heartbeat);
    let mut gui = spawn_gui(&sock, &heartbeat, &[("RILL_TEST_INJECT_PASTE", "hello")]);
    let pid = gui.id();
    let hb = wait_heartbeat(&heartbeat, pid, Duration::from_secs(6), |h| {
        h.sent.unwrap_or(0) >= 5
    });
    let raw = fs::read_to_string(&heartbeat).ok();
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    let _ = gui.wait();
    let hb = hb.unwrap_or_else(|| panic!("paste sent nothing; heartbeat={raw:?}"));
    assert!(
        hb.sent.unwrap_or(0) >= 5,
        "paste body must reach the PTY; heartbeat={raw:?}"
    );
}

/// Bug: replacing mainMenu dropped Quit and Close.
#[test]
fn t_app_menu_has_quit_and_close() {
    let sock = unique_sock();
    let heartbeat = PathBuf::from(format!("{}.hb", sock.display()));
    let _ = fs::remove_file(&heartbeat);
    let mut gui = spawn_gui(&sock, &heartbeat, &[]);
    let pid = gui.id();
    let hb = wait_heartbeat(&heartbeat, pid, Duration::from_secs(8), |h| {
        h.quit == Some(1) && h.close == Some(1)
    });
    let raw = fs::read_to_string(&heartbeat).ok();
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    let _ = gui.wait();
    let hb = hb.unwrap_or_else(|| panic!("quit/close menu missing; heartbeat={raw:?}"));
    assert_eq!(hb.quit, Some(1), "⌘Q must exist; heartbeat={raw:?}");
    assert_eq!(hb.close, Some(1), "⌘W must exist; heartbeat={raw:?}");
}

/// T-NAV close: ⌘W hides the window and must not kill the GUI (PTY stays with rilld).
#[test]
fn t_cmd_w_hides_window_keeps_the_gui() {
    let sock = unique_sock();
    let heartbeat = PathBuf::from(format!("{}.hb", sock.display()));
    let _ = fs::remove_file(&heartbeat);
    let mut gui = spawn_gui(&sock, &heartbeat, &[("RILL_TEST_CLOSE_WINDOW", "1")]);
    let pid = gui.id();
    let hb = wait_heartbeat(&heartbeat, pid, Duration::from_secs(8), |h| {
        h.visible == Some(0)
    });
    let still = alive(pid);
    let raw = fs::read_to_string(&heartbeat).ok();
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    let _ = gui.wait();
    assert!(still, "⌘W must not terminate the GUI; heartbeat={raw:?}");
    let hb = hb.unwrap_or_else(|| panic!("window never hid; heartbeat={raw:?}"));
    assert_eq!(hb.visible, Some(0), "close must hide the window; heartbeat={raw:?}");
}

/// Bug: ⌘W closed the window while a second tab was open (SPEC-NAV §3).
#[test]
fn t_cmd_w_closes_the_tab_not_the_window() {
    let sock = unique_sock();
    let heartbeat = PathBuf::from(format!("{}.hb", sock.display()));
    let _ = fs::remove_file(&heartbeat);
    let mut gui = spawn_gui(
        &sock,
        &heartbeat,
        &[("RILL_TEST_NEW_TAB", "1"), ("RILL_TEST_CLOSE_TAB", "1")],
    );
    let pid = gui.id();
    let hb = wait_heartbeat(&heartbeat, pid, Duration::from_secs(10), |h| {
        h.visible == Some(1) && h.tabs == Some(1) && h.kern_tabs == Some(2)
    });
    let raw = fs::read_to_string(&heartbeat).ok();
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    let _ = gui.wait();
    let hb = hb.unwrap_or_else(|| panic!("⌘W closed the window or the PTY; heartbeat={raw:?}"));
    assert_eq!(hb.visible, Some(1), "window must stay; heartbeat={raw:?}");
    assert_eq!(hb.tabs, Some(1), "one tab presentation; heartbeat={raw:?}");
    assert_eq!(
        hb.kern_tabs,
        Some(2),
        "presentation close must not terminate the leaf; heartbeat={raw:?}"
    );
}

/// Bug: File New Tab did not spawn a kernel leaf. Mutation: chrome_invents_tab.
#[test]
fn t_new_tab_creates_a_kernel_leaf() {
    let sock = unique_sock();
    let heartbeat = PathBuf::from(format!("{}.hb", sock.display()));
    let _ = fs::remove_file(&heartbeat);
    let mut gui = spawn_gui(&sock, &heartbeat, &[("RILL_TEST_NEW_TAB", "1")]);
    let pid = gui.id();
    let hb = wait_heartbeat(&heartbeat, pid, Duration::from_secs(8), |h| {
        h.kern_tabs == Some(2)
    });
    let raw = fs::read_to_string(&heartbeat).ok();
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    let _ = gui.wait();
    let hb = hb.unwrap_or_else(|| panic!("kernel still one tab; heartbeat={raw:?}"));
    assert_eq!(
        hb.kern_tabs,
        Some(2),
        "NEW_TAB must spawn_leaf; heartbeat={raw:?}"
    );
    assert!(
        hb.tabs.unwrap_or(0) >= 2,
        "host must project two tab controls; heartbeat={raw:?}"
    );
}
