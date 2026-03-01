use chopin_core::{get, Chopin, Context, KJson, Method, Response};

#[derive(KJson)]
struct Message {
    message: &'static str,
}

#[get("/json")]
fn json(_ctx: Context) -> Response {
    let msg = Message {
        message: "Hello, World!",
    };
    Response::json(&msg)
}

#[get("/plaintext")]
fn plaintext(_ctx: Context) -> Response {
    Response::text(b"Hello, World!".to_vec())
}

fn main() {
    println!("Starting local TFB test server on http://localhost:8000");
    Chopin::new()
        .mount_all_routes()
        .route(Method::Get, "/json", json)
        .route(Method::Get, "/plaintext", plaintext)
        .serve("0.0.0.0:8000")
        .expect("server failed");
}
