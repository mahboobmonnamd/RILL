//! Honest host-lifecycle outcomes (SPEC-RUNTIME-SUPERVISION §5, #320).

/// What the journal may claim after discovery. A live pid is only legal when
/// the worker is actually present.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalOutcome {
    Running { child_pid: u32 },
    HostTerminated,
    Missing,
    Exited { status: i32 },
}

/// Three-way reconcile: journal record, live worker, host-shutdown marker.
pub fn reconcile_execution(
    recorded_pid: Option<u32>,
    worker_live: bool,
    shutdown_marker: bool,
) -> TerminalOutcome {
    #[cfg(feature = "mutate")]
    if std::env::var("RILL_MUTATE").as_deref() == Ok("missing_worker_reported_running") {
        if let Some(child_pid) = recorded_pid {
            return TerminalOutcome::Running { child_pid };
        }
    }
    if shutdown_marker && !worker_live {
        return TerminalOutcome::HostTerminated;
    }
    if worker_live {
        if let Some(child_pid) = recorded_pid {
            return TerminalOutcome::Running { child_pid };
        }
        return TerminalOutcome::Missing;
    }
    if recorded_pid.is_some() {
        return TerminalOutcome::Missing;
    }
    TerminalOutcome::HostTerminated
}
