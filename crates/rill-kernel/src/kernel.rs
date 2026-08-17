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

    /// Apply one inbound frame to a named leaf. Unknown ids fail closed.
    pub fn on_frame(&mut self, id: SessionId, frame: Frame) -> Result<(), Error> {
        let session = self.leaves.get_mut(&id).ok_or(Error::UnknownSession)?;
        session.on_frame(frame)
    }
}

impl Default for Kernel {
    fn default() -> Self {
        Self::new()
    }
}
