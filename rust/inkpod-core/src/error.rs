//! Core error contract and dependency error conversions.

use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CoreError {
    NoDocument,
    InvalidArgument(&'static str),
    InvalidState(&'static str),
    Raster(RasterError),
    Fill(FillError),
    FillOverflow { x: u32, y: u32 },
    Cancelled,
    UnsavedChanges,
    Format(String),
}

impl fmt::Display for CoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoDocument => formatter.write_str("no cell document is open"),
            Self::InvalidArgument(message) => write!(formatter, "invalid argument: {message}"),
            Self::InvalidState(message) => write!(formatter, "invalid state: {message}"),
            Self::Raster(error) => write!(formatter, "raster error: {error}"),
            Self::Fill(error) => write!(formatter, "fill error: {error}"),
            Self::FillOverflow { x, y } => {
                write!(formatter, "fill reached image edge at ({x}, {y})")
            }
            Self::Cancelled => formatter.write_str("operation was cancelled before commit"),
            Self::UnsavedChanges => formatter
                .write_str("the active cell has unsaved changes and cannot be switched silently"),
            Self::Format(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for CoreError {}

impl From<RasterError> for CoreError {
    fn from(error: RasterError) -> Self {
        if error == RasterError::Cancelled {
            Self::Cancelled
        } else {
            Self::Raster(error)
        }
    }
}

impl From<FillError> for CoreError {
    fn from(error: FillError) -> Self {
        match error {
            FillError::Overflow { x, y } => Self::FillOverflow { x, y },
            FillError::Cancelled => Self::Cancelled,
            other => Self::Fill(other),
        }
    }
}

impl From<FormatError> for CoreError {
    fn from(error: FormatError) -> Self {
        if matches!(error, FormatError::Cancelled) {
            Self::Cancelled
        } else {
            Self::Format(error.to_string())
        }
    }
}
