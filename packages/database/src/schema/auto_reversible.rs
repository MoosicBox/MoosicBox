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
//! ```

use crate::Executable;
use async_trait::async_trait;

/// Marker trait for schema operations that can be automatically reversed.
/// Only implement this for operations where the reverse is deterministic and safe.
#[async_trait]
pub trait AutoReversible: Executable {
    /// The type of the reversed operation
    type Reversed: Executable;

    /// Generate the reverse operation
    fn reverse(self) -> Self::Reversed;
}
