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
