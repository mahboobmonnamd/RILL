use std::fmt;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Attach(rill_attach::Error),
    Chip(rill_vt_types::Error),
    Dead,
    Refused,
    InvalidHostIdentity,
    InvalidContent,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "host io: {e}"),
            Self::Attach(e) => write!(f, "{e}"),
            Self::Chip(e) => write!(f, "{e}"),
            Self::Dead => write!(f, "pane is dead"),
            Self::Refused => write!(f, "attach refused"),
            Self::InvalidHostIdentity => write!(f, "invalid cold kernel host identity"),
            Self::InvalidContent => write!(f, "invalid cold content response"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Attach(e) => Some(e),
            Self::Chip(e) => Some(e),
            Self::Dead | Self::Refused | Self::InvalidHostIdentity | Self::InvalidContent => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<rill_attach::Error> for Error {
    fn from(value: rill_attach::Error) -> Self {
        Self::Attach(value)
    }
}

impl From<rill_vt_types::Error> for Error {
    fn from(value: rill_vt_types::Error) -> Self {
        Self::Chip(value)
    }
}
