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

fn setup_db(count: i32) -> PgPool {
    let config = PgConfig::from_url("postgres://chopin:chopin@127.0.0.1:5432/postgres").unwrap();
    let mut pool = PgPool::connect(config, 1).unwrap();

    let conn = pool.get().unwrap();
    conn.execute(
        "DROP TABLE IF EXISTS bench_users",
        &[],
    ).unwrap();
    conn.execute(
        "CREATE TABLE bench_users (
            id SERIAL PRIMARY KEY,
            name TEXT NOT NULL,
            age INT NOT NULL
        )",
        &[],
    ).unwrap();
    
    // Batch insert for speed
    if count > 0 {
        // Simple batch insert logic for benchmarking setup
        // For 1M rows, we might want to use COPY or multiple large batches
        // But for benchmark setup consistency, we'll do reasonably sized batches
        let batch_size = 5000;
        for i in (0..count).step_by(batch_size) {
            let end = (i + batch_size as i32).min(count);
            let mut query = String::from("INSERT INTO bench_users (name, age) VALUES ");
            let mut val_strings = Vec::new();
            for j in i..end {
                val_strings.push(format!("('User {}', {})", j, j));
            }
            query.push_str(&val_strings.join(", "));
            conn.execute(&query, &[]).unwrap();
        }
    }
    pool
}

fn bench_scale(c: &mut Criterion) {
    let scales = [1_000, 100_000]; // 1M might be too slow for immediate feedback, but user asked. 
                                  // Let's see if we can include it.
    
    for &count in &scales {
        let mut pool = setup_db(count);
        let group_name = format!("scale_{}", count);
        let mut group = c.benchmark_group(group_name);
        
        group.bench_function("raw_pg", |b| {
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

        group.bench_function("chopin_orm", |b| {
            b.iter(|| {
                let users = BenchUser::find().all(&mut pool).unwrap();
                black_box(users);
            })
        });
        
        group.finish();
    }
}

// 1M row benchmark separately because it takes much longer
fn bench_1m(c: &mut Criterion) {
    let count = 1_000_000;
    let mut pool = setup_db(count);
    let mut group = c.benchmark_group("scale_1m");
    group.sample_size(10); // Standard 100 iterations on 1M rows is too slow
    
    group.bench_function("raw_pg", |b| {
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

    group.bench_function("chopin_orm", |b| {
        b.iter(|| {
            let users = BenchUser::find().all(&mut pool).unwrap();
            black_box(users);
        })
    });
    
    group.finish();
}

criterion_group!(benches, bench_scale, bench_1m);
criterion_main!(benches);
