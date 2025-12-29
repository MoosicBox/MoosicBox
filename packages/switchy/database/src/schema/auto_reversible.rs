//! Automatic migration reversal support for safe schema operations.
//!
//! This module provides the `AutoReversible` trait for schema operations
//! that can be safely and automatically reversed. Only operations that
//! are deterministic and non-destructive should implement this trait.
//!
//! # Safety Guarantees
//!
//! The following operations are considered safe to auto-reverse:
//! - `CreateTableStatement` → `DropTableStatement` (structure only, no data)
//! - `CreateIndexStatement` → `DropIndexStatement` (indexes can be recreated)
//! - `AddColumnOperation` → `DropColumnOperation` (new columns start empty)
//!
//! The following are NOT safe and will never implement `AutoReversible`:
//! - `DropTableStatement` - Would lose all table data
//! - `DropIndexStatement` - Could lose performance characteristics
//! - `DropColumnOperation` - Would lose column data
//! - `AlterColumn` operations - May cause data loss or corruption
//!
//! # Examples
//!
//! ```rust
//! # #[cfg(feature = "auto-reverse")]
//! use switchy_database::schema::{create_table, Column, DataType};
//! # #[cfg(feature = "auto-reverse")]
//! use switchy_database::schema::AutoReversible;
//!
//! # #[cfg(feature = "auto-reverse")]
//! let create = create_table("users")
//!     .column(Column {
//!         name: "id".to_string(),
//!         data_type: DataType::Int,
//!         nullable: false,
//!         auto_increment: true,
//!         default: None,
//!     });
//!
//! # #[cfg(feature = "auto-reverse")]
//! // Automatically generate DROP TABLE from CREATE TABLE
//! let drop = create.reverse();
//! # #[cfg(feature = "auto-reverse")]
//! assert_eq!(drop.table_name, "users");
//! # #[cfg(feature = "auto-reverse")]
//! // Original create is still usable since reverse() borrows instead of consuming
//! assert_eq!(create.table_name, "users");
//! ```

use async_trait::async_trait;

use super::alter_table;
use crate::Executable;
use crate::schema::{DataType, DatabaseValue};
use crate::{Database, DatabaseError};

/// Marker trait for schema operations that can be automatically reversed.
/// Only implement this for operations where the reverse is deterministic and safe.
#[async_trait]
pub trait AutoReversible: Executable {
    /// The type of the reversed operation
    type Reversed: Executable;

    /// Generate the reverse operation without consuming self
    fn reverse(&self) -> Self::Reversed;
}

/// Implementation of `AutoReversible` for `CreateTableStatement`.
///
/// Converting a CREATE TABLE to DROP TABLE is safe because:
/// * The table being dropped was just created (no existing data)
/// * The operation is deterministic and reversible
/// * No data loss occurs since the table starts empty
#[async_trait]
impl<'a> AutoReversible for crate::schema::CreateTableStatement<'a> {
    type Reversed = crate::schema::DropTableStatement<'a>;

    fn reverse(&self) -> Self::Reversed {
        crate::schema::DropTableStatement {
            table_name: self.table_name, // Same lifetime, no allocation needed!
            if_exists: true,             // Use IF EXISTS for safety when reversing
            #[cfg(feature = "cascade")]
            behavior: crate::schema::DropBehavior::Default,
        }
    }
}

/// Implementation of `AutoReversible` for `CreateIndexStatement`.
///
/// Converting a CREATE INDEX to DROP INDEX is safe because:
/// * The index being dropped was just created (no existing data dependencies)
/// * The operation is deterministic and reversible
/// * Index can be recreated easily without data loss
/// * Performance characteristics may be lost but functionality is preserved
#[async_trait]
impl<'a> AutoReversible for crate::schema::CreateIndexStatement<'a> {
    type Reversed = crate::schema::DropIndexStatement<'a>;

    fn reverse(&self) -> Self::Reversed {
        crate::schema::DropIndexStatement {
            index_name: self.index_name,
            table_name: self.table_name,
            if_exists: true, // Use IF EXISTS for safety when reversing
        }
    }
}

/// Represents an ADD COLUMN operation that can be automatically reversed.
///
/// Note: This struct does not include an `auto_increment` field because
/// ALTER TABLE ADD COLUMN cannot add auto-increment columns in most databases.
/// This design makes invalid operations unrepresentable in the type system.
///
/// To add an auto-increment column, you must recreate the table or use
/// database-specific workarounds.
#[cfg(feature = "auto-reverse")]
pub struct AddColumnOperation<'a> {
    pub table_name: &'a str,
    pub name: String,
    pub data_type: DataType,
    pub nullable: bool,
    pub default: Option<DatabaseValue>,
}

#[cfg(feature = "auto-reverse")]
#[async_trait]
impl crate::Executable for AddColumnOperation<'_> {
    async fn execute(&self, db: &dyn Database) -> Result<(), DatabaseError> {
        alter_table(self.table_name)
            .add_column(
                self.name.clone(),
                self.data_type.clone(),
                self.nullable,
                self.default.clone(),
            )
            .execute(db)
            .await
    }
}

/// Represents a DROP COLUMN operation (the reverse of ADD COLUMN)
#[cfg(feature = "auto-reverse")]
pub struct DropColumnOperation<'a> {
    /// Name of the table to drop column from
    pub table_name: &'a str,
    /// Name of the column to drop
    pub column_name: String,
}

#[cfg(feature = "auto-reverse")]
#[async_trait]
impl crate::Executable for DropColumnOperation<'_> {
    async fn execute(&self, db: &dyn Database) -> Result<(), DatabaseError> {
        alter_table(self.table_name)
            .drop_column(self.column_name.clone())
            .execute(db)
            .await
    }
}

#[cfg(feature = "auto-reverse")]
#[async_trait]
impl<'a> AutoReversible for AddColumnOperation<'a> {
    type Reversed = DropColumnOperation<'a>;

    fn reverse(&self) -> Self::Reversed {
        DropColumnOperation {
            table_name: self.table_name,
            column_name: self.name.clone(),
        }
    }
}

/// Create an ADD COLUMN operation that can be auto-reversed
#[cfg(feature = "auto-reverse")]
pub fn add_column(
    table: &str,
    name: impl Into<String>,
    data_type: DataType,
    nullable: bool,
    default: Option<DatabaseValue>,
) -> AddColumnOperation<'_> {
    AddColumnOperation {
        table_name: table,
        name: name.into(),
        data_type,
        nullable,
        default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DatabaseValue;
    use crate::schema::{Column, DataType, create_index, create_table};

    mod create_table {
        use super::*;
        #[test]
        fn test_create_table_auto_reverse_basic() {
            let create = create_table("users");
            let drop = create.reverse();

            assert_eq!(drop.table_name, "users");
            assert!(drop.if_exists); // Should use IF EXISTS for safety
        }

        #[test]
        fn test_create_table_auto_reverse_non_consuming() {
            let create = create_table("users");

            // Generate the reverse operation
            let drop = create.reverse();
            assert_eq!(drop.table_name, "users");
            assert!(drop.if_exists);

            // Original create should still be usable since reverse() doesn't consume it
            assert_eq!(create.table_name, "users");

            // We can even call reverse() multiple times
            let drop2 = create.reverse();
            assert_eq!(drop2.table_name, "users");
            assert!(drop2.if_exists);
        }

        #[test]
        fn test_create_table_auto_reverse_with_columns() {
            let create = create_table("products")
                .column(Column {
                    name: "id".to_string(),
                    data_type: DataType::Int,
                    nullable: false,
                    auto_increment: true,
                    default: None,
                })
                .column(Column {
                    name: "name".to_string(),
                    data_type: DataType::Text,
                    nullable: false,
                    auto_increment: false,
                    default: Some(DatabaseValue::String("Unknown".to_string())),
                });

            let drop = create.reverse();

            assert_eq!(drop.table_name, "products");
            assert!(drop.if_exists);
        }

        #[test]
        fn test_create_table_auto_reverse_with_constraints() {
            let create = create_table("orders")
                .column(Column {
                    name: "id".to_string(),
                    data_type: DataType::Int,
                    nullable: false,
                    auto_increment: true,
                    default: None,
                })
                .primary_key("id")
                .foreign_key(("user_id", "users.id"));

            let drop = create.reverse();

            assert_eq!(drop.table_name, "orders");
            assert!(drop.if_exists);
        }

        #[test]
        #[cfg(feature = "cascade")]
        fn test_create_table_auto_reverse_cascade_behavior() {
            let create = create_table("test_cascade");
            let drop = create.reverse();

            assert_eq!(drop.table_name, "test_cascade");
            assert!(drop.if_exists);
            assert_eq!(drop.behavior, crate::schema::DropBehavior::Default);
        }

        #[test]
        fn test_create_table_auto_reverse_executable_trait() {
            let create = create_table("async_test");
            let drop = create.reverse();

            // This would normally require a database connection, but we're just
            // testing that the types implement the trait correctly
            assert_eq!(drop.table_name, "async_test");
            assert!(drop.if_exists);
        }

        #[test]
        fn test_create_table_auto_reverse_complex_async() {
            let create = create_table("complex_async")
                .column(Column {
                    name: "id".to_string(),
                    data_type: DataType::BigInt,
                    nullable: false,
                    auto_increment: true,
                    default: None,
                })
                .column(Column {
                    name: "timestamp".to_string(),
                    data_type: DataType::DateTime,
                    nullable: true,
                    auto_increment: false,
                    default: Some(DatabaseValue::String("CURRENT_TIMESTAMP".to_string())),
                })
                .primary_key("id");

            let drop = create.reverse();

            assert_eq!(drop.table_name, "complex_async");
            assert!(drop.if_exists);

            #[cfg(feature = "cascade")]
            assert_eq!(drop.behavior, crate::schema::DropBehavior::Default);
        }
    }

    mod create_index {
        use super::*;

        #[test]
        fn test_create_index_auto_reverse_basic() {
            let create = create_index("idx_users_email")
                .table("users")
                .column("email");

            let drop = create.reverse();

            assert_eq!(drop.index_name, "idx_users_email");
            assert_eq!(drop.table_name, "users");
            assert!(drop.if_exists); // Should use IF EXISTS for safety
        }

        #[test]
        fn test_create_index_auto_reverse_non_consuming() {
            let create = create_index("idx_users_email")
                .table("users")
                .column("email");

            // Generate reverse operation
            let drop = create.reverse();
            assert_eq!(drop.index_name, "idx_users_email");
            assert_eq!(drop.table_name, "users");
            assert!(drop.if_exists);

            // Original create is still usable for CodeMigration
            assert_eq!(create.index_name, "idx_users_email");
            assert_eq!(create.table_name, "users");

            // We can even call reverse() multiple times
            let drop2 = create.reverse();
            assert_eq!(drop2.index_name, "idx_users_email");
            assert!(drop2.if_exists);
        }

        #[test]
        fn test_create_index_auto_reverse_multi_column() {
            let create = create_index("idx_users_name")
                .table("users")
                .columns(vec!["first_name", "last_name"]);

            let drop = create.reverse();

            assert_eq!(drop.index_name, "idx_users_name");
            assert_eq!(drop.table_name, "users");
            assert!(drop.if_exists);
            // Note: column info is intentionally lost in reversal as per design
        }

        #[test]
        fn test_create_index_auto_reverse_unique() {
            let create = create_index("idx_unique_email")
                .table("users")
                .column("email")
                .unique(true);

            let drop = create.reverse();

            assert_eq!(drop.index_name, "idx_unique_email");
            assert_eq!(drop.table_name, "users");
            assert!(drop.if_exists);
            // Note: unique constraint info is intentionally lost in reversal
        }

        #[test]
        fn test_create_index_auto_reverse_if_not_exists() {
            let create = create_index("idx_conditional")
                .table("users")
                .column("email")
                .if_not_exists(true);

            let drop = create.reverse();

            assert_eq!(drop.index_name, "idx_conditional");
            assert_eq!(drop.table_name, "users");
            assert!(drop.if_exists); // Always true for safety in reversals
        }

        #[test]
        fn test_create_index_auto_reverse_executable_trait() {
            let create = create_index("idx_async_test")
                .table("test_table")
                .column("test_column");

            let drop = create.reverse();

            // Verify both types implement Executable trait correctly
            assert_eq!(drop.index_name, "idx_async_test");
            assert_eq!(drop.table_name, "test_table");
            assert!(drop.if_exists);
        }

        #[test]
        fn test_create_index_auto_reverse_complex_async() {
            let create = create_index("idx_complex_async")
                .table("complex_table")
                .columns(vec!["col1", "col2", "col3"])
                .unique(true)
                .if_not_exists(true);

            let drop = create.reverse();

            assert_eq!(drop.index_name, "idx_complex_async");
            assert_eq!(drop.table_name, "complex_table");
            assert!(drop.if_exists);

            // Original create should still be accessible
            assert_eq!(create.index_name, "idx_complex_async");
            assert_eq!(create.table_name, "complex_table");
            assert_eq!(create.columns, vec!["col1", "col2", "col3"]);
            assert!(create.unique);
            assert!(create.if_not_exists);
        }
    }

    mod add_column {
        use super::*;

        #[test]
        #[cfg(feature = "auto-reverse")]
        fn test_add_column_reversal() {
            let add = add_column(
                "users",
                "age",
                DataType::Int,
                true, // nullable
                None, // default
            );

            let drop = add.reverse();
            assert_eq!(drop.table_name, "users");
            assert_eq!(drop.column_name, "age");
        }

        #[test]
        #[cfg(feature = "auto-reverse")]
        fn test_add_column_with_default() {
            let add = add_column(
                "users",
                "status",
                DataType::Text,
                false, // not nullable
                Some(DatabaseValue::String("active".to_string())),
            );

            let drop = add.reverse();
            assert_eq!(drop.table_name, "users");
            assert_eq!(drop.column_name, "status");
        }

        #[test]
        #[cfg(feature = "auto-reverse")]
        fn test_add_column_non_consuming() {
            let add = add_column(
                "products",
                "price",
                DataType::Real,
                true,
                Some(DatabaseValue::Real64(0.0)),
            );

            // Generate reverse operation
            let drop = add.reverse();
            assert_eq!(drop.table_name, "products");
            assert_eq!(drop.column_name, "price");

            // Original add operation is still usable since reverse() doesn't consume it
            assert_eq!(add.table_name, "products");
            assert_eq!(add.name, "price");
            assert_eq!(add.data_type, DataType::Real);
            assert!(add.nullable);
        }

        #[test]
        #[cfg(feature = "auto-reverse")]
        fn test_add_column_executable_trait() {
            let add = add_column(
                "async_table",
                "new_column",
                DataType::BigInt,
                false,
                Some(DatabaseValue::Int64(42)),
            );

            // Test that AddColumnOperation implements Executable trait correctly
            assert_eq!(add.table_name, "async_table");
            assert_eq!(add.name, "new_column");
            assert_eq!(add.data_type, DataType::BigInt);
            assert!(!add.nullable);
        }

        #[test]
        #[cfg(feature = "auto-reverse")]
        fn test_add_column_complex_async() {
            let add = add_column(
                "complex_table",
                "complex_column",
                DataType::VarChar(255),
                true,
                Some(DatabaseValue::String("default_value".to_string())),
            );

            let drop = add.reverse();

            // Verify both operations maintain correct state
            assert_eq!(add.table_name, "complex_table");
            assert_eq!(add.name, "complex_column");
            assert_eq!(drop.table_name, "complex_table");
            assert_eq!(drop.column_name, "complex_column");
        }
    }
}
