use std::fmt;
use rill_vt_types::Error as VtError;

#[derive(Debug)]
pub enum Error {
    Io(std::io::Error),
    Kernel(rill_kernel::Error),
    Attach(rill_attach::Error),
    Chip(VtError),
    AlreadyRunning,
    NestedLaunch,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "rilld io: {e}"),
            Self::Kernel(e) => write!(f, "{e}"),
            Self::Attach(e) => write!(f, "{e}"),
            Self::Chip(e) => write!(f, "{e}"),
            Self::AlreadyRunning => write!(f, "already running"),
            Self::NestedLaunch => write!(f, "nested rilld refused (set RILL_ALLOW_NESTED=1)"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(e) => Some(e),
            Self::Kernel(e) => Some(e),
            Self::Attach(e) => Some(e),
            Self::Chip(e) => Some(e),
            Self::AlreadyRunning | Self::NestedLaunch => None,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value)
    }
}

impl From<rill_kernel::Error> for Error {
    fn from(value: rill_kernel::Error) -> Self {
        Self::Kernel(value)
    }
}

impl From<rill_attach::Error> for Error {
    fn from(value: rill_attach::Error) -> Self {
        Self::Attach(value)
    }
}

impl From<VtError> for Error {
    fn from(value: VtError) -> Self {
        Self::Chip(value)
    }
}
