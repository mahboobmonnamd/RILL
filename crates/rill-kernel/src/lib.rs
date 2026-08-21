//! Kernel plane: PTY master, sole writer, bounded byte history.
//!
//! This crate must not paint and must not ship JSON cells. The GUI never
//! receives the master fd — readiness is exposed as [`Session::wait_readable`],
//! not as a descriptor (SPEC-KERNEL §1).

mod checkpoint;
mod error;
mod journal;
mod kernel;
mod pty;
mod ring;
mod session;

pub use checkpoint::{StoredCheckpoint, CHECKPOINT_FORMAT_VERSION};
pub use error::Error;
pub use journal::{reconcile_execution, TerminalOutcome};
pub use kernel::{
    ContainerSnapshot, GraphEvent, GraphEventKind, Kernel, LeafLayout, NodeChild, NodeId, NodeKind,
    SessionId,
};
pub use pty::{Discipline, Winsize};
pub use ring::ByteRing;
pub use session::{HostIdentity, InputDelivery, IoEvent, Session};
