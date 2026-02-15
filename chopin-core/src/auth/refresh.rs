use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};

use crate::auth::totp::{generate_secure_token, hash_token};
use crate::error::ChopinError;
use crate::models::refresh_token;

/// Create a refresh token for a user. Returns the raw token string
/// (send to client; the DB stores only the hash).
pub async fn create_refresh_token(
    db: &DatabaseConnection,
    user_id: i32,
    expiry_days: u64,
    ip_address: Option<String>,
    user_agent: Option<String>,
) -> Result<String, ChopinError> {
    let raw_token = generate_secure_token();
    let token_hash = hash_token(&raw_token);
    let now = Utc::now().naive_utc();
    let expires_at = now + Duration::days(expiry_days as i64);

    let model = refresh_token::ActiveModel {
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
    Ok(raw_token)
}

/// Validate and consume a refresh token (token rotation).
/// Returns the user_id. The old token is revoked and a new one must be issued.
pub async fn validate_refresh_token(
    db: &DatabaseConnection,
    raw_token: &str,
) -> Result<i32, ChopinError> {
    let token_hash = hash_token(raw_token);
    let now = Utc::now().naive_utc();

    let token_model = refresh_token::Entity::find()
        .filter(refresh_token::Column::TokenHash.eq(&token_hash))
        .one(db)
        .await?
        .ok_or_else(|| ChopinError::Unauthorized("Invalid refresh token".to_string()))?;

    if token_model.revoked {
        // Possible token reuse attack — revoke all tokens for this user
        revoke_all_user_tokens(db, token_model.user_id).await?;
        return Err(ChopinError::Unauthorized(
            "Refresh token has been revoked. All sessions invalidated for security.".to_string(),
        ));
    }

    if token_model.expires_at < now {
        return Err(ChopinError::Unauthorized(
            "Refresh token has expired".to_string(),
        ));
    }

    // Revoke the consumed token (rotation)
    let mut active: refresh_token::ActiveModel = token_model.clone().into();
    active.revoked = Set(true);
    active.update(db).await?;

    Ok(token_model.user_id)
}

/// Revoke all refresh tokens for a user (logout-all / security event).
pub async fn revoke_all_user_tokens(
    db: &DatabaseConnection,
    user_id: i32,
) -> Result<(), ChopinError> {
    use sea_orm::sea_query::Expr;

    refresh_token::Entity::update_many()
        .col_expr(refresh_token::Column::Revoked, Expr::value(true))
        .filter(refresh_token::Column::UserId.eq(user_id))
        .filter(refresh_token::Column::Revoked.eq(false))
        .exec(db)
        .await?;

    Ok(())
}
