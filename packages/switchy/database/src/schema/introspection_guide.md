# Database Introspection: Common Pitfalls and Solutions

This guide covers common issues and platform-specific behavior when using database schema introspection across SQLite, PostgreSQL, MySQL, and DuckDB backends.

## SQLite-Specific Pitfalls

### 1. PRIMARY KEY doesn't imply NOT NULL

**Issue**: Unlike PostgreSQL and MySQL, SQLite PRIMARY KEY columns can contain NULL values.

```sql
-- SQLite allows this:
CREATE TABLE users (id INTEGER PRIMARY KEY);
INSERT INTO users (id) VALUES (NULL); -- Works! Auto-generates rowid
```

**Solution**: Always explicitly specify NOT NULL for primary keys if required:

```sql
CREATE TABLE users (id INTEGER PRIMARY KEY NOT NULL);
```

**Detection**: Check both `is_primary_key` AND `nullable` fields:

```rust
if column.is_primary_key && column.nullable {
    warn!("Primary key column {} allows NULL in SQLite", column.name);
}
```

### 2. Limited Auto-increment Detection

**Issue**: SQLite's `PRAGMA table_info()` doesn't indicate AUTOINCREMENT columns.

```sql
-- These look identical in PRAGMA output:
CREATE TABLE t1 (id INTEGER PRIMARY KEY);         -- Simple rowid alias
CREATE TABLE t2 (id INTEGER PRIMARY KEY AUTOINCREMENT); -- True autoincrement
```

**Current Limitation**: `auto_increment` is always `false` in introspection results.

**Workaround**: Parse original CREATE TABLE statements or use application-level tracking.

### 3. PRAGMA Case Sensitivity

**Issue**: PRAGMA commands are case-sensitive and may fail with wrong case.

```rust
// Wrong - may fail
connection.execute("pragma table_info(users)", [])?;

// Correct
connection.execute("PRAGMA table_info(users)", [])?;
```

**Best Practice**: Always use uppercase PRAGMA commands.

### 4. Attached Databases

**Issue**: `table_exists()` searches ALL attached databases, which may be unexpected.

```sql
ATTACH DATABASE 'other.db' AS other;
-- table_exists('users') will find users in main OR other database
```

**Solution**: Use fully qualified names if precision is needed, or detach unused databases.

### 5. Type Affinity vs Storage Class

**Issue**: SQLite stores any value in any column (dynamic typing), but CREATE TABLE uses type affinity.

```sql
CREATE TABLE test (num INTEGER);
INSERT INTO test (num) VALUES ('hello'); -- Allowed! Stores as TEXT
```

**Introspection Impact**: Type mappings reflect declared type affinity, not actual stored data types.

## PostgreSQL-Specific Pitfalls

### 1. Schema Awareness - Public Schema Only

**Issue**: Current implementation only searches the 'public' schema.

```sql
CREATE SCHEMA myapp;
CREATE TABLE myapp.users (id SERIAL);
-- table_exists('users') returns false - not in public schema!
```

**Workaround**: Ensure all application tables are in 'public' schema, or modify search queries.

**Future Enhancement**: Support schema-qualified table names or configurable schema search.

### 2. Case Sensitivity - Identifier Folding

**Issue**: PostgreSQL folds unquoted identifiers to lowercase.

```sql
-- These create the same table:
CREATE TABLE Users (Name TEXT);
CREATE TABLE users (name TEXT);

-- This creates different table:
CREATE TABLE "Users" ("Name" TEXT);
```

**Best Practice**: Use lowercase table/column names consistently, or always quote identifiers.

### 3. Serial vs Identity Columns

**Issue**: PostgreSQL has two auto-increment mechanisms with different introspection needs.

```sql
-- SERIAL (PostgreSQL extension)
CREATE TABLE t1 (id SERIAL);
-- Creates: id INTEGER DEFAULT nextval('t1_id_seq')

-- IDENTITY (SQL standard)
CREATE TABLE t2 (id INTEGER GENERATED ALWAYS AS IDENTITY);
```

**Current Limitation**: Auto-increment detection not implemented for either mechanism.

**Detection Strategy**: Look for `nextval()` in default values (SERIAL) or query `information_schema.sequences`.

### 4. Complex Default Values

**Issue**: PostgreSQL default expressions can be complex and may not parse correctly.

```sql
CREATE TABLE logs (
    id SERIAL,
    created_at TIMESTAMP DEFAULT now(),
    expires_at TIMESTAMP DEFAULT (now() + interval '1 year')
);
```

**Result**: `expires_at` default will be `None` (unparseable expression).

**Best Practice**: Use simple default values where introspection is important.

### 5. Type Casting in Defaults

**Issue**: PostgreSQL includes type casts in default value strings.

```sql
CREATE TABLE users (active BOOLEAN DEFAULT true);
-- Default appears as: 'true'::boolean
```

**Parsing**: Current parser handles `'value'::type` format, but complex casts may fail.

## MySQL-Specific Pitfalls

### 1. Case Sensitivity Platform Dependence

**Issue**: Table/column name case sensitivity varies by operating system.

```sql
-- Linux: These are DIFFERENT tables
CREATE TABLE Users (id INT);
CREATE TABLE users (id INT);

-- Windows/macOS: These are the SAME table (second fails)
```

**Best Practice**: Always use lowercase table/column names for portability.

**Configuration**: Check `lower_case_table_names` system variable:

- `0` = Case-sensitive (Linux default)
- `1` = Stored lowercase, comparisons case-insensitive (Windows)
- `2` = Stored as-is, comparisons lowercase (macOS)

### 2. Storage Engine Foreign Key Support

**Issue**: Foreign key introspection only meaningful for InnoDB tables.

```sql
-- MyISAM ignores foreign key constraints
CREATE TABLE posts (
    id INT PRIMARY KEY,
    user_id INT,
    FOREIGN KEY (user_id) REFERENCES users(id)  -- Ignored in MyISAM!
) ENGINE=MyISAM;
```

**Detection**: Check table's storage engine before relying on foreign key information.

**Best Practice**: Use InnoDB (default in MySQL 5.7+) for referential integrity.

### 3. TINYINT(1) vs BOOLEAN

**Issue**: MySQL BOOLEAN is alias for TINYINT(1), but introspection sees TINYINT.

```sql
CREATE TABLE flags (active BOOLEAN);
-- information_schema.columns shows DATA_TYPE = 'tinyint'
```

**Current Behavior**: Maps to `Bool` based on DATA_TYPE = 'tinyint', but could be regular tiny integer.

**Limitation**: Cannot distinguish BOOLEAN from TINYINT(1) in introspection.

### 4. Character Set Length Calculations

**Issue**: `CHARACTER_MAXIMUM_LENGTH` reflects characters, not bytes.

```sql
CREATE TABLE test (name VARCHAR(10) CHARACTER SET utf8mb4);
-- Can store 10 characters, each using 1-4 bytes (up to 40 bytes total)
```

**Impact**: Length limits in DataType::VarChar may not reflect actual byte storage limits.

### 5. Generated Columns (MySQL 5.7+)

**Issue**: Generated/computed columns appear as regular columns in introspection.

```sql
CREATE TABLE products (
    price DECIMAL(10,2),
    tax_rate DECIMAL(3,2),
    total DECIMAL(10,2) GENERATED ALWAYS AS (price * (1 + tax_rate))
);
```

**Result**: `total` appears as regular DECIMAL column with complex default expression.

**Limitation**: Generated column expressions not parsed or indicated in metadata.

## DuckDB-Specific Pitfalls

### 1. `GENERATED ALWAYS AS IDENTITY` Not Supported

**Issue**: DuckDB v1.4.4 throws `"Constraint not implemented!"` for the SQL standard `GENERATED ALWAYS AS IDENTITY` syntax.

```sql
-- This fails in DuckDB:
CREATE TABLE users (id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY);
```

**Solution**: Use sequence-based auto-increment instead:

```sql
CREATE SEQUENCE users_id_seq;
CREATE TABLE users (id BIGINT DEFAULT nextval('users_id_seq') PRIMARY KEY);
```

**Introspection Impact**: The schema builder generates the sequence-based approach automatically when `auto_increment: true` is set. Associated sequences are named `{table_name}_{column_name}_seq` and are dropped when the table is dropped.

### 2. `NOW()` Returns `TIMESTAMP WITH TIME ZONE`

**Issue**: DuckDB's `NOW()` returns `TIMESTAMPTZ`, but `+(TIMESTAMPTZ, INTERVAL)` is not a valid operation.

```sql
-- This fails:
SELECT NOW() + INTERVAL '1 day';
-- Error: No function matches +(TIMESTAMP WITH TIME ZONE, INTERVAL)

-- This works:
SELECT NOW()::TIMESTAMP + INTERVAL '1 day';
```

**Current Behavior**: The DuckDB backend automatically casts `NOW()` to `TIMESTAMP` in all generated SQL, including `DatabaseValue::Now` and `DatabaseValue::NowPlus`.

### 3. No Savepoint Support

**Issue**: DuckDB's SQL parser does not recognize the `SAVEPOINT` statement.

```sql
-- This fails in DuckDB:
SAVEPOINT my_savepoint;
```

**Current Behavior**: `savepoint()` returns `DatabaseError::UnsupportedOperation`. Use full transactions instead of nested savepoints.

### 4. `DELETE ... RETURNING` Workaround

**Issue**: DuckDB's `raw_execute()` consumes `RETURNING` clause results, making them unavailable via subsequent `raw_query()` calls. This means the standard `DELETE FROM ... RETURNING *` pattern returns 0 rows.

**Current Behavior**: The backend uses a SELECT-then-DELETE approach: it first SELECTs the matching rows, then executes the DELETE. This is safe because each DuckDB connection is behind an `Arc<Mutex<>>`, preventing concurrent modifications on the same connection. For file-backed DuckDB with multiple connections, callers should wrap deletes in a transaction.

### 5. In-Memory Databases Are Not Shared

**Issue**: Unlike SQLite's `cache=shared` mode, DuckDB in-memory databases are isolated per connection.

```rust
// These two connections see completely different databases:
let conn1 = Connection::open_in_memory()?;
let conn2 = Connection::open_in_memory()?;
```

**Impact**: The connection pool created by `init_duckdb(None)` has 5 independent in-memory databases. For in-memory usage, only one connection in the pool will contain your data unless you coordinate explicitly.

**Workaround**: For tests, use a single shared connection wrapped in `Arc<Mutex<>>`. For production, use file-backed databases.

### 6. Type Mapping Differences

**Issue**: DuckDB reports types differently than other backends in `information_schema.columns`.

| DuckDB Type | Mapped `DataType` | Notes |
| --- | --- | --- |
| `INTEGER` | `Int` | 32-bit (not 64-bit like some expect) |
| `BIGINT` | `BigInt` | 64-bit |
| `VARCHAR` | `VarChar(n)` | Includes length |
| `BOOLEAN` | `Bool` | Native boolean type |
| `HUGEINT` | Lossy → `String` | 128-bit integer, no direct mapping |
| `LIST`, `STRUCT`, `MAP` | Lossy → `Null` | Complex types logged as warnings |
| `INTERVAL` | Lossy → `String` | Converted to human-readable string |

**Lossy Conversions**: DuckDB types without a direct `DatabaseValue` mapping are converted with a warning log. `HUGEINT` and `UHUGEINT` become strings, `LIST`/`STRUCT`/`MAP`/`UNION` become `Null`.

### 7. Index Introspection

**Issue**: DuckDB uses `duckdb_indexes()` for index metadata, which returns index SQL rather than structured column lists.

**Current Behavior**: Index columns are extracted by parsing the SQL string from the `sql` column of `duckdb_indexes()`. This works for simple indexes but may not correctly parse complex index expressions.

## Cross-Backend Pitfalls

### 1. Data Type Mapping Inconsistencies

**Issue**: Same DataType enum maps to different native types across backends.

| DataType | SQLite             | PostgreSQL              | MySQL         | DuckDB                   |
| -------- | ------------------ | ----------------------- | ------------- | ------------------------ |
| `Real`   | 64-bit REAL        | 32-bit REAL             | 32-bit FLOAT  | 32-bit FLOAT             |
| `Double` | N/A (maps to Real) | 64-bit DOUBLE PRECISION | 64-bit DOUBLE | 64-bit DOUBLE            |

**Solution**: Be aware of precision differences when migrating between backends.

### 2. NULL vs Empty String Defaults

**Issue**: Backends handle empty string defaults differently.

```sql
-- PostgreSQL
CREATE TABLE test (note TEXT DEFAULT '');
-- Default: DatabaseValue::String("")

-- SQLite
CREATE TABLE test (note TEXT DEFAULT '');
-- Default: DatabaseValue::String("")

-- MySQL
CREATE TABLE test (note TEXT DEFAULT '');
-- Default: DatabaseValue::String("")
```

**Generally Consistent**: All backends handle this similarly, but watch for edge cases.

### 3. Auto-increment Behavior Differences

**Issue**: Auto-increment implementation varies significantly.

- **SQLite**: INTEGER PRIMARY KEY becomes alias for rowid
- **PostgreSQL**: SERIAL creates sequence + DEFAULT nextval()
- **MySQL**: AUTO_INCREMENT column attribute
- **DuckDB**: Sequence-based (`CREATE SEQUENCE` + `DEFAULT nextval(...)`)

**Current Status**: Auto-increment detection not reliably implemented across backends.

### 4. Timestamp/DateTime Handling

**Issue**: Date/time types and timezone handling differ.

- **SQLite**: No native date types, stores as TEXT/INTEGER/REAL
- **PostgreSQL**: Rich temporal types with timezone support
- **MySQL**: Separate DATE, TIME, DATETIME, TIMESTAMP types
- **DuckDB**: `TIMESTAMP` (without timezone) and `TIMESTAMPTZ`; `NOW()` returns `TIMESTAMPTZ` and must be cast to `TIMESTAMP` for interval arithmetic

**Mapping**: All map to `DataType::DateTime`, losing timezone and precision information.

## Best Practices for Robust Introspection

### 1. Defensive Coding

```rust
// Always check table exists before introspecting
if !db.table_exists("users").await? {
    return Err("Table 'users' not found".into());
}

// Handle missing columns gracefully
if !db.column_exists("users", "email").await? {
    // Add column or use alternative logic
}

// Validate expected schema
let columns = db.get_table_columns("users").await?;
let id_col = columns.iter().find(|c| c.name == "id")
    .ok_or("Missing required 'id' column")?;

if !id_col.is_primary_key {
    warn!("Expected 'id' to be primary key");
}
```

### 2. Backend-Agnostic Schema Design

```rust
// Use compatible data types
Column {
    name: "id".to_string(),
    data_type: DataType::BigInt,  // Works on all backends
    nullable: false,              // Explicit NOT NULL
    is_primary_key: true,
    auto_increment: true,         // May not be detected, but hint for creation
    default: None,
}

// Avoid backend-specific features in portable code
// - Don't rely on auto-increment detection
// - Use simple default values
// - Stick to common data types
```

### 3. Error Handling

```rust
match db.get_table_info("users").await {
    Ok(Some(table_info)) => {
        // Process table info
    }
    Ok(None) => {
        // Table doesn't exist - handle gracefully
    }
    Err(DatabaseError::UnsupportedDataType(type_name)) => {
        warn!("Unsupported data type '{}' encountered", type_name);
        // Continue with limited info or skip column
    }
    Err(e) => {
        error!("Introspection failed: {}", e);
        return Err(e);
    }
}
```

### 4. Testing Across Backends

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // Test introspection behavior on each backend
    #[tokio::test]
    async fn test_table_introspection_sqlite() {
        let db = create_sqlite_test_db().await;
        test_introspection_behavior(&db).await;
    }

    #[tokio::test]
    async fn test_table_introspection_postgres() {
        let db = create_postgres_test_db().await;
        test_introspection_behavior(&db).await;
    }

    async fn test_introspection_behavior(db: &dyn Database) {
        // Shared test logic that should work on all backends
        assert!(db.table_exists("test_table").await.unwrap());

        let columns = db.get_table_columns("test_table").await.unwrap();
        assert!(!columns.is_empty());

        // Test backend-agnostic expectations
        let id_col = columns.iter().find(|c| c.name == "id").unwrap();
        assert!(id_col.is_primary_key);
        assert!(!id_col.nullable);
    }
}
```

This guide should help avoid common pitfalls and write robust code that works reliably across different database backends. For DuckDB-specific concerns, pay particular attention to the lossy type conversions, the sequence-based auto-increment, and the `TIMESTAMPTZ` vs `TIMESTAMP` distinction.
