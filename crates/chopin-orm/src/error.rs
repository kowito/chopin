use chopin_pg::error::PgError;

/// Error type for the Chopin ORM.
#[derive(Debug)]
pub enum OrmError {
    /// Error from the underlying PostgreSQL driver.
    Database(PgError),
    /// No records were found for a query that expected at least one.
    RecordNotFound,
    /// Multiple records were found for a query that expected exactly one.
    MultipleRecordsFound,
    /// Error during data extraction or type conversion.
    Extraction(String),
    /// Model-specific validation or configuration error.
    ModelError(String),
}

impl std::fmt::Display for OrmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrmError::Database(e) => write!(f, "Database error: {}", e),
            OrmError::RecordNotFound => write!(f, "Record not found"),
            OrmError::MultipleRecordsFound => write!(f, "Multiple records found"),
            OrmError::Extraction(msg) => write!(f, "Extraction error: {}", msg),
            OrmError::ModelError(msg) => write!(f, "Model error: {}", msg),
        }
    }
}

impl std::error::Error for OrmError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            OrmError::Database(e) => Some(e),
            _ => None,
        }
    }
}

impl From<PgError> for OrmError {
    fn from(e: PgError) -> Self {
        OrmError::Database(e)
    }
}

pub type OrmResult<T> = Result<T, OrmError>;

#[cfg(test)]
mod tests {
    use super::*;
    use chopin_pg::error::PgError;

    // ─── Display ─────────────────────────────────────────────────────────────

    #[test]
    fn test_display_record_not_found() {
        let s = OrmError::RecordNotFound.to_string();
        assert!(
            s.contains("not found") || s.contains("Record"),
            "unexpected: {}",
            s
        );
    }

    #[test]
    fn test_display_multiple_records_found() {
        let s = OrmError::MultipleRecordsFound.to_string();
        assert!(
            s.contains("Multiple") || s.contains("multiple"),
            "unexpected: {}",
            s
        );
    }

    #[test]
    fn test_display_extraction() {
        let s = OrmError::Extraction("bad type".to_string()).to_string();
        assert!(s.contains("bad type"), "unexpected: {}", s);
    }

    #[test]
    fn test_display_model_error() {
        let s = OrmError::ModelError("invalid field".to_string()).to_string();
        assert!(s.contains("invalid field"), "unexpected: {}", s);
    }

    #[test]
    fn test_display_database() {
        let pg_err = PgError::Protocol("test error".to_string());
        let s = OrmError::Database(pg_err).to_string();
        assert!(
            s.contains("Database") || s.contains("test error"),
            "unexpected: {}",
            s
        );
    }

    // ─── Error::source() ─────────────────────────────────────────────────────

    #[test]
    fn test_source_database_is_some() {
        use std::error::Error;
        let e = OrmError::Database(PgError::Protocol("x".to_string()));
        assert!(e.source().is_some());
    }

    #[test]
    fn test_source_non_database_is_none() {
        use std::error::Error;
        assert!(OrmError::RecordNotFound.source().is_none());
        assert!(OrmError::MultipleRecordsFound.source().is_none());
        assert!(OrmError::Extraction("e".into()).source().is_none());
        assert!(OrmError::ModelError("m".into()).source().is_none());
    }

    // ─── From<PgError> ───────────────────────────────────────────────────────

    #[test]
    fn test_from_pgerror() {
        let pg_err = PgError::Protocol("from-test".to_string());
        let orm_err: OrmError = pg_err.into();
        assert!(matches!(orm_err, OrmError::Database(_)));
    }

    // ─── Debug ───────────────────────────────────────────────────────────────

    #[test]
    fn test_debug_does_not_panic() {
        let _ = format!("{:?}", OrmError::RecordNotFound);
        let _ = format!("{:?}", OrmError::MultipleRecordsFound);
        let _ = format!("{:?}", OrmError::Extraction("e".into()));
        let _ = format!("{:?}", OrmError::ModelError("m".into()));
        let _ = format!("{:?}", OrmError::Database(PgError::Protocol("x".into())));
    }

    // ─── OrmResult type alias ─────────────────────────────────────────────────

    #[test]
    fn test_orm_result_ok() {
        let r: OrmResult<i32> = Ok(7);
        assert!(matches!(r, Ok(7)));
    }

    #[test]
    fn test_orm_result_err() {
        let r: OrmResult<i32> = Err(OrmError::RecordNotFound);
        assert!(r.is_err());
    }
}
