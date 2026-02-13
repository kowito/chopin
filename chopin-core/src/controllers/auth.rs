use axum::{extract::State, routing::post, Router};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, ColumnTrait, EntityTrait, QueryFilter, Set};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::auth::{create_token, hash_password, verify_password};
use crate::error::ChopinError;
use crate::extractors::Json;
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
}

#[derive(Debug, Serialize, ToSchema)]
pub struct AuthResponse {
    pub access_token: String,
    pub user: UserResponse,
}

// ── Routes ──

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/signup", post(signup))
        .route("/login", post(login))
}

// ── Handlers ──

/// Sign up a new user.
#[utoipa::path(
    post,
    path = "/api/auth/signup",
    request_body = SignupRequest,
    responses(
        (status = 201, description = "User created successfully", body = ApiResponse<AuthResponse>),
        (status = 400, description = "Invalid input"),
        (status = 409, description = "User already exists")
    ),
    tag = "auth"
)]
async fn signup(
    State(state): State<AppState>,
    Json(payload): Json<SignupRequest>,
) -> Result<ApiResponse<AuthResponse>, ChopinError> {
    // Validate input
    if payload.email.is_empty() || payload.username.is_empty() || payload.password.is_empty() {
        return Err(ChopinError::Validation(
            "Email, username, and password are required".to_string(),
        ));
    }

    if payload.password.len() < 8 {
        return Err(ChopinError::Validation(
            "Password must be at least 8 characters".to_string(),
        ));
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

    // Create user
    let new_user = user::ActiveModel {
        email: Set(payload.email),
        username: Set(payload.username),
        password_hash: Set(password_hash),
        role: Set("user".to_string()),
        is_active: Set(true),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    let user_model = new_user.insert(&state.db).await?;

    // Generate JWT
    let token = create_token(
        user_model.id,
        &state.config.jwt_secret,
        state.config.jwt_expiry_hours,
    )?;

    Ok(ApiResponse::success(AuthResponse {
        access_token: token,
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
        (status = 401, description = "Invalid credentials")
    ),
    tag = "auth"
)]
async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<ApiResponse<AuthResponse>, ChopinError> {
    // Find user by email
    let user_model = User::find()
        .filter(user::Column::Email.eq(&payload.email))
        .one(&state.db)
        .await?
        .ok_or_else(|| ChopinError::Unauthorized("Invalid email or password".to_string()))?;

    // Check if user is active
    if !user_model.is_active {
        return Err(ChopinError::Unauthorized(
            "Account is deactivated".to_string(),
        ));
    }

    // Verify password
    let is_valid = verify_password(&payload.password, &user_model.password_hash)?;
    if !is_valid {
        return Err(ChopinError::Unauthorized(
            "Invalid email or password".to_string(),
        ));
    }

    // Generate JWT
    let token = create_token(
        user_model.id,
        &state.config.jwt_secret,
        state.config.jwt_expiry_hours,
    )?;

    Ok(ApiResponse::success(AuthResponse {
        access_token: token,
        user: UserResponse::from(user_model),
    }))
}
