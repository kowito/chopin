/// Errors returned by chopin-pg operations.
#[derive(Debug)]
pub enum PgError {
    /// I/O error from the underlying socket.
    Io(std::io::Error),
    /// Protocol violation or unexpected message from server.
    Protocol(String),
    /// Authentication failure.
    Auth(String),
    /// Server-sent error response (severity, code, message).
    Server {
        severity: String,
        code: String,
        message: String,
    },
    /// Connection is closed or in an invalid state.
    ConnectionClosed,
    /// Query returned no rows when one was expected.
    NoRows,
    /// Type conversion error.
    TypeConversion(String),
    /// Statement not found in cache.
    StatementNotCached,
    /// Buffer overflow — message too large.
    BufferOverflow,
    /// Would block — operation cannot complete without blocking.
    WouldBlock,
}

impl From<std::io::Error> for PgError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::WouldBlock {
            PgError::WouldBlock
        } else {
            PgError::Io(e)
        }
    }
}

impl std::fmt::Display for PgError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PgError::Io(e) => write!(f, "I/O error: {}", e),
            PgError::Protocol(msg) => write!(f, "Protocol error: {}", msg),
            PgError::Auth(msg) => write!(f, "Auth error: {}", msg),
            PgError::Server { severity, code, message } => {
                write!(f, "PG {}: {} ({})", severity, message, code)
            }
            PgError::ConnectionClosed => write!(f, "Connection closed"),
            PgError::NoRows => write!(f, "No rows returned"),
            PgError::TypeConversion(msg) => write!(f, "Type conversion: {}", msg),
            PgError::StatementNotCached => write!(f, "Statement not in cache"),
            PgError::BufferOverflow => write!(f, "Buffer overflow"),
            PgError::WouldBlock => write!(f, "Would block"),
        }
    }
}

impl std::error::Error for PgError {}

pub type PgResult<T> = Result<T, PgError>;
