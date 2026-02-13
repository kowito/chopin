# API Responses

**Last Updated:** February 2026

## ApiResponse\<T\>

All Chopin handlers return a consistent JSON format via `ApiResponse<T>`:

### Success Response

```json
{
  "success": true,
  "data": { ... },
  "message": null
}
```

### Error Response

```json
{
  "success": false,
  "error": "Error type",
  "message": "Human-readable description"
}
```

## Usage

### Success Responses

```rust
use chopin_core::ApiResponse;

// 200 OK with data
async fn get_item() -> ApiResponse<Item> {
    ApiResponse::success(item)
}

// 201 Created
async fn create_item() -> ApiResponse<Item> {
    ApiResponse::created(item)
}

// 200 with message only (no data)
async fn delete_item() -> ApiResponse<()> {
    ApiResponse::success_message("Item deleted")
}
```

### Error Responses

Handlers can return `Result<ApiResponse<T>, ChopinError>`:

```rust
use chopin_core::{ApiResponse, ChopinError};

async fn get_item(
    Path(id): Path<i32>,
) -> Result<ApiResponse<Item>, ChopinError> {
    let item = find_item(id).await
        .ok_or(ChopinError::NotFound("Item not found".into()))?;
    Ok(ApiResponse::success(item))
}
```

## ChopinError

Built-in error types that map to HTTP status codes:

| Error | Status | Usage |
|-------|--------|-------|
| `NotFound(String)` | 404 | Resource not found |
| `BadRequest(String)` | 400 | Invalid input |
| `Unauthorized(String)` | 401 | Missing or invalid auth |
| `Forbidden(String)` | 403 | Insufficient permissions |
| `Validation(ValidationErrors)` | 422 | Field validation failures |
| `Conflict(String)` | 409 | Duplicate resource |
| `Internal(String)` | 500 | Server error |
| `Database(DbErr)` | 500 | Database error |

### Example

```rust
use chopin_core::ChopinError;

// Direct return
async fn handler() -> Result<ApiResponse<()>, ChopinError> {
    Err(ChopinError::BadRequest("Invalid email format".into()))
}

// With the ? operator (database errors auto-convert)
async fn query(State(state): State<AppState>) -> Result<ApiResponse<Vec<Item>>, ChopinError> {
    let items = Items::find().all(&state.db).await?; // DbErr → ChopinError::Database
    Ok(ApiResponse::success(items))
}
```

## Serialization

All responses are serialized with **sonic-rs** (ARM NEON / x86 AVX2 optimized), not `serde_json`. This happens automatically in `IntoResponse` implementations — no manual serialization needed.

## OpenAPI Integration

Add `utoipa::ToSchema` to your response types:

```rust
#[derive(Serialize, utoipa::ToSchema)]
pub struct ItemResponse {
    pub id: i32,
    pub name: String,
}
```

Document endpoints:

```rust
#[utoipa::path(
    get,
    path = "/api/items/{id}",
    tag = "items",
    params(("id" = i32, Path, description = "Item ID")),
    responses(
        (status = 200, body = ApiResponse<ItemResponse>),
        (status = 404, description = "Item not found"),
    ),
    security(("bearer_auth" = []))
)]
async fn get_item(...) -> Result<ApiResponse<ItemResponse>, ChopinError> { ... }
```

The interactive Scalar UI is available at `/api-docs`.
