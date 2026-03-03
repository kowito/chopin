/// Errors returned by chopin-pg operations.
#[derive(Debug)]
pub enum PgError {
    /// I/O error from the underlying socket.
    Io(std::io::Error),
    /// Protocol violation or unexpected message from server.
    Protocol(String),
    /// Authentication failure.
    Auth(String),
    /// Server-sent error response with rich diagnostic fields.
    Server(Box<ServerError>),
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
    /// I/O operation timed out (application-level timeout).
    Timeout,
    /// Pool: timed out waiting for a connection.
    PoolTimeout,
    /// Pool: all connections are in use.
    PoolExhausted,
    /// Pool: connection failed validation.
    PoolValidationFailed,
}

/// Server-sent error response with rich diagnostic fields.
#[derive(Debug)]
pub struct ServerError {
    pub severity: String,
    pub code: String,
    pub message: String,
    pub detail: Option<String>,
    pub hint: Option<String>,
    pub position: Option<i32>,
    pub internal_position: Option<i32>,
    pub internal_query: Option<String>,
    pub where_: Option<String>,
    pub schema_name: Option<String>,
    pub table_name: Option<String>,
    pub column_name: Option<String>,
    pub data_type_name: Option<String>,
    pub constraint_name: Option<String>,
    pub file: Option<String>,
    pub line: Option<String>,
    pub routine: Option<String>,
}

/// Error classification for retry logic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorClass {
    /// Transient error — safe to retry (deadlock, serialization failure, connection reset).
    Transient,
    /// Permanent error — do not retry (syntax error, permission denied).
    Permanent,
    /// Client-side error (invalid parameters, type conversion).
    Client,
    /// Pool-related error (timeout, exhaustion).
    Pool,
}

impl PgError {
    /// Classify this error for retry decisions.
    pub fn classify(&self) -> ErrorClass {
        match self {
            PgError::Io(_) | PgError::ConnectionClosed | PgError::Timeout => ErrorClass::Transient,
            // WouldBlock is a flow-control signal, not a transient failure.
            // It should not trigger retry with backoff.
            PgError::WouldBlock => ErrorClass::Client,
            PgError::Server(err) => classify_sql_state(&err.code),
            PgError::PoolTimeout | PgError::PoolExhausted | PgError::PoolValidationFailed => {
                ErrorClass::Pool
            }
            PgError::TypeConversion(_)
            | PgError::BufferOverflow
            | PgError::StatementNotCached
            | PgError::NoRows => ErrorClass::Client,
            PgError::Protocol(_) | PgError::Auth(_) => ErrorClass::Permanent,
        }
    }

    /// Returns true if this error is transient and the operation can be retried.
    pub fn is_transient(&self) -> bool {
        self.classify() == ErrorClass::Transient
    }

    /// Get the SQLSTATE code, if this is a server error.
    pub fn sql_state(&self) -> Option<&str> {
        match self {
            PgError::Server(err) => Some(&err.code),
            _ => None,
        }
    }

    /// Get the hint from the server, if available.
    pub fn hint(&self) -> Option<&str> {
        match self {
            PgError::Server(err) => err.hint.as_deref(),
            _ => None,
        }
    }

    /// Get the detail from the server, if available.
    pub fn detail(&self) -> Option<&str> {
        match self {
            PgError::Server(err) => err.detail.as_deref(),
            _ => None,
        }
    }

    /// Build a Server error from parsed error/notice fields.
    pub fn from_fields(fields: &[(u8, String)]) -> Self {
        let mut severity = String::new();
        let mut code = String::new();
        let mut message = String::new();
        let mut detail = None;
        let mut hint = None;
        let mut position = None;
        let mut internal_position = None;
        let mut internal_query = None;
        let mut where_ = None;
        let mut schema_name = None;
        let mut table_name = None;
        let mut column_name = None;
        let mut data_type_name = None;
        let mut constraint_name = None;
        let mut file = None;
        let mut line = None;
        let mut routine = None;

        for (field_type, value) in fields {
            match field_type {
                b'S' => severity = value.clone(),
                b'C' => code = value.clone(),
                b'M' => message = value.clone(),
                b'D' => detail = Some(value.clone()),
                b'H' => hint = Some(value.clone()),
                b'P' => position = value.parse().ok(),
                b'p' => internal_position = value.parse().ok(),
                b'q' => internal_query = Some(value.clone()),
                b'W' => where_ = Some(value.clone()),
                b's' => schema_name = Some(value.clone()),
                b't' => table_name = Some(value.clone()),
                b'c' => column_name = Some(value.clone()),
                b'd' => data_type_name = Some(value.clone()),
                b'n' => constraint_name = Some(value.clone()),
                b'F' => file = Some(value.clone()),
                b'L' => line = Some(value.clone()),
                b'R' => routine = Some(value.clone()),
                _ => {}
            }
        }

        PgError::Server(Box::new(ServerError {
            severity,
            code,
            message,
            detail,
            hint,
            position,
            internal_position,
            internal_query,
            where_,
            schema_name,
            table_name,
            column_name,
            data_type_name,
            constraint_name,
            file,
            line,
            routine,
        }))
    }
}

/// Classify a SQLSTATE code.
fn classify_sql_state(code: &str) -> ErrorClass {
    match code {
        // Class 40 — Transaction Rollback (deadlock, serialization failure)
        c if c.starts_with("40") => ErrorClass::Transient,
        // Class 08 — Connection Exception
        c if c.starts_with("08") => ErrorClass::Transient,
        // Class 53 — Insufficient Resources
        c if c.starts_with("53") => ErrorClass::Transient,
        // Class 57 — Operator Intervention (crash recovery, etc.)
        c if c.starts_with("57") => ErrorClass::Transient,
        // Class 42 — Syntax Error / Access Rule Violation
        c if c.starts_with("42") => ErrorClass::Permanent,
        // Class 23 — Integrity Constraint Violation
        c if c.starts_with("23") => ErrorClass::Permanent,
        // Class 28 — Invalid Authorization
        c if c.starts_with("28") => ErrorClass::Permanent,
        // Default to permanent
        _ => ErrorClass::Permanent,
    }
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
            PgError::Server(err) => {
                write!(f, "PG {}: {} ({})", err.severity, err.message, err.code)?;
                if let Some(d) = &err.detail {
                    write!(f, "\n  Detail: {}", d)?;
                }
                if let Some(h) = &err.hint {
                    write!(f, "\n  Hint: {}", h)?;
                }
                Ok(())
            }
            PgError::ConnectionClosed => write!(f, "Connection closed"),
            PgError::NoRows => write!(f, "No rows returned"),
            PgError::TypeConversion(msg) => write!(f, "Type conversion: {}", msg),
            PgError::StatementNotCached => write!(f, "Statement not in cache"),
            PgError::BufferOverflow => write!(f, "Buffer overflow"),
            PgError::WouldBlock => write!(f, "Would block"),
            PgError::Timeout => write!(f, "I/O operation timed out"),
            PgError::PoolTimeout => write!(f, "Pool: connection checkout timed out"),
            PgError::PoolExhausted => write!(f, "Pool: all connections are in use"),
            PgError::PoolValidationFailed => write!(f, "Pool: connection failed validation"),
        }
    }
}

impl std::error::Error for PgError {}

pub type PgResult<T> = Result<T, PgError>;

/// Retry helper: executes an operation with exponential backoff on transient errors.
pub fn retry<F, T>(max_retries: u32, mut f: F) -> PgResult<T>
where
    F: FnMut() -> PgResult<T>,
{
    let mut attempts = 0;
    loop {
        match f() {
            Ok(val) => return Ok(val),
            Err(e) if e.is_transient() && attempts < max_retries => {
                attempts += 1;
                // Exponential backoff: 1ms, 2ms, 4ms, 8ms, ...
                let delay = std::time::Duration::from_millis(1 << attempts.min(10));
                std::thread::sleep(delay);
            }
            Err(e) => return Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Variant Classification ───────────────────────────────────────────────

    #[test]
    fn test_io_error_is_transient() {
        let e = PgError::Io(std::io::Error::new(
            std::io::ErrorKind::BrokenPipe,
            "broken pipe",
        ));
        assert_eq!(e.classify(), ErrorClass::Transient);
        assert!(e.is_transient());
    }

    #[test]
    fn test_connection_closed_is_transient() {
        assert_eq!(PgError::ConnectionClosed.classify(), ErrorClass::Transient);
        assert!(PgError::ConnectionClosed.is_transient());
    }

    #[test]
    fn test_timeout_is_transient() {
        assert_eq!(PgError::Timeout.classify(), ErrorClass::Transient);
        assert!(PgError::Timeout.is_transient());
    }

    // ─── WouldBlock: must be Client, not Transient ────────────────────────────
    // If WouldBlock were Transient, retry() would sleep-loop forever on non-blocking I/O.

    #[test]
    fn test_wouldblock_is_client_not_transient() {
        assert_eq!(PgError::WouldBlock.classify(), ErrorClass::Client);
        assert!(!PgError::WouldBlock.is_transient());
    }

    #[test]
    fn test_type_conversion_is_client() {
        assert_eq!(
            PgError::TypeConversion("bad".to_string()).classify(),
            ErrorClass::Client
        );
        assert!(!PgError::TypeConversion("bad".to_string()).is_transient());
    }

    #[test]
    fn test_buffer_overflow_is_client() {
        assert_eq!(PgError::BufferOverflow.classify(), ErrorClass::Client);
    }

    #[test]
    fn test_no_rows_is_client() {
        assert_eq!(PgError::NoRows.classify(), ErrorClass::Client);
    }

    #[test]
    fn test_statement_not_cached_is_client() {
        assert_eq!(PgError::StatementNotCached.classify(), ErrorClass::Client);
    }

    #[test]
    fn test_pool_timeout_is_pool_class() {
        assert_eq!(PgError::PoolTimeout.classify(), ErrorClass::Pool);
        assert!(!PgError::PoolTimeout.is_transient());
    }

    #[test]
    fn test_pool_exhausted_is_pool_class() {
        assert_eq!(PgError::PoolExhausted.classify(), ErrorClass::Pool);
        assert!(!PgError::PoolExhausted.is_transient());
    }

    #[test]
    fn test_pool_validation_failed_is_pool_class() {
        assert_eq!(PgError::PoolValidationFailed.classify(), ErrorClass::Pool);
        assert!(!PgError::PoolValidationFailed.is_transient());
    }

    #[test]
    fn test_protocol_error_is_permanent() {
        assert_eq!(
            PgError::Protocol("bad".to_string()).classify(),
            ErrorClass::Permanent
        );
        assert!(!PgError::Protocol("bad".to_string()).is_transient());
    }

    #[test]
    fn test_auth_error_is_permanent() {
        assert_eq!(
            PgError::Auth("denied".to_string()).classify(),
            ErrorClass::Permanent
        );
        assert!(!PgError::Auth("denied".to_string()).is_transient());
    }

    // ─── SQLSTATE Classification ──────────────────────────────────────────────

    fn server_err(code: &str) -> PgError {
        PgError::Server(Box::new(ServerError {
            severity: "ERROR".to_string(),
            code: code.to_string(),
            message: "test".to_string(),
            detail: None,
            hint: None,
            position: None,
            internal_position: None,
            internal_query: None,
            where_: None,
            schema_name: None,
            table_name: None,
            column_name: None,
            data_type_name: None,
            constraint_name: None,
            file: None,
            line: None,
            routine: None,
        }))
    }

    #[test]
    fn test_sqlstate_40001_serialization_failure_transient() {
        assert!(server_err("40001").is_transient());
    }

    #[test]
    fn test_sqlstate_40p01_deadlock_transient() {
        assert!(server_err("40P01").is_transient());
    }

    #[test]
    fn test_sqlstate_08006_connection_failure_transient() {
        assert!(server_err("08006").is_transient());
    }

    #[test]
    fn test_sqlstate_53300_too_many_connections_transient() {
        // Class 53 = Insufficient Resources
        assert!(server_err("53300").is_transient());
    }

    #[test]
    fn test_sqlstate_57014_query_canceled_transient() {
        // Class 57 = Operator Intervention
        assert!(server_err("57014").is_transient());
    }

    #[test]
    fn test_sqlstate_42601_syntax_error_permanent() {
        assert_eq!(server_err("42601").classify(), ErrorClass::Permanent);
        assert!(!server_err("42601").is_transient());
    }

    #[test]
    fn test_sqlstate_23505_unique_violation_permanent() {
        assert_eq!(server_err("23505").classify(), ErrorClass::Permanent);
    }

    #[test]
    fn test_sqlstate_28000_invalid_authorization_permanent() {
        assert_eq!(server_err("28000").classify(), ErrorClass::Permanent);
    }

    #[test]
    fn test_sqlstate_unknown_default_permanent() {
        // Unknown codes default to Permanent
        assert_eq!(server_err("99999").classify(), ErrorClass::Permanent);
    }

    // ─── sql_state() Accessor ─────────────────────────────────────────────────

    #[test]
    fn test_sql_state_returns_code() {
        assert_eq!(server_err("42601").sql_state(), Some("42601"));
    }

    #[test]
    fn test_sql_state_non_server_is_none() {
        assert_eq!(PgError::WouldBlock.sql_state(), None);
        assert_eq!(PgError::Timeout.sql_state(), None);
        assert_eq!(PgError::PoolExhausted.sql_state(), None);
        assert_eq!(PgError::ConnectionClosed.sql_state(), None);
    }

    // ─── from_fields() ────────────────────────────────────────────────────────

    #[test]
    fn test_from_fields_complete() {
        let fields = vec![
            (b'S', "ERROR".to_string()),
            (b'C', "42601".to_string()),
            (b'M', "syntax error at position 5".to_string()),
            (b'D', "near SELECT".to_string()),
            (b'H', "check your query".to_string()),
            (b'P', "5".to_string()),
            (b's', "public".to_string()),
            (b't', "users".to_string()),
            (b'n', "users_pkey".to_string()),
        ];
        let e = PgError::from_fields(&fields);
        if let PgError::Server(err) = e {
            assert_eq!(err.severity, "ERROR");
            assert_eq!(err.code, "42601");
            assert_eq!(err.message, "syntax error at position 5");
            assert_eq!(err.detail, Some("near SELECT".to_string()));
            assert_eq!(err.hint, Some("check your query".to_string()));
            assert_eq!(err.position, Some(5));
            assert_eq!(err.schema_name, Some("public".to_string()));
            assert_eq!(err.table_name, Some("users".to_string()));
            assert_eq!(err.constraint_name, Some("users_pkey".to_string()));
        } else {
            panic!("Expected Server variant");
        }
    }

    #[test]
    fn test_from_fields_minimal() {
        let fields = vec![
            (b'S', "ERROR".to_string()),
            (b'C', "99999".to_string()),
            (b'M', "unknown error".to_string()),
        ];
        let e = PgError::from_fields(&fields);
        if let PgError::Server(err) = e {
            assert!(err.detail.is_none());
            assert!(err.hint.is_none());
            assert!(err.position.is_none());
        } else {
            panic!("Expected Server variant");
        }
    }

    #[test]
    fn test_from_fields_unknown_field_ignored() {
        // Unknown field byte 'Z' should be silently ignored
        let fields = vec![
            (b'S', "ERROR".to_string()),
            (b'C', "00000".to_string()),
            (b'M', "ok".to_string()),
            (b'Z', "ignored".to_string()),
        ];
        let e = PgError::from_fields(&fields);
        assert!(matches!(e, PgError::Server(_)));
    }

    // ─── Display Format ───────────────────────────────────────────────────────

    #[test]
    fn test_display_server_includes_message_code_detail() {
        let e = PgError::Server(Box::new(ServerError {
            severity: "ERROR".to_string(),
            code: "42601".to_string(),
            message: "syntax error here".to_string(),
            detail: Some("bad token".to_string()),
            hint: None,
            position: None,
            internal_position: None,
            internal_query: None,
            where_: None,
            schema_name: None,
            table_name: None,
            column_name: None,
            data_type_name: None,
            constraint_name: None,
            file: None,
            line: None,
            routine: None,
        }));
        let s = format!("{}", e);
        assert!(s.contains("syntax error here"), "missing message: {}", s);
        assert!(s.contains("42601"), "missing code: {}", s);
        assert!(s.contains("bad token"), "missing detail: {}", s);
    }

    #[test]
    fn test_display_server_no_detail_or_hint() {
        let e = server_err("42601");
        let s = format!("{}", e);
        // Just confirms Display works without panicking
        assert!(!s.is_empty());
    }

    #[test]
    fn test_display_all_non_server_variants() {
        // Ensure Display is implemented and doesn't panic for every variant
        let _ = format!("{}", PgError::ConnectionClosed);
        let _ = format!("{}", PgError::NoRows);
        let _ = format!("{}", PgError::BufferOverflow);
        let _ = format!("{}", PgError::WouldBlock);
        let _ = format!("{}", PgError::Timeout);
        let _ = format!("{}", PgError::PoolTimeout);
        let _ = format!("{}", PgError::PoolExhausted);
        let _ = format!("{}", PgError::PoolValidationFailed);
        let _ = format!("{}", PgError::StatementNotCached);
        let _ = format!("{}", PgError::TypeConversion("type error".to_string()));
        let _ = format!("{}", PgError::Protocol("protocol error".to_string()));
        let _ = format!("{}", PgError::Auth("auth error".to_string()));
    }

    // ─── From<io::Error> ─────────────────────────────────────────────────────

    #[test]
    fn test_from_io_wouldblock_becomes_wouldblock() {
        let io_err = std::io::Error::new(std::io::ErrorKind::WouldBlock, "would block");
        let pg_err = PgError::from(io_err);
        assert!(matches!(pg_err, PgError::WouldBlock));
    }

    #[test]
    fn test_from_io_other_becomes_io_variant() {
        let io_err = std::io::Error::new(std::io::ErrorKind::ConnectionReset, "reset");
        let pg_err = PgError::from(io_err);
        assert!(matches!(pg_err, PgError::Io(_)));
    }

    #[test]
    fn test_from_io_broken_pipe_is_not_wouldblock() {
        let io_err = std::io::Error::new(std::io::ErrorKind::BrokenPipe, "pipe");
        let pg_err = PgError::from(io_err);
        assert!(!matches!(pg_err, PgError::WouldBlock));
    }

    // ─── retry() ─────────────────────────────────────────────────────────────

    #[test]
    fn test_retry_succeeds_immediately() {
        let result = retry(3, || Ok::<i32, PgError>(42));
        assert_eq!(result.unwrap(), 42);
    }

    #[test]
    fn test_retry_no_retries_on_success() {
        let mut calls = 0;
        let result = retry(3, || {
            calls += 1;
            Ok::<i32, PgError>(1)
        });
        assert_eq!(result.unwrap(), 1);
        assert_eq!(calls, 1);
    }

    #[test]
    fn test_retry_permanent_error_not_retried() {
        // Protocol error must NOT trigger retry — ensures retry() doesn't waste time
        let mut calls = 0;
        let result = retry(5, || {
            calls += 1;
            Err::<i32, PgError>(PgError::Protocol("bad".to_string()))
        });
        assert!(result.is_err());
        assert_eq!(calls, 1, "Permanent errors must not be retried");
    }

    #[test]
    fn test_retry_client_error_not_retried() {
        // WouldBlock must NOT trigger retry (regression test)
        let mut calls = 0;
        let result = retry(5, || {
            calls += 1;
            Err::<i32, PgError>(PgError::WouldBlock)
        });
        assert!(result.is_err());
        assert_eq!(calls, 1, "WouldBlock must not be retried");
    }

    #[test]
    fn test_retry_zero_max_retries_no_sleep_no_retry() {
        let mut calls = 0;
        let result = retry(0, || {
            calls += 1;
            Err::<i32, PgError>(PgError::Io(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                "reset",
            )))
        });
        assert!(result.is_err());
        assert_eq!(calls, 1);
    }

    #[test]
    fn test_retry_transient_error_retried_up_to_limit() {
        let mut calls = 0;
        let result = retry(2, || {
            calls += 1;
            Err::<i32, PgError>(PgError::Io(std::io::Error::new(
                std::io::ErrorKind::ConnectionReset,
                "reset",
            )))
        });
        assert!(result.is_err());
        // 1 initial + 2 retries = 3 total
        assert_eq!(calls, 3);
    }

    #[test]
    fn test_retry_succeeds_on_second_attempt() {
        let mut calls = 0;
        let result = retry(3, || {
            calls += 1;
            if calls < 2 {
                Err(PgError::Io(std::io::Error::new(
                    std::io::ErrorKind::ConnectionReset,
                    "reset",
                )))
            } else {
                Ok::<i32, PgError>(99)
            }
        });
        assert_eq!(result.unwrap(), 99);
        assert_eq!(calls, 2);
    }

    #[test]
    fn test_pool_errors_not_retried() {
        let mut calls = 0;
        let _ = retry(5, || {
            calls += 1;
            Err::<(), PgError>(PgError::PoolTimeout)
        });
        assert_eq!(calls, 1, "PoolTimeout must not be retried");

        calls = 0;
        let _ = retry(5, || {
            calls += 1;
            Err::<(), PgError>(PgError::PoolExhausted)
        });
        assert_eq!(calls, 1, "PoolExhausted must not be retried");

        calls = 0;
        let _ = retry(5, || {
            calls += 1;
            Err::<(), PgError>(PgError::PoolValidationFailed)
        });
        assert_eq!(calls, 1, "PoolValidationFailed must not be retried");
    }
}
