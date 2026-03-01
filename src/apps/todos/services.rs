use super::errors::TodosError;
use super::models::Todos;

/// List all todoss.
pub async fn list() -> Result<Vec<Todos>, TodosError> {
    // TODO: implement database query
    Ok(vec![])
}

/// Get a single todos by ID.
pub async fn get_by_id(id: u64) -> Result<Todos, TodosError> {
    // TODO: implement database query
    Err(TodosError::NotFound(id))
}
