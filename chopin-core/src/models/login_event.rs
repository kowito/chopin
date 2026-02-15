use chrono::NaiveDateTime;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};

/// Login event for IP/device tracking and audit logging.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize)]
#[sea_orm(table_name = "login_events")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    /// The user who attempted to log in
    pub user_id: Option<i32>,

    /// Email used in the attempt (even if user not found)
    pub email: String,

    /// Whether the login was successful
    pub success: bool,

    /// Failure reason if unsuccessful
    pub failure_reason: Option<String>,

    /// IP address of the request
    pub ip_address: Option<String>,

    /// User-Agent header
    pub user_agent: Option<String>,

    pub created_at: NaiveDateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {}

impl ActiveModelBehavior for ActiveModel {}
