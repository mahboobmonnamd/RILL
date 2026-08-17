use std::fmt;

#[derive(Debug)]
pub enum Error {
    Pty(&'static str),
    Spawn(std::io::Error),
    Io(std::io::Error),
    Dead,
    AttachRefused,
    UnknownSession,
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
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}
