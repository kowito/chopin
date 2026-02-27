use crate::parser::ParseError;
use std::io;

/// Central error type for the Chopin core engine.
#[derive(Debug)]
pub enum ChopinError {
    /// Underlying I/O error from the OS or network.
    Io(io::Error),
    /// Error during HTTP request parsing.
    Parse(ParseError),
    /// Slab allocator reached its maximum capacity.
    SlabFull,
    /// A background worker or task panicked.
    WorkerPanic(String),
    /// Generic or miscellaneous error.
    Other(String),
}

impl std::fmt::Display for ChopinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChopinError::Io(e) => write!(f, "I/O error: {}", e),
            ChopinError::Parse(e) => write!(f, "Parse error: {:?}", e),
            ChopinError::SlabFull => write!(f, "Connection slab is full"),
            ChopinError::WorkerPanic(msg) => write!(f, "Worker panic: {}", msg),
            ChopinError::Other(msg) => write!(f, "Error: {}", msg),
        }
    }
}

impl std::error::Error for ChopinError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            ChopinError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for ChopinError {
    fn from(e: io::Error) -> Self {
        ChopinError::Io(e)
    }
}

impl From<ParseError> for ChopinError {
    fn from(e: ParseError) -> Self {
        ChopinError::Parse(e)
    }
}

pub type ChopinResult<T> = Result<T, ChopinError>;
