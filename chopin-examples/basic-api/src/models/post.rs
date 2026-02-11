use chrono::NaiveDateTime;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Post entity for the example application.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize, ToSchema)]
#[sea_orm(table_name = "posts")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    pub title: String,
    pub body: String,
    pub author_id: i32,

    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

/// Public post response.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct PostResponse {
    pub id: i32,
    pub title: String,
    pub body: String,
    pub author_id: i32,
    pub created_at: NaiveDateTime,
}

impl From<Model> for PostResponse {
    fn from(post: Model) -> Self {
        PostResponse {
            id: post.id,
            title: post.title,
            body: post.body,
            author_id: post.author_id,
            created_at: post.created_at,
        }
    }
}
