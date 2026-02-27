// examples/hello_json.rs
use chopin::{Context, Json, Response, Router, Server};
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct UserPayload<'a> {
    name: &'a str,
    age: u32,
}

fn hello_json(_ctx: Context) -> Response {
    Response::json(br#"{"message":"Hello, World!"}"#)
}

fn create_user(ctx: Context) -> Response {
    let Json(payload) = match ctx.extract::<Json<UserPayload>>() {
        Ok(j) => j,
        Err(e) => return e,
    };

    Response::ok(format!(
        "Created user '{}' age {}",
        payload.name, payload.age
    ))
}

fn hello_text(ctx: Context) -> Response {
    let name = ctx
        .params
        .iter()
        .find(|(k, _)| *k == "name")
        .map(|(_, v)| *v)
        .unwrap_or("World");
    let uppercase = ctx.req.query.unwrap_or("") == "upper=true";
    let user_agent = ctx
        .req
        .headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case("User-Agent"))
        .map(|(_, v)| *v)
        .unwrap_or("Unknown");

    let mut greeting = format!("Hello, {}! You are using {}.", name, user_agent);
    if uppercase {
        greeting = greeting.to_uppercase();
    }
    Response::ok(greeting)
}

fn panic_handler(_ctx: Context) -> Response {
    panic!("This is a deliberate panic to test recovery!");
}

fn stream_handler(_ctx: Context) -> Response {
    let iter = (0..5).map(|i| format!("Chunk {}\n", i).into_bytes());
    Response::stream(iter)
}

fn logger_mw(ctx: Context, next: fn(Context) -> Response) -> Response {
    let method = format!("{:?}", ctx.req.method);
    let path = ctx.req.path.to_string();
    let start = std::time::Instant::now();

    let res = next(ctx);

    println!(
        "{} {} -> {} in {:?}",
        method,
        path,
        res.status,
        start.elapsed()
    );
    res
}

fn main() {
    let mut router = Router::new();
    router.wrap(logger_mw);
    router.get("/hello", hello_json);
    router.get("/hello/:name", hello_text);
    router.post("/users", create_user);
    router.get("/stream", stream_handler);
    router.get("/panic", panic_handler);

    println!("Starting Chopin on 0.0.0.0:8082...");
    Server::bind("0.0.0.0:8082")
        .workers(1) // Just 1 for testing Mac
        .serve(router)
        .unwrap();
}
