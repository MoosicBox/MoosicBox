# Turso Database - Transaction Example

This example demonstrates transaction management with the Turso Database backend, including commit, rollback, and multi-step transaction scenarios.

## Summary

Learn how to use database transactions to ensure data integrity and consistency across multiple operations, with proper error handling and rollback capabilities.

## What This Example Demonstrates

- **Transaction Basics**: Creating and managing database transactions
- **Successful Commits**: Transferring money between accounts with proper transaction commit
- **Rollback on Failure**: Detecting insufficient funds and rolling back changes
- **Multi-Step Transactions**: Performing multiple operations within a single transaction
- **ACID Properties**: Ensuring atomicity, consistency, isolation, and durability

## Prerequisites

- Understanding of database transactions and ACID properties
- Familiarity with Rust async/await and error handling
- Knowledge of basic SQL operations

## Running the Example

From the repository root:

```bash
cargo run -p turso_transactions_example
```

Or from this directory:

```bash
cargo run
```

## Expected Output

### Example 1: Successful Transaction (Commit)

Transfers $200 from Alice to Bob:

```
Initial: Alice: $1000, Bob: $500
Final:   Alice: $800,  Bob: $700
```

The transaction ensures both the deduction and addition happen atomically.

### Example 2: Failed Transaction (Rollback)

Attempts to withdraw $1000 from Alice's account (which only has $800):

```
Check balance: $800
Attempt withdrawal: $1000
Result: Insufficient funds - ROLLBACK
Final: Alice: $800 (unchanged)
```

The transaction is rolled back, leaving the database in its original state.

### Example 3: Multi-Step Transaction

Performs multiple operations (insert and update) within a single transaction:

```
Add Charlie: $300
Add bonus: +$100
Final: Charlie: $400
```

All operations commit together as a single atomic unit.

## Code Highlights

### Creating a Transaction

```rust
let tx = db.begin_transaction().await?;
```

### Executing Operations in Transaction

```rust
tx.exec_raw_params(
    "UPDATE accounts SET balance = balance - ?1 WHERE name = ?2",
    &[DatabaseValue::Int64(200), "Alice".into()],
)
.await?;
```

### Committing Changes

```rust
tx.commit().await?;
```

### Rolling Back Changes

```rust
if balance < withdrawal_amount {
    tx.rollback().await?;
}
```

### Querying Within Transaction

```rust
let rows = tx.query_raw("SELECT balance FROM accounts WHERE name = 'Alice'").await?;
let balance = rows[0].get("balance").unwrap().as_i64().unwrap();
```

## Transaction Guarantees

- **Atomicity**: All operations in a transaction succeed or fail together
- **Consistency**: Database constraints are maintained
- **Isolation**: Transactions don't see each other's uncommitted changes
- **Durability**: Committed changes are persisted

## Key Concepts

### ACID Transactions

Transactions provide four critical guarantees (ACID):

1. **Atomicity**: All operations succeed or all fail - no partial commits
2. **Consistency**: Database moves from one valid state to another
3. **Isolation**: Concurrent transactions don't interfere with each other
4. **Durability**: Once committed, changes survive crashes/power loss

### Transaction Lifecycle

```rust
let tx = db.begin_transaction().await?;  // Start
// ... perform operations ...
tx.commit().await?;                       // End (success)
// OR
tx.rollback().await?;                     // End (failure)
```

### Error Handling Pattern

Always handle transaction failures properly:

```rust
let tx = db.begin_transaction().await?;

match risky_operation(&tx).await {
    Ok(_) => {
        tx.commit().await?;
        println!("Success!");
    }
    Err(e) => {
        tx.rollback().await?;
        eprintln!("Failed: {}", e);
    }
}
```

## Testing the Example

Run the example and verify:

1. **Example 1**: Money transfer succeeds, balances update correctly
2. **Example 2**: Insufficient funds triggers rollback, balance unchanged
3. **Example 3**: Multi-step transaction inserts and updates atomically

Check the output matches expected balances at each step.

## Troubleshooting

### Error: "Transaction already committed/rolled back"

You tried to use a transaction after calling `commit()` or `rollback()`. Transactions can only be used once.

### Error: "Already in transaction"

You called `begin_transaction()` on a transaction object. Nested transactions are not supported (use savepoints instead for sub-transactions).

### Deadlocks or lock timeouts

For file-based databases, ensure proper transaction ordering and avoid long-running transactions that hold locks.

## Use Cases

This pattern is essential for:

- Financial transactions (transfers, payments)
- Multi-table updates that must stay synchronized
- Complex operations requiring rollback on error
- Ensuring data integrity across related changes

## Related Examples

- **[turso_basic](../turso_basic/)**: Basic CRUD operations and schema introspection
- **[query_builder](../query_builder/)**: Type-safe query builder API

## Notes

- This example uses an **in-memory database** (`:memory:`), so data is not persisted
- Turso supports full ACID transaction semantics
- Always handle errors and rollback on failure to maintain data integrity
