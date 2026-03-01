mod mock_todos_app;

use chopin_core::Chopin;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;

fn setup_magic_server() {
    thread::spawn(|| {
        Chopin::new()
            .mount_all_routes()
            .serve("127.0.0.1:8082")
            .unwrap();
    });

    // Give server time to bind
    thread::sleep(Duration::from_millis(50));
}

#[test]
fn test_magic_mounting_macros() {
    setup_magic_server();

    // 1. GET /todos
    let mut stream = TcpStream::connect("127.0.0.1:8082").unwrap();
    stream
        .write_all(b"GET /todos HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut res = String::new();
    stream.read_to_string(&mut res).unwrap();
    assert!(res.contains("200 OK"));
    assert!(res.contains("list todos"));

    // 2. GET /todos/:id
    let mut stream = TcpStream::connect("127.0.0.1:8082").unwrap();
    stream
        .write_all(b"GET /todos/123 HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut res = String::new();
    stream.read_to_string(&mut res).unwrap();
    assert!(res.contains("200 OK"));
    assert!(res.contains("get todos"));

    // 3. POST /todos
    let mut stream = TcpStream::connect("127.0.0.1:8082").unwrap();
    stream
        .write_all(b"POST /todos HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .unwrap();
    let mut res = String::new();
    stream.read_to_string(&mut res).unwrap();
    assert!(res.contains("200 OK"));
    assert!(res.contains("create todos"));
}
