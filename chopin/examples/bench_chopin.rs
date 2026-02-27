// examples/bench_chopin.rs
use chopin::{Server, Router, Context, Response};

fn json_handler(_ctx: Context) -> Response {
    let date = httpdate::fmt_http_date(std::time::SystemTime::now());
    Response::json(br#"{"message":"Hello, World!"}"#)
        .header("Server", "Example")
        .header("Date", date)
}

fn plain_handler(_ctx: Context) -> Response {
    let date = httpdate::fmt_http_date(std::time::SystemTime::now());
    let mut res = Response::ok("Hello, World!");
    res.content_type = "text/plain; charset=UTF-8";
    res.header("Server", "Example")
       .header("Date", date)
}

fn main() {
    let mut router = Router::new();
    router.get("/json", json_handler);
    router.get("/plain", plain_handler);

    // Disable print statements for benchmarking by removing the logger_mw
    // and using max workers for throughput.
    Server::bind("0.0.0.0:8080")
        .serve(router)
        .unwrap();
}
