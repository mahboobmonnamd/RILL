//! Kernel plane: PTY master, sole writer, bounded byte history.
//!
//! This crate must not paint and must not ship JSON cells. The GUI never
//! receives the master fd — readiness is `Kernel::poll_with_extras` (and
//! historically `Session::wait_readable`). The master fd is not a `pub` API
//! (SPEC-KERNEL §1).

mod checkpoint;
mod content;
mod error;
mod journal;
mod kernel;
mod pty;
mod ring;
mod session;
mod transcript;

pub use checkpoint::{StoredCheckpoint, CHECKPOINT_FORMAT_VERSION};
pub use content::{
    CaptureOutcome, ContentError, ContentEvent, ContentKind, ContentTimeline, DurableTranscript,
    EventAvailability, RedactedExport, RedactionRule, ReplayState, RetentionMode, RetentionPolicy,
    ScreenMode, SourceRange,
};
pub use error::Error;
pub use journal::{reconcile_execution, TerminalOutcome};
pub use kernel::{
    ContainerSnapshot, GraphEvent, GraphEventKind, Kernel, LeafLayout, NodeChild, NodeId, NodeKind,
    SessionId,
};
pub use pty::{Discipline, Winsize};
pub use ring::ByteRing;
pub use session::{HostIdentity, InputDelivery, IoEvent, Session};
pub use transcript::{
    EventId, EventPayload, EventProvenance, SemanticEvent, TranscriptError, TranscriptLedger,
};
