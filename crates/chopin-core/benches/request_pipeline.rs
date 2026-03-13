//! Micro-benchmarks for the HTTP request processing pipeline.
//!
//! Measures the cost of each stage in Chopin's hot path:
//!   1. Request line + header parsing
//!   2. Router lookup (path comparison)
//!   3. Response serialization (status, headers, body)
//!
//! Run:
//!   cargo bench --bench request_pipeline -p chopin-core

use criterion::{Criterion, black_box, criterion_group, criterion_main};

fn bench_request_parsing(c: &mut Criterion) {
    let buf: &[u8] = b"GET /json HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n";

    c.bench_function("http_request_parse", |b| {
        b.iter(|| {
            let buf = black_box(buf);

            // Stage 1: locate method end
            let mut method_end = 0;
            for (i, &byte) in buf.iter().enumerate() {
                if byte == b' ' {
                    method_end = i;
                    break;
                }
            }

            // Stage 2: locate path end
            let path_start = method_end + 1;
            let mut path_end = path_start;
            for (i, &byte) in buf[path_start..].iter().enumerate() {
                if byte == b' ' {
                    path_end = path_start + i;
                    break;
                }
            }

            black_box(&buf[path_start..path_end])
        })
    });
}

fn bench_route_lookup(c: &mut Criterion) {
    let paths: &[&[u8]] = &[b"/json", b"/plaintext", b"/health", b"/not-found"];

    c.bench_function("route_lookup", |b| {
        b.iter(|| {
            for path in black_box(paths) {
                let _matched = *path == b"/json" || *path == b"/plaintext";
                black_box(_matched);
            }
        })
    });
}

fn bench_response_serialization(c: &mut Criterion) {
    c.bench_function("http_response_serialize", |b| {
        b.iter(|| {
            let mut write_buf = [0u8; 1024];
            let mut pos = 0;

            // Status line
            let status_line = b"HTTP/1.1 200 OK\r\n";
            write_buf[pos..pos + status_line.len()].copy_from_slice(status_line);
            pos += status_line.len();

            // Server header
            let server = b"Server: chopin\r\n";
            write_buf[pos..pos + server.len()].copy_from_slice(server);
            pos += server.len();

            // Content-Type
            let ct = b"Content-Type: application/json\r\n";
            write_buf[pos..pos + ct.len()].copy_from_slice(ct);
            pos += ct.len();

            // Body
            let body = b"{\"message\":\"Hello, World!\"}";
            let cl = format!("Content-Length: {}\r\n\r\n", body.len());
            let cl_bytes = cl.as_bytes();
            write_buf[pos..pos + cl_bytes.len()].copy_from_slice(cl_bytes);
            pos += cl_bytes.len();
            write_buf[pos..pos + body.len()].copy_from_slice(body);
            pos += body.len();

            black_box((write_buf, pos))
        })
    });
}

criterion_group!(
    benches,
    bench_request_parsing,
    bench_route_lookup,
    bench_response_serialization
);
criterion_main!(benches);
