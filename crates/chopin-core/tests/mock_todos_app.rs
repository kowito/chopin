#[allow(dead_code)]
pub mod errors {
    use thiserror::Error;

    #[derive(Error, Debug)]
    pub enum TodosError {
        #[error("Todos not found: {0}")]
        NotFound(u64),
    }
}

#[allow(dead_code)]
pub mod models {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Todos {
        pub id: u64,
    }
}

#[allow(dead_code)]
pub mod services {
    use super::errors::TodosError;
    use super::models::Todos;

    pub async fn list() -> Result<Vec<Todos>, TodosError> {
        Ok(vec![])
    }

    pub async fn get_by_id(id: u64) -> Result<Todos, TodosError> {
        Err(TodosError::NotFound(id))
    }
}

#[allow(dead_code)]
pub mod handlers {
    use chopin_core::{Context, Response};
    use chopin_macros::{get, post};

    #[get("/todos")]
    pub fn list(_ctx: Context) -> Response {
        Response::text("list todos")
    }

    #[get("/todos/:id")]
    pub fn get_by_id(_ctx: Context) -> Response {
        Response::text("get todos")
    }

    #[post("/todos")]
    pub fn create(_ctx: Context) -> Response {
        Response::text("create todos")
    }
}
