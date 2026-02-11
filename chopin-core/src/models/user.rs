use chrono::NaiveDateTime;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// User entity - the built-in user model for authentication.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize, ToSchema)]
#[sea_orm(table_name = "users")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    #[sea_orm(unique)]
    pub email: String,

    #[sea_orm(unique)]
    pub username: String,

    /// Password hash (excluded from serialization via serde skip)
    #[serde(skip_serializing)]
    #[schema(read_only)]
    pub password_hash: String,

    pub is_active: bool,

    pub created_at: NaiveDateTime,
    pub updated_at: NaiveDateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}

/// Public user data (safe to return in API responses).
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct UserResponse {
    pub id: i32,
    pub email: String,
    pub username: String,
    pub is_active: bool,
    pub created_at: NaiveDateTime,
}

impl From<Model> for UserResponse {
    fn from(user: Model) -> Self {
        UserResponse {
            id: user.id,
            email: user.email,
            username: user.username,
            is_active: user.is_active,
            created_at: user.created_at,
        }
    }
}
