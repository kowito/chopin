use chopin_core::{Context, Method, Response, Router, Server};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;

fn setup_test_server() {
    let mut router = Router::new();

    // Simple GET
    router.add(Method::Get, "/hello", |_: Context| {
        Response::ok("Hello, World!")
    });

    // Echo param
    router.add(Method::Get, "/echo/:msg", |ctx: Context| {
        let msg = ctx.get_param("msg").unwrap_or("missing");
        Response::ok(format!("Echo: {}", msg))
    });

    // Chunked response
    router.add(Method::Get, "/stream", |_: Context| {
        let items = vec![b"chunk1".to_vec(), b"chunk2".to_vec()];
        Response::stream(items.into_iter())
    });

    // Chunked request
    router.add(Method::Post, "/upload", |ctx: Context| {
        Response::ok(format!("Received {} bytes", ctx.req.body.len()))
    });

    thread::spawn(|| {
        let server = Server::bind("127.0.0.1:8081").workers(1);
        server.serve(router).unwrap();
    });

    // Give server time to bind
    thread::sleep(Duration::from_millis(50));
}

#[test]
fn test_integration_endpoints() {
    setup_test_server();

    // 1. Simple GET
    let mut stream = TcpStream::connect("127.0.0.1:8081").unwrap();
    stream
        .write_all(b"GET /hello HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut res = String::new();
    stream.read_to_string(&mut res).unwrap();
    assert!(res.contains("200 OK"));
    assert!(res.contains("Hello, World!"));

    // 2. Param echo
    let mut stream = TcpStream::connect("127.0.0.1:8081").unwrap();
    stream
        .write_all(
            b"GET /echo/integration_test HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
        )
        .unwrap();
    let mut res = String::new();
    stream.read_to_string(&mut res).unwrap();
    assert!(res.contains("Echo: integration_test"));

    // 3. Chunked response parsing check
    let mut stream = TcpStream::connect("127.0.0.1:8081").unwrap();
    stream
        .write_all(b"GET /stream HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut res = String::new();
    stream.read_to_string(&mut res).unwrap();
    assert!(res.contains("Transfer-Encoding: chunked"));
    assert!(res.contains("6\r\nchunk1\r\n"));
    assert!(res.contains("6\r\nchunk2\r\n"));
    assert!(res.contains("0\r\n\r\n"));

    // 4. Chunked request (Transfer-Encoding: chunked)
    let mut stream = TcpStream::connect("127.0.0.1:8081").unwrap();
    stream.write_all(b"POST /upload HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n0\r\n\r\n").unwrap();
    let mut res = String::new();
    stream.read_to_string(&mut res).unwrap();
    assert!(res.contains("Received 5 bytes"));
}
