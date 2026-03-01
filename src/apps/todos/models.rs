use serde::{Deserialize, Serialize};

/// todos data model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Todos {
    pub id: u64,
    // Add your fields here
}
