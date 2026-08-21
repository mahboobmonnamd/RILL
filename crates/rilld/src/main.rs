fn main() {
    // Leave the GUI process group. SIGHUP/SIGKILL of Rill.app must not
    // take the kernel or the user shell (ADR 0001 D7).
    //
    // T-KILL's required mutation drops POSIX_SPAWN_SETSID in the GUI *and*
    // this setsid. Either one alone keeps persist_e2e green, so the
    // instrument would be blind (ADR 0002 D3).
    let drop_session = {
        #[cfg(feature = "mutate")]
        {
            std::env::var("RILL_MUTATE").as_deref() == Ok("drop_POSIX_SPAWN_SETSID")
        }
        #[cfg(not(feature = "mutate"))]
        {
            false
        }
    };
    unsafe {
        libc::signal(libc::SIGHUP, libc::SIG_IGN);
        if !drop_session && libc::setsid() < 0 {
            let err = std::io::Error::last_os_error();
            if err.raw_os_error() != Some(libc::EPERM) {
                eprintln!("rilld: setsid: {err}");
                std::process::exit(1);
            }
        }
    }
    let socket = rilld::default_socket();
    if rilld::nested_launch_blocked() {
        eprintln!("rilld: nested launch refused (set RILL_ALLOW_NESTED=1)");
        std::process::exit(1);
    }
    let worker = std::env::var("RILL_WORKER").as_deref() == Ok("1");
    if worker {
        let shell = rilld::default_shell();
        let size = rill_kernel::Winsize::default();
        let daemon = match rilld::Daemon::bind(&socket, &shell, &[], size) {
            Ok(d) => d,
            Err(e) if matches!(e, rilld::Error::AlreadyRunning) => {
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
        return;
    }
    if let Err(e) = rilld::run_control(&socket) {
        eprintln!("rilld: {e}");
        std::process::exit(1);
    }
}
