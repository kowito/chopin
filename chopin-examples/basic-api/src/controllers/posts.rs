use chopin_core::extractors::{Json, Pagination, Path, Query, State};
use chopin_core::response::ApiResponse;
use chopin_core::routing::get;
use chopin_core::{Router, StatusCode};
use sea_orm::*;
use serde::Deserialize;
use utoipa::ToSchema;

use crate::models::post::{self, Entity as Post, PostResponse};
use crate::AppState;

// ─── Routes ────────────────────────────────────────────────────

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/posts", get(list_posts).post(create_post))
        .route(
            "/api/posts/{id}",
            get(get_post).put(update_post).delete(delete_post),
        )
}

// ─── Request DTOs ──────────────────────────────────────────────

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreatePostRequest {
    /// Post title (required)
    pub title: String,
    /// Post body content (required)
    pub body: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct UpdatePostRequest {
    /// Updated title (optional)
    pub title: Option<String>,
    /// Updated body (optional)
    pub body: Option<String>,
    /// Publish or unpublish (optional)
    pub published: Option<bool>,
}

// ─── Handlers ──────────────────────────────────────────────────

/// List all posts with pagination.
#[utoipa::path(
    get,
    path = "/api/posts",
    tag = "posts",
    params(Pagination),
    responses(
        (status = 200, description = "Paginated list of posts", body = ApiResponse<Vec<PostResponse>>)
    )
)]
pub async fn list_posts(
    State(state): State<AppState>,
    Query(pagination): Query<Pagination>,
) -> Result<Json<ApiResponse<Vec<PostResponse>>>, StatusCode> {
    let p = pagination.clamped();

    let posts = Post::find()
        .order_by_desc(post::Column::CreatedAt)
        .offset(p.offset)
        .limit(p.limit)
        .all(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let items: Vec<PostResponse> = posts.into_iter().map(PostResponse::from).collect();
    Ok(Json(ApiResponse::success(items)))
}

/// Create a new post.
#[utoipa::path(
    post,
    path = "/api/posts",
    tag = "posts",
    request_body = CreatePostRequest,
    responses(
        (status = 201, description = "Post created", body = ApiResponse<PostResponse>),
        (status = 400, description = "Invalid input")
    )
)]
pub async fn create_post(
    State(state): State<AppState>,
    Json(payload): Json<CreatePostRequest>,
) -> Result<(StatusCode, Json<ApiResponse<PostResponse>>), StatusCode> {
    let now = chrono::Utc::now().naive_utc();

    let new_post = post::ActiveModel {
        title: Set(payload.title),
        body: Set(payload.body),
        published: Set(false),
        created_at: Set(now),
        updated_at: Set(now),
        ..Default::default()
    };

    let result = new_post
        .insert(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok((
        StatusCode::CREATED,
        Json(ApiResponse::success(PostResponse::from(result))),
    ))
}

/// Get a single post by ID.
#[utoipa::path(
    get,
    path = "/api/posts/{id}",
    tag = "posts",
    params(("id" = i32, Path, description = "Post ID")),
    responses(
        (status = 200, description = "Post found", body = ApiResponse<PostResponse>),
        (status = 404, description = "Post not found")
    )
)]
pub async fn get_post(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<Json<ApiResponse<PostResponse>>, StatusCode> {
    let post = Post::find_by_id(id)
        .one(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    Ok(Json(ApiResponse::success(PostResponse::from(post))))
}

/// Update an existing post.
#[utoipa::path(
    put,
    path = "/api/posts/{id}",
    tag = "posts",
    params(("id" = i32, Path, description = "Post ID")),
    request_body = UpdatePostRequest,
    responses(
        (status = 200, description = "Post updated", body = ApiResponse<PostResponse>),
        (status = 404, description = "Post not found")
    )
)]
pub async fn update_post(
    State(state): State<AppState>,
    Path(id): Path<i32>,
    Json(payload): Json<UpdatePostRequest>,
) -> Result<Json<ApiResponse<PostResponse>>, StatusCode> {
    let existing = Post::find_by_id(id)
        .one(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
        .ok_or(StatusCode::NOT_FOUND)?;

    let mut active: post::ActiveModel = existing.into();

    if let Some(title) = payload.title {
        active.title = Set(title);
    }
    if let Some(body) = payload.body {
        active.body = Set(body);
    }
    if let Some(published) = payload.published {
        active.published = Set(published);
    }
    active.updated_at = Set(chrono::Utc::now().naive_utc());

    let updated = active
        .update(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(ApiResponse::success(PostResponse::from(updated))))
}

/// Delete a post by ID.
#[utoipa::path(
    delete,
    path = "/api/posts/{id}",
    tag = "posts",
    params(("id" = i32, Path, description = "Post ID")),
    responses(
        (status = 200, description = "Post deleted"),
        (status = 404, description = "Post not found")
    )
)]
pub async fn delete_post(
    State(state): State<AppState>,
    Path(id): Path<i32>,
) -> Result<StatusCode, StatusCode> {
    let result = Post::delete_by_id(id)
        .exec(&state.db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if result.rows_affected == 0 {
        Err(StatusCode::NOT_FOUND)
    } else {
        Ok(StatusCode::OK)
    }
}
