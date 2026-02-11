# GraphQL Support

Chopin provides optional GraphQL support via `async-graphql`, enabling you to build GraphQL APIs alongside your REST endpoints.

## Enabling GraphQL

**1. Add the feature flag:**

```toml
# Cargo.toml
[dependencies]
chopin-core = { version = "0.1", features = ["graphql"] }
```

**2. Import the necessary types:**

```rust
use chopin_core::graphql::{async_graphql, graphql_routes};
use async_graphql::{Object, Schema, EmptyMutation, EmptySubscription, Context};
```

## Quick Start

### 1. Define Your Schema

```rust
use async_graphql::{Object, SimpleObject};

#[derive(SimpleObject)]
struct User {
    id: i32,
    username: String,
    email: String,
}

struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn hello(&self) -> &str {
        "Hello from Chopin GraphQL!"
    }
    
    async fn user(&self, ctx: &Context<'_>, id: i32) -> Result<User, String> {
        // Access database via context
        let db = ctx.data::<DatabaseConnection>()
            .map_err(|_| "Database not available")?;
        
        // Query user (simplified)
        Ok(User {
            id,
            username: "alice".to_string(),
            email: "alice@example.com".to_string(),
        })
    }
}
```

### 2. Build and Mount Schema

```rust
use async_graphql::{EmptyMutation, EmptySubscription, Schema};
use chopin_core::graphql::graphql_routes;

pub fn build_graphql_schema(db: DatabaseConnection) -> Schema<QueryRoot, EmptyMutation, EmptySubscription> {
    Schema::build(QueryRoot, EmptyMutation, EmptySubscription)
        .data(db)  // Inject database connection
        .finish()
}

// In your router
pub fn routes(state: AppState) -> Router<AppState> {
    let schema = build_graphql_schema(state.db.clone());
    
    Router::new()
        .merge(graphql_routes(schema))
        // Your other routes...
}
```

### 3. Access GraphQL

**Endpoint:** `POST /graphql`

**Playground:** `GET /graphql` (GraphiQL interface)

**Example query:**

```graphql
query {
  hello
  user(id: 1) {
    id
    username
    email
  }
}
```

## Complete Example

```rust
use async_graphql::{Context, Object, Schema, SimpleObject, EmptyMutation, EmptySubscription};
use sea_orm::{EntityTrait, DatabaseConnection};
use chopin_core::models::user::{Entity as User, Model as UserModel};

// GraphQL types
#[derive(SimpleObject)]
#[graphql(name = "User")]
struct GqlUser {
    id: i32,
    username: String,
    email: String,
}

impl From<UserModel> for GqlUser {
    fn from(model: UserModel) -> Self {
        GqlUser {
            id: model.id,
            username: model.username,
            email: model.email,
        }
    }
}

// Query root
struct QueryRoot;

#[Object]
impl QueryRoot {
    /// Get all users
    async fn users(&self, ctx: &Context<'_>) -> Result<Vec<GqlUser>, String> {
        let db = ctx.data::<DatabaseConnection>()
            .map_err(|_| "Database not available".to_string())?;
        
        let users = User::find()
            .all(db)
            .await
            .map_err(|e| format!("Database error: {}", e))?;
        
        Ok(users.into_iter().map(GqlUser::from).collect())
    }
    
    /// Get user by ID
    async fn user(&self, ctx: &Context<'_>, id: i32) -> Result<Option<GqlUser>, String> {
        let db = ctx.data::<DatabaseConnection>()
            .map_err(|_| "Database not available".to_string())?;
        
        let user = User::find_by_id(id)
            .one(db)
            .await
            .map_err(|e| format!("Database error: {}", e))?;
        
        Ok(user.map(GqlUser::from))
    }
}

// Mutation root
struct MutationRoot;

#[Object]
impl MutationRoot {
    /// Update username
    async fn update_username(
        &self,
        ctx: &Context<'_>,
        id: i32,
        new_username: String,
    ) -> Result<GqlUser, String> {
        let db = ctx.data::<DatabaseConnection>()
            .map_err(|_| "Database not available".to_string())?;
        
        let user = User::find_by_id(id)
            .one(db)
            .await
            .map_err(|e| format!("Database error: {}", e))?
            .ok_or_else(|| "User not found".to_string())?;
        
        let mut active: user::ActiveModel = user.into();
        active.username = Set(new_username);
        
        let updated = active.update(db)
            .await
            .map_err(|e| format!("Update failed: {}", e))?;
        
        Ok(GqlUser::from(updated))
    }
}

// Build schema
pub fn build_schema(db: DatabaseConnection) -> Schema<QueryRoot, MutationRoot, EmptySubscription> {
    Schema::build(QueryRoot, MutationRoot, EmptySubscription)
        .data(db)
        .finish()
}
```

## Authentication

### Method 1: Context Extensions

Extract JWT from headers and inject into context:

```rust
use async_graphql::*;
use axum::http::HeaderMap;

struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn me(&self, ctx: &Context<'_>) -> Result<GqlUser, String> {
        // Get authenticated user ID from context
        let user_id = ctx.data::<i32>()
            .map_err(|_| "Not authenticated")?;
        
        // Fetch user...
    }
}

// In your GraphQL handler setup:
async fn graphql_handler(
    headers: HeaderMap,
    schema: Extension<Schema<QueryRoot, MutationRoot, EmptySubscription>>,
    req: GraphQLRequest,
) -> GraphQLResponse {
    let mut req = req.into_inner();
    
    // Extract and validate JWT
    if let Some(auth) = headers.get("authorization") {
        if let Ok(token) = auth.to_str() {
            if let Some(bearer) = token.strip_prefix("Bearer ") {
                if let Ok(claims) = validate_token(bearer, &jwt_secret) {
                    if let Ok(user_id) = claims.sub.parse::<i32>() {
                        req = req.data(user_id);
                    }
                }
            }
        }
    }
    
    schema.execute(req).await.into()
}
```

### Method 2: Guard Directive

Use async-graphql guards:

```rust
use async_graphql::*;

struct RoleGuard {
    role: String,
}

#[async_trait::async_trait]
impl Guard for RoleGuard {
    async fn check(&self, ctx: &Context<'_>) -> Result<()> {
        let user_role = ctx.data::<String>()
            .map_err(|_| "Not authenticated")?;
        
        if user_role != &self.role {
            return Err("Insufficient permissions".into());
        }
        
        Ok(())
    }
}

#[Object]
impl QueryRoot {
    #[graphql(guard = "RoleGuard { role: \"admin\".to_string() }")]
    async fn admin_data(&self) -> String {
        "Secret admin data".to_string()
    }
}
```

## DataLoader Pattern

Optimize N+1 queries with DataLoader:

```rust
use async_graphql::dataloader::*;
use sea_orm::{EntityTrait, DatabaseConnection};

struct UserLoader {
    db: DatabaseConnection,
}

#[async_trait::async_trait]
impl Loader<i32> for UserLoader {
    type Value = UserModel;
    type Error = Arc<sea_orm::DbErr>;
    
    async fn load(&self, keys: &[i32]) -> Result<HashMap<i32, Self::Value>, Self::Error> {
        let users = User::find()
            .filter(user::Column::Id.is_in(keys.to_vec()))
            .all(&self.db)
            .await
            .map_err(Arc::new)?;
        
        Ok(users.into_iter().map(|u| (u.id, u)).collect())
    }
}

// Use in schema
let loader = DataLoader::new(UserLoader { db: db.clone() }, tokio::spawn);

Schema::build(QueryRoot, EmptyMutation, EmptySubscription)
    .data(loader)
    .finish()
```

## Error Handling

```rust
use async_graphql::{Error, ErrorExtensions};

#[Object]
impl QueryRoot {
    async fn user(&self, ctx: &Context<'_>, id: i32) -> Result<GqlUser> {
        let db = ctx.data::<DatabaseConnection>()?;
        
        let user = User::find_by_id(id)
            .one(db)
            .await
            .map_err(|e| Error::new(format!("Database error: {}", e))
                .extend_with(|_, e| e.set("code", "DB_ERROR")))?
            .ok_or_else(|| Error::new("User not found")
                .extend_with(|_, e| e.set("code", "NOT_FOUND")))?;
        
        Ok(GqlUser::from(user))
    }
}
```

## Subscriptions

Real-time updates via WebSocket:

```rust
use async_graphql::*;
use futures_util::stream::Stream;

struct SubscriptionRoot;

#[Subscription]
impl SubscriptionRoot {
    async fn user_updates(&self, id: i32) -> impl Stream<Item = GqlUser> {
        // Create a stream that emits user updates
        tokio_stream::wrappers::BroadcastStream::new(/* ... */)
    }
}

// Build schema with subscriptions
Schema::build(QueryRoot, MutationRoot, SubscriptionRoot)
    .data(db)
    .finish()
```

## GraphQL + REST Integration

Use both GraphQL and REST endpoints:

```rust
use axum::Router;
use chopin_core::graphql::graphql_routes;

pub fn routes(state: AppState) -> Router<AppState> {
    let schema = build_graphql_schema(state.db.clone());
    
    Router::new()
        // REST endpoints
        .nest("/api/auth", auth::routes())
        .nest("/api/posts", posts::routes())
        
        // GraphQL endpoint
        .merge(graphql_routes(schema))
        
        .with_state(state)
}
```

Now you have:
- REST: `POST /api/auth/login`, `GET /api/posts`
- GraphQL: `POST /graphql`, `GET /graphql` (playground)

## Schema Introspection

GraphQL schemas are self-documenting. The playground (`GET /graphql`) provides:
- Interactive query builder
- Auto-completion
- Schema documentation
- Query history

## Testing GraphQL

```rust
#[tokio::test]
async fn test_graphql_query() {
    let app = TestApp::new().await;
    
    let query = r#"
        query {
            users {
                id
                username
            }
        }
    "#;
    
    let res = app.client
        .post(&app.url("/graphql"))
        .header("Content-Type", "application/json")
        .body(serde_json::json!({ "query": query }).to_string())
        .send()
        .await;
    
    assert_eq!(res.status, 200);
    let json: serde_json::Value = serde_json::from_str(&res.body).unwrap();
    assert!(json["data"]["users"].is_array());
}
```

## Best Practices

### 1. Use SimpleObject for Read-Only Types

```rust
#[derive(SimpleObject)]
struct User {
    id: i32,
    username: String,
}
```

### 2. Implement From Traits

Convert database models to GraphQL types:

```rust
impl From<UserModel> for GqlUser {
    fn from(model: UserModel) -> Self {
        GqlUser { id: model.id, username: model.username }
    }
}
```

### 3. Use DataLoader for Relationships

Avoid N+1 queries when fetching related entities.

### 4. Validate Input

```rust
#[Object]
impl MutationRoot {
    async fn create_user(&self, username: String) -> Result<GqlUser> {
        if username.len() < 3 {
            return Err("Username too short".into());
        }
        // ...
    }
}
```

### 5. Document Your Schema

```rust
#[Object]
impl QueryRoot {
    /// Get user by ID
    ///
    /// Returns None if user doesn't exist
    async fn user(&self, #[graphql(desc = "User ID")] id: i32) -> Result<Option<GqlUser>> {
        // ...
    }
}
```

## Resources

- [`async-graphql` documentation](https://async-graphql.github.io/async-graphql/)
- [GraphQL specification](https://spec.graphql.org/)
- [GraphQL best practices](https://graphql.org/learn/best-practices/)

## When to Use GraphQL vs REST

**Use GraphQL when:**
- Clients need flexible queries
- Mobile apps with limited bandwidth
- Complex nested data structures
- Multiple client types (web, mobile, etc.)

**Use REST when:**
- Simple CRUD operations
- File uploads  
- Caching is critical
- Team unfamiliar with GraphQL

**Use both:** Chopin makes it easy to offer both REST and GraphQL APIs side-by-side!
