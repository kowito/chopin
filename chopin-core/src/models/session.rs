use chrono::NaiveDateTime;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Active session entity for server-side session management / token blacklist.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "sessions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    /// The user who owns this session
    pub user_id: i32,

    /// Hash of the JWT access token (for blacklisting)
    pub token_hash: String,

    /// When the session expires
    pub expires_at: NaiveDateTime,

    /// Whether the session has been revoked (logout)
    #[sea_orm(default_value = false)]
    pub revoked: bool,

    /// IP address
    pub ip_address: Option<String>,

    /// User-Agent
    pub user_agent: Option<String>,

    pub created_at: NaiveDateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
