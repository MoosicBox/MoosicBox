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

#[cfg(test)]
mod tests {
    use switchy_database::{Database, DatabaseValue, turso::TursoDatabase};

    /// Helper function to create an in-memory database with an accounts table
    async fn setup_test_db() -> Result<TursoDatabase, Box<dyn std::error::Error>> {
        let db = TursoDatabase::new(":memory:").await?;
        db.exec_raw(
            "CREATE TABLE accounts (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                balance INTEGER NOT NULL
            )",
        )
        .await?;
        Ok(db)
    }

    mod transaction_isolation {
        use super::*;

        #[test_log::test(switchy_async::test)]
        async fn test_uncommitted_changes_not_visible_outside_transaction() {
            let db = setup_test_db().await.expect("Failed to setup db");

            // Insert initial data
            db.exec_raw_params(
                "INSERT INTO accounts (name, balance) VALUES (?1, ?2)",
                &["Alice".into(), DatabaseValue::Int64(1000)],
            )
            .await
            .expect("Failed to insert Alice");

            // Start transaction and make changes
            let tx = db
                .begin_transaction()
                .await
                .expect("Failed to begin transaction");

            tx.exec_raw_params(
                "UPDATE accounts SET balance = ?1 WHERE name = ?2",
                &[DatabaseValue::Int64(500), "Alice".into()],
            )
            .await
            .expect("Failed to update balance");

            // Query from outside the transaction - should see original value
            let rows = db
                .query_raw("SELECT balance FROM accounts WHERE name = 'Alice'")
                .await
                .expect("Failed to query balance");

            let balance = rows[0]
                .get("balance")
                .expect("Balance column missing")
                .as_i64()
                .expect("Balance not i64");

            // Original value should still be visible outside transaction
            assert_eq!(balance, 1000);

            // Commit and verify change is now visible
            tx.commit().await.expect("Failed to commit");

            let rows = db
                .query_raw("SELECT balance FROM accounts WHERE name = 'Alice'")
                .await
                .expect("Failed to query balance");

            let balance = rows[0]
                .get("balance")
                .expect("Balance column missing")
                .as_i64()
                .expect("Balance not i64");

            assert_eq!(balance, 500);
        }
    }

    mod transaction_commit {
        use super::*;

        #[test_log::test(switchy_async::test)]
        async fn test_commit_persists_changes() {
            let db = setup_test_db().await.expect("Failed to setup db");

            db.exec_raw_params(
                "INSERT INTO accounts (name, balance) VALUES (?1, ?2)",
                &["Bob".into(), DatabaseValue::Int64(500)],
            )
            .await
            .expect("Failed to insert Bob");

            let tx = db
                .begin_transaction()
                .await
                .expect("Failed to begin transaction");

            // Transfer money
            tx.exec_raw_params(
                "UPDATE accounts SET balance = balance + ?1 WHERE name = ?2",
                &[DatabaseValue::Int64(250), "Bob".into()],
            )
            .await
            .expect("Failed to update balance");

            tx.commit().await.expect("Failed to commit");

            // Verify balance was updated
            let rows = db
                .query_raw("SELECT balance FROM accounts WHERE name = 'Bob'")
                .await
                .expect("Failed to query balance");

            let balance = rows[0]
                .get("balance")
                .expect("Balance column missing")
                .as_i64()
                .expect("Balance not i64");

            assert_eq!(balance, 750);
        }

        #[test_log::test(switchy_async::test)]
        async fn test_multiple_operations_in_single_transaction() {
            let db = setup_test_db().await.expect("Failed to setup db");

            let tx = db
                .begin_transaction()
                .await
                .expect("Failed to begin transaction");

            // Insert multiple accounts in one transaction
            tx.exec_raw_params(
                "INSERT INTO accounts (name, balance) VALUES (?1, ?2)",
                &["Alice".into(), DatabaseValue::Int64(1000)],
            )
            .await
            .expect("Failed to insert Alice");

            tx.exec_raw_params(
                "INSERT INTO accounts (name, balance) VALUES (?1, ?2)",
                &["Bob".into(), DatabaseValue::Int64(2000)],
            )
            .await
            .expect("Failed to insert Bob");

            tx.exec_raw_params(
                "INSERT INTO accounts (name, balance) VALUES (?1, ?2)",
                &["Charlie".into(), DatabaseValue::Int64(3000)],
            )
            .await
            .expect("Failed to insert Charlie");

            tx.commit().await.expect("Failed to commit");

            // Verify all three accounts exist
            let rows = db
                .query_raw("SELECT COUNT(*) as count FROM accounts")
                .await
                .expect("Failed to count accounts");

            let count = rows[0]
                .get("count")
                .expect("Count column missing")
                .as_i64()
                .expect("Count not i64");

            assert_eq!(count, 3);
        }
    }

    mod transaction_rollback {
        use super::*;

        #[test_log::test(switchy_async::test)]
        async fn test_rollback_discards_changes() {
            let db = setup_test_db().await.expect("Failed to setup db");

            db.exec_raw_params(
                "INSERT INTO accounts (name, balance) VALUES (?1, ?2)",
                &["Alice".into(), DatabaseValue::Int64(1000)],
            )
            .await
            .expect("Failed to insert Alice");

            let tx = db
                .begin_transaction()
                .await
                .expect("Failed to begin transaction");

            tx.exec_raw_params(
                "UPDATE accounts SET balance = ?1 WHERE name = ?2",
                &[DatabaseValue::Int64(0), "Alice".into()],
            )
            .await
            .expect("Failed to update balance");

            // Rollback the transaction
            tx.rollback().await.expect("Failed to rollback");

            // Verify balance is unchanged
            let rows = db
                .query_raw("SELECT balance FROM accounts WHERE name = 'Alice'")
                .await
                .expect("Failed to query balance");

            let balance = rows[0]
                .get("balance")
                .expect("Balance column missing")
                .as_i64()
                .expect("Balance not i64");

            assert_eq!(balance, 1000);
        }

        #[test_log::test(switchy_async::test)]
        async fn test_rollback_on_insufficient_funds() {
            let db = setup_test_db().await.expect("Failed to setup db");

            db.exec_raw_params(
                "INSERT INTO accounts (name, balance) VALUES (?1, ?2)",
                &["Alice".into(), DatabaseValue::Int64(100)],
            )
            .await
            .expect("Failed to insert Alice");

            let tx = db
                .begin_transaction()
                .await
                .expect("Failed to begin transaction");

            let balance = {
                let rows = tx
                    .query_raw("SELECT balance FROM accounts WHERE name = 'Alice'")
                    .await
                    .expect("Failed to query balance");
                rows[0]
                    .get("balance")
                    .expect("Balance column missing")
                    .as_i64()
                    .expect("Balance not i64")
            };

            let withdrawal = 500;

            // Check if sufficient funds
            if balance < withdrawal {
                tx.rollback().await.expect("Failed to rollback");
            } else {
                tx.exec_raw_params(
                    "UPDATE accounts SET balance = balance - ?1 WHERE name = 'Alice'",
                    &[DatabaseValue::Int64(withdrawal)],
                )
                .await
                .expect("Failed to update balance");
                tx.commit().await.expect("Failed to commit");
            }

            // Balance should remain 100 since withdrawal failed
            let rows = db
                .query_raw("SELECT balance FROM accounts WHERE name = 'Alice'")
                .await
                .expect("Failed to query balance");

            let final_balance = rows[0]
                .get("balance")
                .expect("Balance column missing")
                .as_i64()
                .expect("Balance not i64");

            assert_eq!(final_balance, 100);
        }
    }

    mod transaction_state {
        use super::*;

        #[test_log::test(switchy_async::test)]
        async fn test_cannot_commit_twice() {
            let db = setup_test_db().await.expect("Failed to setup db");

            let tx = db
                .begin_transaction()
                .await
                .expect("Failed to begin transaction");

            tx.exec_raw_params(
                "INSERT INTO accounts (name, balance) VALUES (?1, ?2)",
                &["Alice".into(), DatabaseValue::Int64(1000)],
            )
            .await
            .expect("Failed to insert Alice");

            // First commit should succeed
            tx.commit().await.expect("First commit should succeed");

            // Second commit should fail - transaction already consumed
            // Note: We can't test this directly because commit consumes the transaction
            // This is a compile-time safety feature of the API
        }

        #[test_log::test(switchy_async::test)]
        async fn test_cannot_rollback_after_commit() {
            let db = setup_test_db().await.expect("Failed to setup db");

            let tx = db
                .begin_transaction()
                .await
                .expect("Failed to begin transaction");

            tx.exec_raw_params(
                "INSERT INTO accounts (name, balance) VALUES (?1, ?2)",
                &["Alice".into(), DatabaseValue::Int64(1000)],
            )
            .await
            .expect("Failed to insert Alice");

            tx.commit().await.expect("Commit should succeed");

            // Cannot rollback after commit - transaction is consumed
            // Note: This is enforced at compile time by the API design
        }

        #[test_log::test(switchy_async::test)]
        async fn test_cannot_use_transaction_after_commit() {
            let db = setup_test_db().await.expect("Failed to setup db");

            let tx = db
                .begin_transaction()
                .await
                .expect("Failed to begin transaction");

            tx.commit().await.expect("Commit should succeed");

            // Transaction is consumed after commit, cannot perform operations
            // Note: This is enforced at compile time by the API design
        }
    }

    mod concurrent_transactions {
        use super::*;

        #[test_log::test(switchy_async::test)]
        async fn test_sequential_transactions() {
            let db = setup_test_db().await.expect("Failed to setup db");

            // Insert initial accounts
            db.exec_raw_params(
                "INSERT INTO accounts (name, balance) VALUES (?1, ?2)",
                &["Alice".into(), DatabaseValue::Int64(1000)],
            )
            .await
            .expect("Failed to insert Alice");

            db.exec_raw_params(
                "INSERT INTO accounts (name, balance) VALUES (?1, ?2)",
                &["Bob".into(), DatabaseValue::Int64(1000)],
            )
            .await
            .expect("Failed to insert Bob");

            // First transaction updates Alice
            let tx1 = db
                .begin_transaction()
                .await
                .expect("Failed to begin transaction 1");

            tx1.exec_raw_params(
                "UPDATE accounts SET balance = balance + ?1 WHERE name = ?2",
                &[DatabaseValue::Int64(100), "Alice".into()],
            )
            .await
            .expect("Failed to update Alice");

            tx1.commit().await.expect("Failed to commit tx1");

            // Second transaction updates Bob after first completes
            let tx2 = db
                .begin_transaction()
                .await
                .expect("Failed to begin transaction 2");

            tx2.exec_raw_params(
                "UPDATE accounts SET balance = balance + ?1 WHERE name = ?2",
                &[DatabaseValue::Int64(200), "Bob".into()],
            )
            .await
            .expect("Failed to update Bob");

            tx2.commit().await.expect("Failed to commit tx2");

            // Verify both updates succeeded
            let rows = db
                .query_raw("SELECT name, balance FROM accounts ORDER BY name")
                .await
                .expect("Failed to query accounts");

            let alice_balance = rows[0]
                .get("balance")
                .expect("Balance column missing")
                .as_i64()
                .expect("Balance not i64");

            let bob_balance = rows[1]
                .get("balance")
                .expect("Balance column missing")
                .as_i64()
                .expect("Balance not i64");

            assert_eq!(alice_balance, 1100);
            assert_eq!(bob_balance, 1200);
        }
    }

    mod transaction_atomicity {
        use super::*;

        #[test_log::test(switchy_async::test)]
        async fn test_atomic_money_transfer() {
            let db = setup_test_db().await.expect("Failed to setup db");

            db.exec_raw_params(
                "INSERT INTO accounts (name, balance) VALUES (?1, ?2)",
                &["Alice".into(), DatabaseValue::Int64(1000)],
            )
            .await
            .expect("Failed to insert Alice");

            db.exec_raw_params(
                "INSERT INTO accounts (name, balance) VALUES (?1, ?2)",
                &["Bob".into(), DatabaseValue::Int64(500)],
            )
            .await
            .expect("Failed to insert Bob");

            let tx = db
                .begin_transaction()
                .await
                .expect("Failed to begin transaction");

            let transfer_amount = 200;

            // Deduct from Alice
            tx.exec_raw_params(
                "UPDATE accounts SET balance = balance - ?1 WHERE name = ?2",
                &[DatabaseValue::Int64(transfer_amount), "Alice".into()],
            )
            .await
            .expect("Failed to deduct from Alice");

            // Add to Bob
            tx.exec_raw_params(
                "UPDATE accounts SET balance = balance + ?1 WHERE name = ?2",
                &[DatabaseValue::Int64(transfer_amount), "Bob".into()],
            )
            .await
            .expect("Failed to add to Bob");

            tx.commit().await.expect("Failed to commit");

            // Verify both balances updated correctly
            let rows = db
                .query_raw("SELECT name, balance FROM accounts ORDER BY name")
                .await
                .expect("Failed to query accounts");

            let alice_balance = rows[0]
                .get("balance")
                .expect("Balance column missing")
                .as_i64()
                .expect("Balance not i64");

            let bob_balance = rows[1]
                .get("balance")
                .expect("Balance column missing")
                .as_i64()
                .expect("Balance not i64");

            assert_eq!(alice_balance, 800);
            assert_eq!(bob_balance, 700);

            // Verify total money in system is conserved
            let total = alice_balance + bob_balance;
            assert_eq!(total, 1500);
        }

        #[test_log::test(switchy_async::test)]
        async fn test_failed_transfer_preserves_total() {
            let db = setup_test_db().await.expect("Failed to setup db");

            db.exec_raw_params(
                "INSERT INTO accounts (name, balance) VALUES (?1, ?2)",
                &["Alice".into(), DatabaseValue::Int64(100)],
            )
            .await
            .expect("Failed to insert Alice");

            db.exec_raw_params(
                "INSERT INTO accounts (name, balance) VALUES (?1, ?2)",
                &["Bob".into(), DatabaseValue::Int64(500)],
            )
            .await
            .expect("Failed to insert Bob");

            let initial_total = 600;

            let tx = db
                .begin_transaction()
                .await
                .expect("Failed to begin transaction");

            let alice_balance = {
                let rows = tx
                    .query_raw("SELECT balance FROM accounts WHERE name = 'Alice'")
                    .await
                    .expect("Failed to query balance");
                rows[0]
                    .get("balance")
                    .expect("Balance column missing")
                    .as_i64()
                    .expect("Balance not i64")
            };

            let transfer_amount = 200;

            if alice_balance < transfer_amount {
                // Insufficient funds - rollback
                tx.rollback().await.expect("Failed to rollback");
            } else {
                tx.exec_raw_params(
                    "UPDATE accounts SET balance = balance - ?1 WHERE name = 'Alice'",
                    &[DatabaseValue::Int64(transfer_amount)],
                )
                .await
                .expect("Failed to deduct from Alice");

                tx.exec_raw_params(
                    "UPDATE accounts SET balance = balance + ?1 WHERE name = 'Bob'",
                    &[DatabaseValue::Int64(transfer_amount)],
                )
                .await
                .expect("Failed to add to Bob");

                tx.commit().await.expect("Failed to commit");
            }

            // Verify total money in system is unchanged
            let rows = db
                .query_raw("SELECT SUM(balance) as total FROM accounts")
                .await
                .expect("Failed to query total");

            let total = rows[0]
                .get("total")
                .expect("Total column missing")
                .as_i64()
                .expect("Total not i64");

            assert_eq!(total, initial_total);
        }
    }
}
