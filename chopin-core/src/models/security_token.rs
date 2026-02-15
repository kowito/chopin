use chrono::NaiveDateTime;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Security token for password reset, email verification, etc.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "security_tokens")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    /// The user this token belongs to
    pub user_id: i32,

    /// Hash of the token value
    #[sea_orm(unique)]
    pub token_hash: String,

    /// Token purpose: "password_reset", "email_verification"
    pub token_type: String,

    /// When the token expires
    pub expires_at: NaiveDateTime,

    /// Whether the token has been used
    #[sea_orm(default_value = false)]
    pub used: bool,

    pub created_at: NaiveDateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
