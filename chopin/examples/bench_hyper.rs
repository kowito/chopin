use http_body_util::Full;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde::Serialize;
use std::net::SocketAddr;
use tokio::net::TcpListener;

#[derive(Serialize)]
struct Message {
    message: &'static str,
}

async fn handle(req: Request<hyper::body::Incoming>) -> Result<Response<Full<Bytes>>, hyper::Error> {
    match (req.method(), req.uri().path()) {
        (&Method::GET, "/json") => {
            let msg = Message {
                message: "Hello, World!",
            };
            let json = serde_json::to_vec(&msg).unwrap();
            let mut res = Response::new(Full::new(Bytes::from(json)));
            res.headers_mut()
                .insert("Content-Type", "application/json".parse().unwrap());
            res.headers_mut()
                .insert("Server", "Hyper".parse().unwrap());
            Ok(res)
        }
        (&Method::GET, "/plain") => {
            let date = httpdate::fmt_http_date(std::time::SystemTime::now());
            let mut res = Response::new(Full::new(Bytes::from("Hello, World!")));
            res.headers_mut()
                .insert("Content-Type", "text/plain; charset=UTF-8".parse().unwrap());
            res.headers_mut()
                .insert("Server", "Hyper".parse().unwrap());
            res.headers_mut().insert("Date", date.parse().unwrap());
            Ok(res)
        }
        _ => {
            let mut res = Response::new(Full::new(Bytes::from("Not Found")));
            *res.status_mut() = StatusCode::NOT_FOUND;
            Ok(res)
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let workers: usize = std::env::var("WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(num_cpus::get);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(workers)
        .enable_all()
        .build()?;

    rt.block_on(async {
        let addr = SocketAddr::from(([0, 0, 0, 0], 8081));
        let listener = TcpListener::bind(addr).await?;

        println!("Hyper server listening on http://{} with {} workers", addr, workers);

        loop {
            let (stream, _) = listener.accept().await?;
            let io = TokioIo::new(stream);

            tokio::task::spawn(async move {
                if let Err(err) = http1::Builder::new()
                    .serve_connection(io, service_fn(handle))
                    .await
                {
                    eprintln!("Error serving connection: {:?}", err);
                }
            });
        }
    })
}
