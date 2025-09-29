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

use crate::Executable;
use async_trait::async_trait;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DatabaseValue;
    use crate::schema::{Column, DataType, create_table};

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

    #[switchy_async::test]
    async fn test_create_table_auto_reverse_executable_trait() {
        let create = create_table("async_test");
        let drop = create.reverse();

        // This would normally require a database connection, but we're just
        // testing that the types implement the trait correctly
        assert_eq!(drop.table_name, "async_test");
        assert!(drop.if_exists);
    }

    #[switchy_async::test]
    async fn test_create_table_auto_reverse_complex_async() {
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
