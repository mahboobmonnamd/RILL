use std::fmt;

#[derive(Debug)]
pub enum Error {
    Pty(&'static str),
    Spawn(std::io::Error),
    Io(std::io::Error),
    Dead,
    AttachRefused,
    UnknownSession,
    CwdUnreadable,
    UnsupportedPlatform(&'static str),
    /// ADR 0020 D1: a `NodeId` not held by this kernel's container tree.
    UnknownNode,
    /// Checkpoint format the running kernel does not accept (#313).
    IncompatibleCheckpoint,
    /// Requested offset was discarded by ring eviction.
    CheckpointEvicted,
    /// No checkpoint has been installed on this execution.
    NoCheckpoint,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Pty(s) => write!(f, "pty: {s}"),
            Self::Spawn(e) => write!(f, "spawn: {e}"),
            Self::Io(e) => write!(f, "kernel io: {e}"),
            Self::Dead => write!(f, "child has exited"),
            Self::AttachRefused => write!(f, "second attach refused"),
            Self::UnknownSession => write!(f, "unknown session id"),
            Self::CwdUnreadable => write!(f, "cwd unreadable"),
            Self::UnsupportedPlatform(s) => write!(f, "unsupported platform: {s}"),
            Self::UnknownNode => write!(f, "unknown container node id"),
            Self::IncompatibleCheckpoint => write!(f, "incompatible checkpoint format"),
            Self::CheckpointEvicted => write!(f, "checkpoint offset evicted"),
            Self::NoCheckpoint => write!(f, "no checkpoint installed"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
