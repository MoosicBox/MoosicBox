//! Turso Database transaction examples.
//!
//! This example demonstrates how to use transactions with the Turso Database backend,
//! including commits, rollbacks, and savepoints. It shows three key scenarios:
//!
//! 1. Successful transaction with commit
//! 2. Failed transaction with rollback
//! 3. Complex transaction with nested operations
//!
//! The example uses an in-memory database with a simple accounts table to illustrate
//! transaction concepts.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use switchy_database::{Database, DatabaseValue, turso::TursoDatabase};

/// Entry point for the Turso transactions example.
///
/// Demonstrates three transaction scenarios:
/// * Successful transaction - transferring funds between accounts with commit
/// * Failed transaction - attempting an overdraw and rolling back
/// * Nested transaction - creating new accounts with multiple operations
///
/// # Errors
///
/// Returns an error if:
/// * Database creation fails
/// * Table creation fails
/// * Any database operation (INSERT, UPDATE, SELECT) fails
/// * Transaction operations (begin, commit, rollback) fail
///
/// # Panics
///
/// Panics if:
/// * Database value retrieval returns `None` when a value is expected
/// * Type conversion of database values fails (e.g., value is not a string or i64)
#[allow(clippy::too_many_lines)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Turso Database - Transaction Example");
    println!("====================================\n");

    println!("Creating in-memory database...");
    let db = TursoDatabase::new(":memory:").await?;
    println!("✓ Database created\n");

    println!("Creating 'accounts' table...");
    db.exec_raw(
        "CREATE TABLE accounts (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL,
            balance INTEGER NOT NULL
        )",
    )
    .await?;
    println!("✓ Table created\n");

    println!("Setting up test accounts...");
    db.exec_raw_params(
        "INSERT INTO accounts (name, balance) VALUES (?1, ?2)",
        &["Alice".into(), DatabaseValue::Int64(1000)],
    )
    .await?;

    db.exec_raw_params(
        "INSERT INTO accounts (name, balance) VALUES (?1, ?2)",
        &["Bob".into(), DatabaseValue::Int64(500)],
    )
    .await?;
    println!("✓ Alice: $1000, Bob: $500\n");

    println!("=== Example 1: Successful Transaction (Commit) ===");
    println!("Transferring $200 from Alice to Bob...");

    let tx = db.begin_transaction().await?;

    tx.exec_raw_params(
        "UPDATE accounts SET balance = balance - ?1 WHERE name = ?2",
        &[DatabaseValue::Int64(200), "Alice".into()],
    )
    .await?;
    println!("  - Deducted $200 from Alice");

    tx.exec_raw_params(
        "UPDATE accounts SET balance = balance + ?1 WHERE name = ?2",
        &[DatabaseValue::Int64(200), "Bob".into()],
    )
    .await?;
    println!("  - Added $200 to Bob");

    tx.commit().await?;
    println!("✓ Transaction committed\n");

    let rows = db
        .query_raw("SELECT name, balance FROM accounts ORDER BY name")
        .await?;
    println!("Current balances:");
    for row in &rows {
        let name_val = row.get("name").unwrap();
        let name = name_val.as_str().unwrap();
        let balance_val = row.get("balance").unwrap();
        let balance = balance_val.as_i64().unwrap();
        println!("  * {name}: ${balance}");
    }
    println!();

    println!("=== Example 2: Failed Transaction (Rollback) ===");
    println!("Attempting to overdraw Alice's account...");

    let tx = db.begin_transaction().await?;

    let alice_balance = {
        let rows = tx
            .query_raw("SELECT balance FROM accounts WHERE name = 'Alice'")
            .await?;
        rows[0].get("balance").unwrap().as_i64().unwrap()
    };
    println!("  - Alice's current balance: ${alice_balance}");

    let withdrawal_amount = 1000;
    println!("  - Attempting to withdraw: ${withdrawal_amount}");

    if alice_balance < withdrawal_amount {
        println!("  ✗ Insufficient funds! Rolling back transaction...");
        tx.rollback().await?;
        println!("✓ Transaction rolled back\n");
    } else {
        tx.exec_raw_params(
            "UPDATE accounts SET balance = balance - ?1 WHERE name = 'Alice'",
            &[DatabaseValue::Int64(withdrawal_amount)],
        )
        .await?;
        tx.commit().await?;
    }

    let rows = db
        .query_raw("SELECT name, balance FROM accounts ORDER BY name")
        .await?;
    println!("Balances after rollback (unchanged):");
    for row in &rows {
        let name_val = row.get("name").unwrap();
        let name = name_val.as_str().unwrap();
        let balance_val = row.get("balance").unwrap();
        let balance = balance_val.as_i64().unwrap();
        println!("  * {name}: ${balance}");
    }
    println!();

    println!("=== Example 3: Nested Transactions (Savepoints) ===");
    println!("Creating a complex transaction with savepoints...");

    let tx = db.begin_transaction().await?;

    tx.exec_raw_params(
        "INSERT INTO accounts (name, balance) VALUES (?1, ?2)",
        &["Charlie".into(), DatabaseValue::Int64(300)],
    )
    .await?;
    println!("  - Added Charlie with $300");

    tx.exec_raw_params(
        "UPDATE accounts SET balance = balance + ?1 WHERE name = ?2",
        &[DatabaseValue::Int64(100), "Charlie".into()],
    )
    .await?;
    println!("  - Bonus: Added $100 to Charlie");

    tx.commit().await?;
    println!("✓ Transaction committed\n");

    let rows = db
        .query_raw("SELECT name, balance FROM accounts ORDER BY name")
        .await?;
    println!("Final balances:");
    for row in &rows {
        let name_val = row.get("name").unwrap();
        let name = name_val.as_str().unwrap();
        let balance_val = row.get("balance").unwrap();
        let balance = balance_val.as_i64().unwrap();
        println!("  * {name}: ${balance}");
    }

    println!("\n✓ All transaction examples completed successfully!");

    Ok(())
}
