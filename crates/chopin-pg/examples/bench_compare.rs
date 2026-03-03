//! Unified benchmark: chopin-pg vs sqlx vs tokio-postgres
//!
//! Workloads
//! ─────────
//! 1. Traditional CRUD       — SELECT 1, parameterised query, point SELECT/UPDATE/INSERT
//! 2. TFB Multi-Query   #3   — N=1/5/10/15/20 random World row SELECTs per request
//! 3. TFB Database Updates #5 — N=1/5/10/15/20 random World row SELECT+UPDATE per request
//!
//! All drivers use a single connection; warmup is run before every sub-test.
//!
//! Usage:
//!   cargo run --release --example bench_compare

use std::time::Instant;

// ── shared config ─────────────────────────────────────────────────────────────
const HOST: &str = "127.0.0.1";
const PORT: u16 = 5432;
const USER: &str = "chopin";
const PASS: &str = "chopin";
const DB: &str = "postgres";

// TFB World table constants
const WORLD_ROWS: i32 = 10_000;
const WORLD_SQL: &str = "SELECT id, randomnumber FROM world WHERE id = $1";
const UPDATE_SQL: &str = "UPDATE world SET randomnumber = $1 WHERE id = $2";

const SIMPLE_ITERS: u64 = 100_000;
const WARMUP_ITERS: usize = 200;
const CRUD_ITERS: usize = 10_000;

/// TFB "Multiple Database Queries" counts (queries per simulated request).
const MULTI_QUERY_COUNTS: &[usize] = &[1, 5, 10, 15, 20];
/// How many "requests" to fire per N value.
const MULTI_REQUESTS: usize = 500;

// ── simple inline PRNG (xorshift32) ──────────────────────────────────────────
/// Returns a random integer in [1, WORLD_ROWS].
#[inline]
fn rand_world_id(state: &mut u32) -> i32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    1 + (*state % WORLD_ROWS as u32) as i32
}

// ── result containers ─────────────────────────────────────────────────────────
#[derive(Debug)]
struct DriverResults {
    name: &'static str,
    // Traditional CRUD
    select_one_rps: f64,
    param_rps: f64,
    crud_select_rps: f64,
    crud_update_rps: f64,
    crud_insert_rps: f64,
    // TFB Multi-Query (Test #3): one entry per MULTI_QUERY_COUNTS value
    multi_seq: Vec<f64>,
    // TFB Database Updates (Test #5): one entry per MULTI_QUERY_COUNTS value
    updates_seq: Vec<f64>,
}

// ══════════════════════════════════════════════════════════════════════════════
// chopin-pg  (sync, run via spawn_blocking)
// ══════════════════════════════════════════════════════════════════════════════
fn run_chopin_pg() -> Result<DriverResults, Box<dyn std::error::Error + Send + Sync>> {
    use chopin_pg::{PgConfig, PgConnection};

    let cfg = PgConfig::new(HOST, PORT, USER, PASS, DB);
    let mut conn = PgConnection::connect(&cfg)?;

    // ── CRUD table setup ──────────────────────────────────────────────────────
    conn.execute("DROP TABLE IF EXISTS bench_compare_chopin", &[])?;
    conn.execute(
        "CREATE TABLE bench_compare_chopin (id INT PRIMARY KEY, val TEXT, count INT)",
        &[],
    )?;
    for chunk in (0..10_000usize).collect::<Vec<_>>().chunks(1000) {
        let mut sql = String::from("INSERT INTO bench_compare_chopin (id, val, count) VALUES ");
        for (j, &i) in chunk.iter().enumerate() {
            if j > 0 {
                sql.push_str(", ");
            }
            sql.push_str(&format!("({}, 'val_{}', {})", i, i, i));
        }
        conn.execute(&sql, &[])?;
    }

    // ── helper: run a sub-benchmark with warmup ───────────────────────────────
    macro_rules! timed {
        ($warmup:expr, $iters:expr, $body:expr) => {{
            for _ in 0..$warmup {
                $body;
            }
            let __t = Instant::now();
            for _ in 0..$iters {
                $body;
            }
            $iters as f64 / __t.elapsed().as_secs_f64()
        }};
    }

    // ── SELECT 1 ──────────────────────────────────────────────────────────────
    let select_one_rps = timed!(WARMUP_ITERS, SIMPLE_ITERS, {
        let rows = conn.query("SELECT 1", &[])?;
        let _: i32 = rows[0].get_i32(0)?.unwrap();
    });

    // ── parameterised query ───────────────────────────────────────────────────
    let mut rng = 0x9e37_79b9u32;
    let param_rps = timed!(WARMUP_ITERS, SIMPLE_ITERS, {
        let v = rand_world_id(&mut rng);
        let rows = conn.query("SELECT $1::int4 + $2::int4", &[&v, &10i32])?;
        let _: i32 = rows[0].get_i32(0)?.unwrap();
    });

    // ── CRUD SELECT ───────────────────────────────────────────────────────────
    let mut rng = 0xdead_beefu32;
    let crud_select_rps = timed!(WARMUP_ITERS, CRUD_ITERS, {
        let id = rand_world_id(&mut rng) % 10_000;
        let rows = conn.query("SELECT val FROM bench_compare_chopin WHERE id = $1", &[&id])?;
        let _: Option<&str> = rows[0].get_str(0)?;
    });

    // ── CRUD UPDATE ───────────────────────────────────────────────────────────
    let mut rng = 0xcafe_f00du32;
    let crud_update_rps = timed!(WARMUP_ITERS, CRUD_ITERS, {
        let id = rand_world_id(&mut rng) % 10_000;
        conn.execute(
            "UPDATE bench_compare_chopin SET count = count + 1 WHERE id = $1",
            &[&id],
        )?;
    });

    // ── CRUD INSERT ───────────────────────────────────────────────────────────
    // Use a sequence to avoid PK conflicts; modulo wraps back over old inserts.
    let crud_insert_rps = {
        for _ in 0..WARMUP_ITERS {
            conn.execute(
                "INSERT INTO bench_compare_chopin (id, val, count) VALUES ($1, $2, $3) \
                 ON CONFLICT (id) DO UPDATE SET count = bench_compare_chopin.count + 1",
                &[&(20_000i32), &"new", &0i32],
            )?;
        }
        let t = Instant::now();
        for i in 0..CRUD_ITERS {
            let id = (10_000 + i % 10_000) as i32;
            conn.execute(
                "INSERT INTO bench_compare_chopin (id, val, count) VALUES ($1, $2, $3) \
                 ON CONFLICT (id) DO UPDATE SET count = bench_compare_chopin.count + 1",
                &[&id, &"new", &0i32],
            )?;
        }
        CRUD_ITERS as f64 / t.elapsed().as_secs_f64()
    };

    conn.execute("DROP TABLE IF EXISTS bench_compare_chopin", &[])?;

    // ── TFB Multi-Query: sequential ───────────────────────────────────────────
    let mut multi_seq = Vec::with_capacity(MULTI_QUERY_COUNTS.len());
    for &n in MULTI_QUERY_COUNTS {
        // warmup
        let mut rng = 0x1234_5678u32;
        for _ in 0..WARMUP_ITERS {
            for _ in 0..n {
                let id = rand_world_id(&mut rng);
                let rows = conn.query(WORLD_SQL, &[&id])?;
                let _: i32 = rows[0].get_i32(0)?.unwrap();
            }
        }
        // measure
        let mut rng = 0xabcd_ef01u32;
        let t = Instant::now();
        for _ in 0..MULTI_REQUESTS {
            for _ in 0..n {
                let id = rand_world_id(&mut rng);
                let rows = conn.query(WORLD_SQL, &[&id])?;
                let _: i32 = rows[0].get_i32(0)?.unwrap();
            }
        }
        // report as "requests per second" (each request = N queries)
        multi_seq.push(MULTI_REQUESTS as f64 / t.elapsed().as_secs_f64());
    }

    // ── TFB Database Updates (Test #5): N random read-then-update per request ───
    let mut updates_seq = Vec::with_capacity(MULTI_QUERY_COUNTS.len());
    for &n in MULTI_QUERY_COUNTS {
        // warmup
        let mut rng = 0x2468_1357u32;
        for _ in 0..WARMUP_ITERS {
            for _ in 0..n {
                let id = rand_world_id(&mut rng);
                let rows = conn.query(WORLD_SQL, &[&id])?;
                let row_id: i32 = rows[0].get_i32(0)?.unwrap();
                let new_rn = rand_world_id(&mut rng);
                conn.execute(UPDATE_SQL, &[&new_rn, &row_id])?;
            }
        }
        // measure
        let mut rng = 0x1357_2468u32;
        let t = Instant::now();
        for _ in 0..MULTI_REQUESTS {
            for _ in 0..n {
                let id = rand_world_id(&mut rng);
                let rows = conn.query(WORLD_SQL, &[&id])?;
                let row_id: i32 = rows[0].get_i32(0)?.unwrap();
                let new_rn = rand_world_id(&mut rng);
                conn.execute(UPDATE_SQL, &[&new_rn, &row_id])?;
            }
        }
        updates_seq.push(MULTI_REQUESTS as f64 / t.elapsed().as_secs_f64());
    }

    Ok(DriverResults {
        name: "chopin-pg",
        select_one_rps,
        param_rps,
        crud_select_rps,
        crud_update_rps,
        crud_insert_rps,
        multi_seq,
        updates_seq,
    })
}

// ══════════════════════════════════════════════════════════════════════════════
// sqlx
// ══════════════════════════════════════════════════════════════════════════════
async fn run_sqlx() -> Result<DriverResults, Box<dyn std::error::Error>> {
    use sqlx::postgres::PgPoolOptions;

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&format!(
            "postgres://{}:{}@{}:{}/{}",
            USER, PASS, HOST, PORT, DB
        ))
        .await?;

    // ── CRUD table setup ──────────────────────────────────────────────────────
    sqlx::query("DROP TABLE IF EXISTS bench_compare_sqlx")
        .execute(&pool)
        .await?;
    sqlx::query("CREATE TABLE bench_compare_sqlx (id INT PRIMARY KEY, val TEXT, count INT)")
        .execute(&pool)
        .await?;
    for chunk_start in (0..10_000usize).step_by(1000) {
        let mut qb = sqlx::QueryBuilder::new("INSERT INTO bench_compare_sqlx (id, val, count) ");
        qb.push_values(chunk_start..chunk_start + 1000, |mut b, i| {
            b.push_bind(i as i32)
                .push_bind(format!("val_{}", i))
                .push_bind(i as i32);
        });
        qb.build().execute(&pool).await?;
    }

    // ── SELECT 1 (with warmup) ────────────────────────────────────────────────
    for _ in 0..WARMUP_ITERS {
        let _: (i32,) = sqlx::query_as("SELECT 1").fetch_one(&pool).await?;
    }
    let t = Instant::now();
    for _ in 0..SIMPLE_ITERS {
        let (v,): (i32,) = sqlx::query_as("SELECT 1").fetch_one(&pool).await?;
        let _ = v;
    }
    let select_one_rps = SIMPLE_ITERS as f64 / t.elapsed().as_secs_f64();

    // ── parameterised query (with warmup) ─────────────────────────────────────
    let mut rng = 0x9e37_79b9u32;
    for _ in 0..WARMUP_ITERS {
        let v = rand_world_id(&mut rng);
        let _: (i32,) = sqlx::query_as("SELECT $1::int4 + $2::int4")
            .bind(v)
            .bind(10i32)
            .fetch_one(&pool)
            .await?;
    }
    let t = Instant::now();
    for _ in 0..SIMPLE_ITERS {
        let v = rand_world_id(&mut rng);
        let (r,): (i32,) = sqlx::query_as("SELECT $1::int4 + $2::int4")
            .bind(v)
            .bind(10i32)
            .fetch_one(&pool)
            .await?;
        let _ = r;
    }
    let param_rps = SIMPLE_ITERS as f64 / t.elapsed().as_secs_f64();

    // ── CRUD SELECT (with warmup) ─────────────────────────────────────────────
    let mut rng = 0xdead_beefu32;
    for _ in 0..WARMUP_ITERS {
        let id = rand_world_id(&mut rng) % 10_000;
        let _: (String,) = sqlx::query_as("SELECT val FROM bench_compare_sqlx WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await?;
    }
    let t = Instant::now();
    for _ in 0..CRUD_ITERS {
        let id = rand_world_id(&mut rng) % 10_000;
        let (_val,): (String,) = sqlx::query_as("SELECT val FROM bench_compare_sqlx WHERE id = $1")
            .bind(id)
            .fetch_one(&pool)
            .await?;
    }
    let crud_select_rps = CRUD_ITERS as f64 / t.elapsed().as_secs_f64();

    // ── CRUD UPDATE (with warmup) ─────────────────────────────────────────────
    let mut rng = 0xcafe_f00du32;
    for _ in 0..WARMUP_ITERS {
        let id = rand_world_id(&mut rng) % 10_000;
        sqlx::query("UPDATE bench_compare_sqlx SET count = count + 1 WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await?;
    }
    let t = Instant::now();
    for _ in 0..CRUD_ITERS {
        let id = rand_world_id(&mut rng) % 10_000;
        sqlx::query("UPDATE bench_compare_sqlx SET count = count + 1 WHERE id = $1")
            .bind(id)
            .execute(&pool)
            .await?;
    }
    let crud_update_rps = CRUD_ITERS as f64 / t.elapsed().as_secs_f64();

    // ── CRUD INSERT (with warmup) ─────────────────────────────────────────────
    for _ in 0..WARMUP_ITERS {
        sqlx::query(
            "INSERT INTO bench_compare_sqlx (id, val, count) VALUES ($1, $2, $3) \
             ON CONFLICT (id) DO UPDATE SET count = bench_compare_sqlx.count + 1",
        )
        .bind(20_000i32)
        .bind("new")
        .bind(0i32)
        .execute(&pool)
        .await?;
    }
    let t = Instant::now();
    for i in 0..CRUD_ITERS {
        let id = (10_000 + i % 10_000) as i32;
        sqlx::query(
            "INSERT INTO bench_compare_sqlx (id, val, count) VALUES ($1, $2, $3) \
             ON CONFLICT (id) DO UPDATE SET count = bench_compare_sqlx.count + 1",
        )
        .bind(id)
        .bind("new")
        .bind(0i32)
        .execute(&pool)
        .await?;
    }
    let crud_insert_rps = CRUD_ITERS as f64 / t.elapsed().as_secs_f64();

    sqlx::query("DROP TABLE IF EXISTS bench_compare_sqlx")
        .execute(&pool)
        .await?;

    // ── TFB Multi-Query: sequential ───────────────────────────────────────────
    let mut multi_seq = Vec::with_capacity(MULTI_QUERY_COUNTS.len());
    for &n in MULTI_QUERY_COUNTS {
        let mut rng = 0x1234_5678u32;
        for _ in 0..WARMUP_ITERS {
            for _ in 0..n {
                let id = rand_world_id(&mut rng);
                let _: (i32, i32) = sqlx::query_as(WORLD_SQL).bind(id).fetch_one(&pool).await?;
            }
        }
        let mut rng = 0xabcd_ef01u32;
        let t = Instant::now();
        for _ in 0..MULTI_REQUESTS {
            for _ in 0..n {
                let id = rand_world_id(&mut rng);
                let (_id, _rn): (i32, i32) =
                    sqlx::query_as(WORLD_SQL).bind(id).fetch_one(&pool).await?;
            }
        }
        multi_seq.push(MULTI_REQUESTS as f64 / t.elapsed().as_secs_f64());
    }

    // ── TFB Database Updates (Test #5) ────────────────────────────────────
    let mut updates_seq = Vec::with_capacity(MULTI_QUERY_COUNTS.len());
    for &n in MULTI_QUERY_COUNTS {
        let mut rng = 0x2468_1357u32;
        for _ in 0..WARMUP_ITERS {
            for _ in 0..n {
                let id = rand_world_id(&mut rng);
                let (row_id, _rn): (i32, i32) =
                    sqlx::query_as(WORLD_SQL).bind(id).fetch_one(&pool).await?;
                let new_rn = rand_world_id(&mut rng);
                sqlx::query("UPDATE world SET randomnumber = $1 WHERE id = $2")
                    .bind(new_rn)
                    .bind(row_id)
                    .execute(&pool)
                    .await?;
            }
        }
        let mut rng = 0x1357_2468u32;
        let t = Instant::now();
        for _ in 0..MULTI_REQUESTS {
            for _ in 0..n {
                let id = rand_world_id(&mut rng);
                let (row_id, _rn): (i32, i32) =
                    sqlx::query_as(WORLD_SQL).bind(id).fetch_one(&pool).await?;
                let new_rn = rand_world_id(&mut rng);
                sqlx::query("UPDATE world SET randomnumber = $1 WHERE id = $2")
                    .bind(new_rn)
                    .bind(row_id)
                    .execute(&pool)
                    .await?;
            }
        }
        updates_seq.push(MULTI_REQUESTS as f64 / t.elapsed().as_secs_f64());
    }

    Ok(DriverResults {
        name: "sqlx",
        select_one_rps,
        param_rps,
        crud_select_rps,
        crud_update_rps,
        crud_insert_rps,
        multi_seq,
        updates_seq,
    })
}

// ══════════════════════════════════════════════════════════════════════════════
// tokio-postgres
// ══════════════════════════════════════════════════════════════════════════════
async fn run_tokio_postgres() -> Result<DriverResults, Box<dyn std::error::Error>> {
    use tokio_postgres::NoTls;

    let (client, connection) = tokio_postgres::connect(
        &format!(
            "host={} port={} user={} password={} dbname={}",
            HOST, PORT, USER, PASS, DB
        ),
        NoTls,
    )
    .await?;
    tokio::spawn(async move {
        if let Err(e) = connection.await {
            eprintln!("tokio-postgres connection error: {}", e);
        }
    });

    // ── CRUD table setup ──────────────────────────────────────────────────────
    client
        .batch_execute("DROP TABLE IF EXISTS bench_compare_tp")
        .await?;
    client
        .batch_execute("CREATE TABLE bench_compare_tp (id INT PRIMARY KEY, val TEXT, count INT)")
        .await?;
    for chunk_start in (0..10_000usize).step_by(1000) {
        let mut sql = String::from("INSERT INTO bench_compare_tp (id, val, count) VALUES ");
        for j in 0..1000usize {
            let i = chunk_start + j;
            if j > 0 {
                sql.push_str(", ");
            }
            sql.push_str(&format!("({}, 'val_{}', {})", i, i, i));
        }
        client.batch_execute(&sql).await?;
    }

    // ── SELECT 1 (with warmup) ────────────────────────────────────────────────
    for _ in 0..WARMUP_ITERS {
        let row = client.query_one("SELECT 1", &[]).await?;
        let _: i32 = row.get(0);
    }
    let t = Instant::now();
    for _ in 0..SIMPLE_ITERS {
        let row = client.query_one("SELECT 1", &[]).await?;
        let _: i32 = row.get(0);
    }
    let select_one_rps = SIMPLE_ITERS as f64 / t.elapsed().as_secs_f64();

    // ── parameterised query (with warmup) ─────────────────────────────────────
    let mut rng = 0x9e37_79b9u32;
    for _ in 0..WARMUP_ITERS {
        let v = rand_world_id(&mut rng);
        let row = client
            .query_one("SELECT $1::int4 + $2::int4", &[&v, &10i32])
            .await?;
        let _: i32 = row.get(0);
    }
    let t = Instant::now();
    for _ in 0..SIMPLE_ITERS {
        let v = rand_world_id(&mut rng);
        let row = client
            .query_one("SELECT $1::int4 + $2::int4", &[&v, &10i32])
            .await?;
        let _: i32 = row.get(0);
    }
    let param_rps = SIMPLE_ITERS as f64 / t.elapsed().as_secs_f64();

    // ── CRUD SELECT (with warmup) ─────────────────────────────────────────────
    let mut rng = 0xdead_beefu32;
    for _ in 0..WARMUP_ITERS {
        let id = rand_world_id(&mut rng) % 10_000;
        let row = client
            .query_one("SELECT val FROM bench_compare_tp WHERE id = $1", &[&id])
            .await?;
        let _: &str = row.get(0);
    }
    let t = Instant::now();
    for _ in 0..CRUD_ITERS {
        let id = rand_world_id(&mut rng) % 10_000;
        let row = client
            .query_one("SELECT val FROM bench_compare_tp WHERE id = $1", &[&id])
            .await?;
        let _: &str = row.get(0);
    }
    let crud_select_rps = CRUD_ITERS as f64 / t.elapsed().as_secs_f64();

    // ── CRUD UPDATE (with warmup) ─────────────────────────────────────────────
    let mut rng = 0xcafe_f00du32;
    for _ in 0..WARMUP_ITERS {
        let id = rand_world_id(&mut rng) % 10_000;
        client
            .execute(
                "UPDATE bench_compare_tp SET count = count + 1 WHERE id = $1",
                &[&id],
            )
            .await?;
    }
    let t = Instant::now();
    for _ in 0..CRUD_ITERS {
        let id = rand_world_id(&mut rng) % 10_000;
        client
            .execute(
                "UPDATE bench_compare_tp SET count = count + 1 WHERE id = $1",
                &[&id],
            )
            .await?;
    }
    let crud_update_rps = CRUD_ITERS as f64 / t.elapsed().as_secs_f64();

    // ── CRUD INSERT (with warmup) ─────────────────────────────────────────────
    for _ in 0..WARMUP_ITERS {
        client
            .execute(
                "INSERT INTO bench_compare_tp (id, val, count) VALUES ($1, $2, $3) \
                 ON CONFLICT (id) DO UPDATE SET count = bench_compare_tp.count + 1",
                &[&20_000i32, &"new", &0i32],
            )
            .await?;
    }
    let t = Instant::now();
    for i in 0..CRUD_ITERS {
        let id = (10_000 + i % 10_000) as i32;
        client
            .execute(
                "INSERT INTO bench_compare_tp (id, val, count) VALUES ($1, $2, $3) \
                 ON CONFLICT (id) DO UPDATE SET count = bench_compare_tp.count + 1",
                &[&id, &"new", &0i32],
            )
            .await?;
    }
    let crud_insert_rps = CRUD_ITERS as f64 / t.elapsed().as_secs_f64();

    client
        .batch_execute("DROP TABLE IF EXISTS bench_compare_tp")
        .await?;

    // ── TFB Multi-Query: sequential ───────────────────────────────────────────
    let mut multi_seq = Vec::with_capacity(MULTI_QUERY_COUNTS.len());
    for &n in MULTI_QUERY_COUNTS {
        let mut rng = 0x1234_5678u32;
        for _ in 0..WARMUP_ITERS {
            for _ in 0..n {
                let id = rand_world_id(&mut rng);
                let row = client.query_one(WORLD_SQL, &[&id]).await?;
                let _: i32 = row.get(0);
            }
        }
        let mut rng = 0xabcd_ef01u32;
        let t = Instant::now();
        for _ in 0..MULTI_REQUESTS {
            for _ in 0..n {
                let id = rand_world_id(&mut rng);
                let row = client.query_one(WORLD_SQL, &[&id]).await?;
                let _: i32 = row.get(0);
            }
        }
        multi_seq.push(MULTI_REQUESTS as f64 / t.elapsed().as_secs_f64());
    }

    // ── TFB Database Updates (Test #5) ────────────────────────────────────
    let mut updates_seq = Vec::with_capacity(MULTI_QUERY_COUNTS.len());
    for &n in MULTI_QUERY_COUNTS {
        let mut rng = 0x2468_1357u32;
        for _ in 0..WARMUP_ITERS {
            for _ in 0..n {
                let id = rand_world_id(&mut rng);
                let row = client.query_one(WORLD_SQL, &[&id]).await?;
                let row_id: i32 = row.get(0);
                let new_rn = rand_world_id(&mut rng);
                client
                    .execute(
                        "UPDATE world SET randomnumber = $1 WHERE id = $2",
                        &[&new_rn, &row_id],
                    )
                    .await?;
            }
        }
        let mut rng = 0x1357_2468u32;
        let t = Instant::now();
        for _ in 0..MULTI_REQUESTS {
            for _ in 0..n {
                let id = rand_world_id(&mut rng);
                let row = client.query_one(WORLD_SQL, &[&id]).await?;
                let row_id: i32 = row.get(0);
                let new_rn = rand_world_id(&mut rng);
                client
                    .execute(
                        "UPDATE world SET randomnumber = $1 WHERE id = $2",
                        &[&new_rn, &row_id],
                    )
                    .await?;
            }
        }
        updates_seq.push(MULTI_REQUESTS as f64 / t.elapsed().as_secs_f64());
    }

    Ok(DriverResults {
        name: "tokio-postgres",
        select_one_rps,
        param_rps,
        crud_select_rps,
        crud_update_rps,
        crud_insert_rps,
        multi_seq,
        updates_seq,
    })
}

// ══════════════════════════════════════════════════════════════════════════════
// output helpers
// ══════════════════════════════════════════════════════════════════════════════
fn fmt_rps(v: f64) -> String {
    format!("{:>12.0}", v)
}

fn speedup_label(chopin: f64, other: f64) -> String {
    if other == 0.0 {
        return format!("{:>10}", "N/A");
    }
    let r = chopin / other;
    format!("{:>9.2}x", r)
}

fn print_crud_table(results: &[DriverResults]) {
    let label_w = 24usize;
    let col_w = 14usize;

    println!();
    println!("┌─ Traditional CRUD (req/s, single connection) ─────────────────────────┐");
    println!("│  {SIMPLE_ITERS} iters: SELECT 1 / param   │  {CRUD_ITERS} iters: CRUD       │");
    println!("└───────────────────────────────────────────────────────────────────────┘");
    println!();

    // header
    print!("{:<label_w$}", "Workload");
    for r in results {
        print!("{:>col_w$}", r.name);
    }
    println!();
    println!("{}", "─".repeat(label_w + col_w * results.len()));

    #[allow(clippy::type_complexity)]
    let rows: &[(&str, fn(&DriverResults) -> f64)] = &[
        ("SELECT 1 (req/s)", |r| r.select_one_rps),
        ("Param query (req/s)", |r| r.param_rps),
        ("CRUD SELECT (req/s)", |r| r.crud_select_rps),
        ("CRUD UPDATE (req/s)", |r| r.crud_update_rps),
        ("CRUD INSERT (req/s)", |r| r.crud_insert_rps),
    ];

    for (label, getter) in rows {
        print!("{:<label_w$}", label);
        for r in results {
            print!("{}", fmt_rps(getter(r)));
        }
        println!();
    }

    // speedup section
    let chopin = results.iter().find(|r| r.name == "chopin-pg").unwrap();
    let others: Vec<&DriverResults> = results.iter().filter(|r| r.name != "chopin-pg").collect();
    println!();
    println!("  chopin-pg speedup:");
    print!("{:<label_w$}", "");
    for o in &others {
        print!("{:>col_w$}", format!("vs {}", o.name));
    }
    println!();
    println!("{}", "─".repeat(label_w + col_w * others.len()));
    for (label, getter) in rows {
        print!("{:<label_w$}", label);
        for o in &others {
            print!("{:>col_w$}", speedup_label(getter(chopin), getter(o)));
        }
        println!();
    }
}

fn print_multi_query_table(results: &[DriverResults]) {
    println!();
    println!("┌─ TFB Multi-Query  (requests/s, one connection) ───────────────────────┐");
    println!(
        "│  N=queries per request  •  {MULTI_REQUESTS} requests each  •  world table 10K rows  │"
    );
    println!("└───────────────────────────────────────────────────────────────────────┘");
    println!();

    let label_w = 8usize;
    let col_w = 16usize;

    // Header
    print!("{:<label_w$}", "N");
    for r in results {
        print!("{:>col_w$}", r.name);
    }
    println!();
    println!("{}", "─".repeat(label_w + col_w * results.len()));

    for (i, &n) in MULTI_QUERY_COUNTS.iter().enumerate() {
        print!("{:<label_w$}", n);
        for r in results {
            print!("{}", fmt_rps(r.multi_seq[i]));
        }
        println!();
    }

    // chopin-pg vs others speedup
    if let Some(chopin) = results.iter().find(|r| r.name == "chopin-pg") {
        let others: Vec<&DriverResults> =
            results.iter().filter(|r| r.name != "chopin-pg").collect();
        println!();
        println!("  chopin-pg speedup:");
        print!("{:<label_w$}", "N");
        for o in &others {
            print!("{:>col_w$}", format!("vs {}", o.name));
        }
        println!();
        println!("{}", "─".repeat(label_w + col_w * others.len()));
        for (i, &n) in MULTI_QUERY_COUNTS.iter().enumerate() {
            print!("{:<label_w$}", n);
            for o in &others {
                print!(
                    "{:>col_w$}",
                    speedup_label(chopin.multi_seq[i], o.multi_seq[i])
                );
            }
            println!();
        }
    }
}

fn print_updates_table(results: &[DriverResults]) {
    println!();
    println!("┌─ TFB Database Updates / Test #5  (requests/s, one connection) ───────┐");
    println!(
        "│  N=rows per request (SELECT+UPDATE each)  •  {MULTI_REQUESTS} requests each      │"
    );
    println!("└───────────────────────────────────────────────────────────────────────┘");
    println!();

    let label_w = 8usize;
    let col_w = 16usize;

    print!("{:<label_w$}", "N");
    for r in results {
        print!("{:>col_w$}", r.name);
    }
    println!();
    println!("{}", "─".repeat(label_w + col_w * results.len()));

    for (i, &n) in MULTI_QUERY_COUNTS.iter().enumerate() {
        print!("{:<label_w$}", n);
        for r in results {
            print!("{}", fmt_rps(r.updates_seq[i]));
        }
        println!();
    }

    if let Some(chopin) = results.iter().find(|r| r.name == "chopin-pg") {
        let others: Vec<&DriverResults> =
            results.iter().filter(|r| r.name != "chopin-pg").collect();
        println!();
        println!("  chopin-pg speedup:");
        print!("{:<label_w$}", "N");
        for o in &others {
            print!("{:>col_w$}", format!("vs {}", o.name));
        }
        println!();
        println!("{}", "─".repeat(label_w + col_w * others.len()));
        for (i, &n) in MULTI_QUERY_COUNTS.iter().enumerate() {
            print!("{:<label_w$}", n);
            for o in &others {
                print!(
                    "{:>col_w$}",
                    speedup_label(chopin.updates_seq[i], o.updates_seq[i])
                );
            }
            println!();
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// world table setup (shared by all drivers)
// ══════════════════════════════════════════════════════════════════════════════
fn setup_world_table() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use chopin_pg::{PgConfig, PgConnection};

    let cfg = PgConfig::new(HOST, PORT, USER, PASS, DB);
    let mut conn = PgConnection::connect(&cfg)?;

    conn.execute("DROP TABLE IF EXISTS world", &[])?;
    conn.execute(
        "CREATE TABLE world (id INT PRIMARY KEY, randomnumber INT NOT NULL)",
        &[],
    )?;
    // Insert 10,000 rows (TFB standard)
    for chunk in (1i32..=10_000).collect::<Vec<_>>().chunks(1000) {
        let mut sql = String::from("INSERT INTO world (id, randomnumber) VALUES ");
        let mut rng = chunk[0] as u32 * 0x9e37_79b9;
        for (j, &id) in chunk.iter().enumerate() {
            if j > 0 {
                sql.push_str(", ");
            }
            rng ^= rng << 13;
            rng ^= rng >> 17;
            rng ^= rng << 5;
            let rn = 1 + (rng % 10_000);
            sql.push_str(&format!("({}, {})", id, rn));
        }
        conn.execute(&sql, &[])?;
    }
    println!("  world table: 10,000 rows inserted.");
    Ok(())
}

fn teardown_world_table() {
    use chopin_pg::{PgConfig, PgConnection};
    let cfg = PgConfig::new(HOST, PORT, USER, PASS, DB);
    if let Ok(mut conn) = PgConnection::connect(&cfg) {
        let _ = conn.execute("DROP TABLE IF EXISTS world", &[]);
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// main
// ══════════════════════════════════════════════════════════════════════════════
#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== chopin-pg vs sqlx vs tokio-postgres ===");
    println!("postgres://{}@{}:{}/{}", USER, HOST, PORT, DB);
    println!();

    println!("Setting up shared world table…");
    tokio::task::spawn_blocking(setup_world_table)
        .await?
        .map_err(|e| e.to_string())?;

    println!("[1/3] chopin-pg …");
    let chopin = tokio::task::spawn_blocking(run_chopin_pg)
        .await?
        .map_err(|e| format!("chopin-pg: {e}"))?;
    println!("      done.");

    println!("[2/3] sqlx …");
    let sqlx_res = run_sqlx().await?;
    println!("      done.");

    println!("[3/3] tokio-postgres …");
    let tp = run_tokio_postgres().await?;
    println!("      done.");

    teardown_world_table();

    let results = vec![chopin, sqlx_res, tp];
    print_crud_table(&results);
    println!();
    print_multi_query_table(&results);
    println!();
    print_updates_table(&results);
    println!();
    println!("  >1.0x = chopin-pg is faster   <1.0x = chopin-pg is slower");
    println!();

    Ok(())
}
