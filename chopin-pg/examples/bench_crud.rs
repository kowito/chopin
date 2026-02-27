use chopin_pg::{PgConfig, PgConnection};
use std::time::Instant;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = PgConfig::new("127.0.0.1", 5432, "chopin", "chopin", "postgres");
    
    println!("Connecting to PostgreSQL...");
    let mut conn = PgConnection::connect(&config)?;
    println!("Connected!");

    // Setup table
    println!("Setting up benchmark table...");
    conn.query_simple("DROP TABLE IF EXISTS bench_crud")?;
    conn.query_simple("CREATE TABLE bench_crud (id INT PRIMARY KEY, val TEXT, count INT)")?;

    let scales = [1_000, 100_000, 1_000_000];
    
    for scale in scales {
        println!("\n=== SCALE: {} rows ===", scale);
        conn.query_simple("TRUNCATE bench_crud")?;
        
        // 1. Bulk Insert Using COPY
        println!("Feeding {} rows via COPY...", scale);
        let start = Instant::now();
        let mut writer = conn.copy_in("COPY bench_crud (id, val, count) FROM STDIN")?;
        let mut buffer = String::with_capacity(64 * 1024);
        for i in 0..scale {
            let line = format!("{}\tvalue_{}\t{}\n", i, i, i);
            buffer.push_str(&line);
            if buffer.len() > 32 * 1024 {
                writer.write_data(buffer.as_bytes())?;
                buffer.clear();
            }
        }
        if !buffer.is_empty() {
            writer.write_data(buffer.as_bytes())?;
        }
        writer.finish()?;
        let duration = start.elapsed();
        println!("COPY Throughput: {:.2} rows/s", scale as f64 / duration.as_secs_f64());

        // 2. Point SELECT
        println!("Benchmarking 10,000 Point SELECTs...");
        let start = Instant::now();
        for i in 0..10_000 {
            let id = i % scale;
            let _ = conn.query_one("SELECT val FROM bench_crud WHERE id = $1", &[&id])?;
        }
        let duration = start.elapsed();
        println!("SELECT Throughput: {:.2} req/s", 10_000.0 / duration.as_secs_f64());

        // 3. Point UPDATE
        println!("Benchmarking 10,000 Point UPDATEs...");
        let start = Instant::now();
        for i in 0..10_000 {
            let id = i % scale;
            let _ = conn.execute("UPDATE bench_crud SET count = count + 1 WHERE id = $1", &[&id])?;
        }
        let duration = start.elapsed();
        println!("UPDATE Throughput: {:.2} req/s", 10_000.0 / duration.as_secs_f64());

        // 4. Point INSERT (Single row)
        println!("Benchmarking 10,000 Single INSERTs...");
        let start = Instant::now();
        for i in 0..10_000 {
            let id = scale + i;
            let _ = conn.execute("INSERT INTO bench_crud (id, val, count) VALUES ($1, $2, $3)", &[&id, &"new_val", &0i32])?;
        }
        let duration = start.elapsed();
        println!("INSERT Throughput: {:.2} req/s", 10_000.0 / duration.as_secs_f64());

        // 5. Point DELETE
        println!("Benchmarking 10,000 Point DELETEs...");
        let start = Instant::now();
        for i in 0..10_000 {
            let id = scale + i;
            let _ = conn.execute("DELETE FROM bench_crud WHERE id = $1", &[&id])?;
        }
        let duration = start.elapsed();
        println!("DELETE Throughput: {:.2} req/s", 10_000.0 / duration.as_secs_f64());
    }

    println!("\nBenchmark complete!");
    Ok(())
}
