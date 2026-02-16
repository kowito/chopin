use std::sync::Arc;

use axum::{extract::FromRequestParts, http::request::Parts};

use crate::auth;
use crate::config::Config;
use crate::error::ChopinError;

/// Extractor that validates JWT and provides the authenticated user ID.
///
/// Usage in handlers:
/// ```rust,ignore
/// async fn my_handler(AuthUser(user_id): AuthUser) -> impl IntoResponse {
///     // user_id is the authenticated user's ID
/// }
/// ```
#[derive(Debug, Clone)]
pub struct AuthUser(pub String);

impl<S> FromRequestParts<S> for AuthUser
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

        // Expect "Bearer <token>"
        let token = auth_header.strip_prefix("Bearer ").ok_or_else(|| {
            ChopinError::Unauthorized("Invalid Authorization header format".to_string())
        })?;

        // Get JWT secret from Arc<Config> in extensions (cheap Arc clone per request)
        let config = parts
            .extensions
            .get::<Arc<Config>>()
            .ok_or_else(|| ChopinError::Internal("Config not found in request".to_string()))?;

        let claims = auth::validate_token(token, &config.jwt_secret)?;

        Ok(AuthUser(claims.sub))
    }
}
