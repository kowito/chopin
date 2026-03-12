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
    // text_static passes the body slice directly — zero heap allocation on the hot path.
    Response::text_static(b"Hello, World!")
}

fn main() {
    let mut router = Router::new();
    router.get("/json", json_handler);
    router.get("/plaintext", plain_handler);
    router.get("/plain", plain_handler); // legacy alias

    let workers: usize = std::env::var("WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(num_cpus::get);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8080);

    println!("Chopin server listening on 0.0.0.0:{port} with {workers} workers");

    Server::bind(&format!("0.0.0.0:{port}"))
        .workers(workers)
        .serve(router)
        .unwrap();
}
