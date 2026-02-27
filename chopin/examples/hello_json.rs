// examples/hello_json.rs
use chopin::{Server, Router};
use chopin::http::{Context, Response};
// use kowito_json::KJson;

// #[derive(KJson)]
// struct HelloMessage {
//     message: String,
// }

fn hello_json(_ctx: Context) -> Response {
    // let msg = HelloMessage { message: "Hello, World!".to_string() };
    
    // We would serialize directly into the Response buf in a real zero-allocation app 
    // but for now we allocate into a vec.
    // let mut buf = Vec::new();
    // kowito_json::serialize::write_value(&msg, &mut buf);
    
    Response::json(br#"{"message":"Hello, World!"}"#)
}

fn hello_text(ctx: Context) -> Response {
    let name = ctx.params.get("name").map(|s| s.as_str()).unwrap_or("World");
    let uppercase = ctx.req.query.unwrap_or("") == "upper=true";
    let user_agent = ctx.req.headers.iter().find(|(k, _)| k.eq_ignore_ascii_case("User-Agent")).map(|(_, v)| *v).unwrap_or("Unknown");
    
    let mut greeting = format!("Hello, {}! You are using {}.", name, user_agent);
    if uppercase {
        greeting = greeting.to_uppercase();
    }
    Response::ok(greeting)
}

fn main() {
    let mut router = Router::new();
    router.get("/hello", hello_json);
    router.get("/hello/:name", hello_text);

    println!("Starting Chopin on 0.0.0.0:8082...");
    Server::bind("0.0.0.0:8082")
        .workers(1) // Just 1 for testing Mac
        .serve(router)
        .unwrap();
}
