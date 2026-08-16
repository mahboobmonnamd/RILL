//! Kernel plane: PTY master, sole writer, bounded byte history.
//!
//! This crate must not paint and must not ship JSON cells. The GUI never
//! receives the master fd — readiness is exposed as [`Session::wait_readable`],
//! not as a descriptor (SPEC-KERNEL §1).

mod error;
mod pty;
mod ring;
mod session;

pub use error::Error;
pub use pty::{Discipline, Winsize};
pub use ring::ByteRing;
pub use session::{IoEvent, Session};
