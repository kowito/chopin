use chrono::NaiveDateTime;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Refresh token entity for JWT token rotation.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "refresh_tokens")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    /// The user who owns this refresh token
    pub user_id: i32,

    /// The opaque refresh token string (SHA-256 hash stored)
    #[sea_orm(unique)]
    pub token_hash: String,

    /// When the token expires
    pub expires_at: NaiveDateTime,

    /// Whether this token has been revoked
    #[sea_orm(default_value = false)]
    pub revoked: bool,

    /// IP address that created this token
    pub ip_address: Option<String>,

    /// User-Agent that created this token
    pub user_agent: Option<String>,

    pub created_at: NaiveDateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
