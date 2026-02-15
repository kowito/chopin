use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};

use crate::auth::totp::hash_token;
use crate::error::ChopinError;
use crate::models::session;

/// Create a session record for the given access token.
pub async fn create_session(
    db: &DatabaseConnection,
    user_id: i32,
    access_token: &str,
    expiry_hours: u64,
    ip_address: Option<String>,
    user_agent: Option<String>,
) -> Result<(), ChopinError> {
    let token_hash = hash_token(access_token);
    let now = Utc::now().naive_utc();
    let expires_at = now + Duration::hours(expiry_hours as i64);

    let model = session::ActiveModel {
        user_id: Set(user_id),
        token_hash: Set(token_hash),
        expires_at: Set(expires_at),
        revoked: Set(false),
        ip_address: Set(ip_address),
        user_agent: Set(user_agent),
        created_at: Set(now),
        ..Default::default()
    };

    model.insert(db).await?;
    Ok(())
}

/// Check if an access token is still valid (not revoked / not expired).
pub async fn validate_session(
    db: &DatabaseConnection,
    access_token: &str,
) -> Result<(), ChopinError> {
    let token_hash = hash_token(access_token);
    let now = Utc::now().naive_utc();

    let session_model = session::Entity::find()
        .filter(session::Column::TokenHash.eq(&token_hash))
        .one(db)
        .await?
        .ok_or_else(|| ChopinError::Unauthorized("Session not found".to_string()))?;

    if session_model.revoked {
        return Err(ChopinError::Unauthorized(
            "Session has been revoked".to_string(),
        ));
    }

    if session_model.expires_at < now {
        return Err(ChopinError::Unauthorized("Session has expired".to_string()));
    }

    Ok(())
}

/// Revoke a specific session (logout).
pub async fn revoke_session(
    db: &DatabaseConnection,
    access_token: &str,
) -> Result<(), ChopinError> {
    let token_hash = hash_token(access_token);

    let session_model = session::Entity::find()
        .filter(session::Column::TokenHash.eq(&token_hash))
        .one(db)
        .await?;

    if let Some(session_model) = session_model {
        let mut active: session::ActiveModel = session_model.into();
        active.revoked = Set(true);
        active.update(db).await?;
    }

    Ok(())
}

/// Revoke all sessions for a user (logout everywhere).
pub async fn revoke_all_user_sessions(
    db: &DatabaseConnection,
    user_id: i32,
) -> Result<(), ChopinError> {
    use sea_orm::sea_query::Expr;

    session::Entity::update_many()
        .col_expr(session::Column::Revoked, Expr::value(true))
        .filter(session::Column::UserId.eq(user_id))
        .filter(session::Column::Revoked.eq(false))
        .exec(db)
        .await?;

    Ok(())
}
