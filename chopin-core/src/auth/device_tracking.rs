use chrono::Utc;
use sea_orm::{ActiveModelTrait, DatabaseConnection, Set};

use crate::error::ChopinError;
use crate::models::login_event;

/// Record a login event for auditing and device tracking.
pub async fn record_login_event(
    db: &DatabaseConnection,
    user_id: Option<i32>,
    email: &str,
    success: bool,
    failure_reason: Option<String>,
    ip_address: Option<String>,
    user_agent: Option<String>,
) -> Result<(), ChopinError> {
    let now = Utc::now().naive_utc();

    let model = login_event::ActiveModel {
        user_id: Set(user_id),
        email: Set(email.to_string()),
        success: Set(success),
        failure_reason: Set(failure_reason),
        ip_address: Set(ip_address),
        user_agent: Set(user_agent),
        created_at: Set(now),
        ..Default::default()
    };

    model.insert(db).await?;
    Ok(())
}
