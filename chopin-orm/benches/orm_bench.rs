use chopin_orm::{Model, ExtractValue, FromRow};
use chopin_pg::{PgConfig, PgPool, Row, PgValue, PgResult};
use criterion::{criterion_group, criterion_main, Criterion, black_box};

#[derive(Model, Debug, Clone, PartialEq)]
#[model(table_name = "bench_users")]
pub struct BenchUser {
    #[model(primary_key)]
    pub id: i32,
    pub name: String,
    pub age: i32,
}

fn setup_db() -> PgPool {
    let config = PgConfig::from_url("postgres://chopin:chopin@127.0.0.1:5432/postgres").unwrap();
    let mut pool = PgPool::connect(config, 1).unwrap();

    let conn = pool.get().unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS bench_users (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            age INT NOT NULL
        )",
        &[],
    ).unwrap();
    conn.execute("TRUNCATE TABLE bench_users RESTART IDENTITY", &[]).unwrap();
    
    // Insert 100 rows
    for i in 0..100 {
        conn.execute(
            "INSERT INTO bench_users (name, age) VALUES ($1, $2)",
            &[&format!("User {}", i), &i],
        ).unwrap();
    }
    pool
}

fn bench_raw_pg(c: &mut Criterion) {
    let mut pool = setup_db();
    
    c.bench_function("raw_pg_select_100_rows", |b| {
        b.iter(|| {
            let conn = pool.get().unwrap();
            let rows = conn.query("SELECT id, name, age FROM bench_users", &[]).unwrap();
            let users: Vec<BenchUser> = rows.into_iter().map(|row| {
                BenchUser {
                    id: row.get_i32(0).unwrap().unwrap(),
                    name: row.get_str(1).unwrap().unwrap().to_string(),
                    age: row.get_i32(2).unwrap().unwrap(),
                }
            }).collect();
            black_box(users);
        })
    });
}

fn bench_chopin_orm(c: &mut Criterion) {
    let mut pool = setup_db();
    
    c.bench_function("chopin_orm_select_100_rows", |b| {
        b.iter(|| {
            let users = BenchUser::find().all(&mut pool).unwrap();
            black_box(users);
        })
    });
}

criterion_group!(benches, bench_raw_pg, bench_chopin_orm);
criterion_main!(benches);
