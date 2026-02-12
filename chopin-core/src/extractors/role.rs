use axum::{
    extract::{FromRequestParts, State},
    http::request::Parts,
    middleware::Next,
    response::Response,
};

use std::sync::Arc;

use crate::auth;
use crate::config::Config;
use crate::error::ChopinError;
use crate::models::user::{Entity as User, Role};
use sea_orm::EntityTrait;

/// Extractor that validates JWT and provides the authenticated user with role info.
///
/// Usage in handlers:
/// ```rust,ignore
/// async fn admin_handler(AuthUserWithRole(user_id, role): AuthUserWithRole) -> impl IntoResponse {
///     if !role.has_permission(&Role::Admin) {
///         return Err(ChopinError::Forbidden("Admin access required".into()));
///     }
///     // ...
/// }
/// ```
#[derive(Debug, Clone)]
pub struct AuthUserWithRole(pub i32, pub Role);

impl<S> FromRequestParts<S> for AuthUserWithRole
where
    S: Send + Sync,
{
    type Rejection = ChopinError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        // Extract Authorization header
        let auth_header = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| ChopinError::Unauthorized("Missing Authorization header".to_string()))?;

        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or_else(|| ChopinError::Unauthorized("Invalid Authorization header format".to_string()))?;

        // Get JWT secret from Arc<Config> in extensions (cheap Arc clone per request)
        let config = parts
            .extensions
            .get::<Arc<Config>>()
            .ok_or_else(|| ChopinError::Internal("Config not found in request".to_string()))?;

        let claims = auth::validate_token(token, &config.jwt_secret)?;

        let user_id: i32 = claims
            .sub
            .parse()
            .map_err(|_| ChopinError::Unauthorized("Invalid user ID in token".to_string()))?;

        // Get role from extensions (set by role middleware if used), default to user
        let role = parts
            .extensions
            .get::<Role>()
            .cloned()
            .unwrap_or(Role::User);

        Ok(AuthUserWithRole(user_id, role))
    }
}

/// Middleware that requires a minimum role level.
///
/// Usage:
/// ```rust,ignore
/// use axum::middleware;
///
/// Router::new()
///     .route("/admin/users", get(list_users))
///     .layer(middleware::from_fn_with_state(
///         app_state.clone(),
///         require_role(Role::Admin),
///     ))
/// ```
pub fn require_role(
    required: Role,
) -> impl Fn(
    State<crate::controllers::AppState>,
    axum::extract::Request,
    Next,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Response, ChopinError>> + Send>>
       + Clone
       + Send {
    move |State(state): State<crate::controllers::AppState>,
          mut req: axum::extract::Request,
          next: Next| {
        let required = required.clone();
        Box::pin(async move {
            // Extract token
            let auth_header = req
                .headers()
                .get("Authorization")
                .and_then(|v| v.to_str().ok())
                .ok_or_else(|| ChopinError::Unauthorized("Missing Authorization header".to_string()))?;

            let token = auth_header
                .strip_prefix("Bearer ")
                .ok_or_else(|| ChopinError::Unauthorized("Invalid Authorization header format".to_string()))?;

            let claims = auth::validate_token(token, &state.config.jwt_secret)?;

            let user_id: i32 = claims
                .sub
                .parse()
                .map_err(|_| ChopinError::Unauthorized("Invalid user ID in token".to_string()))?;

            // Look up user's role from database
            let user = User::find_by_id(user_id)
                .one(&state.db)
                .await
                .map_err(|e| ChopinError::Internal(e.to_string()))?
                .ok_or_else(|| ChopinError::Unauthorized("User not found".to_string()))?;

            let user_role = Role::from_str(&user.role);

            if !user_role.has_permission(&required) {
                return Err(ChopinError::Forbidden(format!(
                    "{} access required",
                    required.as_str()
                )));
            }

            // Inject role into extensions for downstream extractors
            req.extensions_mut().insert(user_role);

            Ok(next.run(req).await)
        })
    }
}
