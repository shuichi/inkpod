use std::fmt;

/// An I/O failure. Paths and file contents are deliberately absent from diagnostics.
#[derive(Debug)]
pub enum IoError {
    Io(std::io::Error),
    Format(inkpod_format::FormatError),
    Cancelled,
    InvalidInput(&'static str),
    LimitExceeded(&'static str),
    ResourceBusy(&'static str),
    ChangedDuringRead,
    Shutdown,
    WorkerPanicked,
    ConfirmationRequired,
}

pub type IoResult<T> = Result<T, IoError>;

impl fmt::Display for IoError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(formatter, "file I/O failed: {error}"),
            Self::Format(error) => write!(formatter, "image decode failed: {error}"),
            Self::Cancelled => formatter.write_str("file I/O was cancelled"),
            Self::InvalidInput(message)
            | Self::LimitExceeded(message)
            | Self::ResourceBusy(message) => formatter.write_str(message),
            Self::ChangedDuringRead => formatter.write_str("file changed while it was being read"),
            Self::Shutdown => formatter.write_str("file I/O manager is shutting down"),
            Self::WorkerPanicked => formatter.write_str("file I/O worker failed"),
            Self::ConfirmationRequired => formatter
                .write_str("file destination changed or overwrite confirmation is required"),
        }
    }
}

impl std::error::Error for IoError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Format(error) => Some(error),
            _ => None,
        }
    }
}

impl From<std::io::Error> for IoError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<inkpod_format::FormatError> for IoError {
    fn from(error: inkpod_format::FormatError) -> Self {
        if matches!(error, inkpod_format::FormatError::Cancelled) {
            Self::Cancelled
        } else {
            Self::Format(error)
        }
    }
}
