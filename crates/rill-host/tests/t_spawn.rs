use std::path::PathBuf;
use std::process::Command;

fn gui_bin() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("RILL_GUI_BIN") {
        return Some(PathBuf::from(p));
    }
    let candidates = [
        PathBuf::from("dist/Rill.app/Contents/MacOS/Rill"),
        PathBuf::from("../../dist/Rill.app/Contents/MacOS/Rill"),
        PathBuf::from("../dist/Rill.app/Contents/MacOS/Rill"),
    ];
    candidates.into_iter().find(|p| p.exists())
}

#[test]
fn t_spawn_gui_binary_has_no_user_shell_pty_symbols() {
    let Some(bin) = gui_bin() else {
        if std::env::var("RILL_REQUIRE_PACKAGE").is_ok() {
            panic!("packaged GUI required for T-SPAWN");
        }
        eprintln!("T-SPAWN skipped: dist/Rill.app missing — run scripts/package-macos.sh");
        return;
    };
    let out = Command::new("nm")
        .args(["-U", bin.to_str().expect("utf8")])
        .output()
        .expect("nm");
    let text = String::from_utf8_lossy(&out.stdout);
    for sym in ["_forkpty", "_openpty", "_posix_openpt", "_grantpt", "_unlockpt"] {
        assert!(
            !text.contains(sym),
            "T-SPAWN: {sym} present in GUI binary {}",
            bin.display()
        );
    }
}
