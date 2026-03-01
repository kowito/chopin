use thiserror::Error;

#[derive(Error, Debug)]
pub enum TodosError {
    #[error("Todos not found: {0}")]
    NotFound(u64),
    #[error("Database error")]
    Db(#[from] chopin_pg::PgError),
}
