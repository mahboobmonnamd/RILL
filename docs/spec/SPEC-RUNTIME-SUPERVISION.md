# SPEC-RUNTIME-SUPERVISION — service, workers, restart and update

- **Status:** Red. Specification only; no implementation is authorized.
- **Authority:** [ADR 0053](../adr/0053-runtime-domain-content-and-client-authority.md)
  D3–D4.
- **Lane:** `lane:kernel`.

## 1. Placement and service ownership

The RILL runtime runs on the machine where its PTYs and child processes run.
On macOS the packaged app registers a per-user LaunchAgent through the supported
Service Management API and exposes its enabled/disabled/running state. Direct
GUI `setsid` launch may remain only as an explicitly bounded development path;
it is not the production lifecycle contract.

The service uses a user-owned runtime directory, not a predictable socket
directly under shared `/tmp`. Local peer credentials are verified before the
first protocol frame. Disabling the service is visible and fails closed; the
GUI does not silently substitute a different execution path.

## 2. Control daemon and workers

The control daemon owns domain coordination, authentication, routing, lease
decisions and durable journal transactions. A separate worker owns each
TerminalExecution's PTY master, process group, canonical terminal core,
monotonic offset, bounded delta recovery and checkpoints. The terminal core is
inside the worker failure boundary so daemon restart cannot erase the state a
reconnecting client must reconcile against.

Worker identity binds:

- RuntimeId and TerminalExecutionId;
- user identity;
- protocol and checkpoint format versions;
- unforgeable per-worker discovery credential;
- child PID/process-group metadata; and
- journal generation.

The control daemon dying closes client/control channels but not the worker's PTY
master or canonical terminal state. A worker continues draining PTY output,
advancing its terminal core and maintaining bounded recovery state. It applies
backpressure without unbounded memory and accepts no unauthenticated input while
orphaned.

## 3. Discovery and reconciliation

On restart, the daemon reads the journal, enumerates worker endpoints in the
protected runtime directory, authenticates them and performs a three-way
reconciliation: journal record, live worker identity and child/process state.

Results are `Recovered`, `Exited`, `Incompatible`, `Unverified` or `Missing`.
Only `Recovered` accepts new clients. Incompatible or unverified workers remain
visible and isolated; they are never attached to a different execution or
killed as a side effect of discovery.

Malformed client or worker input terminates that connection only. Parser,
authorization and resource errors MUST NOT unwind the daemon event loop or
affect unrelated TerminalExecutions.

## 4. Update compatibility

Runtime, worker, protocol and checkpoint formats carry explicit versions. An
update may proceed with live workers only when the new daemon supports their
version under a documented N/N-1 compatibility matrix. Otherwise update is
refused or deferred with the blocking executions named.

Updating the GUI or control daemon MUST NOT signal healthy workers. Worker
binary replacement waits until its execution exits unless a later independently
proven handoff protocol exists.

## 5. Host lifecycle

Sleep suspends local execution and resumes it without lifecycle change. User
logout, shutdown, reboot, power loss and operating-system termination may end
workers and children. On the next start, the daemon records the best supported
terminal outcome without claiming a process survived. Policy-permitted durable
graph, transcript and task state may restore; live process identity may not.

## 6. Resource and security bounds

- Every client and worker channel has independent frame, queue, credit and time
  bounds.
- A stalled observer cannot block a controller or PTY drain.
- The daemon never accepts a caller-selected filesystem endpoint outside its
  protected runtime root.
- Secrets and transcript data are not written to service logs.
- Runtime stop is a distinct destructive action. It refuses while workers are
  live unless the owner completes the force-termination workflow.

## 7. Gates

- T-RUNTIME-GUI-INDEPENDENT
- T-RUNTIME-DAEMON-RESTART
- T-RUNTIME-DAEMON-STATE
- T-RUNTIME-UPDATE-COMPAT
- T-RUNTIME-MALFORMED-CLIENT-ISOLATION
- T-RUNTIME-PROTECTED-ENDPOINT
- T-RUNTIME-HOST-SHUTDOWN-JOURNAL

Socket-only tests do not close worker survival. The restart and update gates
must observe the original child PID or an equally downstream process oracle.

## 8. Out of scope

This spec does not promise process survival across host shutdown, select Linux
or Windows service APIs, or authorize root daemons, containers or hosted RILL
infrastructure.

[#313](https://github.com/mahboobmonnamd/RILL/issues/313) implements the
offset/checkpoint *record* on the existing `Session` worker object. Killing a
separate control-daemon process while the worker continues is not claimed
Proven until T-RUNTIME-DAEMON-RESTART has a process-split oracle.
