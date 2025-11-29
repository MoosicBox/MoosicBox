//! Basic usage example for Turso Database backend.
//!
//! This example demonstrates the fundamental operations available with the Turso database
//! backend through the `switchy_database` interface:
//!
//! * Creating an in-memory database instance
//! * Creating tables with raw SQL
//! * Inserting data with parameterized queries
//! * Querying data with and without parameters
//! * Updating and deleting records
//! * Checking table existence and introspecting columns
//!
//! Run this example with:
//! ```bash
//! cargo run --bin turso_basic
//! ```

use switchy_database::{Database, turso::TursoDatabase};

#[switchy_async::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Turso Database - Basic Usage Example");
    println!("=====================================\n");

    println!("Creating in-memory database...");
    let db = TursoDatabase::new(":memory:").await?;
    println!("✓ Database created\n");

    println!("Creating 'users' table...");
    db.exec_raw("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL, email TEXT)")
        .await?;
    println!("✓ Table created\n");

    println!("Inserting users...");
    db.exec_raw_params(
        "INSERT INTO users (name, email) VALUES (?1, ?2)",
        &["Alice".into(), "alice@example.com".into()],
    )
    .await?;

    db.exec_raw_params(
        "INSERT INTO users (name, email) VALUES (?1, ?2)",
        &["Bob".into(), "bob@example.com".into()],
    )
    .await?;

    db.exec_raw_params("INSERT INTO users (name) VALUES (?1)", &["Charlie".into()])
        .await?;
    println!("✓ Inserted 3 users\n");

    println!("Querying all users...");
    let rows = db
        .query_raw("SELECT id, name, COALESCE(email, 'no email') as email FROM users ORDER BY id")
        .await?;
    println!("Found {} users:", rows.len());
    for row in &rows {
        let id_val = row.get("id").unwrap();
        let id = id_val.as_i64().unwrap();
        let name_val = row.get("name").unwrap();
        let name = name_val.as_str().unwrap();
        let email_val = row.get("email").unwrap();
        let email = email_val.as_str().unwrap();
        println!("  * {} - {} ({})", id, name, email);
    }
    println!();

    println!("Querying with parameters...");
    let alice_rows = db
        .query_raw_params(
            "SELECT id, name FROM users WHERE name = ?1",
            &["Alice".into()],
        )
        .await?;
    println!(
        "Found user: {}",
        alice_rows[0].get("name").unwrap().as_str().unwrap()
    );
    println!();

    println!("Updating a user...");
    db.exec_raw_params(
        "UPDATE users SET email = ?1 WHERE name = ?2",
        &["charlie@example.com".into(), "Charlie".into()],
    )
    .await?;
    println!("✓ Updated Charlie's email\n");

    println!("Deleting a user...");
    db.exec_raw_params("DELETE FROM users WHERE name = ?1", &["Bob".into()])
        .await?;
    println!("✓ Deleted Bob\n");

    println!("Final user count:");
    let final_rows = db.query_raw("SELECT COUNT(*) as count FROM users").await?;
    let count = final_rows[0].get("count").unwrap().as_i64().unwrap();
    println!("  {} users remaining\n", count);

    println!("Checking table existence...");
    let db_trait: &dyn Database = &db;
    let users_exists = db_trait.table_exists("users").await?;
    let posts_exists = db_trait.table_exists("posts").await?;
    println!("  * 'users' table exists: {}", users_exists);
    println!("  * 'posts' table exists: {}", posts_exists);
    println!();

    println!("Getting table columns...");
    let columns = db_trait.get_table_columns("users").await?;
    println!("  'users' table has {} columns:", columns.len());
    for col in columns {
        println!("    - {} ({:?})", col.name, col.data_type);
    }

    println!("\n✓ Example completed successfully!");

    Ok(())
}
