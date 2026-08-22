//! Kernel daemon: owns the session PTY, framed attach, cold-path resync.

mod endpoint;
mod error;
mod proxy;

use rill_attach::{
    cold_identity_socket_path, cold_nav_socket_path, Decoder, Frame, PROTOCOL_2, PROTOCOL_VERSION,
};
use rill_kernel::{Kernel, NodeChild, NodeKind, Session, SessionId, Winsize};
use rill_vt_types::{PodGrid, TerminalEmulation};
use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::time::Duration;
use vt_engine::VtEngine;

pub use endpoint::{authorize_peer, default_runtime_dir, ensure_protected_parent, peer_uid};
pub use error::Error;
pub use proxy::run_control;

pub struct Daemon {
    listener: UnixListener,
    socket_path: PathBuf,
    identity_listener: UnixListener,
    identity_socket_path: PathBuf,
    nav_listener: UnixListener,
    nav_socket_path: PathBuf,
    workspace_id: rill_kernel::NodeId,
    _lock: File,
    kernel: Kernel,
    default_id: SessionId,
    /// One connection per attach claim. A second connection MAY attach a
    /// different id (ADR 0011 D3). FR-ONE is per leaf, not per daemon.
    clients: Vec<Client>,
    chip: VtEngine,
    /// Cached at bind. The drain loop must not `getenv` per PTY read.
    is_worker: bool,
    shell: String,
    shell_args: Vec<String>,
    size: Winsize,
    nav_conns: Vec<UnixStream>,
}

struct Client {
    stream: UnixStream,
    decoder: Decoder,
    /// Partial writes. `write_all` on a non-blocking socket plus re-queue of
    /// the whole frame duplicated DATA (quality audit Q1).
    outbox: VecDeque<u8>,
    leaf: Option<SessionId>,
    observe: bool,
    protocol: u8,
    credit: u64,
}

impl Daemon {
    pub fn bind(
        socket_path: impl AsRef<Path>,
        shell: &str,
        args: &[&str],
        size: Winsize,
    ) -> Result<Self, Error> {
        if nested_launch_blocked() {
            return Err(Error::NestedLaunch);
        }
        let socket_path = socket_path.as_ref().to_path_buf();
        endpoint::ensure_protected_parent(&socket_path)?;

        // An exclusive lock, taken before anything is unlinked.
        //
        // The previous sequence was exists() -> connect() -> remove_file() ->
        // bind(), which two daemons racing could both pass, with the second
        // unlinking the first's live socket (audit S3-7).
        let lock_path = socket_path.with_extension("lock");
        let lock = File::options()
            .create(true)
            .read(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)?;
        // SAFETY: lock is an open, owned file for the duration of the call.
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(Error::AlreadyRunning);
        }

        // The lock is ours, so any socket at this path is stale unless someone
        // is answering on it.
        if socket_path.exists() {
            if UnixStream::connect(&socket_path).is_ok() {
                return Err(Error::AlreadyRunning);
            }
            std::fs::remove_file(&socket_path)?;
        }

        let listener = UnixListener::bind(&socket_path)?;
        listener.set_nonblocking(true)?;
        // The attach socket carries keystrokes and shell output. Do not leave
        // it world-writable (SPEC-ATTACH §1).
        std::fs::set_permissions(&socket_path, std::fs::Permissions::from_mode(0o600))?;
        let identity_socket_path = cold_identity_socket_path(&socket_path);
        if identity_socket_path.exists() {
            std::fs::remove_file(&identity_socket_path)?;
        }
        let identity_listener = UnixListener::bind(&identity_socket_path)?;
        identity_listener.set_nonblocking(true)?;
        std::fs::set_permissions(
            &identity_socket_path,
            std::fs::Permissions::from_mode(0o600),
        )?;
        let nav_socket_path = cold_nav_socket_path(&socket_path);
        if nav_socket_path.exists() {
            std::fs::remove_file(&nav_socket_path)?;
        }
        let nav_listener = UnixListener::bind(&nav_socket_path)?;
        nav_listener.set_nonblocking(true)?;
        std::fs::set_permissions(&nav_socket_path, std::fs::Permissions::from_mode(0o600))?;
        let mut kernel = Kernel::new();
        let default_id = kernel.spawn_leaf(shell, args, size)?;
        let workspace_id = kernel.create_node(NodeKind::Workspace, None)?;
        let tab = kernel.create_node(NodeKind::Tab, Some(workspace_id))?;
        kernel.attach_leaf(tab, default_id)?;
        let child_pid = kernel
            .session(default_id)
            .ok_or(rill_kernel::Error::UnknownSession)?
            .child_pid();
        if let Ok(path) = std::env::var("RILL_TEST_PIDFILE") {
            std::fs::write(path, format!("{child_pid}\n"))?;
        }
        if std::env::var("RILL_WORKER").as_deref() != Ok("1") {
            if let Ok(path) = std::env::var("RILL_TEST_DAEMON_PIDFILE") {
                std::fs::write(path, format!("{}\n", std::process::id()))?;
            }
        }
        if std::env::var("RILL_TEST_SECOND_LEAF").is_ok() {
            let second = kernel.spawn_leaf(shell, args, size)?;
            let pid = kernel
                .session(second)
                .ok_or(rill_kernel::Error::UnknownSession)?
                .child_pid();
            if let Ok(path) = std::env::var("RILL_TEST_SECOND_PIDFILE") {
                std::fs::write(path, format!("{pid}\n"))?;
            }
        }
        let chip = VtEngine::new(size.cols, size.rows)?;
        Ok(Self {
            listener,
            socket_path,
            identity_listener,
            identity_socket_path,
            nav_listener,
            nav_socket_path,
            workspace_id,
            _lock: lock,
            kernel,
            default_id,
            clients: Vec::new(),
            chip,
            is_worker: std::env::var("RILL_WORKER").as_deref() == Ok("1"),
            shell: shell.to_string(),
            shell_args: args.iter().map(|s| (*s).to_string()).collect(),
            size,
            nav_conns: Vec::new(),
        })
    }

    pub fn default_id(&self) -> SessionId {
        self.default_id
    }

    pub fn spawn_leaf(
        &mut self,
        shell: &str,
        args: &[&str],
        size: Winsize,
    ) -> Result<SessionId, Error> {
        Ok(self.kernel.spawn_leaf(shell, args, size)?)
    }

    /// Cold destroy of one leaf. MUST NOT kill any other live child.
    pub fn terminate_leaf(&mut self, id: SessionId) -> Result<(), Error> {
        Ok(self.kernel.terminate(id)?)
    }

    fn leaf(&self) -> Result<&Session, Error> {
        self.kernel
            .session(self.default_id)
            .ok_or(rill_kernel::Error::UnknownSession.into())
    }

    pub fn child_pid(&self) -> u32 {
        self.leaf().map(Session::child_pid).unwrap_or(0)
    }

    pub fn child_pid_of(&self, id: SessionId) -> Option<u32> {
        self.kernel.session(id).map(Session::child_pid)
    }

    pub fn socket_path(&self) -> &Path {
        &self.socket_path
    }

    pub fn identity_socket_path(&self) -> &Path {
        &self.identity_socket_path
    }

    /// Must stay at 1 however much the user types. Resync is a cold path
    /// (FR-RESYNC, SPEC-CHIP0 §7).
    pub fn resync_count(&self) -> u32 {
        self.leaf().map(Session::resync_count).unwrap_or(0)
    }

    pub fn stalled_reads(&self) -> u64 {
        self.leaf().map(Session::stalled_reads).unwrap_or(0)
    }

    pub fn child_alive(&self) -> bool {
        self.leaf().map(Session::child_alive).unwrap_or(false)
    }

    pub fn step(&mut self, timeout_ms: i32) -> Result<(), Error> {
        for id in self.kernel.ids() {
            self.kernel.reap(id)?;
        }
        self.poll_io(timeout_ms)?;
        self.flush_outbound()?;
        self.maybe_resync()?;
        Ok(())
    }

    /// Timeout passed to `poll` by `run`.
    ///
    /// Timeout `0` is only for a non-empty outbox that needs another write
    /// attempt. An attached client with credit and an empty outbox MUST block
    /// in `poll` (PTY/socket POLLIN wakes the echo). Q5's always-50 sleep
    /// missed vsync when `poll` did not wait on the PTY; spinning at `0`
    /// with credit burned a core at idle ([#335](https://github.com/mahboobmonnamd/RILL/issues/335)).
    pub fn step_timeout_ms(&self) -> i32 {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("spin_poll_while_attached") {
            let live = self.clients.iter().any(|c| {
                c.credit > 0
                    && c.leaf
                        .and_then(|id| self.kernel.session(id))
                        .is_some_and(|s| s.child_alive())
            });
            return if live { 0 } else { 50 };
        }
        if self.clients.iter().any(|c| !c.outbox.is_empty()) {
            0
        } else {
            50
        }
    }

    pub fn run(mut self) -> Result<(), Error> {
        loop {
            self.step(self.step_timeout_ms())?;
        }
    }

    fn maybe_resync(&mut self) -> Result<(), Error> {
        for id in self.kernel.ids() {
            let history = self
                .kernel
                .session_mut(id)
                .and_then(Session::take_resync_history);
            if let Some(history) = history {
                let proto2 = self
                    .clients
                    .iter()
                    .any(|c| c.leaf == Some(id) && c.protocol == PROTOCOL_2);
                if proto2 {
                    continue;
                }
                let bytes = self.chip.resync_from_history(&history)?;
                if !bytes.is_empty() {
                    if let Some(s) = self.kernel.session_mut(id) {
                        s.enqueue_outbound(Frame::Data(bytes));
                    }
                }
            }
        }
        Ok(())
    }

    fn poll_io(&mut self, timeout_ms: i32) -> Result<(), Error> {
        let mut fds = Vec::new();
        fds.push(libc::pollfd {
            fd: self.listener.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        });
        fds.push(libc::pollfd {
            fd: self.identity_listener.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        });
        fds.push(libc::pollfd {
            fd: self.nav_listener.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        });
        for c in &self.clients {
            let mut events = libc::POLLIN;
            if !c.outbox.is_empty() {
                events |= libc::POLLOUT;
            }
            fds.push(libc::pollfd {
                fd: c.stream.as_raw_fd(),
                events,
                revents: 0,
            });
        }
        let n_clients = self.clients.len();
        let readable = self.kernel.poll_with_extras(&mut fds, timeout_ms)?;
        if fds[0].revents & libc::POLLIN != 0 {
            self.accept_client()?;
        }
        if fds[1].revents & libc::POLLIN != 0 {
            self.serve_cold_identity()?;
        }
        if fds[2].revents & libc::POLLIN != 0 {
            self.accept_cold_nav()?;
        }
        self.poll_nav_commands()?;
        let mut i = 0;
        while i < self.clients.len() && i < n_clients {
            let idx = 3 + i;
            if idx < fds.len()
                && fds[idx].revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0
            {
                self.read_client(i)?;
                if i < self.clients.len() {
                    i += 1;
                }
            } else {
                i += 1;
            }
        }
        for id in readable {
            #[cfg(feature = "mutate")]
            if std::env::var("RILL_MUTATE").as_deref() == Ok("starve_other_leaves")
                && id != self.default_id
            {
                continue;
            }
            #[cfg(feature = "mutate")]
            if std::env::var("RILL_MUTATE").as_deref() == Ok("min_client_credit_gates_pty_read") {
                let min_credit = self
                    .clients
                    .iter()
                    .filter(|c| c.leaf == Some(id))
                    .map(|c| c.credit)
                    .min()
                    .unwrap_or(0);
                if min_credit == 0 {
                    continue;
                }
            }
            loop {
                let proto2 = self
                    .clients
                    .iter()
                    .any(|c| c.leaf == Some(id) && c.protocol == PROTOCOL_2);
                let p1_cap = protocol1_writer_credit(&self.clients, id);
                if proto2 || self.is_worker {
                    if let Some(0) = p1_cap {
                        if let Some(s) = self.kernel.session_mut(id) {
                            s.note_stalled_read();
                        }
                        break;
                    }
                    let max = p1_cap.map(|c| c as usize);
                    let drained = {
                        let Some(s) = self.kernel.session_mut(id) else {
                            break;
                        };
                        s.drain_pty_at_most(max)?
                    };
                    match drained {
                        Some(bytes) => {
                            let offset = self
                                .kernel
                                .session(id)
                                .map(Session::bytes_delivered)
                                .unwrap_or(0);
                            self.fanout_pty_bytes(id, offset, bytes)?;
                        }
                        None => break,
                    }
                } else {
                    let Some(s) = self.kernel.session_mut(id) else {
                        break;
                    };
                    if !(s.credit() > 0 && s.on_pty_readable()? > 0) {
                        break;
                    }
                }
            }
        }
        Ok(())
    }

    /// A connection to the companion socket is a cold read of the daemon's
    /// default leaf. No attach claim, PTY master, frame, or cell data crosses
    /// this boundary (ADR 0020 D6, SPEC-NAV §6).
    fn serve_cold_identity(&mut self) -> Result<(), Error> {
        match self.identity_listener.accept() {
            Ok((mut stream, _)) => {
                let identity = self.kernel.host_identity(self.default_id)?;
                match stream.write_all(identity.as_wire()) {
                    Ok(()) => {}
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
                        ) => {}
                    Err(e) => return Err(e.into()),
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.into()),
        }
        Ok(())
    }

    fn nav_snapshot_line(&self) -> String {
        let mut tabs = Vec::new();
        let mut leaves = Vec::new();
        for row in self.kernel.container_snapshot() {
            if row.kind != NodeKind::Tab {
                continue;
            }
            tabs.push(row.id.as_u64().to_string());
            let leaf = row.children.iter().find_map(|c| match c {
                NodeChild::Leaf(id) => Some(id.as_u64()),
                NodeChild::Node(_) => None,
            });
            leaves.push(leaf.unwrap_or(0).to_string());
        }
        format!(
            "ws={} tabs={} leaves={}\n",
            self.workspace_id.as_u64(),
            tabs.join(","),
            leaves.join(",")
        )
    }

    fn spawn_tab_leaf(&mut self) -> Result<(), Error> {
        if std::env::var("RILL_MUTATE").as_deref() == Ok("skip_nav_new_tab") {
            return Ok(());
        }
        let args: Vec<&str> = self.shell_args.iter().map(String::as_str).collect();
        let leaf = self.kernel.spawn_leaf(&self.shell, &args, self.size)?;
        let tab = self
            .kernel
            .create_node(NodeKind::Tab, Some(self.workspace_id))?;
        self.kernel.attach_leaf(tab, leaf)?;
        Ok(())
    }

    fn accept_cold_nav(&mut self) -> Result<(), Error> {
        match self.nav_listener.accept() {
            Ok((mut stream, _)) => {
                let _ = stream.set_nonblocking(false);
                let line = self.nav_snapshot_line();
                match stream.write_all(line.as_bytes()) {
                    Ok(()) => {
                        let _ = stream.set_nonblocking(true);
                        self.nav_conns.push(stream);
                    }
                    Err(e)
                        if matches!(
                            e.kind(),
                            std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
                        ) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        self.nav_conns.push(stream);
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.into()),
        }
        Ok(())
    }

    fn poll_nav_commands(&mut self) -> Result<(), Error> {
        let mut i = 0;
        while i < self.nav_conns.len() {
            let mut buf = [0u8; 64];
            let n = match self.nav_conns[i].read(&mut buf) {
                Ok(0) => {
                    self.nav_conns.remove(i);
                    continue;
                }
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    i += 1;
                    continue;
                }
                Err(_) => {
                    self.nav_conns.remove(i);
                    continue;
                }
            };
            let cmd = String::from_utf8_lossy(&buf[..n]);
            if cmd.contains("NEW_TAB") {
                self.spawn_tab_leaf()?;
                let reply = self.nav_snapshot_line();
                let _ = self.nav_conns[i].set_nonblocking(false);
                let _ = self.nav_conns[i].write_all(reply.as_bytes());
            }
            self.nav_conns.remove(i);
        }
        Ok(())
    }

    fn resolve_attach(&self, session_id: Option<u64>) -> Result<SessionId, Error> {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("ignore_session_id") {
            return Ok(self.default_id);
        }
        match session_id {
            None => Ok(self.default_id),
            Some(raw) => {
                let id = SessionId::from_u64(raw);
                if self.kernel.session(id).is_some() {
                    Ok(id)
                } else {
                    Err(rill_kernel::Error::UnknownSession.into())
                }
            }
        }
    }

    fn accept_client(&mut self) -> Result<(), Error> {
        match self.listener.accept() {
            Ok((stream, _)) => {
                if endpoint::authorize_peer(&stream).is_err() {
                    drop(stream);
                    return Ok(());
                }
                stream.set_nonblocking(true)?;
                if std::env::var("RILL_TEST_TINY_SNDBUF").is_ok() {
                    // T-PARTIAL-WRITE: force short writes on the accepted
                    // socket. Integration tests link the non-cfg(test) lib.
                    let tiny: libc::c_int = 1024;
                    unsafe {
                        libc::setsockopt(
                            stream.as_raw_fd(),
                            libc::SOL_SOCKET,
                            libc::SO_SNDBUF,
                            &tiny as *const _ as *const libc::c_void,
                            std::mem::size_of_val(&tiny) as libc::socklen_t,
                        );
                    }
                }
                #[cfg(feature = "mutate")]
                if std::env::var("RILL_MUTATE").as_deref() == Ok("accept_replaces_client") {
                    let leaves: Vec<SessionId> =
                        self.clients.iter().filter_map(|c| c.leaf).collect();
                    for id in leaves {
                        if let Some(s) = self.kernel.session_mut(id) {
                            s.detach();
                        }
                    }
                    self.clients.clear();
                }
                self.clients.push(Client {
                    stream,
                    decoder: Decoder::new(),
                    outbox: VecDeque::new(),
                    leaf: None,
                    observe: false,
                    protocol: PROTOCOL_VERSION,
                    credit: 0,
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(e) => return Err(e.into()),
        }
        Ok(())
    }

    fn read_client(&mut self, idx: usize) -> Result<(), Error> {
        let mut buf = [0u8; 8192];
        let n = {
            let Some(client) = self.clients.get_mut(idx) else {
                return Ok(());
            };
            match client.stream.read(&mut buf) {
                Ok(0) => None,
                Ok(n) => Some(n),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
                Err(_) => None,
            }
        };
        let Some(n) = n else {
            self.drop_client(idx);
            return Ok(());
        };
        let frames = {
            let Some(client) = self.clients.get_mut(idx) else {
                return Ok(());
            };
            match client.decoder.push(&buf[..n]) {
                Ok(frames) => frames,
                Err(_) => {
                    self.drop_client(idx);
                    return Ok(());
                }
            }
        };
        for frame in frames {
            self.dispatch_frame(idx, frame)?;
        }
        Ok(())
    }

    fn dispatch_frame(&mut self, idx: usize, frame: Frame) -> Result<(), Error> {
        match frame {
            Frame::Attach {
                generation,
                session_id,
                protocol,
                observe,
            } => {
                let proto_ok = {
                    #[cfg(feature = "mutate")]
                    {
                        std::env::var("RILL_MUTATE").as_deref() == Ok("ignore_protocol_version")
                            || Frame::protocol_supported(protocol)
                    }
                    #[cfg(not(feature = "mutate"))]
                    {
                        Frame::protocol_supported(protocol)
                    }
                };
                if !proto_ok {
                    if let Some(c) = self.clients.get_mut(idx) {
                        c.outbox.extend(
                            Frame::Refused {
                                reason: rill_attach::RefuseReason::ProtocolMismatch,
                            }
                            .encode()?,
                        );
                    }
                    return Ok(());
                }
                let id = match self.resolve_attach(session_id) {
                    Ok(id) => id,
                    Err(_) => {
                        if let Some(c) = self.clients.get_mut(idx) {
                            c.outbox.extend(
                                Frame::Refused {
                                    reason: rill_attach::RefuseReason::Invalid,
                                }
                                .encode()?,
                            );
                        }
                        return Ok(());
                    }
                };
                let already = self.kernel.session(id).is_some_and(Session::attached);
                let mine = self.clients.get(idx).and_then(|c| c.leaf) == Some(id);
                if already && !mine && !observe {
                    if let Some(c) = self.clients.get_mut(idx) {
                        c.outbox.extend(
                            Frame::Refused {
                                reason: rill_attach::RefuseReason::AlreadyAttached,
                            }
                            .encode()?,
                        );
                    }
                    return Ok(());
                }
                match self.kernel.on_frame(
                    id,
                    Frame::Attach {
                        generation,
                        session_id,
                        protocol,
                        observe,
                    },
                ) {
                    Ok(()) => {}
                    Err(rill_kernel::Error::Dead) => {}
                    Err(e) => return Err(e.into()),
                }
                if observe || self.kernel.session(id).is_some_and(Session::attached) {
                    if let Some(c) = self.clients.get_mut(idx) {
                        c.leaf = Some(id);
                        c.observe = observe;
                        c.protocol = protocol;
                    }
                    if protocol == PROTOCOL_2 {
                        self.emit_checkpoint(idx, id)?;
                    } else if observe {
                        if let Some(s) = self.kernel.session(id) {
                            let hist = s.history();
                            if !hist.is_empty() {
                                if let Some(c) = self.clients.get_mut(idx) {
                                    c.outbox.extend(Frame::Data(hist).encode()?);
                                }
                            }
                        }
                    }
                }
            }
            other => {
                let attached = self.clients.get(idx).and_then(|c| c.leaf);
                if attached.is_none() {
                    #[cfg(feature = "mutate")]
                    if std::env::var("RILL_MUTATE").as_deref()
                        == Ok("unattached_falls_back_to_default")
                    {
                        // fall through
                    } else {
                        self.drop_client(idx);
                        return Ok(());
                    }
                    #[cfg(not(feature = "mutate"))]
                    {
                        self.drop_client(idx);
                        return Ok(());
                    }
                }
                let observe = self.clients.get(idx).is_some_and(|c| c.observe);
                if observe && matches!(other, Frame::Data(_)) {
                    #[cfg(feature = "mutate")]
                    if std::env::var("RILL_MUTATE").as_deref() == Ok("allow_observer_write") {
                        // fall through — mutation must turn T-GRAPH-OBSERVE red
                    } else {
                        return Ok(());
                    }
                    #[cfg(not(feature = "mutate"))]
                    {
                        return Ok(());
                    }
                }
                if observe && matches!(other, Frame::Resize { .. }) {
                    #[cfg(feature = "mutate")]
                    if std::env::var("RILL_MUTATE").as_deref() == Ok("allow_observer_resize") {
                        // fall through — T-CLIENT-OBSERVER-ISOLATION
                    } else if let Frame::Resize {
                        cols,
                        rows,
                        px_w,
                        px_h,
                    } = other
                    {
                        if let Some(id) = attached {
                            if let Some(s) = self.kernel.session_mut(id) {
                                s.apply_observer_viewport(cols, rows, px_w, px_h)?;
                            }
                        }
                        return Ok(());
                    }
                    #[cfg(not(feature = "mutate"))]
                    {
                        if let Frame::Resize {
                            cols,
                            rows,
                            px_w,
                            px_h,
                        } = other
                        {
                            if let Some(id) = attached {
                                if let Some(s) = self.kernel.session_mut(id) {
                                    s.apply_observer_viewport(cols, rows, px_w, px_h)?;
                                }
                            }
                            return Ok(());
                        }
                    }
                }
                if let Frame::Credit(n) = other {
                    if let Some(c) = self.clients.get_mut(idx) {
                        c.credit = c.credit.saturating_add(u64::from(n));
                    }
                }
                let id = self
                    .clients
                    .get(idx)
                    .and_then(|c| c.leaf)
                    .unwrap_or(self.default_id);
                match self.kernel.on_frame(id, other) {
                    Ok(()) => {}
                    Err(rill_kernel::Error::Dead) => {}
                    Err(e) => return Err(e.into()),
                }
            }
        }
        Ok(())
    }

    fn drop_client(&mut self, idx: usize) {
        if idx >= self.clients.len() {
            return;
        }
        let leaf = self.clients[idx].leaf;
        let observe = self.clients[idx].observe;
        if let Some(id) = leaf {
            if let Some(s) = self.kernel.session_mut(id) {
                if observe {
                    s.release_observer();
                } else {
                    s.detach();
                }
            }
        }
        self.clients.remove(idx);
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("worker_exits_on_daemon_close")
            && self.is_worker
            && self.clients.is_empty()
        {
            std::process::exit(0);
        }
    }

    fn fanout_pty_bytes(
        &mut self,
        id: SessionId,
        offset: u64,
        bytes: Vec<u8>,
    ) -> Result<(), Error> {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("history_data_only") {
            if let Some(s) = self.kernel.session_mut(id) {
                s.enqueue_outbound(Frame::Data(bytes));
            }
            return Ok(());
        }
        for c in &mut self.clients {
            if c.leaf != Some(id) {
                continue;
            }
            let Some(n) = live_chunk(c.credit, bytes.len()) else {
                continue;
            };
            c.credit -= n as u64;
            let chunk = bytes[..n].to_vec();
            let frame = if c.protocol == PROTOCOL_2 {
                Frame::Delta {
                    start_offset: offset.saturating_sub(bytes.len() as u64),
                    bytes: chunk,
                }
            } else {
                Frame::Data(chunk)
            };
            c.outbox.extend(frame.encode()?);
        }
        Ok(())
    }

    fn emit_checkpoint(&mut self, idx: usize, id: SessionId) -> Result<(), Error> {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("history_data_only") {
            if let Some(s) = self.kernel.session(id) {
                let hist = s.history();
                if let Some(c) = self.clients.get_mut(idx) {
                    c.outbox.extend(Frame::Data(hist).encode()?);
                }
            }
            return Ok(());
        }
        let (hist, offset, _exec) = match self.kernel.session(id) {
            Some(s) => (s.history(), s.bytes_delivered(), id.as_u64()),
            None => return Ok(()),
        };
        self.chip.reset()?;
        if !hist.is_empty() {
            self.chip.feed(&hist)?;
        }
        let grid = self.chip.snapshot()?;
        let body = encode_pod_grid(&grid);
        let hash = pod_hash(&grid);
        if let Some(c) = self.clients.get_mut(idx) {
            c.outbox.extend(
                Frame::Checkpoint {
                    ending_offset: offset,
                    hash,
                    blob: body,
                }
                .encode()?,
            );
        }
        Ok(())
    }

    fn flush_outbound(&mut self) -> Result<(), Error> {
        // With no client for a leaf, leave control frames queued (audit S3-2).
        let claimed: Vec<SessionId> = self.clients.iter().filter_map(|c| c.leaf).collect();
        for id in self.kernel.ids() {
            if claimed.contains(&id) {
                continue;
            }
            let Some(session) = self.kernel.session_mut(id) else {
                continue;
            };
            let mut keep = Vec::new();
            while let Some(f) = session.pop_outbound() {
                if !matches!(f, Frame::Data(_)) {
                    keep.push(f);
                }
            }
            for f in keep {
                session.enqueue_outbound(f);
            }
        }

        let mut pending: std::collections::HashMap<SessionId, Vec<Frame>> =
            std::collections::HashMap::new();
        for id in &claimed {
            if pending.contains_key(id) {
                continue;
            }
            let mut frames = Vec::new();
            if let Some(session) = self.kernel.session_mut(*id) {
                while let Some(f) = session.pop_outbound() {
                    frames.push(f);
                }
            }
            pending.insert(*id, frames);
        }

        let mut drop_idx = None;
        for i in 0..self.clients.len() {
            if let Some(id) = self.clients[i].leaf {
                let frames = pending.get(&id).cloned().unwrap_or_default();
                #[cfg(feature = "mutate")]
                if std::env::var("RILL_MUTATE").as_deref() == Ok("replay_full_frame") {
                    // Defect: write_all a whole frame, then re-queue it.
                    // Do not wait for WouldBlock: hosted kernels often ignore a
                    // tiny SO_SNDBUF, so gating the replay on WouldBlock left
                    // the instrument green (ADR 0002 D3).
                    for frame in frames {
                        let bytes = frame.encode()?;
                        let write_rc = {
                            let client = &mut self.clients[i];
                            client.stream.write_all(&bytes)
                        };
                        if let Some(session) = self.kernel.session_mut(id) {
                            session.enqueue_outbound(frame);
                        }
                        match write_rc {
                            Ok(()) => {}
                            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                            Err(_) => {
                                drop_idx = Some(i);
                                break;
                            }
                        }
                    }
                    if drop_idx.is_some() {
                        break;
                    }
                    continue;
                }
                let protocol = self.clients[i].protocol;
                let client = &mut self.clients[i];
                for frame in frames {
                    if protocol == PROTOCOL_2 && matches!(frame, Frame::Data(_)) {
                        continue;
                    }
                    client.outbox.extend(frame.encode()?);
                }
            }
            let client = &mut self.clients[i];
            if write_outbox(&mut client.stream, &mut client.outbox).is_err() {
                drop_idx = Some(i);
                break;
            }
        }
        if let Some(i) = drop_idx {
            self.drop_client(i);
        }
        Ok(())
    }
}

fn encode_pod_grid(grid: &PodGrid) -> Vec<u8> {
    let mut body = Vec::new();
    body.extend_from_slice(&grid.cols.to_le_bytes());
    body.extend_from_slice(&grid.rows.to_le_bytes());
    body.extend_from_slice(&grid.cursor_col.to_le_bytes());
    body.extend_from_slice(&grid.cursor_row.to_le_bytes());
    body.push(u8::from(grid.cursor_visible));
    for cell in &grid.cells {
        body.extend_from_slice(&cell.codepoint.to_le_bytes());
        body.extend_from_slice(&cell.fg.to_le_bytes());
        body.extend_from_slice(&cell.bg.to_le_bytes());
        body.extend_from_slice(&cell.attrs.to_le_bytes());
        body.extend_from_slice(&cell._pad.to_le_bytes());
    }
    body
}

fn pod_hash(grid: &PodGrid) -> u64 {
    let mut h = 0u64;
    for cell in &grid.cells {
        h = h
            .wrapping_mul(16777619)
            .wrapping_add(u64::from(cell.codepoint));
    }
    h
}

/// How many bytes of a drained PTY chunk may go to one client.
///
/// A short credit MUST skip the client, not send a prefix ([#335](https://github.com/mahboobmonnamd/RILL/issues/335)).
pub(crate) fn live_chunk(credit: u64, n: usize) -> Option<usize> {
    #[cfg(feature = "mutate")]
    if std::env::var("RILL_MUTATE").as_deref() == Ok("truncate_fanout") {
        return Some(n.min(credit as usize));
    }
    if n == 0 {
        return Some(0);
    }
    if credit < n as u64 {
        None
    } else {
        Some(n)
    }
}

fn protocol1_writer_credit(clients: &[Client], id: SessionId) -> Option<u64> {
    let mut min_c: Option<u64> = None;
    for c in clients {
        if c.leaf != Some(id) || c.observe || c.protocol == PROTOCOL_2 {
            continue;
        }
        min_c = Some(min_c.map_or(c.credit, |m| m.min(c.credit)));
    }
    min_c
}

/// Non-blocking drain of `outbox`. Partial progress stays queued (Q1).
pub(crate) fn write_outbox(
    stream: &mut UnixStream,
    outbox: &mut VecDeque<u8>,
) -> Result<(), Error> {
    while !outbox.is_empty() {
        let n = {
            let (front, back) = outbox.as_slices();
            let slice = if !front.is_empty() { front } else { back };
            match stream.write(slice) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(e) => return Err(e.into()),
            }
        };
        outbox.drain(..n);
    }
    Ok(())
}

impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.socket_path);
        let _ = std::fs::remove_file(&self.identity_socket_path);
        let _ = std::fs::remove_file(&self.nav_socket_path);
    }
}

pub fn default_socket() -> PathBuf {
    if let Ok(p) = std::env::var("RILL_SOCKET") {
        return PathBuf::from(p);
    }
    default_runtime_dir().join("attach.sock")
}

pub fn default_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into())
}

/// ADR 0015 D2. Mutation `skip_nested_guard` must turn the gate red.
pub fn nested_launch_blocked() -> bool {
    #[cfg(feature = "mutate")]
    if std::env::var("RILL_MUTATE").as_deref() == Ok("skip_nested_guard") {
        return false;
    }
    std::env::var("RILL_INSIDE").as_deref() == Ok("1")
        && std::env::var("RILL_ALLOW_NESTED").as_deref() != Ok("1")
}

/// Drive the daemon for a bounded time. Used by named tests.
pub fn pump(daemon: &mut Daemon, duration: Duration) -> Result<(), Error> {
    let start = std::time::Instant::now();
    while start.elapsed() < duration {
        daemon.step(20)?;
    }
    Ok(())
}

#[cfg(test)]
mod write_outbox_q1 {
    use super::*;
    use std::io::ErrorKind;
    use std::os::fd::AsRawFd;

    fn tiny_buf(fd: std::os::fd::RawFd, send: bool) {
        let n: libc::c_int = 2048;
        let opt = if send {
            libc::SO_SNDBUF
        } else {
            libc::SO_RCVBUF
        };
        unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                opt,
                &n as *const _ as *const libc::c_void,
                std::mem::size_of_val(&n) as libc::socklen_t,
            );
        }
    }

    fn payload(n: usize) -> Vec<u8> {
        (0..n).map(|i| (i % 251) as u8).collect()
    }

    #[test]
    fn t_write_outbox_does_not_replay_bytes_the_peer_already_has() {
        let (mut w, mut r) = UnixStream::pair().expect("pair");
        w.set_nonblocking(true).expect("w nb");
        r.set_nonblocking(true).expect("r nb");
        tiny_buf(w.as_raw_fd(), true);
        tiny_buf(r.as_raw_fd(), false);
        let src = payload(120_000);
        let mut outbox = VecDeque::from(src.clone());
        let mut got = Vec::new();
        let mut buf = [0u8; 4096];
        for _ in 0..50_000 {
            write_outbox(&mut w, &mut outbox).expect("flush");
            match r.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => got.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(e) => panic!("read: {e}"),
            }
            if got.len() == src.len() && outbox.is_empty() {
                break;
            }
        }
        assert_eq!(got, src, "outbox replayed or dropped bytes");
    }

    /// T-WORKER-CREDIT — short credit must not emit a truncated chunk.
    ///
    /// Required mutation: `RILL_MUTATE=truncate_fanout`.
    #[test]
    fn t_worker_fanout_does_not_truncate_when_credit_is_short() {
        assert_eq!(
            live_chunk(2, 5),
            None,
            "short credit truncated a live DATA chunk"
        );
        assert_eq!(live_chunk(5, 5), Some(5));
        assert_eq!(live_chunk(0, 5), None);
    }

    #[test]
    fn t_write_all_requeue_replays_a_prefix() {
        let (mut w, mut r) = UnixStream::pair().expect("pair");
        w.set_nonblocking(true).expect("w nb");
        r.set_nonblocking(true).expect("r nb");
        tiny_buf(w.as_raw_fd(), true);
        tiny_buf(r.as_raw_fd(), false);
        let src = payload(120_000);
        let mut pending = VecDeque::from(vec![src.clone()]);
        let mut got = Vec::new();
        let mut buf = [0u8; 4096];
        for _ in 0..50_000 {
            if let Some(frame) = pending.pop_front() {
                match w.write_all(&frame) {
                    Ok(()) => {}
                    Err(e) if e.kind() == ErrorKind::WouldBlock => {
                        pending.push_back(frame);
                    }
                    Err(e) => panic!("write: {e}"),
                }
            }
            match r.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => got.extend_from_slice(&buf[..n]),
                Err(e) if e.kind() == ErrorKind::WouldBlock => {}
                Err(e) => panic!("read: {e}"),
            }
            if pending.is_empty() && got.len() >= src.len() {
                break;
            }
        }
        assert_ne!(
            got, src,
            "oracle is blind: write_all+requeue must differ from the source"
        );
    }
}
