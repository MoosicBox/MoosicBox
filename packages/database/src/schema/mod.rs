//! Database schema management and introspection types
//!
//! This module provides types and functionality for both creating and inspecting database schemas.
//! It includes both schema construction (creating tables, indexes) and schema introspection
//! (discovering existing database structure).
//!
//! # Schema Creation
//!
//! Use the builder patterns to create database schema elements:
//!
//! ```rust,no_run
//! use switchy_database::schema::{create_table, Column, DataType};
//! use switchy_database::{Database, DatabaseValue};
//!
//! # async fn example(db: &dyn Database) -> Result<(), switchy_database::DatabaseError> {
//! // Create a users table
//! create_table("users")
//!     .column(Column {
//!         name: "id".to_string(),
//!         nullable: false,
//!         auto_increment: true,
//!         data_type: DataType::BigInt,
//!         default: None,
//!     })
//!     .column(Column {
//!         name: "email".to_string(),
//!         nullable: false,
//!         auto_increment: false,
//!         data_type: DataType::VarChar(255),
//!         default: None,
//!     })
//!     .column(Column {
//!         name: "created_at".to_string(),
//!         nullable: false,
//!         auto_increment: false,
//!         data_type: DataType::DateTime,
//!         default: Some(DatabaseValue::Now),
//!     })
//!     .primary_key("id")
//!     .execute(db)
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! # Schema Introspection
//!
//! Discover existing database structure using the introspection types:
//!
//! ```rust,no_run
//! use switchy_database::Database;
//!
//! # async fn example(db: &dyn Database) -> Result<(), switchy_database::DatabaseError> {
//! // Check if table exists before creating
//! if !db.table_exists("users").await? {
//!     // Create table (see creation example above)
//! }
//!
//! // Get complete table information
//! if let Some(table_info) = db.get_table_info("users").await? {
//!     println!("Table: {}", table_info.name);
//!
//!     // Inspect columns
//!     for (col_name, col_info) in &table_info.columns {
//!         println!("  Column: {} {:?} {}",
//!             col_name,
//!             col_info.data_type,
//!             if col_info.nullable { "NULL" } else { "NOT NULL" }
//!         );
//!
//!         if col_info.is_primary_key {
//!             println!("    (Primary Key)");
//!         }
//!
//!         if let Some(default) = &col_info.default_value {
//!             println!("    Default: {:?}", default);
//!         }
//!     }
//!
//!     // Inspect indexes
//!     for (idx_name, idx_info) in &table_info.indexes {
//!         println!("  Index: {} on {:?} {}",
//!             idx_name,
//!             idx_info.columns,
//!             if idx_info.unique { "(UNIQUE)" } else { "" }
//!         );
//!     }
//!
//!     // Inspect foreign keys
//!     for (fk_name, fk_info) in &table_info.foreign_keys {
//!         println!("  FK: {}.{} -> {}.{}",
//!             table_info.name, fk_info.column,
//!             fk_info.referenced_table, fk_info.referenced_column
//!         );
//!     }
//! }
//! # Ok(())
//! # }
//! ```
//!
//! # Migration-Safe Schema Operations
//!
//! Combine introspection with schema creation for safe migrations:
//!
//! ```rust,no_run
//! use switchy_database::schema::{create_table, Column, DataType};
//! use switchy_database::{Database, DatabaseValue};
//!
//! # async fn example(db: &dyn Database) -> Result<(), switchy_database::DatabaseError> {
//! // Safe migration: add column if it doesn't exist
//! if db.table_exists("users").await? {
//!     if !db.column_exists("users", "last_login").await? {
//!         // In real code, you'd use ALTER TABLE here
//!         // This example shows the introspection pattern
//!         println!("Need to add last_login column");
//!     }
//! } else {
//!     // Table doesn't exist - create it
//!     create_table("users")
//!         .column(Column {
//!             name: "id".to_string(),
//!             nullable: false,
//!             auto_increment: true,
//!             data_type: DataType::BigInt,
//!             default: None,
//!         })
//!         .column(Column {
//!             name: "last_login".to_string(),
//!             nullable: true,
//!             auto_increment: false,
//!             data_type: DataType::DateTime,
//!             default: None,
//!         })
//!         .primary_key("id")
//!         .execute(db)
//!         .await?;
//! }
//!
//! // Validate schema matches expectations
//! let columns = db.get_table_columns("users").await?;
//! let id_column = columns.iter().find(|c| c.name == "id")
//!     .expect("users table should have id column");
//!
//! assert!(id_column.is_primary_key, "id should be primary key");
//! assert!(!id_column.nullable, "id should be NOT NULL");
//! # Ok(())
//! # }
//! ```
//!
//! # Working with Different Data Types
//!
//! The [`DataType`] enum provides a common representation across database backends:
//!
//! ```rust,no_run
//! use switchy_database::schema::{DataType, Column};
//! use switchy_database::DatabaseValue;
//!
//! // Text types
//! let short_string = Column {
//!     name: "code".to_string(),
//!     data_type: DataType::VarChar(10), // Fixed-width string
//!     nullable: false,
//!     auto_increment: false,
//!     default: None,
//! };
//!
//! let long_text = Column {
//!     name: "description".to_string(),
//!     data_type: DataType::Text, // Variable-length text
//!     nullable: true,
//!     auto_increment: false,
//!     default: Some(DatabaseValue::String("".to_string())),
//! };
//!
//! // Numeric types
//! let counter = Column {
//!     name: "count".to_string(),
//!     data_type: DataType::Int, // 32-bit integer
//!     nullable: false,
//!     auto_increment: false,
//!     default: Some(DatabaseValue::Number(0)),
//! };
//!
//! let big_id = Column {
//!     name: "big_id".to_string(),
//!     data_type: DataType::BigInt, // 64-bit integer
//!     nullable: false,
//!     auto_increment: true,
//!     default: None,
//! };
//!
//! let price = Column {
//!     name: "price".to_string(),
//!     data_type: DataType::Decimal(10, 2), // DECIMAL(10,2) for currency
//!     nullable: false,
//!     auto_increment: false,
//!     default: Some(DatabaseValue::Real(0.0)),
//! };
//!
//! // Boolean and date types
//! let active = Column {
//!     name: "active".to_string(),
//!     data_type: DataType::Bool,
//!     nullable: false,
//!     auto_increment: false,
//!     default: Some(DatabaseValue::Bool(true)),
//! };
//!
//! let created_at = Column {
//!     name: "created_at".to_string(),
//!     data_type: DataType::DateTime,
//!     nullable: false,
//!     auto_increment: false,
//!     default: Some(DatabaseValue::Now), // Use current timestamp
//! };
//! ```

use std::collections::BTreeMap;

use crate::{Database, DatabaseError, DatabaseValue};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataType {
    // Text types
    Text,
    VarChar(u16),
    Char(u16),

    // Integer types
    SmallInt,
    Int,
    BigInt,
    Serial,    // Auto-incrementing integer (PostgreSQL)
    BigSerial, // Auto-incrementing bigint (PostgreSQL)

    // Floating point types
    Real,
    Double,
    Decimal(u8, u8),
    Money, // Monetary type

    // Boolean type
    Bool,

    // Date/Time types
    Date,      // Date without time
    Time,      // Time without date
    DateTime,  // Date and time
    Timestamp, // Timestamp (distinct from DateTime)

    // Binary types
    Blob,                // Binary data
    Binary(Option<u32>), // Binary with optional length

    // JSON types
    Json,  // JSON column type
    Jsonb, // PostgreSQL binary JSON

    // Specialized types
    Uuid,                 // UUID type
    Xml,                  // XML type
    Array(Box<DataType>), // PostgreSQL arrays
    Inet,                 // IP address
    MacAddr,              // MAC address

    // Fallback for database-specific types
    Custom(String), // For types we don't explicitly handle
}

#[derive(Debug, Clone)]
pub struct Column {
    pub name: String,
    pub nullable: bool,
    pub auto_increment: bool,
    pub data_type: DataType,
    pub default: Option<DatabaseValue>,
}

pub struct CreateTableStatement<'a> {
    pub table_name: &'a str,
    pub if_not_exists: bool,
    pub columns: Vec<Column>,
    pub primary_key: Option<&'a str>,
    pub foreign_keys: Vec<(&'a str, &'a str)>,
}

#[must_use]
pub const fn create_table(table_name: &str) -> CreateTableStatement<'_> {
    CreateTableStatement {
        table_name,
        if_not_exists: false,
        columns: vec![],
        primary_key: None,
        foreign_keys: vec![],
    }
}

impl<'a> CreateTableStatement<'a> {
    #[must_use]
    pub const fn if_not_exists(mut self, if_not_exists: bool) -> Self {
        self.if_not_exists = if_not_exists;
        self
    }

    #[must_use]
    pub fn column(mut self, column: Column) -> Self {
        self.columns.push(column);
        self
    }

    #[must_use]
    pub fn columns(mut self, columns: Vec<Column>) -> Self {
        self.columns.extend(columns);
        self
    }

    #[must_use]
    pub const fn primary_key(mut self, primary_key: &'a str) -> Self {
        self.primary_key = Some(primary_key);
        self
    }

    #[must_use]
    pub fn foreign_key(mut self, foreign_key: (&'a str, &'a str)) -> Self {
        self.foreign_keys.push(foreign_key);
        self
    }

    #[must_use]
    pub fn foreign_keys(mut self, foreign_keys: Vec<(&'a str, &'a str)>) -> Self {
        self.foreign_keys = foreign_keys;
        self
    }

    /// # Errors
    ///
    /// Will return `Err` if the `exec_create_table` execution failed.
    pub async fn execute(self, db: &dyn Database) -> Result<(), DatabaseError> {
        db.exec_create_table(&self).await
    }
}

/// DROP behavior for CASCADE/RESTRICT operations
#[cfg(feature = "cascade")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropBehavior {
    /// Use backend default behavior
    Default,
    /// Drop all dependents
    Cascade,
    /// Fail if dependencies exist
    Restrict,
}

pub struct DropTableStatement<'a> {
    pub table_name: &'a str,
    pub if_exists: bool,
    #[cfg(feature = "cascade")]
    pub behavior: DropBehavior,
}

#[must_use]
#[allow(clippy::missing_const_for_fn)] // Cannot be const due to conditional compilation
pub fn drop_table(table_name: &str) -> DropTableStatement<'_> {
    DropTableStatement {
        table_name,
        if_exists: false,
        #[cfg(feature = "cascade")]
        behavior: DropBehavior::Default,
    }
}

impl DropTableStatement<'_> {
    #[must_use]
    pub const fn if_exists(mut self, if_exists: bool) -> Self {
        self.if_exists = if_exists;
        self
    }

    /// Set CASCADE behavior
    #[cfg(feature = "cascade")]
    #[must_use]
    pub const fn cascade(mut self) -> Self {
        self.behavior = DropBehavior::Cascade;
        self
    }

    /// Set RESTRICT behavior
    #[cfg(feature = "cascade")]
    #[must_use]
    pub const fn restrict(mut self) -> Self {
        self.behavior = DropBehavior::Restrict;
        self
    }

    /// # Errors
    ///
    /// Will return `Err` if the `exec_drop_table` execution failed.
    pub async fn execute(self, db: &dyn Database) -> Result<(), DatabaseError> {
        // Simple delegation to database-specific implementations
        db.exec_drop_table(&self).await
    }
}

pub struct CreateIndexStatement<'a> {
    pub index_name: &'a str,
    pub table_name: &'a str,
    pub columns: Vec<&'a str>,
    pub unique: bool,
    pub if_not_exists: bool,
}

#[must_use]
pub const fn create_index(index_name: &str) -> CreateIndexStatement<'_> {
    CreateIndexStatement {
        index_name,
        table_name: "",
        columns: vec![],
        unique: false,
        if_not_exists: false,
    }
}

impl<'a> CreateIndexStatement<'a> {
    #[must_use]
    pub const fn table(mut self, table_name: &'a str) -> Self {
        self.table_name = table_name;
        self
    }

    #[must_use]
    pub fn column(mut self, column: &'a str) -> Self {
        self.columns.push(column);
        self
    }

    #[must_use]
    pub fn columns(mut self, columns: Vec<&'a str>) -> Self {
        self.columns = columns;
        self
    }

    #[must_use]
    pub const fn unique(mut self, unique: bool) -> Self {
        self.unique = unique;
        self
    }

    /// Set whether to use IF NOT EXISTS clause
    ///
    /// # Database Compatibility
    ///
    /// * **`SQLite`**: Full support
    /// * **`PostgreSQL`**: Full support
    /// * **`MySQL`**: Requires `MySQL` 8.0.29 or later. Will produce a syntax error on older versions.
    ///
    /// # Example
    ///
    /// ```rust
    /// # use switchy_database::schema::create_index;
    /// let stmt = create_index("idx_name")
    ///     .table("users")
    ///     .column("email")
    ///     .if_not_exists(true);  // MySQL 8.0.29+ required for this
    /// ```
    #[must_use]
    pub const fn if_not_exists(mut self, if_not_exists: bool) -> Self {
        self.if_not_exists = if_not_exists;
        self
    }

    /// # Errors
    ///
    /// Will return `Err` if the `exec_create_index` execution failed.
    pub async fn execute(self, db: &dyn Database) -> Result<(), DatabaseError> {
        db.exec_create_index(&self).await
    }
}

pub struct DropIndexStatement<'a> {
    pub index_name: &'a str,
    pub table_name: &'a str,
    pub if_exists: bool,
}

#[must_use]
pub const fn drop_index<'a>(index_name: &'a str, table_name: &'a str) -> DropIndexStatement<'a> {
    DropIndexStatement {
        index_name,
        table_name,
        if_exists: false,
    }
}

impl DropIndexStatement<'_> {
    #[must_use]
    pub const fn if_exists(mut self) -> Self {
        self.if_exists = true;
        self
    }

    /// Execute the drop index statement against the provided database.
    ///
    /// # Errors
    ///
    /// Will return `Err` if the `exec_drop_index` execution failed.
    pub async fn execute(self, db: &dyn Database) -> Result<(), DatabaseError> {
        db.exec_drop_index(&self).await
    }
}

#[derive(Debug, Clone)]
pub enum AlterOperation {
    AddColumn {
        name: String,
        data_type: DataType,
        nullable: bool,
        default: Option<DatabaseValue>,
    },
    DropColumn {
        name: String,
    },
    RenameColumn {
        old_name: String,
        new_name: String,
    },
    ModifyColumn {
        name: String,
        new_data_type: DataType,
        new_nullable: Option<bool>,
        new_default: Option<DatabaseValue>,
    },
}

pub struct AlterTableStatement<'a> {
    pub table_name: &'a str,
    pub operations: Vec<AlterOperation>,
}

#[must_use]
pub const fn alter_table(table_name: &str) -> AlterTableStatement<'_> {
    AlterTableStatement {
        table_name,
        operations: vec![],
    }
}

impl AlterTableStatement<'_> {
    #[must_use]
    pub fn add_column(
        mut self,
        name: String,
        data_type: DataType,
        nullable: bool,
        default: Option<DatabaseValue>,
    ) -> Self {
        self.operations.push(AlterOperation::AddColumn {
            name,
            data_type,
            nullable,
            default,
        });
        self
    }

    #[must_use]
    pub fn drop_column(mut self, name: String) -> Self {
        self.operations.push(AlterOperation::DropColumn { name });
        self
    }

    #[must_use]
    pub fn rename_column(mut self, old_name: String, new_name: String) -> Self {
        self.operations
            .push(AlterOperation::RenameColumn { old_name, new_name });
        self
    }

    #[must_use]
    pub fn modify_column(
        mut self,
        name: String,
        new_data_type: DataType,
        new_nullable: Option<bool>,
        new_default: Option<DatabaseValue>,
    ) -> Self {
        self.operations.push(AlterOperation::ModifyColumn {
            name,
            new_data_type,
            new_nullable,
            new_default,
        });
        self
    }

    /// Execute the alter table statement against the provided database.
    ///
    /// # Errors
    ///
    /// Will return `Err` if the `exec_alter_table` execution failed.
    ///
    /// # Notes
    ///
    /// * **Column Order Changes**: For `SQLite` MODIFY COLUMN operations, column order may change
    ///   as new columns are added at the end of the table. Do not rely on SELECT * or positional
    ///   parameters.
    /// * **Transaction Safety**: All operations are wrapped in transactions for atomicity.
    /// * **`SQLite` Workarounds**: Uses hybrid approach - native operations when possible,
    ///   column-based workarounds for MODIFY COLUMN, table recreation as fallback.
    pub async fn execute(self, db: &dyn Database) -> Result<(), DatabaseError> {
        db.exec_alter_table(&self).await
    }
}

// Table introspection types for Phase 16 (Table Introspection API)

/// Information about a single column in a database table
#[derive(Debug, Clone, PartialEq)]
pub struct ColumnInfo {
    /// Column name
    pub name: String,
    /// Column data type
    pub data_type: DataType,
    /// Whether the column allows NULL values
    pub nullable: bool,
    /// Whether this column is part of the primary key
    pub is_primary_key: bool,
    /// Whether the column has auto-increment/serial behavior
    pub auto_increment: bool,
    /// Default value for the column
    pub default_value: Option<DatabaseValue>,
    /// Position/ordinal of the column in the table (1-based)
    pub ordinal_position: u32,
}

/// Information about a database index
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IndexInfo {
    /// Index name
    pub name: String,
    /// Whether this is a unique index
    pub unique: bool,
    /// Ordered list of column names in the index
    pub columns: Vec<String>,
    /// Whether this is a primary key index
    pub is_primary: bool,
}

/// Information about a foreign key constraint
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForeignKeyInfo {
    /// Foreign key constraint name
    pub name: String,
    /// Column name in the current table
    pub column: String,
    /// Referenced table name
    pub referenced_table: String,
    /// Referenced column name
    pub referenced_column: String,
    /// Action to take on UPDATE (CASCADE, RESTRICT, SET NULL, etc.)
    pub on_update: Option<String>,
    /// Action to take on DELETE (CASCADE, RESTRICT, SET NULL, etc.)
    pub on_delete: Option<String>,
}

/// Complete information about a database table
#[derive(Debug, Clone, PartialEq)]
pub struct TableInfo {
    /// Table name
    pub name: String,
    /// All columns in the table, indexed by column name for fast lookup
    pub columns: BTreeMap<String, ColumnInfo>,
    /// All indexes on the table, indexed by index name
    pub indexes: BTreeMap<String, IndexInfo>,
    /// All foreign key constraints, indexed by constraint name
    pub foreign_keys: BTreeMap<String, ForeignKeyInfo>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_drop_table_builder_default() {
        let statement = drop_table("test_table");
        assert_eq!(statement.table_name, "test_table");
        assert!(!statement.if_exists);
    }

    #[test]
    fn test_drop_table_builder_with_if_exists() {
        let statement = drop_table("test_table").if_exists(true);
        assert_eq!(statement.table_name, "test_table");
        assert!(statement.if_exists);
    }

    #[test]
    fn test_drop_table_builder_chain() {
        let statement = drop_table("users").if_exists(true);

        assert_eq!(statement.table_name, "users");
        assert!(statement.if_exists);
    }

    #[test]
    fn test_drop_table_builder_if_exists_false() {
        let statement = drop_table("test_table").if_exists(true).if_exists(false);

        assert_eq!(statement.table_name, "test_table");
        assert!(!statement.if_exists);
    }

    // CreateIndexStatement tests
    #[test]
    fn test_create_index_builder_default() {
        let statement = create_index("test_index");
        assert_eq!(statement.index_name, "test_index");
        assert_eq!(statement.table_name, "");
        assert!(statement.columns.is_empty());
        assert!(!statement.unique);
        assert!(!statement.if_not_exists);
    }

    #[test]
    fn test_create_index_builder_single_column() {
        let statement = create_index("idx_name").table("users").column("name");

        assert_eq!(statement.index_name, "idx_name");
        assert_eq!(statement.table_name, "users");
        assert_eq!(statement.columns, vec!["name"]);
        assert!(!statement.unique);
        assert!(!statement.if_not_exists);
    }

    #[test]
    fn test_create_index_builder_multi_column() {
        let statement = create_index("idx_multi")
            .table("users")
            .columns(vec!["first_name", "last_name"]);

        assert_eq!(statement.index_name, "idx_multi");
        assert_eq!(statement.table_name, "users");
        assert_eq!(statement.columns, vec!["first_name", "last_name"]);
        assert!(!statement.unique);
        assert!(!statement.if_not_exists);
    }

    #[test]
    fn test_create_index_builder_unique() {
        let statement = create_index("idx_email")
            .table("users")
            .column("email")
            .unique(true);

        assert_eq!(statement.index_name, "idx_email");
        assert_eq!(statement.table_name, "users");
        assert_eq!(statement.columns, vec!["email"]);
        assert!(statement.unique);
        assert!(!statement.if_not_exists);
    }

    #[test]
    fn test_create_index_builder_if_not_exists() {
        let statement = create_index("idx_test")
            .table("test")
            .column("col")
            .if_not_exists(true);

        assert_eq!(statement.index_name, "idx_test");
        assert_eq!(statement.table_name, "test");
        assert_eq!(statement.columns, vec!["col"]);
        assert!(!statement.unique);
        assert!(statement.if_not_exists);
    }

    #[test]
    fn test_create_index_builder_method_chaining() {
        let statement = create_index("idx_complex")
            .table("products")
            .column("category_id")
            .column("price")
            .unique(true)
            .if_not_exists(true);

        assert_eq!(statement.index_name, "idx_complex");
        assert_eq!(statement.table_name, "products");
        assert_eq!(statement.columns, vec!["category_id", "price"]);
        assert!(statement.unique);
        assert!(statement.if_not_exists);
    }

    #[test]
    fn test_create_index_builder_columns_overwrite() {
        let statement = create_index("idx_test")
            .table("test")
            .column("col1")
            .column("col2")
            .columns(vec!["col3", "col4"]); // This should overwrite

        assert_eq!(statement.index_name, "idx_test");
        assert_eq!(statement.table_name, "test");
        assert_eq!(statement.columns, vec!["col3", "col4"]);
        assert!(!statement.unique);
        assert!(!statement.if_not_exists);
    }

    // DropIndexStatement tests
    #[test]
    fn test_drop_index_builder_default() {
        let statement = drop_index("test_index", "test_table");
        assert_eq!(statement.index_name, "test_index");
        assert_eq!(statement.table_name, "test_table");
        assert!(!statement.if_exists);
    }

    #[test]
    fn test_drop_index_builder_with_if_exists() {
        let statement = drop_index("idx_email", "users").if_exists();
        assert_eq!(statement.index_name, "idx_email");
        assert_eq!(statement.table_name, "users");
        assert!(statement.if_exists);
    }

    #[test]
    fn test_drop_index_builder_if_exists_chaining() {
        let statement = drop_index("idx_complex", "products").if_exists();

        assert_eq!(statement.index_name, "idx_complex");
        assert_eq!(statement.table_name, "products");
        assert!(statement.if_exists);
    }

    // AlterTableStatement tests
    #[test]
    fn test_alter_table_builder_default() {
        let statement = alter_table("test_table");
        assert_eq!(statement.table_name, "test_table");
        assert!(statement.operations.is_empty());
    }

    #[test]
    fn test_alter_table_add_column() {
        let statement = alter_table("users").add_column(
            "email".to_string(),
            DataType::VarChar(255),
            false,
            None,
        );

        assert_eq!(statement.table_name, "users");
        assert_eq!(statement.operations.len(), 1);
        match &statement.operations[0] {
            AlterOperation::AddColumn {
                name,
                data_type,
                nullable,
                default,
            } => {
                assert_eq!(name, "email");
                assert!(matches!(data_type, DataType::VarChar(255)));
                assert!(!nullable);
                assert!(default.is_none());
            }
            _ => panic!("Expected AddColumn operation"),
        }
    }

    #[test]
    fn test_alter_table_add_column_with_default() {
        let statement = alter_table("users").add_column(
            "active".to_string(),
            DataType::Bool,
            true,
            Some(DatabaseValue::Bool(true)),
        );

        assert_eq!(statement.table_name, "users");
        assert_eq!(statement.operations.len(), 1);
        match &statement.operations[0] {
            AlterOperation::AddColumn {
                name,
                data_type,
                nullable,
                default,
            } => {
                assert_eq!(name, "active");
                assert!(matches!(data_type, DataType::Bool));
                assert!(nullable);
                assert!(matches!(default, Some(DatabaseValue::Bool(true))));
            }
            _ => panic!("Expected AddColumn operation"),
        }
    }

    #[test]
    fn test_alter_table_drop_column() {
        let statement = alter_table("users").drop_column("old_column".to_string());

        assert_eq!(statement.table_name, "users");
        assert_eq!(statement.operations.len(), 1);
        match &statement.operations[0] {
            AlterOperation::DropColumn { name } => {
                assert_eq!(name, "old_column");
            }
            _ => panic!("Expected DropColumn operation"),
        }
    }

    #[test]
    fn test_alter_table_rename_column() {
        let statement =
            alter_table("users").rename_column("old_name".to_string(), "new_name".to_string());

        assert_eq!(statement.table_name, "users");
        assert_eq!(statement.operations.len(), 1);
        match &statement.operations[0] {
            AlterOperation::RenameColumn { old_name, new_name } => {
                assert_eq!(old_name, "old_name");
                assert_eq!(new_name, "new_name");
            }
            _ => panic!("Expected RenameColumn operation"),
        }
    }

    #[test]
    fn test_alter_table_modify_column() {
        let statement = alter_table("users").modify_column(
            "age".to_string(),
            DataType::Int,
            Some(false),
            Some(DatabaseValue::Number(0)),
        );

        assert_eq!(statement.table_name, "users");
        assert_eq!(statement.operations.len(), 1);
        match &statement.operations[0] {
            AlterOperation::ModifyColumn {
                name,
                new_data_type,
                new_nullable,
                new_default,
            } => {
                assert_eq!(name, "age");
                assert!(matches!(new_data_type, DataType::Int));
                assert_eq!(new_nullable, &Some(false));
                assert!(matches!(new_default, Some(DatabaseValue::Number(0))));
            }
            _ => panic!("Expected ModifyColumn operation"),
        }
    }

    #[test]
    fn test_alter_table_modify_column_partial() {
        let statement =
            alter_table("users").modify_column("name".to_string(), DataType::Text, None, None);

        assert_eq!(statement.table_name, "users");
        assert_eq!(statement.operations.len(), 1);
        match &statement.operations[0] {
            AlterOperation::ModifyColumn {
                name,
                new_data_type,
                new_nullable,
                new_default,
            } => {
                assert_eq!(name, "name");
                assert!(matches!(new_data_type, DataType::Text));
                assert_eq!(new_nullable, &None);
                assert_eq!(new_default, &None);
            }
            _ => panic!("Expected ModifyColumn operation"),
        }
    }

    #[test]
    fn test_alter_table_multiple_operations() {
        let statement = alter_table("users")
            .add_column("email".to_string(), DataType::VarChar(255), false, None)
            .drop_column("old_field".to_string())
            .rename_column("first_name".to_string(), "given_name".to_string())
            .modify_column("age".to_string(), DataType::SmallInt, Some(true), None);

        assert_eq!(statement.table_name, "users");
        assert_eq!(statement.operations.len(), 4);

        // Check AddColumn operation
        match &statement.operations[0] {
            AlterOperation::AddColumn {
                name,
                data_type,
                nullable,
                default,
            } => {
                assert_eq!(name, "email");
                assert!(matches!(data_type, DataType::VarChar(255)));
                assert!(!nullable);
                assert!(default.is_none());
            }
            _ => panic!("Expected AddColumn operation at index 0"),
        }

        // Check DropColumn operation
        match &statement.operations[1] {
            AlterOperation::DropColumn { name } => {
                assert_eq!(name, "old_field");
            }
            _ => panic!("Expected DropColumn operation at index 1"),
        }

        // Check RenameColumn operation
        match &statement.operations[2] {
            AlterOperation::RenameColumn { old_name, new_name } => {
                assert_eq!(old_name, "first_name");
                assert_eq!(new_name, "given_name");
            }
            _ => panic!("Expected RenameColumn operation at index 2"),
        }

        // Check ModifyColumn operation
        match &statement.operations[3] {
            AlterOperation::ModifyColumn {
                name,
                new_data_type,
                new_nullable,
                new_default,
            } => {
                assert_eq!(name, "age");
                assert!(matches!(new_data_type, DataType::SmallInt));
                assert_eq!(new_nullable, &Some(true));
                assert_eq!(new_default, &None);
            }
            _ => panic!("Expected ModifyColumn operation at index 3"),
        }
    }

    #[test]
    fn test_alter_table_builder_chaining() {
        let statement = alter_table("products")
            .add_column("sku".to_string(), DataType::VarChar(50), false, None)
            .add_column(
                "created_at".to_string(),
                DataType::DateTime,
                false,
                Some(DatabaseValue::Now),
            );

        assert_eq!(statement.table_name, "products");
        assert_eq!(statement.operations.len(), 2);

        // Both operations should be AddColumn
        for (i, operation) in statement.operations.iter().enumerate() {
            match operation {
                AlterOperation::AddColumn { .. } => {
                    // Expected
                }
                _ => panic!("Expected AddColumn operation at index {i}"),
            }
        }
    }

    #[test]
    fn test_alter_operation_clone() {
        let operation = AlterOperation::AddColumn {
            name: "test_col".to_string(),
            data_type: DataType::Int,
            nullable: true,
            default: Some(DatabaseValue::Number(42)),
        };

        let cloned = operation.clone();
        match (&operation, &cloned) {
            (
                AlterOperation::AddColumn {
                    name: n1,
                    data_type: d1,
                    nullable: null1,
                    default: def1,
                },
                AlterOperation::AddColumn {
                    name: n2,
                    data_type: d2,
                    nullable: null2,
                    default: def2,
                },
            ) => {
                assert_eq!(n1, n2);
                assert!(matches!((d1, d2), (DataType::Int, DataType::Int)));
                assert_eq!(null1, null2);
                assert!(matches!(
                    (def1, def2),
                    (
                        Some(DatabaseValue::Number(42)),
                        Some(DatabaseValue::Number(42))
                    )
                ));
            }
            _ => panic!("Clone should match original"),
        }
    }
}

// Dependency management for CASCADE and RESTRICT operations
#[cfg(feature = "schema")]
pub mod dependencies;

#[cfg(feature = "schema")]
pub use dependencies::{CycleError, DependencyGraph, DropPlan};
