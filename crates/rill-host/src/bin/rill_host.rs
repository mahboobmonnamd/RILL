use rill_host::{default_socket, load_surface, nfr_key, Client};
use std::os::unix::net::UnixStream;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--nfr-key") {
        let socket = default_socket();
        if UnixStream::connect(&socket).is_err() {
            eprintln!("rill-host: rilld not listening at {}", socket.display());
            std::process::exit(2);
        }
        let surface = load_surface().unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1);
        });
        let mut client = Client::connect(&socket, surface).unwrap_or_else(|e| {
            eprintln!("{e}");
            std::process::exit(1);
        });
        for _ in 0..30 {
            let _ = client.pump();
        }
        match nfr_key(&mut client, 1000) {
            Ok(r) => {
                println!(
                    "T-NFR p95={:.3}ms count={} control_rpc={} battery={}",
                    r.p95_ms, r.count, r.control_rpc, r.on_battery
                );
                if r.control_rpc || r.p95_ms >= 16.7 {
                    std::process::exit(1);
                }
            }
            Err(e) => {
                eprintln!("T-NFR failed: {e}");
                std::process::exit(1);
            }
        }
        return;
    }
    eprintln!("rill-host: open the packaged Rill.app NSWindow, or pass --nfr-key");
}
