use chrono::Utc;
use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};

use crate::error::ChopinError;
use crate::models::user;

/// Check if a user account is currently locked.
/// Returns `Ok(())` if not locked, `Err` if locked.
pub async fn check_lockout(user_model: &user::Model) -> Result<(), ChopinError> {
    if let Some(locked_until) = user_model.locked_until {
        let now = Utc::now().naive_utc();
        if now < locked_until {
            let remaining = (locked_until - now).num_seconds();
            return Err(ChopinError::Unauthorized(format!(
                "Account is locked. Try again in {} seconds.",
                remaining
            )));
        }
    }
    Ok(())
}

/// Record a failed login attempt. Locks the account if max attempts exceeded.
pub async fn record_failed_attempt(
    db: &DatabaseConnection,
    user_model: &user::Model,
    max_attempts: u32,
    lockout_duration_secs: u64,
) -> Result<(), ChopinError> {
    let new_count = user_model.failed_login_attempts + 1;
    let now = Utc::now().naive_utc();

    let locked_until = if new_count >= max_attempts as i32 {
        Some(now + chrono::Duration::seconds(lockout_duration_secs as i64))
    } else {
        user_model.locked_until
    };

    let mut active: user::ActiveModel = user_model.clone().into();
    active.failed_login_attempts = Set(new_count);
    active.locked_until = Set(locked_until);
    active.updated_at = Set(now);
    active.update(db).await?;

    Ok(())
}

/// Reset failed login attempts on successful login.
pub async fn reset_failed_attempts(
    db: &DatabaseConnection,
    user_model: &user::Model,
) -> Result<(), ChopinError> {
    if user_model.failed_login_attempts > 0 || user_model.locked_until.is_some() {
        let now = Utc::now().naive_utc();
        let mut active: user::ActiveModel = user_model.clone().into();
        active.failed_login_attempts = Set(0);
        active.locked_until = Set(None);
        active.updated_at = Set(now);
        active.update(db).await?;
    }
    Ok(())
}
