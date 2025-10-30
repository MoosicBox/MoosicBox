#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Query Builder Example
//!
//! This example demonstrates the type-safe query builder API provided by `switchy_database`.
//! It shows how to construct SQL queries using the fluent builder pattern instead of raw SQL.

use switchy_database::query::FilterableQuery;
use switchy_database::turso::TursoDatabase;
use switchy_database::{Database, DatabaseError};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Switchy Database - Query Builder Example ===\n");

    // Create an in-memory Turso database
    let db = TursoDatabase::new(":memory:").await?;

    // Setup: Create tables using raw SQL
    setup_database(&db).await?;

    // INSERT operations using query builder
    println!("--- INSERT Operations ---");
    insert_examples(&db).await?;

    // SELECT operations using query builder
    println!("\n--- SELECT Operations ---");
    select_examples(&db).await?;

    // UPDATE operations using query builder
    println!("\n--- UPDATE Operations ---");
    update_examples(&db).await?;

    // DELETE operations using query builder
    println!("\n--- DELETE Operations ---");
    delete_examples(&db).await?;

    // UPSERT operations using query builder
    println!("\n--- UPSERT Operations ---");
    upsert_examples(&db).await?;

    println!("\n=== Example Complete ===");
    Ok(())
}

/// Setup database schema
async fn setup_database(db: &dyn Database) -> Result<(), DatabaseError> {
    // Create users table
    db.exec_raw(
        "CREATE TABLE users (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            username TEXT NOT NULL UNIQUE,
            email TEXT NOT NULL,
            age INTEGER,
            active BOOLEAN DEFAULT 1
        )",
    )
    .await?;

    // Create posts table
    db.exec_raw(
        "CREATE TABLE posts (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            user_id INTEGER NOT NULL,
            title TEXT NOT NULL,
            content TEXT,
            views INTEGER DEFAULT 0,
            FOREIGN KEY (user_id) REFERENCES users(id)
        )",
    )
    .await?;

    println!("Database schema created");
    Ok(())
}

/// Demonstrate INSERT operations with the query builder
async fn insert_examples(db: &dyn Database) -> Result<(), DatabaseError> {
    // Insert a single user using the query builder
    let alice = db
        .insert("users")
        .value("username", "alice")
        .value("email", "alice@example.com")
        .value("age", 30)
        .value("active", true)
        .execute(db)
        .await?;

    let alice_id = alice.id().and_then(|v| v.as_i64()).unwrap();
    println!("Inserted user 'alice' with ID: {alice_id}");

    // Insert another user
    let bob = db
        .insert("users")
        .value("username", "bob")
        .value("email", "bob@example.com")
        .value("age", 25)
        .value("active", true)
        .execute(db)
        .await?;

    let bob_id = bob.id().and_then(|v| v.as_i64()).unwrap();
    println!("Inserted user 'bob' with ID: {bob_id}");

    // Insert a post for Alice
    let post = db
        .insert("posts")
        .value("user_id", alice_id)
        .value("title", "Hello World")
        .value("content", "This is my first post!")
        .value("views", 0)
        .execute(db)
        .await?;

    let post_id = post.id().and_then(|v| v.as_i64()).unwrap();
    println!("Inserted post with ID: {post_id}");

    Ok(())
}

/// Demonstrate SELECT operations with the query builder
async fn select_examples(db: &dyn Database) -> Result<(), DatabaseError> {
    // Select all users
    println!("All users:");
    let users = db.select("users").execute(db).await?;
    for user in &users {
        let username = user
            .get("username")
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap();
        let email = user
            .get("email")
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap();
        println!("  - {username} ({email})");
    }

    // Select specific columns
    println!("\nUsernames only:");
    let usernames = db
        .select("users")
        .columns(&["username", "age"])
        .execute(db)
        .await?;
    for row in &usernames {
        let username = row
            .get("username")
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap();
        let age = row.get("age").and_then(|v| v.as_i64());
        println!("  - {username} (age: {age:?})");
    }

    // Select with WHERE clause
    println!("\nUsers where age >= 30:");
    let adults = db.select("users").where_gte("age", 30).execute(db).await?;
    for row in &adults {
        let username = row
            .get("username")
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap();
        let age = row.get("age").and_then(|v| v.as_i64()).unwrap();
        println!("  - {username} (age: {age})");
    }

    // Select with LIMIT
    println!("\nFirst user only:");
    let first = db.select("users").limit(1).execute_first(db).await?;
    if let Some(row) = first {
        let username = row
            .get("username")
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap();
        println!("  - {username}");
    }

    Ok(())
}

/// Demonstrate UPDATE operations with the query builder
async fn update_examples(db: &dyn Database) -> Result<(), DatabaseError> {
    // Update a user's email
    db.update("users")
        .value("email", "alice.new@example.com")
        .where_eq("username", "alice")
        .execute(db)
        .await?;
    println!("Updated alice's email");

    // Update with multiple values
    db.update("users")
        .value("age", 31)
        .value("active", false)
        .where_eq("username", "alice")
        .execute(db)
        .await?;
    println!("Updated alice's age and active status");

    // Verify the update
    let alice = db
        .select("users")
        .where_eq("username", "alice")
        .execute_first(db)
        .await?
        .unwrap();
    let email = alice
        .get("email")
        .and_then(|v| v.as_str().map(str::to_string))
        .unwrap();
    let age = alice.get("age").and_then(|v| v.as_i64()).unwrap();
    let active = alice.get("active").and_then(|v| v.as_bool()).unwrap();
    println!("Alice now: email={email}, age={age}, active={active}");

    Ok(())
}

/// Demonstrate DELETE operations with the query builder
async fn delete_examples(db: &dyn Database) -> Result<(), DatabaseError> {
    // Insert a temporary user
    db.insert("users")
        .value("username", "temp_user")
        .value("email", "temp@example.com")
        .value("age", 20)
        .execute(db)
        .await?;
    println!("Created temporary user");

    // Count users before delete
    let before = db.select("users").execute(db).await?;
    println!("Users before delete: {}", before.len());

    // Delete the temporary user
    db.delete("users")
        .where_eq("username", "temp_user")
        .execute(db)
        .await?;
    println!("Deleted temporary user");

    // Count users after delete
    let after = db.select("users").execute(db).await?;
    println!("Users after delete: {}", after.len());

    Ok(())
}

/// Demonstrate UPSERT operations with the query builder
async fn upsert_examples(db: &dyn Database) -> Result<(), DatabaseError> {
    // Try to insert a user with a duplicate username
    // This will update the existing record instead of failing
    let result = db
        .upsert("users")
        .value("username", "alice")
        .value("email", "alice.updated@example.com")
        .value("age", 32)
        .where_eq("username", "alice")
        .execute(db)
        .await?;

    println!("Upserted alice (should update existing)");
    if let Some(row) = result.first() {
        let email = row
            .get("email")
            .and_then(|v| v.as_str().map(str::to_string))
            .unwrap();
        let age = row.get("age").and_then(|v| v.as_i64()).unwrap();
        println!("  Result: email={email}, age={age}");
    }

    // Upsert a new user (will insert)
    db.upsert("users")
        .value("username", "charlie")
        .value("email", "charlie@example.com")
        .value("age", 28)
        .where_eq("username", "charlie")
        .execute(db)
        .await?;
    println!("Upserted charlie (should insert new)");

    // Verify final state
    let users = db.select("users").execute(db).await?;
    println!("\nFinal user count: {}", users.len());
    for user in &users {
        let username = user
            .get("username")
            .and_then(|v| v.as_str().map(String::from))
            .unwrap();
        let email = user
            .get("email")
            .and_then(|v| v.as_str().map(String::from))
            .unwrap();
        println!("  - {username}: {email}");
    }

    Ok(())
}
