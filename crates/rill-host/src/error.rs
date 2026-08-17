use std::fmt;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Attach(rill_attach::Error),
    Chip(rill_chip0::Error),
    Dead,
    Refused,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "host io: {e}"),
            Self::Attach(e) => write!(f, "{e}"),
            Self::Chip(e) => write!(f, "{e}"),
            Self::Dead => write!(f, "pane is dead"),
            Self::Refused => write!(f, "attach refused"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Attach(e) => Some(e),
            Self::Chip(e) => Some(e),
            Self::Dead | Self::Refused => None,
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

impl From<rill_chip0::Error> for Error {
    fn from(value: rill_chip0::Error) -> Self {
        Self::Chip(value)
    }
}
