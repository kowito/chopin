use chrono::NaiveDateTime;
use sea_orm::entity::prelude::*;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

/// Permission entity — represents a granular permission that can be assigned to roles.
///
/// Permissions are identified by their `codename` (e.g., `"can_edit_posts"`, `"can_delete_users"`).
/// They are assigned to roles via the `role_permissions` junction table.
#[derive(Clone, Debug, PartialEq, Eq, DeriveEntityModel, Serialize, Deserialize, ToSchema)]
#[sea_orm(table_name = "permissions")]
pub struct Model {
    #[sea_orm(primary_key)]
    pub id: i32,

    /// Unique machine-readable identifier (e.g., "can_edit_posts")
    #[sea_orm(unique)]
    pub codename: String,

    /// Human-readable name (e.g., "Can Edit Posts")
    pub name: String,

    /// Optional description
    pub description: Option<String>,

    pub created_at: NaiveDateTime,
}

#[derive(Copy, Clone, Debug, EnumIter, DeriveRelation)]
pub enum Relation {
    #[sea_orm(has_many = "super::role_permission::Entity")]
    RolePermissions,
}

impl Related<super::role_permission::Entity> for Entity {
    fn to() -> RelationDef {
        Relation::RolePermissions.def()
    }
}

impl ActiveModelBehavior for ActiveModel {}
