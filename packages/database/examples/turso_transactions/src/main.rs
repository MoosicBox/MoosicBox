use switchy_database::{Database, DatabaseValue, turso::TursoDatabase};

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
        println!("  * {}: ${}", name, balance);
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
    println!("  - Alice's current balance: ${}", alice_balance);

    let withdrawal_amount = 1000;
    println!("  - Attempting to withdraw: ${}", withdrawal_amount);

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
        println!("  * {}: ${}", name, balance);
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
        println!("  * {}: ${}", name, balance);
    }

    println!("\n✓ All transaction examples completed successfully!");

    Ok(())
}
