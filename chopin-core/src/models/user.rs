use chrono::NaiveDateTime;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use utoipa::ToSchema;

/// User roles for permissions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
pub enum Role {
    #[serde(rename = "user")]
    User,
    #[serde(rename = "admin")]
    Admin,
    #[serde(rename = "superuser")]
    Superuser,
}

impl Role {
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::User => "user",
            Role::Admin => "admin",
            Role::Superuser => "superuser",
        }
    }



    /// Check if this role has at least the given permission level.
    pub fn has_permission(&self, required: &Role) -> bool {
        self.level() >= required.level()
    }

    fn level(&self) -> u8 {
        match self {
            Role::User => 0,
            Role::Admin => 1,
            Role::Superuser => 2,
        }
    }
}

impl FromStr for Role {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "admin" => Ok(Role::Admin),
            "superuser" => Ok(Role::Superuser),
            "user" => Ok(Role::User),
            _ => Ok(Role::User),
        }
    }
}

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

    /// User role: "user", "admin", "superuser"
    #[sea_orm(default_value = "user")]
    pub role: String,

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
    pub role: String,
    pub is_active: bool,
    pub created_at: NaiveDateTime,
}

impl From<Model> for UserResponse {
    fn from(user: Model) -> Self {
        UserResponse {
            id: user.id,
            email: user.email,
            username: user.username,
            role: user.role,
            is_active: user.is_active,
            created_at: user.created_at,
        }
    }
}
