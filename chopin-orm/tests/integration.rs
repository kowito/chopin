use chopin_orm::Model;
use chopin_pg::types::ToParam;
use chopin_pg::{PgConfig, PgPool};

#[derive(Model, Debug, Clone, PartialEq)]
#[model(table_name = "orm_users")]
pub struct User {
    #[model(primary_key)]
    pub id: i32,
    pub name: String,
    pub age: i32,
    pub is_active: bool,
}

#[test]
fn test_orm_crud() {
    let config =
        PgConfig::from_url("postgres://chopin:chopin@127.0.0.1:5432/postgres").unwrap();
    let mut pool = PgPool::connect(config, 1).unwrap();

    {
        let conn = pool.get().unwrap();
        // Setup table
        conn.execute(
            "CREATE TABLE IF NOT EXISTS orm_users (
                id SERIAL PRIMARY KEY,
                name TEXT NOT NULL,
                age INT NOT NULL,
                is_active BOOLEAN NOT NULL
            )",
            &[],
        )
        .unwrap();
        conn.execute("TRUNCATE TABLE orm_users RESTART IDENTITY", &[])
            .unwrap();
    }

    // Test Insert
    let mut user = User {
        id: 0,
        name: "Alice".to_string(),
        age: 30,
        is_active: true,
    };
    user.insert(&mut pool).unwrap();
    assert!(user.id > 0);

    // Test Find One
    let found = User::find()
        .filter("name = $1", vec!["Alice".to_param()])
        .one(&mut pool)
        .unwrap()
        .expect("User should exist");

    assert_eq!(found.name, "Alice");
    assert_eq!(found.age, 30);

    // Test Update
    user.age = 31;
    user.update(&mut pool).unwrap();

    let updated = User::find()
        .filter("id = $1", vec![user.id.to_param()])
        .one(&mut pool)
        .unwrap()
        .unwrap();
    assert_eq!(updated.age, 31);

    // Test Find All
    let mut user2 = User {
        id: 0,
        name: "Bob".to_string(),
        age: 25,
        is_active: false,
    };
    user2.insert(&mut pool).unwrap();

    let all_users = User::find().order_by("id ASC").all(&mut pool).unwrap();
    assert_eq!(all_users.len(), 2);
    assert_eq!(all_users[0].name, "Alice");
    assert_eq!(all_users[1].name, "Bob");

    // Test Delete
    user.delete(&mut pool).unwrap();
    let all_users_after_delete = User::find().all(&mut pool).unwrap();
    assert_eq!(all_users_after_delete.len(), 1);
    assert_eq!(all_users_after_delete[0].name, "Bob");
}

#[derive(Model, Debug, Clone, PartialEq)]
#[model(table_name = "orm_all_types")]
pub struct AllTypesModel {
    #[model(primary_key)]
    pub id: i64,
    pub name: String,
    pub age: i32,
    pub is_active: bool,
    pub score: f64,
    pub optional_text: Option<String>,
    pub optional_int: Option<i32>,
}

#[test]
fn test_exhaustive_types() {
    let config =
        PgConfig::from_url("postgres://chopin:chopin@127.0.0.1:5432/postgres").unwrap();
    let mut pool = PgPool::connect(config, 1).unwrap();

    {
        let conn = pool.get().unwrap();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS orm_all_types (
                id BIGSERIAL PRIMARY KEY,
                name TEXT NOT NULL,
                age INT NOT NULL,
                is_active BOOLEAN NOT NULL,
                score DOUBLE PRECISION NOT NULL,
                optional_text TEXT,
                optional_int INT
            )",
            &[],
        )
        .unwrap();
        conn.execute("TRUNCATE TABLE orm_all_types RESTART IDENTITY", &[])
            .unwrap();
    }

    // Insert fully populated model
    let mut full_model = AllTypesModel {
        id: 0,
        name: "Full".to_string(),
        age: 42,
        is_active: true,
        score: 99.9,
        optional_text: Some("hello".to_string()),
        optional_int: Some(84),
    };
    full_model.insert(&mut pool).unwrap();

    // Insert model with Nulls
    let mut null_model = AllTypesModel {
        id: 0,
        name: "Nulls".to_string(),
        age: 0,
        is_active: false,
        score: 0.0,
        optional_text: None,
        optional_int: None,
    };
    null_model.insert(&mut pool).unwrap();

    let all = AllTypesModel::find().order_by("id ASC").all(&mut pool).unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].optional_text, Some("hello".to_string()));
    assert_eq!(all[0].optional_int, Some(84));
    assert_eq!(all[1].optional_text, None);
    assert_eq!(all[1].optional_int, None);

    // Filter by Option wrapper None / Null mapping natively
    let found = AllTypesModel::find().filter("optional_text IS NULL", vec![]).all(&mut pool).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].name, "Nulls");
}

#[derive(Model, Debug, Clone, PartialEq)]
#[model(table_name = "orm_tx")]
pub struct TxModel {
    #[model(primary_key)]
    pub id: i32,
    pub name: String,
}

#[test]
fn test_transactions() {
    let config =
        PgConfig::from_url("postgres://chopin:chopin@127.0.0.1:5432/postgres").unwrap();
    let mut pool = PgPool::connect(config, 1).unwrap();

    {
        let conn = pool.get().unwrap();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS orm_tx (
                id SERIAL PRIMARY KEY,
                name TEXT NOT NULL
            )",
            &[],
        )
        .unwrap();
        conn.execute("TRUNCATE TABLE orm_tx RESTART IDENTITY", &[])
            .unwrap();
    }

    // 1. Commit Test
    {
        let mut pg_conn = pool.get().unwrap();
        let mut tx = chopin_orm::Transaction::begin(&mut pg_conn).unwrap();
        let mut model1 = TxModel { id: 0, name: "Commit Me".to_string() };
        model1.insert(&mut tx).unwrap();
        tx.commit().unwrap();
    }

    let found_commit = TxModel::find().all(&mut pool).unwrap();
    assert_eq!(found_commit.len(), 1);

    // 2. Rollback Test
    {
        let mut pg_conn2 = pool.get().unwrap();
        let mut tx2 = chopin_orm::Transaction::begin(&mut pg_conn2).unwrap();
        let mut model2 = TxModel { id: 0, name: "Rollback Me".to_string() };
        model2.insert(&mut tx2).unwrap();
        
        // verify it's inside the transaction
        let found_in_tx = TxModel::find().all(&mut tx2).unwrap();
        assert_eq!(found_in_tx.len(), 2);

        tx2.rollback().unwrap();
    }

    // Verify it is gone
    let final_found = TxModel::find().all(&mut pool).unwrap();
    assert_eq!(final_found.len(), 1);
    assert_eq!(final_found[0].name, "Commit Me");
}

#[derive(Model, Debug, Clone, PartialEq)]
#[model(table_name = "orm_adv")]
pub struct AdvModel {
    #[model(primary_key)]
    pub id: i32,
    pub name: String,
    pub hits: i32,
}

#[test]
fn test_advanced_queries() {
    let config =
        PgConfig::from_url("postgres://chopin:chopin@127.0.0.1:5432/postgres").unwrap();
    let mut pool = PgPool::connect(config, 1).unwrap();

    {
        let conn = pool.get().unwrap();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS orm_adv (
                id SERIAL PRIMARY KEY,
                name TEXT NOT NULL,
                hits INT NOT NULL
            )",
            &[],
        )
        .unwrap();
        conn.execute("TRUNCATE TABLE orm_adv RESTART IDENTITY", &[])
            .unwrap();
    }

    // 1. Test Count
    let mut m1 = AdvModel { id: 0, name: "A".to_string(), hits: 10 };
    let mut m2 = AdvModel { id: 0, name: "B".to_string(), hits: 20 };
    m1.insert(&mut pool).unwrap();
    m2.insert(&mut pool).unwrap();

    let total = AdvModel::find().count(&mut pool).unwrap();
    assert_eq!(total, 2);

    let filtered_count = AdvModel::find().filter("hits > 15", vec![]).count(&mut pool).unwrap();
    assert_eq!(filtered_count, 1);

    // 2. Test Upsert
    // m1 currently has hits: 10. We will try to 'insert' a new record but force the same ID.
    let mut upsert_model = AdvModel {
        id: m1.id,
        name: "A_Updated".to_string(),
        hits: 99,
    };
    
    // This would normally fail with unique constraint violation on `id`.
    // However, `upsert` handles it and updates the row.
    upsert_model.upsert(&mut pool).unwrap();

    let total_after_upsert = AdvModel::find().count(&mut pool).unwrap();
    assert_eq!(total_after_upsert, 2); // Still 2 rows!

    let updated_a = AdvModel::find().filter("id = $1", vec![m1.id.to_param()]).one(&mut pool).unwrap().unwrap();
    assert_eq!(updated_a.name, "A_Updated");
    assert_eq!(updated_a.hits, 99);
}
