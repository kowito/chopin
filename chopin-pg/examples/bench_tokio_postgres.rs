use std::time::Instant;
use tokio_postgres::NoTls;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (client, connection) = tokio_postgres::connect(
        "host=127.0.0.1 user=chopin password=chopin dbname=postgres",
        NoTls,
    )
    .await?;

    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("connection error: {}", e);
        }
    });

    // Setup table
    client
        .batch_execute("DROP TABLE IF EXISTS bench_tokio")
        .await?;
    client
        .batch_execute("CREATE TABLE bench_tokio (id INT PRIMARY KEY, val TEXT, count INT)")
        .await?;

    let scales = [1_000, 100_000, 1_000_000];

    for scale in scales {
        println!("\n=== SCALE: {} rows ===", scale);
        client.batch_execute("TRUNCATE bench_tokio").await?;

        let start = Instant::now();
        // Use batch_execute for "COPY-like" simulation or just standard inserts if COPY is complex to setup here
        // For tokio-postgres, COPY is supported via copy_in but let's stick to a fair comparison of point ops if possible
        // Actually, for preparation, we need any fast way.

        // Single INSERTs for scale (pre-fill)
        // Note: Real benchmarking would use COPY, but we want to test DRIVER overhead.
        // Let's use batch for pre-fill.
        let mut query = String::from("INSERT INTO bench_tokio (id, val, count) VALUES ");
        let batch_size = 1000;
        for i in 0..scale {
            query.push_str(&format!("({}, 'val_{}', {})", i, i, i));
            if (i + 1) % batch_size == 0 || i + 1 == scale {
                client.batch_execute(&query).await?;
                query = String::from("INSERT INTO bench_tokio (id, val, count) VALUES ");
            } else {
                query.push_str(", ");
            }
        }
        println!("Pre-fill complete in {:?}", start.elapsed());

        // SELECT
        let start = Instant::now();
        for i in 0..10_000 {
            let id = i % scale;
            let _ = client
                .query_one("SELECT val FROM bench_tokio WHERE id = $1", &[&id])
                .await?;
        }
        let duration = start.elapsed();
        println!(
            "SELECT Throughput: {:.2} req/s",
            10_000.0 / duration.as_secs_f64()
        );

        // UPDATE
        let start = Instant::now();
        for i in 0..10_000 {
            let id = i % scale;
            let _ = client
                .execute(
                    "UPDATE bench_tokio SET count = count + 1 WHERE id = $1",
                    &[&id],
                )
                .await?;
        }
        let duration = start.elapsed();
        println!(
            "UPDATE Throughput: {:.2} req/s",
            10_000.0 / duration.as_secs_f64()
        );

        // INSERT
        let start = Instant::now();
        for i in 0..10_000 {
            let id = scale + i;
            let _ = client
                .execute(
                    "INSERT INTO bench_tokio (id, val, count) VALUES ($1, $2, $3)",
                    &[&id, &"new", &0i32],
                )
                .await?;
        }
        let duration = start.elapsed();
        println!(
            "INSERT Throughput: {:.2} req/s",
            10_000.0 / duration.as_secs_f64()
        );
    }

    Ok(())
}
