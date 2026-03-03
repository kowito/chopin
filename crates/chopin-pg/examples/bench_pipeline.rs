//! Benchmark: sequential query() vs pipeline_query()
//!
//! Measures the throughput gain from batching N independent SELECT statements
//! into a single round-trip instead of N round-trips.
//!
//! # Usage
//! ```
//! cargo run --release --example bench_pipeline
//! ```
//!
//! Expects a local PostgreSQL instance:
//!   host=127.0.0.1  port=5432  user=chopin  password=chopin  database=chopin|postgres

use chopin_pg::{PgConfig, PgConnection, ToSql};
use std::time::Instant;

fn connect() -> PgConnection {
    for db in &["chopin", "postgres"] {
        let cfg = PgConfig::new("127.0.0.1", 5432, "chopin", "chopin", db);
        if let Ok(c) = PgConnection::connect(&cfg) {
            return c;
        }
    }
    panic!("Could not connect to local PostgreSQL");
}

fn main() {
    let mut conn = connect();

    // --- Setup -----------------------------------------------------------
    conn.query_simple("CREATE TEMP TABLE pip_bench (id INT PRIMARY KEY, val INT NOT NULL)")
        .unwrap();

    // Insert 10 000 rows via COPY for a realistic working set.
    {
        let mut cw = conn
            .copy_in("COPY pip_bench FROM STDIN")
            .expect("COPY IN failed");
        for i in 0i32..10_000 {
            cw.write_row(&[&i.to_string(), &(i * 2).to_string()])
                .unwrap();
        }
        cw.finish().unwrap();
    }

    println!("Setup: 10 000 rows inserted into pip_bench");
    println!();

    // --- Benchmark parameters -------------------------------------------
    const WARMUP_ROUNDS: u32 = 200;
    const BENCH_ROUNDS: u32 = 2_000;
    const BATCH_SIZES: &[usize] = &[1, 5, 10, 25, 50, 100];

    let sql = "SELECT val FROM pip_bench WHERE id = $1";

    for &batch in BATCH_SIZES {
        // ── Sequential queries (N × query()) ──────────────────────────────
        for _ in 0..WARMUP_ROUNDS {
            for i in 0..batch as i32 {
                conn.query(sql, &[&i]).unwrap();
            }
        }

        let t0 = Instant::now();
        for round in 0..BENCH_ROUNDS {
            let base = (round as i32 * batch as i32) % 9_900;
            for i in 0..batch as i32 {
                conn.query(sql, &[&(base + i)]).unwrap();
            }
        }
        let seq_elapsed = t0.elapsed();
        let seq_total_queries = BENCH_ROUNDS as u64 * batch as u64;
        let seq_qps = seq_total_queries as f64 / seq_elapsed.as_secs_f64();
        let seq_rtt_us = seq_elapsed.as_micros() as f64 / (BENCH_ROUNDS as f64); // µs per batch

        // ── Pipelined queries (pipeline_query()) ───────────────────────────
        // Build the query list once per batch size; vary param each round.
        for _ in 0..WARMUP_ROUNDS {
            let params_vec: Vec<i32> = (0..batch as i32).collect();
            // Each element is a one-item array of `&dyn ToSql` so we can take
            // a slice of it for the `&[&dyn ToSql]` expected by pipeline_query.
            let param_slices: Vec<[&dyn ToSql; 1]> =
                params_vec.iter().map(|v| [v as &dyn ToSql]).collect();
            let queries: Vec<(&str, &[&dyn ToSql])> = param_slices
                .iter()
                .map(|p| (sql, p.as_slice()))
                .collect();
            conn.pipeline_query(&queries).unwrap();
        }

        let t0 = Instant::now();
        for round in 0..BENCH_ROUNDS {
            let base = (round as i32 * batch as i32) % 9_900;
            let params_vec: Vec<i32> = (0..batch as i32).map(|i| base + i).collect();
            let param_slices: Vec<[&dyn ToSql; 1]> =
                params_vec.iter().map(|v| [v as &dyn ToSql]).collect();
            let queries: Vec<(&str, &[&dyn ToSql])> = param_slices
                .iter()
                .map(|p| (sql, p.as_slice()))
                .collect();
            conn.pipeline_query(&queries).unwrap();
        }
        let pip_elapsed = t0.elapsed();
        let pip_total_queries = BENCH_ROUNDS as u64 * batch as u64;
        let pip_qps = pip_total_queries as f64 / pip_elapsed.as_secs_f64();
        let pip_rtt_us = pip_elapsed.as_micros() as f64 / (BENCH_ROUNDS as f64); // µs per batch

        let speedup = pip_qps / seq_qps;

        println!(
            "batch={batch:3}  sequential: {seq_qps:>8.0} q/s  {seq_rtt_us:>7.1} µs/batch  \
             │  pipeline: {pip_qps:>8.0} q/s  {pip_rtt_us:>7.1} µs/batch  \
             │  speedup: {speedup:.2}x"
        );
    }

    println!();
    println!("Note: speedup grows with batch size — each extra query in the pipeline");
    println!("      is free (no additional network round-trip).");
}
