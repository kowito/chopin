use chrono::{Duration, Utc};
use sea_orm::{ActiveModelTrait, ColumnTrait, DatabaseConnection, EntityTrait, QueryFilter, Set};

use crate::auth::totp::{generate_secure_token, hash_token};
use crate::error::ChopinError;
use crate::models::security_token;

/// Create a password-reset token. Returns the raw token to send to the user.
pub async fn create_password_reset_token(
    db: &DatabaseConnection,
    user_id: i32,
    expiry_secs: u64,
) -> Result<String, ChopinError> {
    let raw_token = generate_secure_token();
    let token_hash = hash_token(&raw_token);
    let now = Utc::now().naive_utc();
    let expires_at = now + Duration::seconds(expiry_secs as i64);

    let model = security_token::ActiveModel {
        user_id: Set(user_id),
        token_hash: Set(token_hash),
        token_type: Set("password_reset".to_string()),
        expires_at: Set(expires_at),
        used: Set(false),
        created_at: Set(now),
        ..Default::default()
    };
    model.insert(db).await?;

    Ok(raw_token)
}

/// Validate a password-reset token. Returns the user_id.
/// Marks the token as used.
pub async fn validate_password_reset_token(
    db: &DatabaseConnection,
    raw_token: &str,
) -> Result<i32, ChopinError> {
    let token_hash = hash_token(raw_token);
    let now = Utc::now().naive_utc();

    let token_model = security_token::Entity::find()
        .filter(security_token::Column::TokenHash.eq(&token_hash))
        .filter(security_token::Column::TokenType.eq("password_reset"))
        .one(db)
        .await?
        .ok_or_else(|| {
            ChopinError::BadRequest("Invalid or expired password reset token".to_string())
        })?;

    if token_model.used {
        return Err(ChopinError::BadRequest(
            "Password reset token has already been used".to_string(),
        ));
    }

    if token_model.expires_at < now {
        return Err(ChopinError::BadRequest(
            "Password reset token has expired".to_string(),
        ));
    }

    // Mark as used
    let mut active: security_token::ActiveModel = token_model.clone().into();
    active.used = Set(true);
    active.update(db).await?;

    Ok(token_model.user_id)
}

/// Create an email-verification token. Returns the raw token.
pub async fn create_email_verification_token(
    db: &DatabaseConnection,
    user_id: i32,
    expiry_secs: u64,
) -> Result<String, ChopinError> {
    let raw_token = generate_secure_token();
    let token_hash = hash_token(&raw_token);
    let now = Utc::now().naive_utc();
    let expires_at = now + Duration::seconds(expiry_secs as i64);

    let model = security_token::ActiveModel {
        user_id: Set(user_id),
        token_hash: Set(token_hash),
        token_type: Set("email_verification".to_string()),
        expires_at: Set(expires_at),
        used: Set(false),
        created_at: Set(now),
        ..Default::default()
    };
    model.insert(db).await?;

    Ok(raw_token)
}

/// Validate an email-verification token. Returns the user_id.
/// Marks the token as used.
pub async fn validate_email_verification_token(
    db: &DatabaseConnection,
    raw_token: &str,
) -> Result<i32, ChopinError> {
    let token_hash = hash_token(raw_token);
    let now = Utc::now().naive_utc();

    let token_model = security_token::Entity::find()
        .filter(security_token::Column::TokenHash.eq(&token_hash))
        .filter(security_token::Column::TokenType.eq("email_verification"))
        .one(db)
        .await?
        .ok_or_else(|| {
            ChopinError::BadRequest("Invalid or expired verification token".to_string())
        })?;

    if token_model.used {
        return Err(ChopinError::BadRequest(
            "Verification token has already been used".to_string(),
        ));
    }

    if token_model.expires_at < now {
        return Err(ChopinError::BadRequest(
            "Verification token has expired".to_string(),
        ));
    }

    // Mark as used
    let mut active: security_token::ActiveModel = token_model.clone().into();
    active.used = Set(true);
    active.update(db).await?;

    Ok(token_model.user_id)
}
