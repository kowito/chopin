// examples/bench_chopin.rs
use chopin_core::{Context, Response, Router, Server};

#[derive(kowito_json::KJson, Default)]
struct Message {
    message: &'static str,
}

fn json_handler(ctx: Context) -> Response {
    let msg = Message {
        message: "Hello, World!",
    };
    // ctx.json() serializes with the Schema-JIT engine; the worker adds Date/Server headers
    ctx.json(&msg)
}

fn plain_handler(_ctx: Context) -> Response {
    // Response::text() sets Content-Type: text/plain
    Response::text("Hello, World!")
}

fn main() {
    let mut router = Router::new();
    router.get("/json", json_handler);
    router.get("/plain", plain_handler);

    let workers: usize = std::env::var("WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(num_cpus::get);

    println!(
        "Chopin server listening on 0.0.0.0:8080 with {} workers",
        workers
    );

    Server::bind("0.0.0.0:8080")
        .workers(workers)
        .serve(router)
        .unwrap();
}
