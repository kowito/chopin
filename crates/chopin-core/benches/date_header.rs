//! Micro-benchmarks for the HTTP date header pipeline.
//!
//! Measures the cost of `SystemTime::now()` and `format_http_date()` — the two
//! syscall/compute steps that run on every response in the Chopin hot path.
//!
//! Run:
//!   cargo bench --bench date_header -p chopin-core

use criterion::{Criterion, black_box, criterion_group, criterion_main};
use std::time::{SystemTime, UNIX_EPOCH};

fn bench_system_time_now(c: &mut Criterion) {
    c.bench_function("SystemTime::now", |b| {
        b.iter(|| black_box(SystemTime::now()))
    });
}

fn bench_format_http_date(c: &mut Criterion) {
    // Use a fixed timestamp so the bench measures only format, not clock.
    let unix_secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as u32;

    c.bench_function("format_http_date", |b| {
        b.iter(|| {
            let mut buf = [0u8; 37];
            chopin_core::http_date::format_http_date(black_box(unix_secs), &mut buf);
            black_box(buf)
        })
    });
}

fn bench_date_header_pipeline(c: &mut Criterion) {
    // End-to-end: read clock + format — the real per-response cost.
    c.bench_function("date_header_pipeline", |b| {
        b.iter(|| {
            let unix_secs = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as u32;
            let mut buf = [0u8; 37];
            chopin_core::http_date::format_http_date(unix_secs, &mut buf);
            black_box(buf)
        })
    });
}

criterion_group!(
    benches,
    bench_system_time_now,
    bench_format_http_date,
    bench_date_header_pipeline
);
criterion_main!(benches);
