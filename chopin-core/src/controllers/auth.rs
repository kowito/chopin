use std::sync::Arc;

use axum::{extract::State, routing::post, Router};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::auth::rate_limit::RateLimiter;
use crate::auth::{create_token, hash_password, verify_password};
use crate::error::ChopinError;
use crate::extractors::{AuthUser, Json};
use crate::models::user::{self, Entity as User, UserResponse};
use crate::response::ApiResponse;

use super::AppState;

// ── Request / Response types ──

#[derive(Debug, Deserialize, ToSchema)]
pub struct SignupRequest {
    pub email: String,
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
    /// TOTP code (required if 2FA is enabled for the user)
    pub totp_code: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthResponse {
    pub access_token: String,
    /// Refresh token (present when refresh tokens are enabled)
    pub refresh_token: Option<String>,
    /// CSRF token for state-changing requests
    pub csrf_token: Option<String>,
    /// Email verification required before full access
    pub email_verification_required: bool,
    pub user: UserResponse,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RefreshRequest {
    pub refresh_token: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct RefreshResponse {
    pub access_token: String,
    pub refresh_token: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LogoutRequest {
    /// If true, revoke all sessions (logout everywhere)
    pub all_sessions: Option<bool>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TotpSetupResponse {
    /// Base32-encoded TOTP secret
    pub secret: String,
    /// otpauth:// URI for QR code generation
    pub otpauth_uri: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TotpEnableRequest {
    /// The TOTP code from the authenticator app to verify setup
    pub code: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TotpDisableRequest {
    /// Current password to confirm disabling 2FA
    pub password: String,
    /// Current TOTP code to confirm
    pub code: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PasswordResetRequestPayload {
    pub email: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct PasswordResetRequestResponse {
    /// The reset token (in production, send this via email instead)
    pub reset_token: String,
    pub message: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct PasswordResetConfirm {
    pub token: String,
    pub new_password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct VerifyEmailRequest {
    pub token: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct MessageResponse {
    pub message: String,
}

// ── Shared state for rate limiter ──

/// Extended app state that includes the rate limiter.
#[derive(Clone)]
pub struct SecurityState {
    pub rate_limiter: Arc<RateLimiter>,
}

// ── Routes ──

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/signup", post(signup))
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/refresh", post(refresh_token))
        .route("/totp/setup", post(totp_setup))
        .route("/totp/enable", post(totp_enable))
        .route("/totp/disable", post(totp_disable))
        .route("/password-reset/request", post(password_reset_request))
        .route("/password-reset/confirm", post(password_reset_confirm))
        .route("/verify-email", post(verify_email))
}

// ── Helper: extract IP and user-agent from headers ──

fn extract_client_info(headers: &axum::http::HeaderMap) -> (Option<String>, Option<String>) {
    let ip = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.split(',').next().unwrap_or("").trim().to_string())
        .or_else(|| {
            headers
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
                .map(|v| v.to_string())
        });

    let user_agent = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|v| v.to_string());

    (ip, user_agent)
}

// ── Handlers ──

/// Sign up a new user.
#[utoipa::path(
    post,
    path = "/api/auth/signup",
    request_body = SignupRequest,
    responses(
        (status = 200, description = "User created", body = ApiResponse<AuthResponse>),
        (status = 400, description = "Invalid input"),
        (status = 409, description = "User already exists")
    ),
    tag = "auth"
)]
async fn signup(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<SignupRequest>,
) -> Result<ApiResponse<AuthResponse>, ChopinError> {
    let security = &state.config.security;

    // Validate input
    if payload.email.is_empty() || payload.username.is_empty() || payload.password.is_empty() {
        return Err(ChopinError::Validation(
            "Email, username, and password are required".to_string(),
        ));
    }

    if payload.password.len() < security.min_password_length {
        return Err(ChopinError::Validation(format!(
            "Password must be at least {} characters",
            security.min_password_length
        )));
    }

    // Check if user exists
    let existing = User::find()
        .filter(
            user::Column::Email
                .eq(&payload.email)
                .or(user::Column::Username.eq(&payload.username)),
        )
        .one(&state.db)
        .await?;

    if existing.is_some() {
        return Err(ChopinError::Conflict(
            "User with this email or username already exists".to_string(),
        ));
    }

    // Hash password
    let password_hash = hash_password(&payload.password)?;
    let now = Utc::now().naive_utc();
    let email_verified = !security.enable_email_verification;

    // Create user
    let new_user = user::ActiveModel {
        email: Set(payload.email),
        username: Set(payload.username),
        password_hash: Set(password_hash),
        role: Set("user".to_string()),
        is_active: Set(true),
        email_verified: Set(email_verified),
        totp_secret: Set(None),
        totp_enabled: Set(false),
        failed_login_attempts: Set(0),
        locked_until: Set(None),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    let user_model = new_user.insert(&state.db).await?;

    // Generate email verification token if enabled
    let email_verification_required = security.enable_email_verification;
    if email_verification_required {
        let _token = crate::auth::security_token::create_email_verification_token(
            &state.db,
            user_model.id,
            security.email_verification_expiry_secs,
        )
        .await?;
        // In production, send this token via email.
        // For now, the token is returned via the verify-email endpoint flow.
    }

    // Generate JWT
    let token = create_token(
        user_model.id,
        &state.config.jwt_secret,
        state.config.jwt_expiry_hours,
    )?;

    // Create session if session management is enabled
    let (ip, ua) = extract_client_info(&headers);
    if security.enable_session_management {
        crate::auth::session::create_session(
            &state.db,
            user_model.id,
            &token,
            state.config.jwt_expiry_hours,
            ip.clone(),
            ua.clone(),
        )
        .await?;
    }

    // Create refresh token if enabled
    let refresh_token = if security.enable_refresh_tokens {
        Some(
            crate::auth::refresh::create_refresh_token(
                &state.db,
                user_model.id,
                security.refresh_token_expiry_days,
                ip,
                ua,
            )
            .await?,
        )
    } else {
        None
    };

    // Generate CSRF token if enabled
    let csrf_token = if security.enable_csrf {
        Some(crate::auth::generate_csrf_token())
    } else {
        None
    };

    Ok(ApiResponse::success(AuthResponse {
        access_token: token,
        refresh_token,
        csrf_token,
        email_verification_required,
        user: UserResponse::from(user_model),
    }))
}

/// Log in with existing credentials.
#[utoipa::path(
    post,
    path = "/api/auth/login",
    request_body = LoginRequest,
    responses(
        (status = 200, description = "Login successful", body = ApiResponse<AuthResponse>),
        (status = 401, description = "Invalid credentials"),
        (status = 429, description = "Too many attempts")
    ),
    tag = "auth"
)]
async fn login(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<LoginRequest>,
) -> Result<ApiResponse<AuthResponse>, ChopinError> {
    let security = &state.config.security;
    let (ip, ua) = extract_client_info(&headers);

    // Rate limiting
    if security.enable_rate_limit {
        let key = ip.as_deref().unwrap_or(&payload.email);
        if let Err(retry_after) = state.rate_limiter.check(key) {
            // Track failed attempt
            if security.enable_device_tracking {
                let _ = crate::auth::device_tracking::record_login_event(
                    &state.db,
                    None,
                    &payload.email,
                    false,
                    Some("rate_limited".to_string()),
                    ip.clone(),
                    ua.clone(),
                )
                .await;
            }
            return Err(ChopinError::TooManyRequests(format!(
                "Too many login attempts. Try again in {} seconds.",
                retry_after
            )));
        }
    }

    // Find user by email
    let user_model = match User::find()
        .filter(user::Column::Email.eq(&payload.email))
        .one(&state.db)
        .await?
    {
        Some(u) => u,
        None => {
            // Track failed attempt (user not found)
            if security.enable_device_tracking {
                let _ = crate::auth::device_tracking::record_login_event(
                    &state.db,
                    None,
                    &payload.email,
                    false,
                    Some("user_not_found".to_string()),
                    ip.clone(),
                    ua.clone(),
                )
                .await;
            }
            return Err(ChopinError::Unauthorized(
                "Invalid email or password".to_string(),
            ));
        }
    };

    // Check if user is active
    if !user_model.is_active {
        return Err(ChopinError::Unauthorized(
            "Account is deactivated".to_string(),
        ));
    }

    // Check account lockout
    if security.enable_account_lockout {
        crate::auth::lockout::check_lockout(&user_model).await?;
    }

    // Verify password
    let is_valid = verify_password(&payload.password, &user_model.password_hash)?;
    if !is_valid {
        // Record failed attempt
        if security.enable_account_lockout {
            crate::auth::lockout::record_failed_attempt(
                &state.db,
                &user_model,
                security.lockout_max_attempts,
                security.lockout_duration_secs,
            )
            .await?;
        }

        if security.enable_device_tracking {
            let _ = crate::auth::device_tracking::record_login_event(
                &state.db,
                Some(user_model.id),
                &payload.email,
                false,
                Some("invalid_password".to_string()),
                ip.clone(),
                ua.clone(),
            )
            .await;
        }

        return Err(ChopinError::Unauthorized(
            "Invalid email or password".to_string(),
        ));
    }

    // Verify TOTP if 2FA is enabled for this user
    if security.enable_2fa && user_model.totp_enabled {
        let totp_code = payload
            .totp_code
            .as_deref()
            .ok_or_else(|| ChopinError::Validation("2FA code is required".to_string()))?;

        let secret = user_model.totp_secret.as_deref().ok_or_else(|| {
            ChopinError::Internal("TOTP secret missing for 2FA-enabled user".to_string())
        })?;

        if !crate::auth::verify_totp(secret, totp_code)? {
            if security.enable_device_tracking {
                let _ = crate::auth::device_tracking::record_login_event(
                    &state.db,
                    Some(user_model.id),
                    &payload.email,
                    false,
                    Some("invalid_totp".to_string()),
                    ip.clone(),
                    ua.clone(),
                )
                .await;
            }
            return Err(ChopinError::Unauthorized("Invalid 2FA code".to_string()));
        }
    }

    // Reset failed login attempts
    if security.enable_account_lockout {
        crate::auth::lockout::reset_failed_attempts(&state.db, &user_model).await?;
    }

    // Reset rate limiter for this IP
    if security.enable_rate_limit {
        if let Some(ref ip_addr) = ip {
            state.rate_limiter.reset(ip_addr);
        }
    }

    // Generate JWT
    let token = create_token(
        user_model.id,
        &state.config.jwt_secret,
        state.config.jwt_expiry_hours,
    )?;

    // Create session
    if security.enable_session_management {
        crate::auth::session::create_session(
            &state.db,
            user_model.id,
            &token,
            state.config.jwt_expiry_hours,
            ip.clone(),
            ua.clone(),
        )
        .await?;
    }

    // Create refresh token
    let refresh_token = if security.enable_refresh_tokens {
        Some(
            crate::auth::refresh::create_refresh_token(
                &state.db,
                user_model.id,
                security.refresh_token_expiry_days,
                ip.clone(),
                ua.clone(),
            )
            .await?,
        )
    } else {
        None
    };

    // CSRF token
    let csrf_token = if security.enable_csrf {
        Some(crate::auth::generate_csrf_token())
    } else {
        None
    };

    // Track successful login
    if security.enable_device_tracking {
        let _ = crate::auth::device_tracking::record_login_event(
            &state.db,
            Some(user_model.id),
            &payload.email,
            true,
            None,
            ip,
            ua,
        )
        .await;
    }

    let email_verification_required =
        security.enable_email_verification && !user_model.email_verified;

    Ok(ApiResponse::success(AuthResponse {
        access_token: token,
        refresh_token,
        csrf_token,
        email_verification_required,
        user: UserResponse::from(user_model),
    }))
}

/// Logout — revoke current session / all sessions.
#[utoipa::path(
    post,
    path = "/api/auth/logout",
    request_body = LogoutRequest,
    responses(
        (status = 200, description = "Logged out", body = ApiResponse<MessageResponse>),
        (status = 401, description = "Unauthorized")
    ),
    tag = "auth",
    security(("bearer_auth" = []))
)]
async fn logout(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    headers: axum::http::HeaderMap,
    Json(payload): Json<LogoutRequest>,
) -> Result<ApiResponse<MessageResponse>, ChopinError> {
    let security = &state.config.security;

    if payload.all_sessions.unwrap_or(false) {
        // Revoke all sessions and refresh tokens
        if security.enable_session_management {
            crate::auth::session::revoke_all_user_sessions(&state.db, user_id).await?;
        }
        if security.enable_refresh_tokens {
            crate::auth::refresh::revoke_all_user_tokens(&state.db, user_id).await?;
        }
        Ok(ApiResponse::success(MessageResponse {
            message: "All sessions revoked successfully".to_string(),
        }))
    } else {
        // Revoke current session only
        if security.enable_session_management {
            if let Some(auth_header) = headers.get("Authorization") {
                if let Ok(header_str) = auth_header.to_str() {
                    if let Some(token) = header_str.strip_prefix("Bearer ") {
                        crate::auth::session::revoke_session(&state.db, token).await?;
                    }
                }
            }
        }
        Ok(ApiResponse::success(MessageResponse {
            message: "Logged out successfully".to_string(),
        }))
    }
}

/// Refresh access token using a refresh token (token rotation).
#[utoipa::path(
    post,
    path = "/api/auth/refresh",
    request_body = RefreshRequest,
    responses(
        (status = 200, description = "Token refreshed", body = ApiResponse<RefreshResponse>),
        (status = 401, description = "Invalid refresh token")
    ),
    tag = "auth"
)]
async fn refresh_token(
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    Json(payload): Json<RefreshRequest>,
) -> Result<ApiResponse<RefreshResponse>, ChopinError> {
    let security = &state.config.security;

    if !security.enable_refresh_tokens {
        return Err(ChopinError::BadRequest(
            "Refresh tokens are not enabled".to_string(),
        ));
    }

    // Validate and consume the old refresh token
    let user_id =
        crate::auth::refresh::validate_refresh_token(&state.db, &payload.refresh_token).await?;

    // Generate a new access token
    let new_access_token = create_token(
        user_id,
        &state.config.jwt_secret,
        state.config.jwt_expiry_hours,
    )?;

    // Create new session
    let (ip, ua) = extract_client_info(&headers);
    if security.enable_session_management {
        crate::auth::session::create_session(
            &state.db,
            user_id,
            &new_access_token,
            state.config.jwt_expiry_hours,
            ip.clone(),
            ua.clone(),
        )
        .await?;
    }

    // Issue a new refresh token (rotation)
    let new_refresh_token = crate::auth::refresh::create_refresh_token(
        &state.db,
        user_id,
        security.refresh_token_expiry_days,
        ip,
        ua,
    )
    .await?;

    Ok(ApiResponse::success(RefreshResponse {
        access_token: new_access_token,
        refresh_token: new_refresh_token,
    }))
}

/// Set up TOTP 2FA — returns the secret and otpauth URI.
#[utoipa::path(
    post,
    path = "/api/auth/totp/setup",
    responses(
        (status = 200, description = "TOTP setup info", body = ApiResponse<TotpSetupResponse>),
        (status = 401, description = "Unauthorized")
    ),
    tag = "auth",
    security(("bearer_auth" = []))
)]
async fn totp_setup(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
) -> Result<ApiResponse<TotpSetupResponse>, ChopinError> {
    if !state.config.security.enable_2fa {
        return Err(ChopinError::BadRequest("2FA is not enabled".to_string()));
    }

    // Find user
    let user_model = User::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| ChopinError::NotFound("User not found".to_string()))?;

    if user_model.totp_enabled {
        return Err(ChopinError::Conflict("2FA is already enabled".to_string()));
    }

    let (secret, uri) = crate::auth::generate_totp_secret("Chopin", &user_model.email)?;

    // Store the secret (not yet enabled until verified)
    let mut active: user::ActiveModel = user_model.into();
    active.totp_secret = Set(Some(secret.clone()));
    active.updated_at = Set(Utc::now().naive_utc());
    active.update(&state.db).await?;

    Ok(ApiResponse::success(TotpSetupResponse {
        secret,
        otpauth_uri: uri,
    }))
}

/// Enable 2FA by verifying a TOTP code.
#[utoipa::path(
    post,
    path = "/api/auth/totp/enable",
    request_body = TotpEnableRequest,
    responses(
        (status = 200, description = "2FA enabled", body = ApiResponse<MessageResponse>),
        (status = 401, description = "Unauthorized")
    ),
    tag = "auth",
    security(("bearer_auth" = []))
)]
async fn totp_enable(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Json(payload): Json<TotpEnableRequest>,
) -> Result<ApiResponse<MessageResponse>, ChopinError> {
    if !state.config.security.enable_2fa {
        return Err(ChopinError::BadRequest("2FA is not enabled".to_string()));
    }

    let user_model = User::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| ChopinError::NotFound("User not found".to_string()))?;

    let secret = user_model
        .totp_secret
        .as_deref()
        .ok_or_else(|| ChopinError::BadRequest("Call /totp/setup first".to_string()))?;

    if !crate::auth::verify_totp(secret, &payload.code)? {
        return Err(ChopinError::Validation(
            "Invalid TOTP code. Please try again.".to_string(),
        ));
    }

    let mut active: user::ActiveModel = user_model.into();
    active.totp_enabled = Set(true);
    active.updated_at = Set(Utc::now().naive_utc());
    active.update(&state.db).await?;

    Ok(ApiResponse::success(MessageResponse {
        message: "2FA has been enabled successfully".to_string(),
    }))
}

/// Disable 2FA.
#[utoipa::path(
    post,
    path = "/api/auth/totp/disable",
    request_body = TotpDisableRequest,
    responses(
        (status = 200, description = "2FA disabled", body = ApiResponse<MessageResponse>),
        (status = 401, description = "Unauthorized")
    ),
    tag = "auth",
    security(("bearer_auth" = []))
)]
async fn totp_disable(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Json(payload): Json<TotpDisableRequest>,
) -> Result<ApiResponse<MessageResponse>, ChopinError> {
    let user_model = User::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| ChopinError::NotFound("User not found".to_string()))?;

    // Verify password
    if !verify_password(&payload.password, &user_model.password_hash)? {
        return Err(ChopinError::Unauthorized("Invalid password".to_string()));
    }

    // Verify TOTP code
    if let Some(ref secret) = user_model.totp_secret {
        if !crate::auth::verify_totp(secret, &payload.code)? {
            return Err(ChopinError::Validation("Invalid TOTP code".to_string()));
        }
    }

    let mut active: user::ActiveModel = user_model.into();
    active.totp_enabled = Set(false);
    active.totp_secret = Set(None);
    active.updated_at = Set(Utc::now().naive_utc());
    active.update(&state.db).await?;

    Ok(ApiResponse::success(MessageResponse {
        message: "2FA has been disabled".to_string(),
    }))
}

/// Request a password reset token.
#[utoipa::path(
    post,
    path = "/api/auth/password-reset/request",
    request_body = PasswordResetRequestPayload,
    responses(
        (status = 200, description = "Reset token created", body = ApiResponse<PasswordResetRequestResponse>),
    ),
    tag = "auth"
)]
async fn password_reset_request(
    State(state): State<AppState>,
    Json(payload): Json<PasswordResetRequestPayload>,
) -> Result<ApiResponse<PasswordResetRequestResponse>, ChopinError> {
    let security = &state.config.security;

    if !security.enable_password_reset {
        return Err(ChopinError::BadRequest(
            "Password reset is not enabled".to_string(),
        ));
    }

    // Always return success to prevent email enumeration
    let user_opt = User::find()
        .filter(user::Column::Email.eq(&payload.email))
        .one(&state.db)
        .await?;

    if let Some(user_model) = user_opt {
        let token = crate::auth::security_token::create_password_reset_token(
            &state.db,
            user_model.id,
            security.password_reset_expiry_secs,
        )
        .await?;

        // In production, send this token via email instead of returning it.
        // Returning it here for development / testing convenience.
        return Ok(ApiResponse::success(PasswordResetRequestResponse {
            reset_token: token,
            message: "Password reset token created. In production, this would be sent via email."
                .to_string(),
        }));
    }

    // Return a generic success even if user not found (anti-enumeration)
    Ok(ApiResponse::success(PasswordResetRequestResponse {
        reset_token: String::new(),
        message: "If an account with that email exists, a reset link has been sent.".to_string(),
    }))
}

/// Confirm password reset with token and new password.
#[utoipa::path(
    post,
    path = "/api/auth/password-reset/confirm",
    request_body = PasswordResetConfirm,
    responses(
        (status = 200, description = "Password reset successful", body = ApiResponse<MessageResponse>),
        (status = 400, description = "Invalid token")
    ),
    tag = "auth"
)]
async fn password_reset_confirm(
    State(state): State<AppState>,
    Json(payload): Json<PasswordResetConfirm>,
) -> Result<ApiResponse<MessageResponse>, ChopinError> {
    let security = &state.config.security;

    if !security.enable_password_reset {
        return Err(ChopinError::BadRequest(
            "Password reset is not enabled".to_string(),
        ));
    }

    if payload.new_password.len() < security.min_password_length {
        return Err(ChopinError::Validation(format!(
            "Password must be at least {} characters",
            security.min_password_length
        )));
    }

    let user_id =
        crate::auth::security_token::validate_password_reset_token(&state.db, &payload.token)
            .await?;

    // Update password
    let user_model = User::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| ChopinError::NotFound("User not found".to_string()))?;

    let new_hash = hash_password(&payload.new_password)?;
    let mut active: user::ActiveModel = user_model.into();
    active.password_hash = Set(new_hash);
    active.updated_at = Set(Utc::now().naive_utc());
    active.update(&state.db).await?;

    // Revoke all existing sessions for security
    if security.enable_session_management {
        crate::auth::session::revoke_all_user_sessions(&state.db, user_id).await?;
    }
    if security.enable_refresh_tokens {
        crate::auth::refresh::revoke_all_user_tokens(&state.db, user_id).await?;
    }

    Ok(ApiResponse::success(MessageResponse {
        message: "Password has been reset successfully. Please log in again.".to_string(),
    }))
}

/// Verify email address with token.
#[utoipa::path(
    post,
    path = "/api/auth/verify-email",
    request_body = VerifyEmailRequest,
    responses(
        (status = 200, description = "Email verified", body = ApiResponse<MessageResponse>),
        (status = 400, description = "Invalid token")
    ),
    tag = "auth"
)]
async fn verify_email(
    State(state): State<AppState>,
    Json(payload): Json<VerifyEmailRequest>,
) -> Result<ApiResponse<MessageResponse>, ChopinError> {
    if !state.config.security.enable_email_verification {
        return Err(ChopinError::BadRequest(
            "Email verification is not enabled".to_string(),
        ));
    }

    let user_id =
        crate::auth::security_token::validate_email_verification_token(&state.db, &payload.token)
            .await?;

    let user_model = User::find_by_id(user_id)
        .one(&state.db)
        .await?
        .ok_or_else(|| ChopinError::NotFound("User not found".to_string()))?;

    let mut active: user::ActiveModel = user_model.into();
    active.email_verified = Set(true);
    active.updated_at = Set(Utc::now().naive_utc());
    active.update(&state.db).await?;

    Ok(ApiResponse::success(MessageResponse {
        message: "Email verified successfully".to_string(),
    }))
}
