fn main() {
    // Leave the GUI process group. SIGHUP/SIGKILL of Rill.app must not
    // take the kernel or the user shell (ADR 0001 D7).
    //
    // T-KILL's required mutation drops POSIX_SPAWN_SETSID in the GUI *and*
    // this setsid. Either one alone keeps persist_e2e green, so the
    // instrument would be blind (ADR 0002 D3).
    let drop_session = std::env::var("RILL_MUTATE").as_deref() == Ok("drop_POSIX_SPAWN_SETSID");
    unsafe {
        libc::signal(libc::SIGHUP, libc::SIG_IGN);
        if !drop_session {
            if libc::setsid() < 0 {
                let err = std::io::Error::last_os_error();
                if err.raw_os_error() != Some(libc::EPERM) {
                    eprintln!("rilld: setsid: {err}");
                    std::process::exit(1);
                }
            }
        }
    }
    let socket = rilld::default_socket();
    let shell = rilld::default_shell();
    let size = rill_kernel::Winsize::default();
    let daemon = match rilld::Daemon::bind(&socket, &shell, &[], size) {
        Ok(d) => d,
        Err(e) if e.to_string().contains("already running") => {
            eprintln!("rilld: {e}");
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("rilld: {e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = daemon.run() {
        eprintln!("rilld: {e}");
        std::process::exit(1);
    }
}
