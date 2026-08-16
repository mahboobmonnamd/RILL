//! T-SPAWN — the GUI never creates the user shell's PTY.
//!
//! Spec: PRD FR-SPAWN / NFR-SPAWN, SPEC-DISPLAY §1, docs/TEST-CASES.md.
//!
//! The previous version ran `nm -U`, which restricts the listing to **defined**
//! symbols. `_forkpty` and friends live in libSystem and can only ever appear
//! in `Rill` as **undefined** imports — so the command excluded precisely the
//! symbol class the assertion inspected, and the gate passed on a binary that
//! called `forkpty` on every keystroke (docs/SPIKE-0-AUDIT.md S1-1).
//!
//! Two changes make it real:
//!   1. Inspect imports (`nm -u`) and the dynamic bind tables (`otool -Iv`).
//!   2. Run the identical check against a fixture that *does* create a PTY.
//!      If the check comes back clean on that, the check is broken and this
//!      gate fails — whatever it said about `Rill.app`.
//!
//! `main.m` legitimately calls `posix_spawn` to launch `rilld`, so `posix_spawn`
//! is deliberately *not* in the forbidden set. PTY **creation** primitives are
//! what distinguish "launched the daemon" from "spawned the user's shell".

use std::path::{Path, PathBuf};
use std::process::Command;

/// PTY-creation primitives. A binary that imports none of these cannot have
/// made the terminal the user's shell is attached to.
const FORBIDDEN: &[&str] = &[
    "_forkpty",
    "_openpty",
    "_posix_openpt",
    "_grantpt",
    "_unlockpt",
    "_ptsname",
    "_login_tty",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn gui_binary() -> PathBuf {
    if let Ok(p) = std::env::var("RILL_GUI_BIN") {
        return PathBuf::from(p);
    }
    repo_root().join("dist/Rill.app/Contents/MacOS/Rill")
}

/// Every PTY-creation symbol this binary imports.
///
/// Fails rather than returning empty when a tool is missing: a missing
/// precondition is a failure, not a skip (ADR 0002 D5).
fn pty_imports(bin: &Path) -> Vec<String> {
    assert!(
        bin.exists(),
        "T-SPAWN needs a packaged binary at {}. Run: sh scripts/package-macos.sh",
        bin.display()
    );

    let nm = Command::new("nm")
        .arg("-u") // undefined symbols == imports. NOT -U.
        .arg(bin)
        .output()
        .expect("nm must be available (Xcode Command Line Tools)");
    assert!(
        nm.status.success(),
        "nm -u failed on {}: {}",
        bin.display(),
        String::from_utf8_lossy(&nm.stderr)
    );
    let nm_text = String::from_utf8_lossy(&nm.stdout).into_owned();

    // Second, independent view: the dynamic bind tables. A symbol reached
    // through a lazy stub still shows up here.
    let otool = Command::new("otool")
        .args(["-Iv"])
        .arg(bin)
        .output()
        .expect("otool must be available (Xcode Command Line Tools)");
    let otool_text = String::from_utf8_lossy(&otool.stdout).into_owned();

    let haystack = format!("{nm_text}\n{otool_text}");
    FORBIDDEN
        .iter()
        .filter(|sym| {
            haystack
                .lines()
                .any(|l| l.split_whitespace().any(|tok| tok == **sym))
        })
        .map(|s| s.to_string())
        .collect()
}

/// The check must be able to say yes. Built fresh so it cannot drift from the
/// binary the real assertion inspects.
#[test]
fn t_spawn_the_check_itself_detects_a_binary_that_creates_a_pty() {
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/spawner.c");
    let out = std::env::temp_dir().join(format!(
        "rill-spawner-{}",
        std::process::id()
    ));

    let cc = Command::new("cc")
        .arg(&src)
        .arg("-o")
        .arg(&out)
        .output()
        .expect("cc must be available");
    assert!(
        cc.status.success(),
        "positive control failed to build: {}",
        String::from_utf8_lossy(&cc.stderr)
    );

    let found = pty_imports(&out);
    let _ = std::fs::remove_file(&out);

    assert!(
        found.contains(&"_forkpty".to_string()),
        "POSITIVE CONTROL FAILED: the T-SPAWN check reported no PTY-creation \
         imports for a binary that calls forkpty. The check is blind; its \
         verdict on Rill.app means nothing. found={found:?}"
    );
}

#[test]
fn t_spawn_gui_binary_does_not_import_pty_creation_symbols() {
    let bin = gui_binary();
    let found = pty_imports(&bin);
    assert!(
        found.is_empty(),
        "T-SPAWN: {} imports PTY-creation primitives {found:?}. \
         The GUI must not create the user shell's terminal (PRD FR-SPAWN).",
        bin.display()
    );
}

/// `posix_spawn` is expected — it launches `rilld`. Asserting its presence
/// keeps the forbidden list honest: if a future refactor removed the daemon
/// launch, the gate above would start passing for the wrong reason.
#[test]
fn t_spawn_gui_binary_still_launches_the_daemon() {
    let bin = gui_binary();
    assert!(bin.exists(), "run: sh scripts/package-macos.sh");
    let nm = Command::new("nm").arg("-u").arg(&bin).output().expect("nm");
    let text = String::from_utf8_lossy(&nm.stdout);
    assert!(
        text.lines()
            .any(|l| l.split_whitespace().any(|t| t == "_posix_spawn")),
        "GUI no longer imports posix_spawn — it is not launching rilld, so \
         T-SPAWN's clean result would be vacuous"
    );
}
