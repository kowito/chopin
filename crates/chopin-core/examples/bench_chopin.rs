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
    ctx.respond_json(&msg).header("Server", "Chopin/Kowito")
}

fn plain_handler(_ctx: Context) -> Response {
    let date = httpdate::fmt_http_date(std::time::SystemTime::now());
    let mut res = Response::ok("Hello, World!");
    res.content_type = "text/plain; charset=UTF-8";
    res.header("Server", "Example").header("Date", date)
}

fn main() {
    let mut router = Router::new();
    router.get("/json", json_handler);
    router.get("/plain", plain_handler);

    // Disable print statements for benchmarking by removing the logger_mw
    // and using max workers for throughput.
    let workers: usize = std::env::var("WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(num_cpus::get);

    Server::bind("0.0.0.0:8080")
        .workers(workers)
        .serve(router)
        .unwrap();
}
