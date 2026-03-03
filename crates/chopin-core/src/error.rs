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
    /// System clock returned an invalid time.
    ClockError,
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
            ChopinError::ClockError => write!(f, "System clock went backwards"),
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

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Display ──────────────────────────────────────────────────────────────

    #[test]
    fn test_display_io_error() {
        let e = ChopinError::Io(std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe"));
        let s = format!("{}", e);
        assert!(s.starts_with("I/O error:"), "unexpected: {}", s);
    }

    #[test]
    fn test_display_slab_full() {
        let s = format!("{}", ChopinError::SlabFull);
        assert!(s.contains("full"), "unexpected: {}", s);
    }

    #[test]
    fn test_display_clock_error() {
        let s = format!("{}", ChopinError::ClockError);
        assert!(
            s.contains("clock") || s.contains("clock"),
            "unexpected: {}",
            s
        );
    }

    #[test]
    fn test_display_worker_panic() {
        let e = ChopinError::WorkerPanic("oops".to_string());
        let s = format!("{}", e);
        assert!(s.contains("oops"), "unexpected: {}", s);
    }

    #[test]
    fn test_display_other() {
        let e = ChopinError::Other("custom msg".to_string());
        let s = format!("{}", e);
        assert!(s.contains("custom msg"), "unexpected: {}", s);
    }

    // ─── Error::source() ─────────────────────────────────────────────────────

    #[test]
    fn test_source_io_is_some() {
        use std::error::Error;
        let e = ChopinError::Io(std::io::Error::other("x"));
        assert!(e.source().is_some());
    }

    #[test]
    fn test_source_non_io_is_none() {
        use std::error::Error;
        assert!(ChopinError::SlabFull.source().is_none());
        assert!(ChopinError::ClockError.source().is_none());
        assert!(ChopinError::WorkerPanic("x".into()).source().is_none());
        assert!(ChopinError::Other("x".into()).source().is_none());
    }

    // ─── From conversions ────────────────────────────────────────────────────

    #[test]
    fn test_from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionRefused, "refused");
        let e: ChopinError = io_err.into();
        assert!(matches!(e, ChopinError::Io(_)));
    }

    #[test]
    fn test_chopin_result_ok() {
        let r: ChopinResult<i32> = Ok(42);
        assert!(r.is_ok());
    }

    #[test]
    fn test_chopin_result_err() {
        let r: ChopinResult<i32> = Err(ChopinError::SlabFull);
        assert!(r.is_err());
    }

    // ─── Debug ───────────────────────────────────────────────────────────────

    #[test]
    fn test_debug_does_not_panic() {
        let _ = format!("{:?}", ChopinError::SlabFull);
        let _ = format!("{:?}", ChopinError::ClockError);
        let _ = format!("{:?}", ChopinError::Other("test".into()));
        let _ = format!("{:?}", ChopinError::WorkerPanic("w".into()));
    }
}
