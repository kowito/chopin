use std::time::Instant;
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let pool = PgPoolOptions::new()
        .max_connections(1) // Keep it single-threaded/single-conn for fair comparison
        .connect("postgres://chopin:chopin@127.0.0.1/postgres").await?;

    // Setup table
    sqlx::query("DROP TABLE IF EXISTS bench_sqlx").execute(&pool).await?;
    sqlx::query("CREATE TABLE bench_sqlx (id INT PRIMARY KEY, val TEXT, count INT)").execute(&pool).await?;

    let scales = [1_000, 100_000, 1_000_000];
    
    for scale in scales {
        println!("\n=== SCALE: {} rows ===", scale);
        sqlx::query("TRUNCATE bench_sqlx").execute(&pool).await?;
        
        let start = Instant::now();
        // Pre-fill (simulated batch)
        for i in 0..(scale / 1000).max(1) {
            let mut query_builder = sqlx::QueryBuilder::new("INSERT INTO bench_sqlx (id, val, count) ");
            query_builder.push_values(0..1000.min(scale - i * 1000), |mut b, j| {
                let id = i * 1000 + j;
                b.push_bind(id as i32)
                 .push_bind(format!("val_{}", id))
                 .push_bind(id as i32);
            });
            query_builder.build().execute(&pool).await?;
        }
        println!("Pre-fill complete in {:?}", start.elapsed());

        // SELECT
        let start = Instant::now();
        for i in 0..10_000 {
            let id = (i % scale) as i32;
            let _: (String,) = sqlx::query_as("SELECT val FROM bench_sqlx WHERE id = $1")
                .bind(id)
                .fetch_one(&pool).await?;
        }
        let duration = start.elapsed();
        println!("SELECT Throughput: {:.2} req/s", 10_000.0 / duration.as_secs_f64());

        // UPDATE
        let start = Instant::now();
        for i in 0..10_000 {
            let id = (i % scale) as i32;
            let _ = sqlx::query("UPDATE bench_sqlx SET count = count + 1 WHERE id = $1")
                .bind(id)
                .execute(&pool).await?;
        }
        let duration = start.elapsed();
        println!("UPDATE Throughput: {:.2} req/s", 10_000.0 / duration.as_secs_f64());

        // INSERT
        let start = Instant::now();
        for i in 0..10_000 {
            let id = (scale + i) as i32;
            let _ = sqlx::query("INSERT INTO bench_sqlx (id, val, count) VALUES ($1, $2, $3)")
                .bind(id)
                .bind("new")
                .bind(0i32)
                .execute(&pool).await?;
        }
        let duration = start.elapsed();
        println!("INSERT Throughput: {:.2} req/s", 10_000.0 / duration.as_secs_f64());
    }

    Ok(())
}
