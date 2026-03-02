use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    println!("Profiling key hot paths in Chopin...\n");

    // Test 1: SystemTime::now() performance
    println!("=== Test 1: SystemTime::now() overhead ===");
    let iterations = 1_000_000;
    let start = std::time::Instant::now();
    for _ in 0..iterations {
        let _ = SystemTime::now();
    }
    let elapsed = start.elapsed();
    let ns_per_call = (elapsed.as_nanos() as f64) / (iterations as f64);
    println!(
        "Performed {} SystemTime::now() calls in {:?}",
        iterations, elapsed
    );
    println!("  Average: {:.2} ns/call ({:.2} μs/call)", 
        ns_per_call, ns_per_call / 1000.0);

    // Test 2: format_http_date performance
    println!("\n=== Test 2: format_http_date() overhead ===");
    let iterations = 1_000_000;
    let unix_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32;

    let start = std::time::Instant::now();
    for _ in 0..iterations {
        let mut buf = [0u8; 37];
        chopin_core::http_date::format_http_date(unix_secs, &mut buf);
    }
    let elapsed = start.elapsed();
    let ns_per_call = (elapsed.as_nanos() as f64) / (iterations as f64);
    println!(
        "Performed {} format_http_date() calls in {:?}",
        iterations, elapsed
    );
    println!("  Average: {:.2} ns/call ({:.2} μs/call)", 
        ns_per_call, ns_per_call / 1000.0);

    // Test 3: Combined per-response cost
    println!("\n=== Test 3: Combined per-response cost (SystemTime + format_http_date) ===");
    let iterations = 1_000_000;
    let start = std::time::Instant::now();
    for _ in 0..iterations {
        let unix_secs = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as u32;
        let mut buf = [0u8; 37];
        chopin_core::http_date::format_http_date(unix_secs, &mut buf);
    }
    let elapsed = start.elapsed();
    let ns_per_response = (elapsed.as_nanos() as f64) / (iterations as f64);
    println!(
        "Performed {} per-response operations in {:?}",
        iterations, elapsed
    );
    println!("  Average: {:.2} ns/response ({:.2} μs/response)", 
        ns_per_response, ns_per_response / 1000.0);
    println!("  Throughput: {:.0} responses/second", 
        1_000_000_000.0 / ns_per_response);

    println!("\n=== Summary ===");
    println!("If each response calls SystemTime::now():");
    println!("  - At 100k req/s: {:.3}% CPU overhead", 
        (ns_per_response / 10_000.0) * 100.0);
    println!("  - At 1M req/s: {:.3}% CPU overhead", 
        (ns_per_response / 1_000.0) * 100.0);
}
