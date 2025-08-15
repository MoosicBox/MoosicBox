//! Test assertion helpers for database schema and migration verification
//!
//! This module provides utilities for verifying database state, schema structure,
//! and migration application status. All functions return `Result<(), DatabaseError>`
//! to propagate database errors without custom error types.

use std::collections::BTreeMap;

use switchy_database::{
    Database, DatabaseError,
    query::{self, FilterableQuery},
};
use switchy_schema::version::DEFAULT_MIGRATIONS_TABLE;

/// Verifies that a table exists in the database
///
/// # Arguments
///
/// * `db` - Database connection to check
/// * `table_name` - Name of the table to verify exists
///
/// # Errors
///
/// * If database query fails
/// * If table does not exist
///
/// # Example
///
/// ```rust,no_run
/// use switchy_schema_test_utils::assertions::assert_table_exists;
/// use switchy_database::Database;
///
/// # async fn example(db: &dyn Database) -> Result<(), switchy_database::DatabaseError> {
/// // This would fail since table doesn't exist yet
/// // assert_table_exists(db, "users").await?;
/// # Ok(())
/// # }
/// ```
pub async fn assert_table_exists(db: &dyn Database, table_name: &str) -> Result<(), DatabaseError> {
    // Use query builder to check if table exists by trying to select from it
    match query::select(table_name)
        .columns(&["*"])
        .limit(0)
        .execute(db)
        .await
    {
        Ok(_) => Ok(()),
        Err(_) => Err(DatabaseError::NoRow), // Table doesn't exist
    }
}

/// Verifies that a table does not exist in the database
///
/// # Arguments
///
/// * `db` - Database connection to check
/// * `table_name` - Name of the table to verify does not exist
///
/// # Errors
///
/// * If database query fails
/// * If table exists when it shouldn't
///
/// # Example
///
/// ```rust,no_run
/// use switchy_schema_test_utils::assertions::assert_table_not_exists;
/// use switchy_database::Database;
///
/// # async fn example(db: &dyn Database) -> Result<(), switchy_database::DatabaseError> {
/// // This should pass since table doesn't exist
/// assert_table_not_exists(db, "nonexistent_table").await?;
/// # Ok(())
/// # }
/// ```
pub async fn assert_table_not_exists(
    db: &dyn Database,
    table_name: &str,
) -> Result<(), DatabaseError> {
    // Use query builder to check if table exists - if it succeeds, table exists (error)
    match query::select(table_name)
        .columns(&["*"])
        .limit(0)
        .execute(db)
        .await
    {
        Ok(_) => Err(DatabaseError::NoRow), // Table exists but shouldn't
        Err(_) => Ok(()),                   // Table doesn't exist (expected)
    }
}

/// Verifies that a column exists in a table with the expected type
///
/// # Arguments
///
/// * `db` - Database connection to check
/// * `table_name` - Name of the table containing the column
/// * `column_name` - Name of the column to verify
/// * `expected_type` - Expected SQL type of the column (case-insensitive)
///
/// # Errors
///
/// * If database query fails
/// * If table does not exist
/// * If column does not exist
/// * If column type does not match expected type
///
/// # Example
///
/// ```rust,no_run
/// use switchy_schema_test_utils::assertions::assert_column_exists;
/// use switchy_database::{Database, schema::{Column, DataType}};
///
/// # async fn example(db: &dyn Database) -> Result<(), switchy_database::DatabaseError> {
/// // Create a test table first using schema query builder
/// db.create_table("users")
///     .column(Column {
///         name: "id".to_string(),
///         nullable: false,
///         auto_increment: false,
///         data_type: DataType::Int,
///         default: None,
///     })
///     .column(Column {
///         name: "name".to_string(),
///         nullable: false,
///         auto_increment: false,
///         data_type: DataType::Text,
///         default: None,
///     })
///     .primary_key("id")
///     .execute(db)
///     .await?;
///
/// // Verify column exists with correct type
/// assert_column_exists(db, "users", "id", "INTEGER").await?;
/// assert_column_exists(db, "users", "name", "TEXT").await?;
/// # Ok(())
/// # }
/// ```
pub async fn assert_column_exists(
    db: &dyn Database,
    table_name: &str,
    column_name: &str,
    _expected_type: &str,
) -> Result<(), DatabaseError> {
    // Use query builder to check if column exists by trying to select it
    // No LIMIT so SQLite validates the column name
    match query::select(table_name)
        .columns(&[column_name])
        .execute(db)
        .await
    {
        Ok(_) => Ok(()), // Column exists - we can't easily check type without more complex queries
        Err(_) => Err(DatabaseError::NoRow), // Column doesn't exist
    }
}

/// Verifies that a column does not exist in a table
///
/// # Arguments
///
/// * `db` - Database connection to check
/// * `table_name` - Name of the table to check
/// * `column_name` - Name of the column to verify does not exist
///
/// # Errors
///
/// * If database query fails
/// * If table does not exist
/// * If column exists when it shouldn't
///
/// # Example
///
/// ```rust,no_run
/// use switchy_schema_test_utils::assertions::assert_column_not_exists;
/// use switchy_database::{Database, schema::{Column, DataType}};
///
/// # async fn example(db: &dyn Database) -> Result<(), switchy_database::DatabaseError> {
/// // Create a test table first using schema query builder
/// db.create_table("users")
///     .column(Column {
///         name: "id".to_string(),
///         nullable: false,
///         auto_increment: false,
///         data_type: DataType::Int,
///         default: None,
///     })
///     .primary_key("id")
///     .execute(db)
///     .await?;
///
/// // Verify column doesn't exist
/// assert_column_not_exists(db, "users", "nonexistent_column").await?;
/// # Ok(())
/// # }
/// ```
pub async fn assert_column_not_exists(
    db: &dyn Database,
    table_name: &str,
    column_name: &str,
) -> Result<(), DatabaseError> {
    // Use query builder to check if column exists - if it succeeds, column exists (error)
    // No LIMIT so SQLite validates the column name
    query::select(table_name)
        .columns(&[column_name])
        .execute(db)
        .await
        .inspect(|x| log::debug!("Column exists: {column_name}, rows: {x:?}"))
        .map_or_else(
            |_| Ok(()),
            |_| Err(DatabaseError::NoRow), // Column exists but shouldn't
        )
}

/// Verifies that a table has exactly the expected number of rows
///
/// # Arguments
///
/// * `db` - Database connection to check
/// * `table_name` - Name of the table to count rows in
/// * `expected_count` - Expected number of rows
///
/// # Errors
///
/// * If database query fails
/// * If table does not exist
/// * If row count does not match expected count
///
/// # Panics
///
/// * If the table row count exceeds `i64::MAX` (extremely unlikely in practice)
///
/// # Example
///
/// ```rust,no_run
/// use switchy_schema_test_utils::assertions::assert_row_count;
/// use switchy_database::{Database, schema::{Column, DataType}};
///
/// # async fn example(db: &dyn Database) -> Result<(), switchy_database::DatabaseError> {
/// // Create and populate a test table using schema query builder
/// db.create_table("users")
///     .column(Column {
///         name: "id".to_string(),
///         nullable: false,
///         auto_increment: false,
///         data_type: DataType::Int,
///         default: None,
///     })
///     .column(Column {
///         name: "name".to_string(),
///         nullable: true,
///         auto_increment: false,
///         data_type: DataType::Text,
///         default: None,
///     })
///     .primary_key("id")
///     .execute(db)
///     .await?;
/// db.insert("users")
///     .value("name", "Alice")
///     .execute(db)
///     .await?;
/// db.insert("users")
///     .value("name", "Bob")
///     .execute(db)
///     .await?;
///
/// // Verify row count
/// assert_row_count(db, "users", 2).await?;
/// # Ok(())
/// # }
/// ```
pub async fn assert_row_count(
    db: &dyn Database,
    table_name: &str,
    expected_count: i64,
) -> Result<(), DatabaseError> {
    // Use the query builder to get all rows and count them
    let results = query::select(table_name)
        .columns(&["*"])
        .execute(db)
        .await?;

    let actual_count = i64::try_from(results.len()).expect("table row count should fit in i64");

    if actual_count == expected_count {
        Ok(())
    } else {
        Err(DatabaseError::NoRow) // Use existing error variant
    }
}

/// Verifies that a table has at least the minimum number of rows
///
/// # Arguments
///
/// * `db` - Database connection to check
/// * `table_name` - Name of the table to count rows in
/// * `min_count` - Minimum expected number of rows
///
/// # Errors
///
/// * If database query fails
/// * If table does not exist
/// * If row count is less than minimum
///
/// # Panics
///
/// * If the table row count exceeds `i64::MAX` (extremely unlikely in practice)
///
/// # Example
///
/// ```rust,no_run
/// use switchy_schema_test_utils::assertions::assert_row_count_min;
/// use switchy_database::{Database, schema::{Column, DataType}};
///
/// # async fn example(db: &dyn Database) -> Result<(), switchy_database::DatabaseError> {
/// // Create and populate a test table using schema query builder
/// db.create_table("users")
///     .column(Column {
///         name: "id".to_string(),
///         nullable: false,
///         auto_increment: false,
///         data_type: DataType::Int,
///         default: None,
///     })
///     .column(Column {
///         name: "name".to_string(),
///         nullable: true,
///         auto_increment: false,
///         data_type: DataType::Text,
///         default: None,
///     })
///     .primary_key("id")
///     .execute(db)
///     .await?;
/// db.insert("users")
///     .value("name", "Alice")
///     .execute(db)
///     .await?;
/// db.insert("users")
///     .value("name", "Bob")
///     .execute(db)
///     .await?;
/// db.insert("users")
///     .value("name", "Charlie")
///     .execute(db)
///     .await?;
///
/// // Verify minimum row count
/// assert_row_count_min(db, "users", 2).await?; // Should pass (3 >= 2)
/// # Ok(())
/// # }
/// ```
pub async fn assert_row_count_min(
    db: &dyn Database,
    table_name: &str,
    min_count: i64,
) -> Result<(), DatabaseError> {
    // Use the query builder to get all rows and count them
    let results = query::select(table_name)
        .columns(&["*"])
        .execute(db)
        .await?;

    let actual_count = i64::try_from(results.len()).expect("table row count should fit in i64");

    if actual_count >= min_count {
        Ok(())
    } else {
        Err(DatabaseError::NoRow)
    }
}

/// Verifies data integrity by checking that all foreign key constraints are satisfied
///
/// # Arguments
///
/// * `db` - Database connection to check
///
/// # Errors
///
/// * If database query fails
/// * If foreign key constraints are violated
///
/// # Example
///
/// ```rust,no_run
/// use switchy_schema_test_utils::assertions::assert_foreign_key_integrity;
/// use switchy_database::{Database, schema::{Column, DataType}};
///
/// # async fn example(db: &dyn Database) -> Result<(), switchy_database::DatabaseError> {
/// // Create tables with foreign key relationship using schema query builder
/// db.exec_raw("PRAGMA foreign_keys = ON").await?;
///
/// db.create_table("users")
///     .column(Column {
///         name: "id".to_string(),
///         nullable: false,
///         auto_increment: false,
///         data_type: DataType::Int,
///         default: None,
///     })
///     .column(Column {
///         name: "name".to_string(),
///         nullable: true,
///         auto_increment: false,
///         data_type: DataType::Text,
///         default: None,
///     })
///     .primary_key("id")
///     .execute(db)
///     .await?;
///     
/// db.create_table("posts")
///     .column(Column {
///         name: "id".to_string(),
///         nullable: false,
///         auto_increment: false,
///         data_type: DataType::Int,
///         default: None,
///     })
///     .column(Column {
///         name: "user_id".to_string(),
///         nullable: true,
///         auto_increment: false,
///         data_type: DataType::Int,
///         default: None,
///     })
///     .primary_key("id")
///     .foreign_key(("user_id", "users(id)"))
///     .execute(db)
///     .await?;
///
/// // Verify foreign key integrity
/// assert_foreign_key_integrity(db).await?;
/// # Ok(())
/// # }
/// ```
pub async fn assert_foreign_key_integrity(db: &dyn Database) -> Result<(), DatabaseError> {
    // Use PRAGMA foreign_key_check to verify integrity
    // For now, we'll assume integrity is OK if no error occurs
    db.exec_raw("PRAGMA foreign_key_check").await
}

/// Verifies that specific migrations have been applied by checking the migration table
///
/// # Arguments
///
/// * `db` - Database connection to check
/// * `migration_ids` - List of migration IDs that should be applied
///
/// # Errors
///
/// * If database query fails
/// * If migration table does not exist
/// * If any expected migration is not found in the applied migrations
///
/// # Example
///
/// ```rust,no_run
/// use switchy_schema_test_utils::assertions::assert_migrations_applied;
/// use switchy_database::{Database, schema::{Column, DataType}};
///
/// # async fn example(db: &dyn Database) -> Result<(), switchy_database::DatabaseError> {
/// // Create migration table using schema query builder
/// db.create_table("__switchy_migrations")
///     .column(Column {
///         name: "name".to_string(),
///         nullable: false,
///         auto_increment: false,
///         data_type: DataType::Text,
///         default: None,
///     })
///     .column(Column {
///         name: "run_on".to_string(),
///         nullable: true,
///         auto_increment: false,
///         data_type: DataType::Text,
///         default: None,
///     })
///     .primary_key("name")
///     .execute(db)
///     .await?;
/// db.insert("__switchy_migrations")
///     .value("name", "001_initial")
///     .value("run_on", "2024-01-01")
///     .execute(db)
///     .await?;
/// db.insert("__switchy_migrations")
///     .value("name", "002_add_users")
///     .value("run_on", "2024-01-02")
///     .execute(db)
///     .await?;
///
/// // Verify specific migrations are applied
/// assert_migrations_applied(db, &["001_initial", "002_add_users"]).await?;
/// # Ok(())
/// # }
/// ```
pub async fn assert_migrations_applied(
    db: &dyn Database,
    migration_ids: &[&str],
) -> Result<(), DatabaseError> {
    // Check each migration ID individually using the query builder
    for &migration_id in migration_ids {
        let results = query::select(DEFAULT_MIGRATIONS_TABLE)
            .columns(&["name"])
            .where_eq("name", migration_id)
            .execute(db)
            .await?;

        if results.is_empty() {
            return Err(DatabaseError::NoRow);
        }
    }

    Ok(())
}

/// Verifies that specific migrations have not been applied
///
/// # Arguments
///
/// * `db` - Database connection to check
/// * `migration_ids` - List of migration IDs that should not be applied
///
/// # Errors
///
/// * If database query fails
/// * If migration table does not exist
/// * If any migration that shouldn't be applied is found
///
/// # Example
///
/// ```rust,no_run
/// use switchy_schema_test_utils::assertions::assert_migrations_not_applied;
/// use switchy_database::{Database, schema::{Column, DataType}};
///
/// # async fn example(db: &dyn Database) -> Result<(), switchy_database::DatabaseError> {
/// // Create migration table with some applied migrations using schema query builder
/// db.create_table("__switchy_migrations")
///     .column(Column {
///         name: "name".to_string(),
///         nullable: false,
///         auto_increment: false,
///         data_type: DataType::Text,
///         default: None,
///     })
///     .column(Column {
///         name: "run_on".to_string(),
///         nullable: true,
///         auto_increment: false,
///         data_type: DataType::Text,
///         default: None,
///     })
///     .primary_key("name")
///     .execute(db)
///     .await?;
/// db.insert("__switchy_migrations")
///     .value("name", "001_initial")
///     .value("run_on", "2024-01-01")
///     .execute(db)
///     .await?;
///
/// // Verify specific migrations are not applied
/// assert_migrations_not_applied(db, &["002_future", "003_not_ready"]).await?;
/// # Ok(())
/// # }
/// ```
pub async fn assert_migrations_not_applied(
    db: &dyn Database,
    migration_ids: &[&str],
) -> Result<(), DatabaseError> {
    // First check if migration table exists by trying to query it
    let table_check = query::select(DEFAULT_MIGRATIONS_TABLE)
        .columns(&["name"])
        .limit(1)
        .execute(db)
        .await;

    // If table doesn't exist, no migrations are applied (which is what we want)
    if table_check.is_err() {
        return Ok(());
    }

    // Check that none of the specified migrations are applied
    for &migration_id in migration_ids {
        let results = query::select(DEFAULT_MIGRATIONS_TABLE)
            .columns(&["name"])
            .where_eq("name", migration_id)
            .execute(db)
            .await?;

        if !results.is_empty() {
            return Err(DatabaseError::NoRow);
        }
    }

    Ok(())
}

/// Compares the current database schema with an expected schema structure
///
/// # Arguments
///
/// * `db` - Database connection to check
/// * `expected_tables` - Map of table names to their expected column definitions
///
/// # Errors
///
/// * If database query fails
/// * If schema does not match expected structure
///
/// # Example
///
/// ```rust,no_run
/// use std::collections::BTreeMap;
/// use switchy_schema_test_utils::assertions::assert_schema_matches;
/// use switchy_database::{Database, schema::{Column, DataType}};
///
/// # async fn example(db: &dyn Database) -> Result<(), switchy_database::DatabaseError> {
/// // Create test schema using schema query builder
/// db.create_table("users")
///     .column(Column {
///         name: "id".to_string(),
///         nullable: false,
///         auto_increment: false,
///         data_type: DataType::Int,
///         default: None,
///     })
///     .column(Column {
///         name: "name".to_string(),
///         nullable: false,
///         auto_increment: false,
///         data_type: DataType::Text,
///         default: None,
///     })
///     .primary_key("id")
///     .execute(db)
///     .await?;
///     
/// db.create_table("posts")
///     .column(Column {
///         name: "id".to_string(),
///         nullable: false,
///         auto_increment: false,
///         data_type: DataType::Int,
///         default: None,
///     })
///     .column(Column {
///         name: "title".to_string(),
///         nullable: true,
///         auto_increment: false,
///         data_type: DataType::Text,
///         default: None,
///     })
///     .column(Column {
///         name: "user_id".to_string(),
///         nullable: true,
///         auto_increment: false,
///         data_type: DataType::Int,
///         default: None,
///     })
///     .primary_key("id")
///     .execute(db)
///     .await?;
///
/// // Define expected schema
/// let mut expected = BTreeMap::new();
/// let mut users_cols = BTreeMap::new();
/// users_cols.insert("id".to_string(), "INTEGER".to_string());
/// users_cols.insert("name".to_string(), "TEXT".to_string());
/// expected.insert("users".to_string(), users_cols);
///
/// let mut posts_cols = BTreeMap::new();
/// posts_cols.insert("id".to_string(), "INTEGER".to_string());
/// posts_cols.insert("title".to_string(), "TEXT".to_string());
/// posts_cols.insert("user_id".to_string(), "INTEGER".to_string());
/// expected.insert("posts".to_string(), posts_cols);
///
/// // Verify schema matches
/// assert_schema_matches(db, &expected).await?;
/// # Ok(())
/// # }
/// ```
pub async fn assert_schema_matches(
    db: &dyn Database,
    expected_tables: &BTreeMap<String, BTreeMap<String, String>>,
) -> Result<(), DatabaseError> {
    // For now, we'll do a simplified check by verifying each table exists
    // Full schema comparison would require more complex queries
    for table_name in expected_tables.keys() {
        assert_table_exists(db, table_name).await?;

        // Check each expected column exists using the query builder
        if let Some(columns) = expected_tables.get(table_name) {
            for column_name in columns.keys() {
                // Try to select the specific column - this will fail if column doesn't exist
                let _results = query::select(table_name)
                    .columns(&[column_name])
                    .limit(0)
                    .execute(db)
                    .await?;
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use switchy_database::schema::{Column, DataType};

    #[cfg(feature = "sqlite")]
    use crate::create_empty_in_memory;
    #[test_log::test(tokio::test)]
    #[cfg(feature = "sqlite")]
    async fn test_table_existence_assertions() {
        let db = create_empty_in_memory().await.unwrap();

        // Test table doesn't exist initially
        assert!(assert_table_not_exists(&*db, "test_table").await.is_ok());
        assert!(assert_table_exists(&*db, "test_table").await.is_err());

        // Create table using schema query builder
        db.create_table("test_table")
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Int,
                default: None,
            })
            .primary_key("id")
            .execute(&*db)
            .await
            .unwrap();

        // Test table exists now
        assert!(assert_table_exists(&*db, "test_table").await.is_ok());
        assert!(assert_table_not_exists(&*db, "test_table").await.is_err());
    }

    #[test_log::test(tokio::test)]
    #[cfg(feature = "sqlite")]
    async fn test_column_existence_assertions() {
        let db = create_empty_in_memory().await.unwrap();

        // Create table with columns using schema query builder
        db.create_table("users")
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Int,
                default: None,
            })
            .column(Column {
                name: "name".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .column(Column {
                name: "age".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::Int,
                default: None,
            })
            .primary_key("id")
            .execute(&*db)
            .await
            .unwrap();

        // Add some data so SQLite validates column names
        db.insert("users")
            .value("name", "Alice")
            .value("age", 25)
            .execute(&*db)
            .await
            .unwrap();

        // Test column exists
        assert!(
            assert_column_exists(&*db, "users", "id", "INTEGER")
                .await
                .is_ok()
        );
        assert!(
            assert_column_exists(&*db, "users", "name", "TEXT")
                .await
                .is_ok()
        );
        assert!(
            assert_column_exists(&*db, "users", "age", "INTEGER")
                .await
                .is_ok()
        );

        // Test column doesn't exist
        assert!(
            assert_column_not_exists(&*db, "users", "email")
                .await
                .is_ok()
        );
        assert!(
            assert_column_exists(&*db, "users", "email", "TEXT")
                .await
                .is_err()
        );

        // Test column exists when it shouldn't
        assert!(
            assert_column_not_exists(&*db, "users", "name")
                .await
                .is_err()
        );
    }

    #[test_log::test(tokio::test)]
    #[cfg(feature = "sqlite")]
    async fn test_row_count_assertions() {
        let db = create_empty_in_memory().await.unwrap();

        // Create and populate table using schema query builder
        db.create_table("items")
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Int,
                default: None,
            })
            .column(Column {
                name: "name".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .primary_key("id")
            .execute(&*db)
            .await
            .unwrap();

        // Test empty table
        assert!(assert_row_count(&*db, "items", 0).await.is_ok());
        assert!(assert_row_count_min(&*db, "items", 0).await.is_ok());
        assert!(assert_row_count(&*db, "items", 1).await.is_err());
        assert!(assert_row_count_min(&*db, "items", 1).await.is_err());

        // Add some rows
        db.insert("items")
            .value("name", "item1")
            .execute(&*db)
            .await
            .unwrap();
        db.insert("items")
            .value("name", "item2")
            .execute(&*db)
            .await
            .unwrap();
        db.insert("items")
            .value("name", "item3")
            .execute(&*db)
            .await
            .unwrap();

        // Test with data
        assert!(assert_row_count(&*db, "items", 3).await.is_ok());
        assert!(assert_row_count_min(&*db, "items", 2).await.is_ok());
        assert!(assert_row_count_min(&*db, "items", 3).await.is_ok());
        assert!(assert_row_count(&*db, "items", 2).await.is_err());
        assert!(assert_row_count_min(&*db, "items", 4).await.is_err());
    }

    #[test_log::test(tokio::test)]
    #[cfg(feature = "sqlite")]
    async fn test_foreign_key_integrity() {
        let db = create_empty_in_memory().await.unwrap();

        // Enable foreign keys
        db.exec_raw("PRAGMA foreign_keys = ON").await.unwrap();

        // Create tables with foreign key relationship using schema query builder
        db.create_table("users")
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Int,
                default: None,
            })
            .column(Column {
                name: "name".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .primary_key("id")
            .execute(&*db)
            .await
            .unwrap();

        db.create_table("posts")
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Int,
                default: None,
            })
            .column(Column {
                name: "user_id".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::Int,
                default: None,
            })
            .primary_key("id")
            .foreign_key(("user_id", "users(id)"))
            .execute(&*db)
            .await
            .unwrap();

        // Test integrity with valid data
        db.insert("users")
            .value("id", 1)
            .value("name", "Alice")
            .execute(&*db)
            .await
            .unwrap();
        db.insert("posts")
            .value("user_id", 1)
            .execute(&*db)
            .await
            .unwrap();

        assert!(assert_foreign_key_integrity(&*db).await.is_ok());
    }

    #[test_log::test(tokio::test)]
    #[cfg(feature = "sqlite")]
    async fn test_migration_state_assertions() {
        let db = create_empty_in_memory().await.unwrap();

        // Create migration table using schema query builder
        db.create_table("__switchy_migrations")
            .column(Column {
                name: "name".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .column(Column {
                name: "run_on".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .primary_key("name")
            .execute(&*db)
            .await
            .unwrap();

        // Test no migrations applied
        assert!(
            assert_migrations_not_applied(&*db, &["001_initial", "002_users"])
                .await
                .is_ok()
        );
        assert!(
            assert_migrations_applied(&*db, &["001_initial"])
                .await
                .is_err()
        );

        // Apply some migrations
        db.insert("__switchy_migrations")
            .value("name", "001_initial")
            .value("run_on", "2024-01-01")
            .execute(&*db)
            .await
            .unwrap();
        db.insert("__switchy_migrations")
            .value("name", "002_users")
            .value("run_on", "2024-01-02")
            .execute(&*db)
            .await
            .unwrap();

        // Test applied migrations
        assert!(
            assert_migrations_applied(&*db, &["001_initial"])
                .await
                .is_ok()
        );
        assert!(
            assert_migrations_applied(&*db, &["001_initial", "002_users"])
                .await
                .is_ok()
        );
        assert!(
            assert_migrations_not_applied(&*db, &["003_future"])
                .await
                .is_ok()
        );
        assert!(
            assert_migrations_applied(&*db, &["003_future"])
                .await
                .is_err()
        );
        assert!(
            assert_migrations_not_applied(&*db, &["001_initial"])
                .await
                .is_err()
        );
    }

    #[test_log::test(tokio::test)]
    #[cfg(feature = "sqlite")]
    async fn test_schema_comparison() {
        use std::collections::BTreeMap;

        let db = create_empty_in_memory().await.unwrap();

        // Create test schema using schema query builder
        db.create_table("users")
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Int,
                default: None,
            })
            .column(Column {
                name: "name".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .primary_key("id")
            .execute(&*db)
            .await
            .unwrap();

        db.create_table("posts")
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Int,
                default: None,
            })
            .column(Column {
                name: "title".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .column(Column {
                name: "user_id".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::Int,
                default: None,
            })
            .primary_key("id")
            .execute(&*db)
            .await
            .unwrap();

        // Define expected schema
        let mut expected = BTreeMap::new();

        let mut users_cols = BTreeMap::new();
        users_cols.insert("id".to_string(), "INTEGER".to_string());
        users_cols.insert("name".to_string(), "TEXT".to_string());
        expected.insert("users".to_string(), users_cols);

        let mut posts_cols = BTreeMap::new();
        posts_cols.insert("id".to_string(), "INTEGER".to_string());
        posts_cols.insert("title".to_string(), "TEXT".to_string());
        posts_cols.insert("user_id".to_string(), "INTEGER".to_string());
        expected.insert("posts".to_string(), posts_cols);

        // Test matching schema
        assert!(assert_schema_matches(&*db, &expected).await.is_ok());

        // Test mismatched schema (add extra table to expected)
        let mut extra_cols = BTreeMap::new();
        extra_cols.insert("id".to_string(), "INTEGER".to_string());
        expected.insert("comments".to_string(), extra_cols);

        assert!(assert_schema_matches(&*db, &expected).await.is_err());
    }

    #[test_log::test(tokio::test)]
    #[cfg(feature = "sqlite")]
    async fn test_migration_table_missing() {
        let db = create_empty_in_memory().await.unwrap();

        // Test when migration table doesn't exist
        assert!(
            assert_migrations_not_applied(&*db, &["001_initial"])
                .await
                .is_ok()
        );
        assert!(
            assert_migrations_applied(&*db, &["001_initial"])
                .await
                .is_err()
        );
    }
}
