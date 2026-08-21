//! T-RUNTIME-HOST-SHUTDOWN-JOURNAL (#320).
//!
//! Required mutation: `RILL_MUTATE=missing_worker_reported_running`.

use rill_kernel::{reconcile_execution, TerminalOutcome};

#[test]
fn t_runtime_host_shutdown_journal_is_honest() {
    let recorded = Some(4242u32);
    let got = reconcile_execution(recorded, false, true);
    assert_eq!(
        got,
        TerminalOutcome::HostTerminated,
        "missing worker after shutdown marker must not claim pid {recorded:?}: {got:?}"
    );
    assert!(
        !matches!(got, TerminalOutcome::Running { .. }),
        "journal claimed a live pid"
    );
}
