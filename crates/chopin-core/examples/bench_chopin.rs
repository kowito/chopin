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
    // ctx.respond_json handles the serialization and engine adds Date/Server
    ctx.respond_json(&msg)
}

fn plain_handler(_ctx: Context) -> Response {
    // Response::ok handles text/plain and engine adds Date/Server
    Response::ok("Hello, World!")
}

fn main() {
    let mut router = Router::new();
    router.get("/json", json_handler);
    router.get("/plain", plain_handler);

    let workers: usize = std::env::var("WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(num_cpus::get);

    println!("Chopin server listening on 0.0.0.0:8080 with {} workers", workers);

    Server::bind("0.0.0.0:8080")
        .workers(workers)
        .serve(router)
        .unwrap();
}
