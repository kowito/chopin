use chrono::NaiveDateTime;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Post entity — a blog post stored in the `posts` table.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "posts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    pub title: String,

    #[sea_orm(column_type = "Text")]
    pub body: String,

    #[sea_orm(default_value = "false")]
    pub published: bool,

    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

// ─── Response DTO ──────────────────────────────────────────────

/// The JSON representation returned to clients.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PostResponse {
    pub id: i32,
    pub title: String,
    pub body: String,
    pub published: bool,
    pub created_at: String,
    pub updated_at: String,
}

impl From<Model> for PostResponse {
    fn from(m: Model) -> Self {
        PostResponse {
            id: m.id,
            title: m.title,
            body: m.body,
            published: m.published,
            created_at: m.created_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
            updated_at: m.updated_at.format("%Y-%m-%dT%H:%M:%S").to_string(),
        }
    }
}
