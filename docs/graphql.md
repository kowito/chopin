# GraphQL

**Last Updated:** February 2026

> Requires the `graphql` feature flag.

## Setup

```toml
# Cargo.toml
[dependencies]
chopin-core = { version = "0.1", features = ["graphql"] }
```

## Overview

Chopin integrates with **async-graphql 7.x** for optional GraphQL support. When enabled, it provides:

- GraphQL query/mutation handlers
- Playground UI at `/graphql/playground`
- Schema builder utilities

## Defining a Schema

```rust
use async_graphql::{Object, Schema, EmptySubscription};
use sea_orm::DatabaseConnection;

pub struct QueryRoot;

#[Object]
impl QueryRoot {
    async fn hello(&self) -> &str {
        "Hello from Chopin GraphQL!"
    }

    async fn user(
        &self,
        ctx: &async_graphql::Context<'_>,
        id: i32,
    ) -> async_graphql::Result<Option<UserResponse>> {
        let db = ctx.data::<DatabaseConnection>()?;
        let user = user::Entity::find_by_id(id).one(db).await?;
        Ok(user.map(UserResponse::from))
    }
}

pub struct MutationRoot;

#[Object]
impl MutationRoot {
    async fn create_post(
        &self,
        ctx: &async_graphql::Context<'_>,
        title: String,
        body: String,
    ) -> async_graphql::Result<PostResponse> {
        let db = ctx.data::<DatabaseConnection>()?;
        // Insert into database...
        Ok(post_response)
    }
}

pub type AppSchema = Schema<QueryRoot, MutationRoot, EmptySubscription>;
```

## Adding Routes

```rust
use chopin_core::graphql;

// In your router setup
let schema = Schema::build(QueryRoot, MutationRoot, EmptySubscription)
    .data(db.clone())
    .finish();

let router = Router::new()
    .merge(graphql::routes(schema));
```

This adds:
- `POST /graphql` — Query endpoint
- `GET /graphql/playground` — GraphQL Playground UI

## Usage

```graphql
# Query
query {
  hello
  user(id: 1) {
    id
    email
    username
  }
}

# Mutation
mutation {
  createPost(title: "Hello", body: "World") {
    id
    title
  }
}
```

## Authentication in GraphQL

Pass the auth context through the schema:

```rust
use chopin_core::auth::jwt;

async fn graphql_handler(
    headers: HeaderMap,
    State(schema): State<AppSchema>,
    req: async_graphql_axum::GraphQLRequest,
) -> async_graphql_axum::GraphQLResponse {
    let mut request = req.into_inner();

    // Extract user from JWT if present
    if let Some(auth) = headers.get("Authorization") {
        if let Ok(claims) = jwt::validate_token(auth.to_str().unwrap(), &secret) {
            request = request.data(claims);
        }
    }

    schema.execute(request).await.into()
}
```
