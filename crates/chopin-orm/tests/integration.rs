use chopin_orm::Model;
use chopin_orm::builder::ColumnTrait;
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
impl chopin_orm::Validate for User {}

#[test]
fn test_orm_crud() {
    let config = PgConfig::from_url("postgres://chopin:chopin@127.0.0.1:5432/postgres").unwrap();
    let mut pool = PgPool::connect(config, 1).unwrap();

    {
        let mut conn = pool.get().unwrap();
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

    // Test Find One using DSL!
    let found = User::find()
        .filter(UserColumn::name.eq("Alice".to_param()))
        .one(&mut pool)
        .unwrap()
        .expect("User should exist");

    assert_eq!(found.name, "Alice");
    assert_eq!(found.age, 30);

    // Test Update
    user.age = 31;
    user.update(&mut pool).unwrap();

    let updated = User::find()
        .filter(UserColumn::id.eq(user.id.to_param()))
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
impl chopin_orm::Validate for AllTypesModel {}

#[test]
fn test_exhaustive_types() {
    let config = PgConfig::from_url("postgres://chopin:chopin@127.0.0.1:5432/postgres").unwrap();
    let mut pool = PgPool::connect(config, 1).unwrap();

    {
        let mut conn = pool.get().unwrap();
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

    let all = AllTypesModel::find()
        .order_by("id ASC")
        .all(&mut pool)
        .unwrap();
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].optional_text, Some("hello".to_string()));
    assert_eq!(all[0].optional_int, Some(84));
    assert_eq!(all[1].optional_text, None);
    assert_eq!(all[1].optional_int, None);

    // Filter by Option wrapper None / Null mapping natively
    let found = AllTypesModel::find()
        .filter(AllTypesModelColumn::optional_text.is_null())
        .all(&mut pool)
        .unwrap();
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
impl chopin_orm::Validate for TxModel {}

#[test]
fn test_transactions() {
    let config = PgConfig::from_url("postgres://chopin:chopin@127.0.0.1:5432/postgres").unwrap();
    let mut pool = PgPool::connect(config, 1).unwrap();

    {
        let mut conn = pool.get().unwrap();
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
        let mut model1 = TxModel {
            id: 0,
            name: "Commit Me".to_string(),
        };
        model1.insert(&mut tx).unwrap();
        tx.commit().unwrap();
    }

    let found_commit = TxModel::find().all(&mut pool).unwrap();
    assert_eq!(found_commit.len(), 1);

    // 2. Rollback Test
    {
        let mut pg_conn2 = pool.get().unwrap();
        let mut tx2 = chopin_orm::Transaction::begin(&mut pg_conn2).unwrap();
        let mut model2 = TxModel {
            id: 0,
            name: "Rollback Me".to_string(),
        };
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
impl chopin_orm::Validate for AdvModel {}

#[test]
fn test_advanced_queries() {
    let config = PgConfig::from_url("postgres://chopin:chopin@127.0.0.1:5432/postgres").unwrap();
    let mut pool = PgPool::connect(config, 1).unwrap();

    {
        let mut conn = pool.get().unwrap();
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
    let mut m1 = AdvModel {
        id: 0,
        name: "A".to_string(),
        hits: 10,
    };
    let mut m2 = AdvModel {
        id: 0,
        name: "B".to_string(),
        hits: 20,
    };
    m1.insert(&mut pool).unwrap();
    m2.insert(&mut pool).unwrap();

    let total = AdvModel::find().count(&mut pool).unwrap();
    assert_eq!(total, 2);

    let filtered_count = AdvModel::find()
        .filter(AdvModelColumn::hits.gt(15.to_param()))
        .count(&mut pool)
        .unwrap();
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

    let updated_a = AdvModel::find()
        .filter(AdvModelColumn::id.eq(m1.id.to_param()))
        .one(&mut pool)
        .unwrap()
        .unwrap();
    assert_eq!(updated_a.name, "A_Updated");
    assert_eq!(updated_a.hits, 99);
}

#[test]
fn test_active_model_partial_update() {
    let config = PgConfig::from_url("postgres://chopin:chopin@127.0.0.1:5432/postgres").unwrap();
    let mut pool = PgPool::connect(config, 1).unwrap();

    {
        let mut conn = pool.get().unwrap();
        conn.execute(
            "CREATE TABLE IF NOT EXISTS orm_active_test (
                id SERIAL PRIMARY KEY,
                name TEXT NOT NULL,
                age INT NOT NULL
            )",
            &[],
        )
        .unwrap();
        conn.execute("TRUNCATE TABLE orm_active_test RESTART IDENTITY", &[])
            .unwrap();
    }

    #[derive(Model, Debug, Clone, PartialEq)]
    #[model(table_name = "orm_active_test")]
    pub struct ActiveTest {
        #[model(primary_key)]
        pub id: i32,
        pub name: String,
        pub age: i32,
    }
    impl chopin_orm::Validate for ActiveTest {}

    use chopin_orm::active_model::ActiveModel;

    // 1. Insert initial record
    let mut model = ActiveTest {
        id: 0,
        name: "Initial".to_string(),
        age: 20,
    };
    model.insert(&mut pool).unwrap();

    // 2. Convert to ActiveModel and perform partial update
    let mut active_model: ActiveModel<ActiveTest> = model.clone().into();

    // Only set 'age'
    active_model.set("age", 25);

    active_model.update(&mut pool).unwrap();
    let updated_model = active_model.inner;

    // Verify 'age' was updated but 'name' stayed the same
    assert_eq!(updated_model.age, 25);
    assert_eq!(updated_model.name, "Initial");

    // 3. Test with another partial update
    let mut active_model2: ActiveModel<ActiveTest> = updated_model.into();
    active_model2.set("name", "Updated Name".to_string());

    active_model2.update(&mut pool).unwrap();
    let final_model = active_model2.inner;
    assert_eq!(final_model.name, "Updated Name");
    assert_eq!(final_model.age, 25);
}

#[derive(Model, Debug, Clone, PartialEq)]
#[model(table_name = "orm_introspection_test")]
pub struct IntrospectionTest {
    #[model(primary_key)]
    pub id: i32,
    pub title: String,
    pub body: Option<String>,
}
impl chopin_orm::Validate for IntrospectionTest {}

#[test]
fn test_schema_introspection() {
    let config = PgConfig::from_url("postgres://chopin:chopin@127.0.0.1:5432/postgres").unwrap();
    let mut pool = PgPool::connect(config, 1).unwrap();

    let stmt = IntrospectionTest::create_table_stmt();
    println!("Generated CREATE TABLE stmt:\n{}", stmt);

    assert!(stmt.contains("CREATE TABLE IF NOT EXISTS orm_introspection_test"));
    assert!(stmt.contains("id SERIAL PRIMARY KEY"));
    assert!(stmt.contains("title TEXT NOT NULL"));
    assert!(stmt.contains("body TEXT")); // Optional, so no NOT NULL

    {
        let mut conn = pool.get().unwrap();
        // Drop it first to ensure clean state
        conn.execute("DROP TABLE IF EXISTS orm_introspection_test", &[])
            .unwrap();
    }

    // Now create it natively using the ORM's trait method
    IntrospectionTest::create_table(&mut pool).unwrap();

    // Verify we can insert into it
    let mut model = IntrospectionTest {
        id: 0, // Should be generated
        title: "Hello Introspection".to_string(),
        body: None,
    };
    model.insert(&mut pool).unwrap();
    assert!(model.id > 0);

    let found = IntrospectionTest::find().one(&mut pool).unwrap().unwrap();
    assert_eq!(found.title, "Hello Introspection");
}

#[derive(Model, Debug, Clone, PartialEq)]
#[model(table_name = "orm_agg_test")]
pub struct AggTest {
    #[model(primary_key)]
    pub id: i32,
    pub group_id: i32,
    pub score: i32,
}
impl chopin_orm::Validate for AggTest {}

#[test]
fn test_aggregations() {
    let config = PgConfig::from_url("postgres://chopin:chopin@127.0.0.1:5432/postgres").unwrap();
    let mut pool = PgPool::connect(config, 1).unwrap();

    {
        let mut conn = pool.get().unwrap();
        conn.execute("DROP TABLE IF EXISTS orm_agg_test", &[])
            .unwrap();
    }
    AggTest::create_table(&mut pool).unwrap();

    // Insert data
    let data = vec![
        (1, 10),
        (1, 20),
        (1, 30), // Group 1, sum: 60, count: 3, max: 30
        (2, 5),
        (2, 5),   // Group 2, sum: 10, count: 2, max: 5
        (3, 100), // Group 3, sum: 100, count: 1, max: 100
    ];

    for (g, s) in data {
        AggTest {
            id: 0,
            group_id: g,
            score: s,
        }
        .insert(&mut pool)
        .unwrap();
    }

    // Test specific aggregation: SELECT group_id, SUM(score) FROM table GROUP BY group_id HAVING SUM(score) > 50 ORDER BY group_id ASC
    let raw_rows = AggTest::find()
        .select_only(vec![
            chopin_orm::builder::Expr::new("group_id", vec![]),
            AggTestColumn::score.sum(),
        ])
        .group_by("group_id")
        .having(chopin_orm::builder::Expr::new(
            "SUM(score) > {}",
            vec![50i64.to_param()],
        ))
        .order_by("group_id ASC")
        .into_raw(&mut pool)
        .unwrap();

    // Expecting Group 1 (sum 60) and Group 3 (sum 100)
    assert_eq!(raw_rows.len(), 2);

    let g1 = if let chopin_pg::PgValue::Int4(v) = raw_rows[0].get(0).unwrap() {
        v
    } else {
        panic!("Wrong type")
    };
    let s1 = if let chopin_pg::PgValue::Int8(v) = raw_rows[0].get(1).unwrap() {
        v
    } else {
        panic!("Wrong type")
    };
    assert_eq!(g1, 1);
    assert_eq!(s1, 60);

    let g3 = if let chopin_pg::PgValue::Int4(v) = raw_rows[1].get(0).unwrap() {
        v
    } else {
        panic!("Wrong type")
    };
    let s3 = if let chopin_pg::PgValue::Int8(v) = raw_rows[1].get(1).unwrap() {
        v
    } else {
        panic!("Wrong type")
    };
    assert_eq!(g3, 3);
    assert_eq!(s3, 100);
}

#[derive(Model, Debug, Clone, PartialEq)]
#[model(table_name = "orm_authors")]
#[model(has_many(RelPost, fk = "author_id"))]
pub struct Author {
    #[model(primary_key)]
    pub id: i32,
    pub name: String,
}
impl chopin_orm::Validate for Author {}

#[derive(Model, Debug, Clone, PartialEq)]
#[model(table_name = "orm_posts")]
pub struct RelPost {
    #[model(primary_key)]
    pub id: i32,
    pub title: String,
    #[model(belongs_to(Author))]
    pub author_id: i32,
}
impl chopin_orm::Validate for RelPost {}

#[test]
fn test_relationships() {
    let config = PgConfig::from_url("postgres://chopin:chopin@127.0.0.1:5432/postgres").unwrap();
    let mut pool = PgPool::connect(config, 1).unwrap();

    {
        let mut conn = pool.get().unwrap();
        conn.execute("DROP TABLE IF EXISTS orm_posts", &[]).unwrap();
        conn.execute("DROP TABLE IF EXISTS orm_authors", &[])
            .unwrap();
    }

    let author_stmt = Author::create_table_stmt();
    let post_stmt = RelPost::create_table_stmt();
    println!("Author STMT:\n{}", author_stmt);
    println!("Post STMT:\n{}", post_stmt);

    assert!(post_stmt.contains("FOREIGN KEY (author_id) REFERENCES orm_authors (id)"));

    Author::create_table(&mut pool).unwrap();
    RelPost::create_table(&mut pool).unwrap();

    let mut a1 = Author {
        id: 0,
        name: "Alice".to_string(),
    };
    a1.insert(&mut pool).unwrap();

    let mut p1 = RelPost {
        id: 0,
        title: "Alice First Post".to_string(),
        author_id: a1.id,
    };
    p1.insert(&mut pool).unwrap();

    // Custom JOIN Test
    let raw = RelPost::find()
        .join("JOIN orm_authors ON orm_authors.id = orm_posts.author_id")
        .select_only(vec![
            chopin_orm::builder::Expr::new("orm_posts.title", vec![]),
            chopin_orm::builder::Expr::new("orm_authors.name", vec![]),
        ])
        .into_raw(&mut pool)
        .unwrap();

    assert_eq!(raw.len(), 1);

    let post_title = if let chopin_pg::PgValue::Text(ref s) = raw[0].get(0).unwrap() {
        s.clone()
    } else {
        panic!()
    };
    let author_name = if let chopin_pg::PgValue::Text(ref s) = raw[0].get(1).unwrap() {
        s.clone()
    } else {
        panic!()
    };

    assert_eq!(post_title, "Alice First Post");
    assert_eq!(author_name, "Alice");

    // Lazy Loading test
    let loaded_author = p1.fetch_author(&mut pool).unwrap().unwrap();
    assert_eq!(loaded_author.id, a1.id);
    assert_eq!(loaded_author.name, "Alice");

    let loaded_posts = a1.fetch_relposts(&mut pool).unwrap();
    assert_eq!(loaded_posts.len(), 1);
    assert_eq!(loaded_posts[0].title, "Alice First Post");
}

#[derive(Model, Debug, Clone, PartialEq)]
#[model(table_name = "orm_automigrate_test")]
pub struct AutoMigrateV1 {
    #[model(primary_key)]
    pub id: i32,
    pub name: String,
}
impl chopin_orm::Validate for AutoMigrateV1 {}

#[derive(Model, Debug, Clone, PartialEq)]
#[model(table_name = "orm_automigrate_test")]
pub struct AutoMigrateV2 {
    #[model(primary_key)]
    pub id: i32,
    pub name: String,
    pub age: Option<i32>,
}
impl chopin_orm::Validate for AutoMigrateV2 {}

#[test]
fn test_auto_migrate_and_partial_updates() {
    let config = PgConfig::from_url("postgres://chopin:chopin@127.0.0.1:5432/postgres").unwrap();
    let mut pool = PgPool::connect(config, 1).unwrap();

    {
        let mut conn = pool.get().unwrap();
        conn.execute("DROP TABLE IF EXISTS orm_automigrate_test", &[])
            .unwrap();
    }

    // Step 1: Create V1 table
    AutoMigrateV1::sync_schema(&mut pool).unwrap();

    // Insert V1 data
    let mut item1 = AutoMigrateV1 {
        id: 0,
        name: "V1 Item".to_string(),
    };
    item1.insert(&mut pool).unwrap();

    // Step 2: Auto Migrate to V2 (adds 'age' column)
    AutoMigrateV2::sync_schema(&mut pool).unwrap();

    // Verify V2 can fetch V1 record
    let found_v2 = AutoMigrateV2::find().one(&mut pool).unwrap().unwrap();
    assert_eq!(found_v2.name, "V1 Item");
    assert_eq!(found_v2.age, None);

    // Step 3: Test partial update (update_columns)
    let mut item_to_update = found_v2.clone();
    item_to_update.age = Some(42);
    item_to_update.name = "Updated Name".to_string();

    // Only update 'age', name should remain unchanged in DB if we were using a raw query,
    // but update_columns uses the current values in the struct.
    // Wait, update_columns takes a list of columns to update.
    let updated = item_to_update.update_columns(&mut pool, &["age"]).unwrap();

    assert_eq!(updated.age, Some(42));
    assert_eq!(updated.name, "V1 Item"); // Struct had "Updated Name", but we only pushed "age" literal to DB. 
    // Wait, our update_columns implementation takes values from the struct.
    // So if item_to_update.name was "Updated Name", but we only updated "age",
    // the DB still has "V1 Item". The returned struct from `update_columns` should reflect the DB state.

    let final_check = AutoMigrateV2::find().one(&mut pool).unwrap().unwrap();
    assert_eq!(final_check.age, Some(42));
    assert_eq!(final_check.name, "V1 Item");
}
