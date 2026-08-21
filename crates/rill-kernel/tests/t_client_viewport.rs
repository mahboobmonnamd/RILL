//! T-CLIENT-VIEWPORT-AUTHORITY (#323).
//!
//! Required mutation: `RILL_MUTATE=largest_observer_wins`.

use rill_attach::Frame;
use rill_kernel::{Session, Winsize};
use std::path::PathBuf;
use std::time::{Duration, Instant};

fn tmp_path(tag: &str) -> PathBuf {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    PathBuf::from(format!("/tmp/rill-vp-{tag}-{n}"))
}

fn wait_stty(path: &PathBuf, timeout: Duration) -> String {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if let Ok(s) = std::fs::read_to_string(path) {
            let t = s.trim();
            if !t.is_empty() {
                return t.to_string();
            }
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    String::new()
}

/// Observer crop must not change the child's `TIOCGWINSZ`. Controller resize must.
#[test]
fn t_client_viewport_authority_observer_does_not_set_child_winsize() {
    let report = tmp_path("stty");
    let script = format!(
        "trap 'stty size > {r} 2>/dev/null' WINCH; while :; do sleep 0.05; done",
        r = report.display()
    );
    let mut session =
        Session::spawn("/bin/sh", &["-c", &script], Winsize::default()).expect("spawn");
    session
        .on_frame(Frame::attach(1, None))
        .expect("controller");
    std::thread::sleep(Duration::from_millis(150));

    session
        .apply_observer_viewport(200, 100, 1600, 1600)
        .expect("observer crop");
    std::thread::sleep(Duration::from_millis(200));
    let after_obs = wait_stty(&report, Duration::from_millis(400));
    assert_ne!(
        after_obs.as_str(),
        "100 200",
        "observer viewport changed child TIOCGWINSZ to {after_obs:?}"
    );

    let _ = std::fs::remove_file(&report);
    session
        .on_frame(Frame::Resize {
            cols: 91,
            rows: 31,
            px_w: 728,
            px_h: 496,
        })
        .expect("controller resize");
    let after_ctrl = wait_stty(&report, Duration::from_secs(5));
    assert_eq!(
        after_ctrl, "31 91",
        "controller resize did not reach child's TIOCGWINSZ (got {after_ctrl:?})"
    );
    let _ = session.terminate();
    let _ = std::fs::remove_file(&report);
}
