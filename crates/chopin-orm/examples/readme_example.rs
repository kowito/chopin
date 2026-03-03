use chopin_orm::{Model, PgPool, Validate, builder::ColumnTrait};
use chopin_pg::PgConfig;

#[derive(Model, Debug, Clone)]
#[model(table_name = "users")]
pub struct User {
    #[model(primary_key)]
    pub id: i32,
    pub name: String,
    pub email: String,
    pub age: Option<i32>,
}

impl Validate for User {
    fn validate(&self) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        if self.email.is_empty() {
            errors.push("Email cannot be empty".to_string());
        }
        if let Some(age) = self.age
            && age < 0
        {
            errors.push("Age cannot be negative".to_string());
        }
        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors)
        }
    }
}

#[derive(Model, Debug, Clone)]
#[model(table_name = "posts")]
pub struct Post {
    #[model(primary_key)]
    pub id: i32,
    pub title: String,
    #[model(belongs_to(User))]
    pub user_id: i32,
}

impl Validate for Post {}

fn main() {
    let config = PgConfig::from_url("postgres://chopin:chopin@127.0.0.1:5432/postgres").unwrap();
    let mut pool = PgPool::connect(config, 5).unwrap();

    // 1. Auto-Migration
    User::sync_schema(&mut pool).expect("User migration failed");
    Post::sync_schema(&mut pool).expect("Post migration failed");

    // 2. Type-Safe Insertion
    let mut alice = User {
        id: 0,
        name: "Alice".to_string(),
        email: "alice@example.com".to_string(),
        age: Some(30),
    };
    alice.insert(&mut pool).expect("Insert failed");
    println!("Inserted User ID: {}", alice.id);

    // 3. Relationships & Lazy Loading
    let mut post = Post {
        id: 0,
        title: "Hello World".to_string(),
        user_id: alice.id,
    };
    post.insert(&mut pool).expect("Post insert failed");

    let author = post
        .fetch_user_id(&mut pool)
        .unwrap()
        .expect("Author not found");
    println!("Post author: {}", author.name);

    // 4. Fluent DSL with Type-Safe Columns
    use UserColumn::*;
    let users = User::find()
        .filter(name.eq("Alice"))
        .filter(age.gt(25))
        .all(&mut pool)
        .expect("Query failed");

    println!("Found {} users", users.len());

    // 5. Partial Updates (No ActiveModel needed)
    let mut to_update = users[0].clone();
    to_update.name = "Alice Updated".to_string();
    to_update
        .update_columns(&mut pool, &["name"])
        .expect("Partial update failed");

    // 6. Aggregations
    let count = User::find().count(&mut pool).expect("Count failed");
    println!("Total users: {}", count);
}
