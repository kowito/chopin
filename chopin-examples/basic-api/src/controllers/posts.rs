use axum::{extract::State, routing::get, Router};
use chrono::Utc;
use sea_orm::{ActiveModelTrait, EntityTrait, PaginatorTrait, QueryOrder, Set};
use serde::Deserialize;
use utoipa::ToSchema;

use chopin_core::extractors::{AuthUser, Json, Pagination};
use chopin_core::{ChopinError, ApiResponse};

use crate::models::post::{self, Entity as Post, PostResponse};
use crate::AppState;

// ── Request types ──

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePostRequest {
    pub title: String,
    pub body: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdatePostRequest {
    pub title: Option<String>,
    pub body: Option<String>,
}

// ── Routes ──

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/posts", get(list_posts).post(create_post))
        .route("/posts/:id", get(get_post))
}

// ── Handlers ──

/// List all posts with pagination.
#[utoipa::path(
    get,
    path = "/api/posts",
    params(Pagination),
    responses(
        (status = 200, description = "List of posts", body = ApiResponse<Vec<PostResponse>>),
    ),
    tag = "posts",
    security(("bearer_auth" = []))
)]
async fn list_posts(
    State(state): State<AppState>,
    pagination: Pagination,
) -> Result<ApiResponse<Vec<PostResponse>>, ChopinError> {
    let p = pagination.clamped();

    // Use paginate for cleaner pagination
    let page = p.offset / p.limit;
    let posts = Post::find()
        .order_by_desc(post::Column::CreatedAt)
        .paginate(&state.db, p.limit)
        .fetch_page(page)
        .await?;

    let response: Vec<PostResponse> = posts.into_iter().map(|p| p.into()).collect();
    Ok(ApiResponse::success(response))
}

/// Create a new post (requires authentication).
#[utoipa::path(
    post,
    path = "/api/posts",
    request_body = CreatePostRequest,
    responses(
        (status = 201, description = "Post created", body = ApiResponse<PostResponse>),
        (status = 400, description = "Invalid input"),
        (status = 401, description = "Not authenticated")
    ),
    tag = "posts",
    security(("bearer_auth" = []))
)]
async fn create_post(
    State(state): State<AppState>,
    AuthUser(user_id): AuthUser,
    Json(payload): Json<CreatePostRequest>,
) -> Result<ApiResponse<PostResponse>, ChopinError> {
    if payload.title.is_empty() {
        return Err(ChopinError::Validation("Title is required".to_string()));
    }

    let now = Utc::now().naive_utc();

    let new_post = post::ActiveModel {
        title: Set(payload.title),
        body: Set(payload.body),
        author_id: Set(user_id),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    let post_model = new_post.insert(&state.db).await?;
    Ok(ApiResponse::success(PostResponse::from(post_model)))
}

/// Get a single post by ID.
#[utoipa::path(
    get,
    path = "/api/posts/{id}",
    params(
        ("id" = i32, Path, description = "Post ID")
    ),
    responses(
        (status = 200, description = "Post found", body = ApiResponse<PostResponse>),
        (status = 404, description = "Post not found")
    ),
    tag = "posts"
)]
async fn get_post(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<i32>,
) -> Result<ApiResponse<PostResponse>, ChopinError> {
    let post = Post::find_by_id(id)
        .one(&state.db)
        .await?
        .ok_or_else(|| ChopinError::NotFound(format!("Post with id {} not found", id)))?;

    Ok(ApiResponse::success(PostResponse::from(post)))
}
