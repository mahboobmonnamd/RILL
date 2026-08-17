//! Session graph: `SessionId` → `Session` (ADR 0011, SPEC-GRAPH).
//!
//! Creating a leaf is a cold call. The warm path does not allocate sessions.

use crate::error::Error;
use crate::pty::{Discipline, Winsize};
use crate::session::Session;
use rill_attach::Frame;
use std::collections::HashMap;

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

/// Map of leaves. Spike 0 behaviour is this map at size 1.
pub struct Kernel {
    next: u64,
    leaves: HashMap<SessionId, Session>,
}

impl Kernel {
    pub fn new() -> Self {
        Self {
            next: 1,
            leaves: HashMap::new(),
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

    /// Apply one inbound frame to a named leaf. Unknown ids fail closed.
    pub fn on_frame(&mut self, id: SessionId, frame: Frame) -> Result<(), Error> {
        let session = self.leaves.get_mut(&id).ok_or(Error::UnknownSession)?;
        session.on_frame(frame)
    }

    /// Cold destroy of one leaf. MUST NOT kill any other live child
    /// (ADR 0011 D2). The session stays in the map so EXIT can still replay.
    pub fn terminate(&mut self, id: SessionId) -> Result<(), Error> {
        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("terminate_all_leaves") {
            for session in self.leaves.values_mut() {
                session.terminate()?;
            }
            return Ok(());
        }
        self.leaves
            .get_mut(&id)
            .ok_or(Error::UnknownSession)?
            .terminate()
    }
}

impl Default for Kernel {
    fn default() -> Self {
        Self::new()
    }
}
