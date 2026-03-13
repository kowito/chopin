// examples/bench_hyper.rs
//
// Hyper plaintext server — thread-per-core, SO_REUSEPORT.  Port: 8081.
//
// Architecture: N kernel threads, each owning a dedicated single-threaded Tokio
// runtime with its own SO_REUSEPORT listener on the same port.  The kernel
// distributes incoming connections across all listeners without any user-space
// locking, matching Chopin's own design and the TFB hyper reference implementation.
// HTTP/1.1 pipelining is handled transparently by hyper's http1 builder.
//
// Run with:
//   cargo run --release --example bench_hyper
//   WORKERS=8 cargo run --release --example bench_hyper
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::header::HeaderValue;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::os::unix::io::FromRawFd;
use std::{io, mem, thread};
use tokio::net::TcpListener;

fn listen_port() -> u16 {
    std::env::var("PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8081)
}

// Static response bodies — zero heap allocation on the hot path.
static PLAIN_BODY: &[u8] = b"Hello, World!";
static JSON_BODY: &[u8] = br#"{"message":"Hello, World!"}"#;

async fn handle(
    req: Request<hyper::body::Incoming>,
) -> Result<Response<Full<Bytes>>, hyper::Error> {
    let date = httpdate::fmt_http_date(std::time::SystemTime::now());
    let date_val = HeaderValue::from_str(&date).unwrap_or_else(|_| HeaderValue::from_static(""));

    let (body, ct) = match (req.method(), req.uri().path()) {
        (&Method::GET, "/plaintext") | (&Method::GET, "/plain") => (
            Bytes::from_static(PLAIN_BODY),
            HeaderValue::from_static("text/plain; charset=UTF-8"),
        ),
        (&Method::GET, "/json") => (
            Bytes::from_static(JSON_BODY),
            HeaderValue::from_static("application/json"),
        ),
        _ => {
            let mut res = Response::new(Full::new(Bytes::from_static(b"Not Found")));
            *res.status_mut() = StatusCode::NOT_FOUND;
            return Ok(res);
        }
    };

    let mut res = Response::new(Full::new(body));
    let h = res.headers_mut();
    h.insert(hyper::header::CONTENT_TYPE, ct);
    h.insert(hyper::header::SERVER, HeaderValue::from_static("Hyper"));
    h.insert(hyper::header::DATE, date_val);
    Ok(res)
}

/// Create a non-blocking SO_REUSEPORT TCP listener bound to `0.0.0.0:PORT`.
/// Each worker thread calls this independently so the kernel can distribute
/// incoming connections lock-free across all per-thread listeners.
fn make_reuseport_listener(port: u16) -> io::Result<std::net::TcpListener> {
    unsafe {
        // SOCK_NONBLOCK is Linux-only; macOS requires a separate fcntl.
        #[cfg(target_os = "linux")]
        let fd = libc::socket(libc::AF_INET, libc::SOCK_STREAM | libc::SOCK_NONBLOCK, 0);
        #[cfg(not(target_os = "linux"))]
        let fd = libc::socket(libc::AF_INET, libc::SOCK_STREAM, 0);

        if fd < 0 {
            return Err(io::Error::last_os_error());
        }

        #[cfg(not(target_os = "linux"))]
        {
            let flags = libc::fcntl(fd, libc::F_GETFL, 0);
            libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }

        let one: libc::c_int = 1;
        macro_rules! sopt {
            ($level:expr, $opt:expr) => {
                libc::setsockopt(
                    fd,
                    $level,
                    $opt,
                    &one as *const _ as *const libc::c_void,
                    mem::size_of_val(&one) as libc::socklen_t,
                );
            };
        }
        sopt!(libc::SOL_SOCKET, libc::SO_REUSEADDR);
        sopt!(libc::SOL_SOCKET, libc::SO_REUSEPORT);
        sopt!(libc::IPPROTO_TCP, libc::TCP_NODELAY);

        // sockaddr_in differs between Linux (no sin_len) and macOS/BSD (has sin_len).
        #[cfg(target_os = "linux")]
        let sin = libc::sockaddr_in {
            sin_family: libc::AF_INET as libc::sa_family_t,
            sin_port: port.to_be(),
            sin_addr: libc::in_addr { s_addr: 0 }, // INADDR_ANY
            sin_zero: [0; 8],
        };
        #[cfg(not(target_os = "linux"))]
        let sin = libc::sockaddr_in {
            sin_len: mem::size_of::<libc::sockaddr_in>() as u8,
            sin_family: libc::AF_INET as libc::sa_family_t,
            sin_port: port.to_be(),
            sin_addr: libc::in_addr { s_addr: 0 },
            sin_zero: [0; 8],
        };

        if libc::bind(
            fd,
            &sin as *const _ as *const libc::sockaddr,
            mem::size_of_val(&sin) as libc::socklen_t,
        ) < 0
        {
            let err = io::Error::last_os_error();
            libc::close(fd);
            return Err(err);
        }

        if libc::listen(fd, 4096) < 0 {
            let err = io::Error::last_os_error();
            libc::close(fd);
            return Err(err);
        }

        Ok(std::net::TcpListener::from_raw_fd(fd))
    }
}

fn run_worker(port: u16) {
    let std_listener = make_reuseport_listener(port).expect("SO_REUSEPORT listener failed");

    // Single-threaded runtime: all tasks for this worker run on one OS thread.
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_io()
        .enable_time()
        .build()
        .expect("tokio runtime build failed");

    rt.block_on(async move {
        let listener = TcpListener::from_std(std_listener).expect("TcpListener conversion failed");
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    tokio::spawn(async move {
                        let io = TokioIo::new(stream);
                        // keep_alive(true) + default http1 settings support HTTP/1.1 pipelining.
                        let _ = http1::Builder::new()
                            .keep_alive(true)
                            .serve_connection(io, service_fn(handle))
                            .await;
                    });
                }
                Err(e) => {
                    // EMFILE (os error 24): out of file descriptors.
                    // Back off 1 ms to let connections drain; don't spam stderr.
                    if e.raw_os_error() == Some(24) {
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    } else {
                        eprintln!("accept error: {e}");
                    }
                }
            }
        }
    });
}

fn main() {
    let workers: usize = std::env::var("WORKERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or_else(num_cpus::get);
    let port = listen_port();

    println!(
        "Hyper server listening on 0.0.0.0:{port} with {workers} worker threads (SO_REUSEPORT)"
    );

    // Spawn N-1 worker threads; current thread becomes the last worker.
    for i in 1..workers {
        thread::Builder::new()
            .name(format!("hyper-worker-{i}"))
            .spawn(move || run_worker(port))
            .expect("thread spawn failed");
    }
    run_worker(port);
}
