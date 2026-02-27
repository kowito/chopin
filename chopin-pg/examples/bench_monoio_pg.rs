use std::time::Instant;

#[monoio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let addr = "127.0.0.1:5432";
    let user = "chopin";
    let pass = "chopin";
    let db = "postgres";

    println!("Connecting to PostgreSQL (monoio-pg) at {}...", addr);
    let mut client = monoio_pg::Client::connect(addr, user, Some(pass), Some(db)).await?;
    println!("Connected!");

    // Setup table
    client.execute("DROP TABLE IF EXISTS bench_mono").await?;
    client
        .execute("CREATE TABLE bench_mono (id INT PRIMARY KEY, val TEXT, count INT)")
        .await?;

    let scales = [1_000, 100_000, 1_000_000];

    for scale in scales {
        println!("\n=== SCALE: {} rows ===", scale);
        client.execute("TRUNCATE bench_mono").await?;

        let start = Instant::now();
        // pre-fill (manual batch since monoio-pg lacks high-level batch)
        for i in 0..(scale / 100).max(1) {
            let mut sql = String::from("INSERT INTO bench_mono (id, val, count) VALUES ");
            for j in 0..100 {
                let id = i * 100 + j;
                if id >= scale {
                    break;
                }
                sql.push_str(&format!(
                    "({}, 'val_{}', {}){}",
                    id,
                    id,
                    id,
                    if j == 99 || i * 100 + j == scale - 1 {
                        ""
                    } else {
                        ","
                    }
                ));
            }
            client.execute(&sql).await?;
        }
        println!("Pre-fill complete in {:?}", start.elapsed());

        // SELECT
        let start = Instant::now();
        for i in 0..10_000 {
            let id = i % scale;
            let _ = client
                .query(&format!("SELECT val FROM bench_mono WHERE id = {}", id))
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
            client
                .execute(&format!(
                    "UPDATE bench_mono SET count = count + 1 WHERE id = {}",
                    id
                ))
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
            client
                .execute(&format!(
                    "INSERT INTO bench_mono (id, val, count) VALUES ({}, 'new', 0)",
                    id
                ))
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
