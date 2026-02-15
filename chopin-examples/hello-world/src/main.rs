//! # Chopin Hello World — with OpenAPI
//!
//! A minimal Chopin app with **3 custom endpoints** fully documented in
//! the Scalar OpenAPI UI. No extra crates needed — `chopin_core::prelude`
//! re-exports `OpenApi`, `ToSchema`, `Scalar`, and `SecurityAddon`.
//!
//! ## Run
//!
//! ```bash
//! cargo run -p chopin-hello-world
//! ```
//!
//! ## Endpoints
//!
//! | Method | Path              | Description          |
//! |--------|-------------------|----------------------|
//! | GET    | `/api/items`      | List all items       |
//! | POST   | `/api/items`      | Create a new item    |
//! | GET    | `/api/items/{id}` | Get item by ID       |
//! | GET    | `/api-docs`       | Scalar OpenAPI UI    |
//! | POST   | `/api/auth/signup` | Built-in auth       |
//! | POST   | `/api/auth/login`  | Built-in auth       |

use chopin_core::prelude::*;
use chopin_core::response::ApiResponse;

// ═══════════════════════════════════════════════════════════════
// Step 1: Define request/response types with #[derive(ToSchema)]
// ═══════════════════════════════════════════════════════════════

/// An item in the store.
#[derive(Debug, Serialize, Deserialize, ToSchema)]
pub struct Item {
    /// Unique item ID
    #[schema(example = 1)]
    pub id: i32,
    /// Item name
    #[schema(example = "Notebook")]
    pub name: String,
    /// Item price in cents
    #[schema(example = 999)]
    pub price: i32,
    /// Whether the item is in stock
    #[schema(example = true)]
    pub in_stock: bool,
}

/// Request body for creating a new item.
#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateItemRequest {
    /// Item name (required)
    #[schema(example = "Notebook")]
    pub name: String,
    /// Item price in cents (required)
    #[schema(example = 999)]
    pub price: i32,
}

// ═══════════════════════════════════════════════════════════════
// Step 2: Write handlers with #[utoipa::path(...)] annotations
// ═══════════════════════════════════════════════════════════════

/// List all items.
#[utoipa::path(
    get,
    path = "/api/items",
    tag = "items",
    responses(
        (status = 200, description = "All items", body = ApiResponse<Vec<Item>>)
    )
)]
async fn list_items() -> Json<ApiResponse<Vec<Item>>> {
    let items = vec![
        Item {
            id: 1,
            name: "Notebook".into(),
            price: 999,
            in_stock: true,
        },
        Item {
            id: 2,
            name: "Pen".into(),
            price: 199,
            in_stock: true,
        },
        Item {
            id: 3,
            name: "Eraser".into(),
            price: 50,
            in_stock: false,
        },
    ];
    Json(ApiResponse::success(items))
}

/// Create a new item.
#[utoipa::path(
    post,
    path = "/api/items",
    tag = "items",
    request_body = CreateItemRequest,
    responses(
        (status = 201, description = "Item created", body = ApiResponse<Item>),
        (status = 400, description = "Invalid input")
    ),
    security(("bearer_auth" = []))
)]
async fn create_item(
    Json(payload): Json<CreateItemRequest>,
) -> (StatusCode, Json<ApiResponse<Item>>) {
    let item = Item {
        id: 42,
        name: payload.name,
        price: payload.price,
        in_stock: true,
    };
    (StatusCode::CREATED, Json(ApiResponse::success(item)))
}

/// Get a single item by ID.
#[utoipa::path(
    get,
    path = "/api/items/{id}",
    tag = "items",
    params(
        ("id" = i32, Path, description = "Item ID")
    ),
    responses(
        (status = 200, description = "Item found", body = ApiResponse<Item>),
        (status = 404, description = "Item not found")
    )
)]
async fn get_item(Path(id): Path<i32>) -> Result<Json<ApiResponse<Item>>, StatusCode> {
    // Dummy: return a fake item for any ID
    if id <= 0 {
        return Err(StatusCode::NOT_FOUND);
    }
    let item = Item {
        id,
        name: format!("Item #{}", id),
        price: id * 100,
        in_stock: true,
    };
    Ok(Json(ApiResponse::success(item)))
}

// ═══════════════════════════════════════════════════════════════
// Step 3: Create a #[derive(OpenApi)] struct listing paths/schemas
// ═══════════════════════════════════════════════════════════════

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Chopin Hello World",
        version = "1.0.0",
        description = "A minimal API with 3 endpoints and OpenAPI docs"
    ),
    paths(list_items, create_item, get_item),
    components(schemas(
        Item,
        CreateItemRequest,
        ApiResponse<Item>,
        ApiResponse<Vec<Item>>,
    )),
    tags(
        (name = "items", description = "Item endpoints")
    ),
    security(("bearer_auth" = [])),
    modifiers(&SecurityAddon)  // ← adds JWT Bearer scheme to the spec
)]
struct MyApiDoc;

// ═══════════════════════════════════════════════════════════════
// Step 4: Build a Router, pass it + your spec to App — done!
// ═══════════════════════════════════════════════════════════════

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();

    // Custom routes
    let items = Router::new()
        .route("/api/items", get(list_items).post(create_item))
        .route("/api/items/{id}", get(get_item));

    let app = chopin_core::App::new()
        .await?
        .routes(items) // mount your endpoints
        .api_docs(MyApiDoc::openapi()); // merge OpenAPI docs

    app.run().await?;

    Ok(())
}

// Visit http://127.0.0.1:3000/api-docs to see the Scalar UI with all endpoints:
//   auth  → POST /api/auth/signup, POST /api/auth/login  (built-in, auto-merged)
//   items → GET /api/items, POST /api/items, GET /api/items/{id}  (your endpoints)
