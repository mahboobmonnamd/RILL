//! Session graph: `SessionId` → `Session` (ADR 0011, SPEC-GRAPH).
//!
//! Creating a leaf is a cold call. The warm path does not allocate sessions.

use crate::error::Error;
use crate::pty::{Discipline, Winsize};
use crate::session::Session;
use rill_attach::Frame;
use std::collections::HashMap;
use std::path::PathBuf;

/// Opaque kernel-allocated id. Not a path, a title, or a GUI index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SessionId(u64);

impl SessionId {
    pub fn as_u64(self) -> u64 {
        self.0
    }

    pub fn from_u64(v: u64) -> Self {
        Self(v)
    }
}

/// Stable event id (ADR 0015 D4). Not a warm-path frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GraphEventKind {
    Spawn,
    Attach,
    Terminate,
    Exit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphEvent {
    pub id: u64,
    pub session: SessionId,
    pub kind: GraphEventKind,
}

/// Kernel layout row. Not window chrome (ADR 0015 D6).
#[derive(Clone, Debug)]
pub struct LeafLayout {
    pub id: SessionId,
    pub cols: u16,
    pub rows: u16,
    pub child_pid: u32,
    pub cwd: Option<PathBuf>,
}

/// Map of leaves. Spike 0 behaviour is this map at size 1.
pub struct Kernel {
    next: u64,
    leaves: HashMap<SessionId, Session>,
    events: Vec<GraphEvent>,
    next_event: u64,
}

impl Kernel {
    pub fn new() -> Self {
        Self {
            next: 1,
            leaves: HashMap::new(),
            events: Vec::new(),
            next_event: 1,
        }
    }

    pub fn spawn_leaf(
        &mut self,
        shell: &str,
        args: &[&str],
        size: Winsize,
    ) -> Result<SessionId, Error> {
        self.spawn_leaf_with(shell, args, size, Discipline::Interactive)
    }

    pub fn spawn_leaf_with(
        &mut self,
        shell: &str,
        args: &[&str],
        size: Winsize,
        discipline: Discipline,
    ) -> Result<SessionId, Error> {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("single_session") {
            if let Some(&id) = self.leaves.keys().next() {
                return Ok(id);
            }
        }

        let session = Session::spawn_with(shell, args, size, discipline)?;
        let id = self.allocate_id();
        self.leaves.insert(id, session);
        self.record(id, GraphEventKind::Spawn);
        Ok(id)
    }

    fn allocate_id(&mut self) -> SessionId {
        loop {
            let id = SessionId(self.next);
            self.next = self.next.checked_add(1).unwrap_or(1);
            if !self.leaves.contains_key(&id) {
                return id;
            }
        }
    }

    pub fn session(&self, id: SessionId) -> Option<&Session> {
        self.leaves.get(&id)
    }

    pub fn session_mut(&mut self, id: SessionId) -> Option<&mut Session> {
        self.leaves.get_mut(&id)
    }

    pub fn ids(&self) -> Vec<SessionId> {
        self.leaves.keys().copied().collect()
    }

    /// Poll every leaf master together with caller sockets. Master fds stay
    /// inside this crate (SPEC-KERNEL §1).
    pub fn poll_with_extras(
        &self,
        extras: &mut [libc::pollfd],
        timeout_ms: i32,
    ) -> Result<Vec<SessionId>, Error> {
        let mut ready_ids = Vec::new();
        let mut fds = Vec::new();
        for (&id, session) in &self.leaves {
            if session.credit() > 0 && session.child_alive() {
                ready_ids.push(id);
                fds.push(session.master_pollfd(libc::POLLIN));
            }
        }
        let n_pty = fds.len();
        fds.extend_from_slice(extras);
        if fds.is_empty() {
            return Ok(Vec::new());
        }
        // SAFETY: fds is a valid pollfd slice we own for this call.
        let rc = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout_ms) };
        if rc < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                return Ok(Vec::new());
            }
            return Err(Error::Io(err));
        }
        for (dst, src) in extras.iter_mut().zip(fds.iter().skip(n_pty)) {
            dst.revents = src.revents;
        }
        let mut readable = Vec::new();
        for (i, id) in ready_ids.into_iter().enumerate() {
            if fds[i].revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0 {
                readable.push(id);
            }
        }
        Ok(readable)
    }

    fn record(&mut self, session: SessionId, kind: GraphEventKind) {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("duplicate_event_ids") {
            self.events.push(GraphEvent {
                id: 1,
                session,
                kind,
            });
            return;
        }
        let id = self.next_event;
        self.next_event = self.next_event.saturating_add(1);
        self.events.push(GraphEvent { id, session, kind });
    }

    pub fn events(&self) -> &[GraphEvent] {
        &self.events
    }

    /// Reap one leaf. Records a single `Exit` event the first time the child
    /// is observed dead (ADR 0015 D4).
    pub fn reap(&mut self, id: SessionId) -> Result<(), Error> {
        let session = self.leaves.get_mut(&id).ok_or(Error::UnknownSession)?;
        if session.poll_child()? {
            self.record(id, GraphEventKind::Exit);
        }
        Ok(())
    }

    /// Apply one inbound frame to a named leaf. Unknown ids fail closed.
    pub fn on_frame(&mut self, id: SessionId, frame: Frame) -> Result<(), Error> {
        let is_writer_attach = matches!(frame, Frame::Attach { observe: false, .. });
        let was = self.leaves.get(&id).is_some_and(Session::attached);
        let session = self.leaves.get_mut(&id).ok_or(Error::UnknownSession)?;
        session.on_frame(frame)?;
        let now = self.leaves.get(&id).is_some_and(Session::attached);
        if is_writer_attach && !was && now {
            self.record(id, GraphEventKind::Attach);
        }
        Ok(())
    }

    /// Cold destroy of one leaf. MUST NOT kill any other live child
    /// (ADR 0011 D2). The session stays in the map so EXIT can still replay.
    /// A second terminate of a dead leaf is a no-op (ADR 0015 D4).
    pub fn terminate(&mut self, id: SessionId) -> Result<(), Error> {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("terminate_all_leaves") {
            for session in self.leaves.values_mut() {
                session.terminate()?;
            }
            return Ok(());
        }
        let session = self.leaves.get_mut(&id).ok_or(Error::UnknownSession)?;
        if session.is_terminated() {
            return Ok(());
        }
        session.terminate()?;
        self.record(id, GraphEventKind::Terminate);
        Ok(())
    }

    pub fn cwd(&mut self, id: SessionId) -> Result<std::path::PathBuf, Error> {
        self.leaves.get_mut(&id).ok_or(Error::UnknownSession)?.cwd()
    }

    pub fn layout_snapshot(&mut self) -> Vec<LeafLayout> {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("omit_second_leaf") {
            return self
                .leaves
                .iter_mut()
                .take(1)
                .map(|(&id, s)| leaf_layout(id, s))
                .collect();
        }
        let mut ids: Vec<SessionId> = self.leaves.keys().copied().collect();
        ids.sort_by_key(|id| id.as_u64());
        ids.into_iter()
            .filter_map(|id| self.leaves.get_mut(&id).map(|s| leaf_layout(id, s)))
            .collect()
    }
}

fn leaf_layout(id: SessionId, session: &mut Session) -> LeafLayout {
    let size = session.size();
    LeafLayout {
        id,
        cols: size.cols,
        rows: size.rows,
        child_pid: session.child_pid(),
        cwd: session.cwd().ok(),
    }
}

impl Default for Kernel {
    fn default() -> Self {
        Self::new()
    }
}
