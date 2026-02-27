use chopin_pg::{PgConnection, PgConfig, PgResult};
use std::time::Instant;

fn main() -> PgResult<()> {
    let config = PgConfig::new("127.0.0.1", 5432, "chopin", "chopin", "chopin");
    
    println!("Connecting to PostgreSQL...");
    let mut conn = match PgConnection::connect(&config) {
        Ok(c) => c,
        Err(_) => {
            println!("Retrying with 'postgres' database...");
            let mut fallback_config = config;
            fallback_config.database = "postgres".to_string();
            PgConnection::connect(&fallback_config)?
        }
    };
    println!("Connected!");

    // Simple SELECT 1 benchmark
    let iterations = 100_000;
    println!("Running {} iterations of 'SELECT 1'...", iterations);
    
    let start = Instant::now();
    for _ in 0..iterations {
        let rows = conn.query("SELECT 1", &[])?;
        let _val: i32 = rows[0].get_i32(0)?.unwrap();
    }
    let duration = start.elapsed();
    
    println!("Throughput: {:.2} req/s", iterations as f64 / duration.as_secs_f64());
    println!("Average latency: {:.2} µs", duration.as_micros() as f64 / iterations as f64);

    // SQL with parameters
    println!("\nRunning {} iterations of parameterized query...", iterations);
    let start = Instant::now();
    for i in 0..iterations {
        let rows = conn.query("SELECT $1::int4 + $2::int4", &[&(i as i32), &10i32])?;
        let _val: i32 = rows[0].get_i32(0)?.unwrap();
    }
    let duration = start.elapsed();
    
    println!("Throughput (parameterized): {:.2} req/s", iterations as f64 / duration.as_secs_f64());
    println!("Average latency: {:.2} µs", duration.as_micros() as f64 / iterations as f64);

    // Clean up
    drop(conn);
    Ok(())
}
