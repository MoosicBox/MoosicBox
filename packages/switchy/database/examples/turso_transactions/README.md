# Turso Database - Transaction Example

This example demonstrates transaction management with the Turso Database backend, including commit, rollback, and multi-step transaction scenarios.

## What This Example Demonstrates

- **Transaction Basics**: Creating and managing database transactions
- **Successful Commits**: Transferring money between accounts with proper transaction commit
- **Rollback on Failure**: Detecting insufficient funds and rolling back changes
- **Multi-Step Transactions**: Performing multiple operations within a single transaction
- **ACID Properties**: Ensuring atomicity, consistency, isolation, and durability

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

## Use Cases

This pattern is essential for:

- Financial transactions (transfers, payments)
- Multi-table updates that must stay synchronized
- Complex operations requiring rollback on error
- Ensuring data integrity across related changes

## Related Examples

- **[turso_basic](../turso_basic/)**: Basic CRUD operations and schema introspection

## Notes

- This example uses an **in-memory database** (`:memory:`), so data is not persisted
- Turso supports full ACID transaction semantics
- Always handle errors and rollback on failure to maintain data integrity
