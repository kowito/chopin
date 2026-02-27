// examples/bench_hyper.rs
use std::convert::Infallible;
use std::net::SocketAddr;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper::body::Bytes;
use http_body_util::Full;
use tokio::net::TcpListener;

async fn handle(req: Request<impl hyper::body::Body>) -> Result<Response<Full<Bytes>>, Infallible> {
    match req.uri().path() {
        "/json" => {
            let body = Full::new(Bytes::from(r#"{"message":"Hello, World!"}"#));
            let res = Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "application/json")
                .header("Server", "Example")
                .header("Date", "Wed, 17 Apr 2013 12:00:00 GMT")
                .body(body)
                .unwrap();
            Ok(res)
        }
        "/plain" => {
            let body = Full::new(Bytes::from("Hello, World!"));
            let res = Response::builder()
                .status(StatusCode::OK)
                .header("Content-Type", "text/plain; charset=UTF-8")
                .header("Server", "Example")
                .header("Date", "Wed, 17 Apr 2013 12:00:00 GMT")
                .body(body)
                .unwrap();
            Ok(res)
        }
        _ => {
            let res = Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Full::new(Bytes::from("Not Found")))
                .unwrap();
            Ok(res)
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let addr = SocketAddr::from(([0, 0, 0, 0], 8081));
    let listener = TcpListener::bind(addr).await?;

    loop {
        let (stream, _) = listener.accept().await?;
        let io = hyper_util::rt::TokioIo::new(stream);

        tokio::task::spawn(async move {
            if let Err(err) = http1::Builder::new()
                .serve_connection(io, service_fn(handle))
                .await
            {
                eprintln!("Error serving connection: {:?}", err);
            }
        });
    }
}
