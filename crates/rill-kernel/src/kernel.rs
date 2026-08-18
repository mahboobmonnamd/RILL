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

/// Kernel-allocated container id (ADR 0020 D1). A distinct type from
/// `SessionId` by construction — the compiler refuses to confuse the two.
/// Never a path, a title, or a GUI index. Never reused while the node is
/// live.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct NodeId(u64);

impl NodeId {
    pub fn as_u64(self) -> u64 {
        self.0
    }
}

/// The four container kinds SPEC-NAV names. A leaf is not a `NodeKind`; it
/// is addressed by `SessionId` and referenced from a node as a child.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeKind {
    Workspace,
    Group,
    Tab,
    Split,
}

/// One child slot of a container: another container, or a leaf this kernel
/// already owns (ADR 0011). Chrome MUST NOT invent a variant of this that
/// the kernel did not create (SPEC-NAV §1).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeChild {
    Node(NodeId),
    Leaf(SessionId),
}

struct ContainerNode {
    kind: NodeKind,
    parent: Option<NodeId>,
    children: Vec<NodeChild>,
}

/// Cold, read-only projection of one container for `container_snapshot`
/// (ADR 0020 D1). This is what chrome renders from; chrome MUST NOT hold a
/// second, authoritative copy (SPEC-NAV §1).
#[derive(Clone, Debug)]
pub struct ContainerSnapshot {
    pub id: NodeId,
    pub kind: NodeKind,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeChild>,
}

/// Map of leaves. Spike 0 behaviour is this map at size 1.
pub struct Kernel {
    next: u64,
    leaves: HashMap<SessionId, Session>,
    events: Vec<GraphEvent>,
    next_event: u64,
    nodes: HashMap<NodeId, ContainerNode>,
    next_node: u64,
}

impl Kernel {
    pub fn new() -> Self {
        Self {
            next: 1,
            leaves: HashMap::new(),
            events: Vec::new(),
            next_event: 1,
            nodes: HashMap::new(),
            next_node: 1,
        }
    }

    fn allocate_node_id(&mut self) -> NodeId {
        loop {
            let id = NodeId(self.next_node);
            self.next_node = self.next_node.checked_add(1).unwrap_or(1);
            if !self.nodes.contains_key(&id) {
                return id;
            }
        }
    }

    /// Cold create. Not a warm `DATA` frame (ADR 0020 D1, ADR 0011 D2).
    pub fn create_node(&mut self, kind: NodeKind, parent: Option<NodeId>) -> Result<NodeId, Error> {
        if let Some(p) = parent {
            if !self.nodes.contains_key(&p) {
                return Err(Error::UnknownNode);
            }
        }
        let id = self.allocate_node_id();
        self.nodes.insert(
            id,
            ContainerNode {
                kind,
                parent,
                children: Vec::new(),
            },
        );
        if let Some(p) = parent {
            // Presence was just checked above; fail closed rather than
            // panic if that invariant is ever wrong (PRD NFR-FAIL).
            match self.nodes.get_mut(&p) {
                Some(parent_node) => parent_node.children.push(NodeChild::Node(id)),
                None => {
                    self.nodes.remove(&id);
                    return Err(Error::UnknownNode);
                }
            }
        }
        Ok(id)
    }

    /// Attach an already-live leaf into a pane slot (ADR 0020 D2's surface
    /// stack, membership only). MUST NOT spawn, MUST NOT detach the leaf.
    pub fn attach_leaf(&mut self, node: NodeId, leaf: SessionId) -> Result<(), Error> {
        if !self.leaves.contains_key(&leaf) {
            return Err(Error::UnknownSession);
        }
        let n = self.nodes.get_mut(&node).ok_or(Error::UnknownNode)?;
        n.children.push(NodeChild::Leaf(leaf));
        Ok(())
    }

    /// Reparent `node` cold (drag, zoom, split/unsplit, template restore).
    /// MUST NOT change any `SessionId` under it, MUST NOT `posix_spawn`, and
    /// MUST NOT detach/reattach a leaf (ADR 0020 D4). The node keeps its
    /// `NodeId` and every descendant keeps its `SessionId`/pid — this
    /// function only ever moves pointers.
    pub fn reparent_node(&mut self, node: NodeId, new_parent: Option<NodeId>) -> Result<(), Error> {
        if !self.nodes.contains_key(&node) {
            return Err(Error::UnknownNode);
        }
        if let Some(p) = new_parent {
            if !self.nodes.contains_key(&p) {
                return Err(Error::UnknownNode);
            }
            if p == node {
                return Err(Error::UnknownNode);
            }
        }

        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("reparent_recreates_node") {
            // Required mutation for T-NAV-REPARENT: instead of moving the
            // existing node, tear it down and mint a fresh NodeId under the
            // new parent. Children move, but identity does not survive —
            // exactly the regression ADR 0020 D4 forbids. Presence of
            // `node` was checked above; fail closed rather than panic if
            // that invariant is ever wrong (PRD NFR-FAIL).
            let Some(old) = self.nodes.remove(&node) else {
                return Err(Error::UnknownNode);
            };
            if let Some(old_parent) = old.parent {
                if let Some(op) = self.nodes.get_mut(&old_parent) {
                    op.children
                        .retain(|c| !matches!(c, NodeChild::Node(n) if *n == node));
                }
            }
            let fresh = self.allocate_node_id();
            self.nodes.insert(
                fresh,
                ContainerNode {
                    kind: old.kind,
                    parent: new_parent,
                    children: old.children,
                },
            );
            if let Some(p) = new_parent {
                if let Some(parent_node) = self.nodes.get_mut(&p) {
                    parent_node.children.push(NodeChild::Node(fresh));
                }
            }
            return Ok(());
        }

        let old_parent = self.nodes.get(&node).and_then(|n| n.parent);
        if let Some(op) = old_parent {
            if let Some(parent_node) = self.nodes.get_mut(&op) {
                parent_node
                    .children
                    .retain(|c| !matches!(c, NodeChild::Node(n) if *n == node));
            }
        }
        if let Some(p) = new_parent {
            if let Some(parent_node) = self.nodes.get_mut(&p) {
                parent_node.children.push(NodeChild::Node(node));
            }
        }
        // Presence was checked at the top of this function; fail closed
        // rather than panic if that invariant is ever wrong (PRD NFR-FAIL).
        match self.nodes.get_mut(&node) {
            Some(n) => n.parent = new_parent,
            None => return Err(Error::UnknownNode),
        }
        Ok(())
    }

    /// Close `node`: terminate every leaf owned by its subtree via explicit
    /// `Kernel::terminate` (ADR 0020 D3). MUST NOT terminate a leaf outside
    /// the subtree, and MUST NOT rely on `Drop`.
    pub fn close_node(&mut self, node: NodeId) -> Result<(), Error> {
        let subtree = self.collect_subtree(node)?;
        let leaves = self.leaves_in(&subtree);

        #[cfg(feature = "mutate")]
        if std::env::var("RILL_MUTATE").as_deref() == Ok("close_node_terminates_all_leaves") {
            let all: Vec<SessionId> = self.leaves.keys().copied().collect();
            for id in all {
                let _ = self.terminate(id);
            }
            self.remove_subtree(&subtree, node);
            return Ok(());
        }

        for id in leaves {
            self.terminate(id)?;
        }
        self.remove_subtree(&subtree, node);
        Ok(())
    }

    fn collect_subtree(&self, root: NodeId) -> Result<Vec<NodeId>, Error> {
        if !self.nodes.contains_key(&root) {
            return Err(Error::UnknownNode);
        }
        let mut out = vec![root];
        let mut frontier = vec![root];
        while let Some(id) = frontier.pop() {
            if let Some(n) = self.nodes.get(&id) {
                for c in &n.children {
                    if let NodeChild::Node(child) = c {
                        out.push(*child);
                        frontier.push(*child);
                    }
                }
            }
        }
        Ok(out)
    }

    fn leaves_in(&self, subtree: &[NodeId]) -> Vec<SessionId> {
        let mut out = Vec::new();
        for id in subtree {
            if let Some(n) = self.nodes.get(id) {
                for c in &n.children {
                    if let NodeChild::Leaf(s) = c {
                        out.push(*s);
                    }
                }
            }
        }
        out
    }

    fn remove_subtree(&mut self, subtree: &[NodeId], root: NodeId) {
        let root_parent = self.nodes.get(&root).and_then(|n| n.parent);
        if let Some(p) = root_parent {
            if let Some(parent_node) = self.nodes.get_mut(&p) {
                parent_node
                    .children
                    .retain(|c| !matches!(c, NodeChild::Node(n) if *n == root));
            }
        }
        for id in subtree {
            self.nodes.remove(id);
        }
    }

    /// Cold, sorted read of the whole container tree (ADR 0020 D1). This is
    /// what `layout_snapshot` exposes to chrome; it MUST NOT be sampled on
    /// the warm path.
    pub fn container_snapshot(&self) -> Vec<ContainerSnapshot> {
        let mut ids: Vec<NodeId> = self.nodes.keys().copied().collect();
        ids.sort_by_key(|id| id.as_u64());
        ids.into_iter()
            .filter_map(|id| {
                self.nodes.get(&id).map(|n| ContainerSnapshot {
                    id,
                    kind: n.kind,
                    parent: n.parent,
                    #[cfg(feature = "mutate")]
                    children: if std::env::var("RILL_MUTATE").as_deref() == Ok("omit_node_children")
                    {
                        Vec::new()
                    } else {
                        n.children.clone()
                    },
                    #[cfg(not(feature = "mutate"))]
                    children: n.children.clone(),
                })
            })
            .collect()
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
