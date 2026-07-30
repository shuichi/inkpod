//! Core error contract and dependency error conversions.

use super::*;

#[derive(Clone, Debug, Eq, PartialEq)]
/// Recoverable error returned by public Core operations.
///
/// Public operations do not panic for invalid caller input. An error means that
/// no partial document, history, revision, dirty, or savepoint change was committed.
pub enum CoreError {
    /// The operation requires an open document.
    NoDocument,
    /// A supplied value, ID, range, or combination is invalid.
    InvalidArgument(&'static str),
    /// Current Core state does not permit the requested operation.
    InvalidState(&'static str),
    /// A raster allocation or pixel operation failed.
    Raster(RasterError),
    /// A bounded fill operation failed.
    Fill(FillError),
    /// A fill configured to abort at the image edge overflowed.
    FillOverflow {
        /// Edge x-coordinate in document pixels.
        x: u32,
        /// Edge y-coordinate in document pixels.
        y: u32,
    },
    /// Cancellation was observed before the transaction commit point.
    Cancelled,
    /// A sequence switch was refused because the current document is dirty.
    UnsavedChanges,
    /// Native-format validation, decode, or encode failed.
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
