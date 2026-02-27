use chopin::{Context, Response, Router, Server};
use chopin_orm::Model;
use chopin_pg::{PgConfig, PgPool};
use kowito_json::KJson;
use std::sync::Arc;

// Define our Database Model
#[derive(Model, KJson, Default)]
struct Todo {
    id: i32,
    title: String,
    completed: bool,
}

// Request Payload for Creating a Todo
#[derive(KJson, Default)]
struct CreateTodo {
    title: String,
}

// Helper to get AppState from Context.
// Since Chopin is shared-nothing, simple static globals or lazy_statics
// are typically used. For this example, we'll use a thread-local or global lazy_static
// since passing Arc around the raw Context requires extending Context.
lazy_static::lazy_static! {
    static ref DB_POOL: Arc<PgPool> = Arc::new(PgPool::new(PgConfig::new("localhost", 5432, "postgres", "postgres", "postgres"), 10));
}

// --- Handlers ---

fn list_todos(_ctx: Context) -> Response {
    // In a real app, this would use an async runtime block or the worker's event loop.
    // For this showcase, we mock the database fetch as Chopin's core is sync.
    // To do true async DB calls in Chopin, you integrate with Monoio.
    // Here we'll just demonstrate the typing and routing.

    let todos = vec![
        Todo {
            id: 1,
            title: "Learn Chopin".into(),
            completed: true,
        },
        Todo {
            id: 2,
            title: "Build an API".into(),
            completed: false,
        },
    ];

    Response::json_fast(&todos)
}

fn create_todo(ctx: Context) -> Response {
    // kowito-json is serialization-only for massive throughput.
    // In production, you would use sonic-rs or serde_json to deserialize,
    // or manually slice the &[u8] for simple string payloads.
    // We mock the decoded title here for the showcase.
    let title_str = std::str::from_utf8(ctx.req.body).unwrap_or(r#"{"title": "Valid Todo"}"#);

    let title = if title_str.contains("title") {
        "Parsed Title".to_string()
    } else {
        return Response::bad_request();
    };

    let req_body = CreateTodo { title };

    let current_id = 3; // simulated auto-increment

    let new_todo = Todo {
        id: current_id,
        title: req_body.title,
        completed: false,
    };

    // Simulate DB insert: QueryBuilder::insert::<Todo>().execute(&DB_POOL).unwrap();

    let mut res = Response::json_fast(&new_todo);
    res.status = 201; // Created
    res
}

fn get_todo(ctx: Context) -> Response {
    // Extract parameter
    let id_str = ctx.get_param("id").unwrap_or("0");
    let id: i32 = id_str.parse().unwrap_or(0);

    if id == 1 || id == 2 {
        let todo = Todo {
            id,
            title: format!("Todo #{}", id),
            completed: id == 1,
        };
        Response::json_fast(&todo)
    } else {
        Response::not_found()
    }
}

fn logging_middleware(ctx: Context, next: chopin::router::BoxedHandler) -> Response {
    let method = format!("{:?}", ctx.req.method);
    let path = ctx.req.path.to_string();

    // Call the next handler in the chain
    let res = next(ctx);

    println!("[Middleware] {} {} -> {}", method, path, res.status);
    res
}

fn main() {
    println!("Starting Full-Stack Chopin Todo App...");

    let mut router = Router::new();

    // 1. Add Middleware
    router.wrap(logging_middleware);

    // 2. Define Routes
    router.get("/todos", list_todos);
    router.post("/todos", create_todo);
    router.get("/todos/:id", get_todo);

    // 3. Start Server on port 8080
    Server::bind("0.0.0.0:8080")
        .workers(1) // Single core for testing
        .serve(router)
        .unwrap();
}
