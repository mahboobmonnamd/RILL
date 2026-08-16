//! Diagnostic entry point for the attach client.
//!
//! T-NFR is deliberately **not** here. Its start and end timestamps are an
//! `NSEvent` and a `CAMetalDrawable` presentation (ADR 0003 D5), neither of
//! which exists outside the app. The previous `--nfr-key` in this binary
//! measured to a POD snapshot inside the Rust client and reported 0.032ms —
//! three orders of magnitude below a PTY round trip, because it was measuring
//! nothing (docs/SPIKE-0-AUDIT.md S1-2).
//!
//! Run the gate with: `dist/Rill.app/Contents/MacOS/Rill --nfr-key=hid`

use rill_host::{default_socket, load_surface, Client};
use std::os::unix::net::UnixStream;

fn main() {
    let args: Vec<String> = std::env::args().collect();

    if args.iter().any(|a| a.starts_with("--nfr-key")) {
        eprintln!(
            "rill-host: T-NFR cannot run headless. It measures key-down to drawable \
             presentation, which requires the window.\n  Run: \
             dist/Rill.app/Contents/MacOS/Rill --nfr-key=hid"
        );
        std::process::exit(2);
    }

    if args.iter().any(|a| a == "--probe") {
        let socket = default_socket();
        if UnixStream::connect(&socket).is_err() {
            eprintln!("rill-host: rilld not listening at {}", socket.display());
            std::process::exit(2);
        }
        let surface = match load_surface() {
            Ok(s) => s,
            Err(e) => {
                eprintln!("rill-host: {e}");
                std::process::exit(1);
            }
        };
        let mut client = match Client::connect(&socket, surface) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("rill-host: {e}");
                std::process::exit(1);
            }
        };
        for _ in 0..30 {
            if let Err(e) = client.pump() {
                eprintln!("rill-host: pump: {e}");
                std::process::exit(1);
            }
        }
        match client.snapshot() {
            Ok(g) => println!(
                "probe ok: {}x{} cursor=({},{}) alive={}",
                g.cols,
                g.rows,
                g.cursor_col,
                g.cursor_row,
                client.alive()
            ),
            Err(e) => {
                eprintln!("rill-host: snapshot: {e}");
                std::process::exit(1);
            }
        }
        return;
    }

    eprintln!("rill-host: open the packaged Rill.app, or pass --probe");
}
