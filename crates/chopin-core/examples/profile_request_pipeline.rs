use std::time::{SystemTime, UNIX_EPOCH};

/// Simulate a typical HTTP request parsing
fn simulate_request_parsing() -> (std::time::Duration, std::time::Duration, std::time::Duration) {
    use std::time::Instant;

    // Simulate reading socket buffer
    let buf = b"GET /json HTTP/1.1\r\nHost: localhost\r\nConnection: keep-alive\r\n\r\n";

    let t1 = Instant::now();

    // Stage 1: Parse request line & headers (typical chopin-core parser)
    // This is what happens in parser.rs
    let mut method_end = 0;
    for (i, &b) in buf.iter().enumerate() {
        if b == b' ' {
            method_end = i;
            break;
        }
    }
    let _method_bytes = &buf[..method_end];

    let path_start = method_end + 1;
    let mut path_end = 0;
    for (i, &b) in buf[path_start..].iter().enumerate() {
        if b == b' ' {
            path_end = path_start + i;
            break;
        }
    }
    let path_bytes = &buf[path_start..path_end];

    let parse_time = t1.elapsed();

    // Stage 2: Router lookup (radix tree, O(K) where K = path length)
    let t2 = Instant::now();
    let _matched = path_bytes == b"/json";
    let route_time = t2.elapsed();

    // Stage 3: Response serialization
    let t3 = Instant::now();
    let mut write_buf = [0u8; 1024];
    let mut pos = 0;

    // Write status line
    let status_line = b"HTTP/1.1 200 OK\r\n";
    write_buf[pos..pos + status_line.len()].copy_from_slice(status_line);
    pos += status_line.len();

    // Write Server header
    let server = b"Server: chopin\r\n";
    write_buf[pos..pos + server.len()].copy_from_slice(server);
    pos += server.len();

    // Write Date header
    let mut date_buf = [0u8; 37];
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32;
    chopin_core::http_date::format_http_date(now, &mut date_buf);
    write_buf[pos..pos + 37].copy_from_slice(&date_buf);
    pos += 37;

    // Write Content-Type
    let ct = b"Content-Type: application/json\r\n";
    write_buf[pos..pos + ct.len()].copy_from_slice(ct);
    pos += ct.len();

    // Write Content-Length
    let body = b"{\"message\":\"Hello, World!\"}";
    let cl = format!("Content-Length: {}\r\n", body.len());
    let cl_bytes = cl.as_bytes();
    write_buf[pos..pos + cl_bytes.len()].copy_from_slice(cl_bytes);
    pos += cl_bytes.len();

    // Write Connection header
    let conn = b"Connection: keep-alive\r\n\r\n";
    write_buf[pos..pos + conn.len()].copy_from_slice(conn);
    pos += conn.len();

    // Write body
    write_buf[pos..pos + body.len()].copy_from_slice(body);

    let serialize_time = t3.elapsed();

    (parse_time, route_time, serialize_time)
}

fn main() {
    println!("=== Full Request Pipeline Profiling ===\n");

    // Profile a single request to see the breakdown
    println!("Single request measurement:");
    let (parse_time, route_time, serialize_time) = simulate_request_parsing();

    println!("  Parsing:        {:>10.2?}  ({:>6.1}%)", parse_time, 
        (parse_time.as_nanos() as f64 / (parse_time.as_nanos() as f64 + route_time.as_nanos() as f64 + serialize_time.as_nanos() as f64)) * 100.0);
    println!("  Routing:        {:>10.2?}  ({:>6.1}%)", route_time,
        (route_time.as_nanos() as f64 / (parse_time.as_nanos() as f64 + route_time.as_nanos() as f64 + serialize_time.as_nanos() as f64)) * 100.0);
    println!("  Serialization:  {:>10.2?}  ({:>6.1}%)", serialize_time,
        (serialize_time.as_nanos() as f64 / (parse_time.as_nanos() as f64 + route_time.as_nanos() as f64 + serialize_time.as_nanos() as f64)) * 100.0);

    let total = parse_time.as_nanos() as f64 + route_time.as_nanos() as f64 + serialize_time.as_nanos() as f64;
    println!("  TOTAL:          {:>10.2?}\n", std::time::Duration::from_nanos(total as u64));

    // Now profile at scale
    println!("=== Scale Profiling (1 million requests) ===\n");

    let iterations = 1_000_000;
    let mut total_parse = std::time::Duration::ZERO;
    let mut total_route = std::time::Duration::ZERO;
    let mut total_serialize = std::time::Duration::ZERO;

    let start_all = std::time::Instant::now();
    for _ in 0..iterations {
        let (p, r, s) = simulate_request_parsing();
        total_parse += p;
        total_route += r;
        total_serialize += s;
    }
    let elapsed_all = start_all.elapsed();

    let parse_pct = (total_parse.as_nanos() as f64 / elapsed_all.as_nanos() as f64) * 100.0;
    let route_pct = (total_route.as_nanos() as f64 / elapsed_all.as_nanos() as f64) * 100.0;
    let serialize_pct = (total_serialize.as_nanos() as f64 / elapsed_all.as_nanos() as f64) * 100.0;

    println!("Over {} requests:", iterations);
    println!("  Parsing:        {:>12}  ({:>6.2}%)", format!("{:.2?}", total_parse), parse_pct);
    println!("  Routing:        {:>12}  ({:>6.2}%)", format!("{:.2?}", total_route), route_pct);
    println!("  Serialization:  {:>12}  ({:>6.2}%)", format!("{:.2?}", total_serialize), serialize_pct);
    println!("  TOTAL:          {:>12}", format!("{:.2?}", elapsed_all));
    println!("  Per request:    {:>12.2?}", elapsed_all / iterations);

    let ns_per_req = (elapsed_all.as_nanos() as f64) / (iterations as f64);
    let throughput = 1_000_000_000.0 / ns_per_req;
    println!("  Throughput:     {:>12.0} req/sec\n", throughput);

    // Highlight the bottleneck
    println!("=== BOTTLENECK ANALYSIS ===");
    let stages = vec![
        ("Parsing", parse_pct),
        ("Routing", route_pct),
        ("Serialization", serialize_pct),
    ];
    let (stage, pct) = stages.iter().max_by(|a, b| a.1.partial_cmp(&b.1).unwrap()).unwrap();
    println!("🔴 BOTTLENECK: {} ({:.2}% of total time)", stage, pct);
    println!("   This is where optimization efforts should focus.\n");

    // Estimate syscall overhead
    println!("=== Syscall Overhead Estimate ===");
    println!("Not measured (would require kernel instrumentation), but typical costs:");
    println!("  - accept():      ~5,000 ns (per connection)");
    println!("  - read():        ~1,000 ns (per batch of requests)");
    println!("  - write():       ~1,000 ns (per response)");
    println!("  - Total CPU work above: {:.2?} (excludes syscalls)", elapsed_all / iterations);
    println!("\nNote: If syscalls dominate, consider:");
    println!("  - Batch I/O (io_uring, io_buffering)");
    println!("  - SO_REUSEADDR for rapid restart");
    println!("  - TCP_CORK to batch response writes");
}
