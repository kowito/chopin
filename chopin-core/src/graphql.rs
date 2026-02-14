//! GraphQL support for Chopin applications.
//!
//! Enable with the `graphql` feature flag in your `Cargo.toml`:
//!
//! ```toml
//! chopin-core = { version = "0.1", features = ["graphql"] }
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use async_graphql::{Object, Schema, EmptyMutation, EmptySubscription};
//! use chopin::graphql::graphql_routes;
//!
//! struct QueryRoot;
//!
//! #[Object]
//! impl QueryRoot {
//!     async fn hello(&self) -> &str {
//!         "Hello from Chopin GraphQL!"
//!     }
//! }
//!
//! let schema = Schema::build(QueryRoot, EmptyMutation, EmptySubscription)
//!     .finish();
//!
//! // Add to your router:
//! let graphql = graphql_routes(schema);
//! ```

#[cfg(feature = "graphql")]
pub use async_graphql;
#[cfg(feature = "graphql")]
pub use async_graphql_axum;

/// Create GraphQL routes (handler + playground) for an async-graphql schema.
///
/// This sets up:
/// - `POST /graphql` — the GraphQL endpoint
/// - `GET /graphql` — GraphQL Playground UI
#[cfg(feature = "graphql")]
pub fn graphql_routes<Q, M, S>(schema: async_graphql::Schema<Q, M, S>) -> axum::Router
where
    Q: async_graphql::ObjectType + 'static,
    M: async_graphql::ObjectType + 'static,
    S: async_graphql::SubscriptionType + 'static,
{
    use axum::response::{Html, IntoResponse};
    use axum::routing::{get, post};

    async fn graphql_playground() -> impl IntoResponse {
        Html(
            r#"<!DOCTYPE html>
<html>
<head>
    <title>Chopin GraphQL Playground</title>
    <link rel="stylesheet" href="https://unpkg.com/graphiql/graphiql.min.css" />
</head>
<body style="margin: 0;">
    <div id="graphiql" style="height: 100vh;"></div>
    <script crossorigin src="https://unpkg.com/react/umd/react.production.min.js"></script>
    <script crossorigin src="https://unpkg.com/react-dom/umd/react-dom.production.min.js"></script>
    <script crossorigin src="https://unpkg.com/graphiql/graphiql.min.js"></script>
    <script>
        const fetcher = GraphiQL.createFetcher({ url: '/graphql' });
        ReactDOM.render(
            React.createElement(GraphiQL, { fetcher }),
            document.getElementById('graphiql'),
        );
    </script>
</body>
</html>"#,
        )
    }

    async fn graphql_handler<Q2, M2, S2>(
        schema: axum::extract::Extension<async_graphql::Schema<Q2, M2, S2>>,
        req: async_graphql_axum::GraphQLRequest,
    ) -> async_graphql_axum::GraphQLResponse
    where
        Q2: async_graphql::ObjectType + 'static,
        M2: async_graphql::ObjectType + 'static,
        S2: async_graphql::SubscriptionType + 'static,
    {
        schema.execute(req.into_inner()).await.into()
    }

    axum::Router::new()
        .route("/graphql", post(graphql_handler::<Q, M, S>))
        .route("/graphql", get(graphql_playground))
        .layer(axum::extract::Extension(schema))
}
