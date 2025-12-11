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
//!     default: Some(DatabaseValue::Int64(0)),
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
//!     default: Some(DatabaseValue::Real64(0.0)),
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

/// Represents common database column data types
///
/// This enum provides a unified representation of column types across different
/// database backends (`SQLite`, `PostgreSQL`, `MySQL`). Each variant maps to appropriate
/// native types for each backend.
///
/// ## Cross-Database Type Mapping
///
/// | `DataType` | `SQLite` | `PostgreSQL` | `MySQL` |
/// |------------|---------|------------|-------|
/// | `Text` | `TEXT` | `TEXT` | `TEXT` |
/// | `VarChar(n)` | `VARCHAR(n)` | `VARCHAR(n)` | `VARCHAR(n)` |
/// | `Int` | `INTEGER` | `INTEGER` | `INT` |
/// | `BigInt` | `INTEGER` | `BIGINT` | `BIGINT` |
/// | `Real` | `REAL` | `REAL` | `FLOAT` |
/// | `Double` | `REAL` | `DOUBLE PRECISION` | `DOUBLE` |
/// | `Bool` | `INTEGER` (0/1) | `BOOLEAN` | `BOOLEAN` |
/// | `DateTime` | `TEXT` (ISO8601) | `TIMESTAMP` | `DATETIME` |
/// | `Uuid` | `TEXT` | `UUID` | `CHAR(36)` |
/// | `Json` | `TEXT` | `JSON` | `JSON` |
///
/// ## Examples
///
/// ```rust
/// use switchy_database::schema::{DataType, Column};
/// use switchy_database::DatabaseValue;
///
/// // Text types
/// let short_code = Column {
///     name: "code".to_string(),
///     data_type: DataType::VarChar(10),
///     nullable: false,
///     auto_increment: false,
///     default: None,
/// };
///
/// let description = Column {
///     name: "description".to_string(),
///     data_type: DataType::Text,
///     nullable: true,
///     auto_increment: false,
///     default: None,
/// };
///
/// // Numeric types
/// let id = Column {
///     name: "id".to_string(),
///     data_type: DataType::BigInt,
///     nullable: false,
///     auto_increment: true,
///     default: None,
/// };
///
/// let price = Column {
///     name: "price".to_string(),
///     data_type: DataType::Decimal(10, 2),
///     nullable: false,
///     auto_increment: false,
///     default: Some(DatabaseValue::Real64(0.0)),
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DataType {
    // Text types
    /// Variable-length text without size limit
    Text,
    /// Variable-length string with maximum length
    VarChar(u16),
    /// Fixed-length string with padding
    Char(u16),

    // Integer types
    /// 8-bit signed integer (-128 to 127)
    TinyInt,
    /// 16-bit signed integer (-32,768 to 32,767)
    SmallInt,
    /// 32-bit signed integer (-2,147,483,648 to 2,147,483,647)
    Int,
    /// 64-bit signed integer
    BigInt,
    /// Auto-incrementing integer (`PostgreSQL` `SERIAL`)
    Serial,
    /// Auto-incrementing 64-bit integer (`PostgreSQL` `BIGSERIAL`)
    BigSerial,

    // TODO: Unsigned integer types - schema support pending
    // TinyIntUnsigned,  // 0-255 (MySQL) or 0-127 (PostgreSQL/SQLite)
    // SmallIntUnsigned, // 0-65535 (MySQL) or 0-32767 (PostgreSQL/SQLite)
    // IntUnsigned,      // 0-4294967295 (MySQL) or 0-2147483647 (PostgreSQL/SQLite)
    // BigIntUnsigned,   // 0-18446744073709551615 (MySQL only)

    // Floating point types
    /// Single-precision floating point (32-bit)
    Real,
    /// Double-precision floating point (64-bit)
    Double,
    /// Fixed-precision decimal with precision and scale
    Decimal(u8, u8),
    /// Monetary type for currency values
    Money,

    // Boolean type
    /// Boolean true/false value
    Bool,

    // Date/Time types
    /// Date without time component
    Date,
    /// Time without date component
    Time,
    /// Date and time combined
    DateTime,
    /// Timestamp (may have different timezone behavior than `DateTime`)
    Timestamp,

    // Binary types
    Blob,                // Binary data
    Binary(Option<u32>), // Binary with optional length

    // JSON types
    Json,  // JSON column type
    Jsonb, // PostgreSQL binary JSON

    // Specialized types
    Uuid,             // UUID type
    Xml,              // XML type
    Array(Box<Self>), // PostgreSQL arrays
    Inet,             // IP address
    MacAddr,          // MAC address

    // Fallback for database-specific types
    Custom(String), // For types we don't explicitly handle
}

/// Represents a database column definition
///
/// This struct defines all properties of a database column including its name,
/// data type, constraints, and default value. Used when creating tables via
/// [`CreateTableStatement`].
///
/// ## Fields
///
/// * `name` - Column name (must be valid SQL identifier)
/// * `nullable` - Whether column allows NULL values
/// * `auto_increment` - Whether column auto-increments (typically for primary keys)
/// * `data_type` - Column data type from [`DataType`] enum
/// * `default` - Optional default value for the column
///
/// ## Examples
///
/// ```rust
/// use switchy_database::schema::{Column, DataType};
/// use switchy_database::DatabaseValue;
///
/// // Auto-incrementing ID column
/// let id_column = Column {
///     name: "id".to_string(),
///     nullable: false,
///     auto_increment: true,
///     data_type: DataType::BigInt,
///     default: None,
/// };
///
/// // Nullable email column with VARCHAR type
/// let email_column = Column {
///     name: "email".to_string(),
///     nullable: true,
///     auto_increment: false,
///     data_type: DataType::VarChar(255),
///     default: None,
/// };
///
/// // Timestamp column with NOW() default
/// let created_column = Column {
///     name: "created_at".to_string(),
///     nullable: false,
///     auto_increment: false,
///     data_type: DataType::DateTime,
///     default: Some(DatabaseValue::Now),
/// };
/// ```
#[derive(Debug, Clone)]
pub struct Column {
    pub name: String,
    pub nullable: bool,
    pub auto_increment: bool,
    pub data_type: DataType,
    pub default: Option<DatabaseValue>,
}

/// Builder for CREATE TABLE SQL statements
///
/// Provides a fluent API for constructing table creation statements with columns,
/// primary keys, and foreign key constraints. Use [`create_table`] to construct.
///
/// ## Examples
///
/// ```rust,no_run
/// use switchy_database::schema::{create_table, Column, DataType};
/// use switchy_database::{Database, DatabaseValue};
///
/// # async fn example(db: &dyn Database) -> Result<(), switchy_database::DatabaseError> {
/// create_table("users")
///     .column(Column {
///         name: "id".to_string(),
///         nullable: false,
///         auto_increment: true,
///         data_type: DataType::BigInt,
///         default: None,
///     })
///     .column(Column {
///         name: "username".to_string(),
///         nullable: false,
///         auto_increment: false,
///         data_type: DataType::VarChar(50),
///         default: None,
///     })
///     .primary_key("id")
///     .execute(db)
///     .await?;
/// # Ok(())
/// # }
/// ```
pub struct CreateTableStatement<'a> {
    pub table_name: &'a str,
    pub if_not_exists: bool,
    pub columns: Vec<Column>,
    pub primary_key: Option<&'a str>,
    pub foreign_keys: Vec<(&'a str, &'a str)>,
}

/// Creates a new CREATE TABLE statement builder
///
/// # Examples
///
/// ```rust,no_run
/// use switchy_database::schema::{create_table, Column, DataType};
/// use switchy_database::Database;
///
/// # async fn example(db: &dyn Database) -> Result<(), switchy_database::DatabaseError> {
/// let stmt = create_table("users")
///     .if_not_exists(true)
///     .column(Column {
///         name: "id".to_string(),
///         nullable: false,
///         auto_increment: true,
///         data_type: DataType::BigInt,
///         default: None,
///     });
/// # Ok(())
/// # }
/// ```
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
    /// Sets the IF NOT EXISTS clause for the CREATE TABLE statement
    #[must_use]
    pub const fn if_not_exists(mut self, if_not_exists: bool) -> Self {
        self.if_not_exists = if_not_exists;
        self
    }

    /// Adds a single column to the table definition
    #[must_use]
    pub fn column(mut self, column: Column) -> Self {
        self.columns.push(column);
        self
    }

    /// Adds multiple columns to the table definition
    #[must_use]
    pub fn columns(mut self, columns: Vec<Column>) -> Self {
        self.columns.extend(columns);
        self
    }

    /// Sets the primary key column for the table
    #[must_use]
    pub const fn primary_key(mut self, primary_key: &'a str) -> Self {
        self.primary_key = Some(primary_key);
        self
    }

    /// Adds a foreign key constraint (column, `referenced_table.column`)
    #[must_use]
    pub fn foreign_key(mut self, foreign_key: (&'a str, &'a str)) -> Self {
        self.foreign_keys.push(foreign_key);
        self
    }

    /// Sets all foreign key constraints for the table
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

/// Specifies the behavior when dropping database objects that have dependencies.
///
/// # Examples
///
/// ```rust
/// use switchy_database::schema::{drop_table, DropBehavior};
///
/// // Explicitly fail if dependencies exist
/// let stmt = drop_table("users").restrict();
///
/// // Drop all dependent objects recursively
/// let stmt = drop_table("users").cascade();
///
/// // Use database default behavior
/// let stmt = drop_table("users"); // Default behavior
/// ```
#[cfg(feature = "cascade")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropBehavior {
    /// Use the database backend's default behavior.
    /// - `PostgreSQL`: RESTRICT (fails on dependencies)
    /// - `MySQL`: Varies (depends on `foreign_key_checks` setting)
    /// - `SQLite`: Allows drop (unless `PRAGMA foreign_keys=ON`)
    Default,

    /// Drop all dependent objects recursively.
    /// All backends use manual dependency discovery for consistent behavior.
    Cascade,

    /// Fail if any dependencies exist.
    /// - `PostgreSQL`: Uses native RESTRICT for performance
    /// - `MySQL`/`SQLite`: Uses manual dependency checking
    Restrict,
}

/// Builder for DROP TABLE SQL statements
///
/// Provides a fluent API for dropping database tables with optional IF EXISTS and
/// CASCADE/RESTRICT behaviors. Use [`drop_table`] to construct.
///
/// ## Examples
///
/// ```rust,no_run
/// use switchy_database::schema::drop_table;
/// use switchy_database::Database;
///
/// # async fn example(db: &dyn Database) -> Result<(), switchy_database::DatabaseError> {
/// // Drop a table, failing if it doesn't exist
/// drop_table("old_users")
///     .execute(db)
///     .await?;
///
/// // Drop with IF EXISTS to avoid errors
/// drop_table("maybe_exists")
///     .if_exists(true)
///     .execute(db)
///     .await?;
/// # Ok(())
/// # }
/// ```
///
/// ## CASCADE/RESTRICT Behavior
///
/// When the `cascade` feature is enabled:
///
/// ```rust,ignore
/// // Fail if table has dependencies
/// drop_table("users").restrict().execute(db).await?;
///
/// // Drop all dependent objects recursively
/// drop_table("users").cascade().execute(db).await?;
/// ```
pub struct DropTableStatement<'a> {
    pub table_name: &'a str,
    pub if_exists: bool,
    #[cfg(feature = "cascade")]
    pub behavior: DropBehavior,
}

/// Creates a new DROP TABLE statement builder
///
/// # Examples
///
/// ```rust
/// use switchy_database::schema::drop_table;
///
/// let stmt = drop_table("old_table")
///     .if_exists(true);
/// ```
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
    /// Sets the IF EXISTS clause for the DROP TABLE statement
    #[must_use]
    pub const fn if_exists(mut self, if_exists: bool) -> Self {
        self.if_exists = if_exists;
        self
    }

    /// Sets CASCADE behavior to drop all dependent objects recursively
    #[cfg(feature = "cascade")]
    #[must_use]
    pub const fn cascade(mut self) -> Self {
        self.behavior = DropBehavior::Cascade;
        self
    }

    /// Sets RESTRICT behavior to fail if any dependencies exist
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

/// Builder for CREATE INDEX SQL statements
///
/// Provides a fluent API for creating database indexes with optional uniqueness
/// constraints. Use [`create_index`] to construct.
///
/// ## Examples
///
/// ```rust,no_run
/// use switchy_database::schema::create_index;
/// use switchy_database::Database;
///
/// # async fn example(db: &dyn Database) -> Result<(), switchy_database::DatabaseError> {
/// // Create a simple index
/// create_index("idx_email")
///     .table("users")
///     .column("email")
///     .execute(db)
///     .await?;
///
/// // Create a unique index on multiple columns
/// create_index("idx_username_email")
///     .table("users")
///     .columns(vec!["username", "email"])
///     .unique(true)
///     .execute(db)
///     .await?;
/// # Ok(())
/// # }
/// ```
pub struct CreateIndexStatement<'a> {
    pub index_name: &'a str,
    pub table_name: &'a str,
    pub columns: Vec<&'a str>,
    pub unique: bool,
    pub if_not_exists: bool,
}

/// Creates a new CREATE INDEX statement builder
///
/// # Examples
///
/// ```rust
/// use switchy_database::schema::create_index;
///
/// let stmt = create_index("idx_user_email")
///     .table("users")
///     .column("email")
///     .unique(true);
/// ```
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
    /// Sets the table name for the index
    #[must_use]
    pub const fn table(mut self, table_name: &'a str) -> Self {
        self.table_name = table_name;
        self
    }

    /// Adds a single column to the index
    #[must_use]
    pub fn column(mut self, column: &'a str) -> Self {
        self.columns.push(column);
        self
    }

    /// Sets all columns for the index
    #[must_use]
    pub fn columns(mut self, columns: Vec<&'a str>) -> Self {
        self.columns = columns;
        self
    }

    /// Sets whether the index should enforce uniqueness
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

/// Builder for DROP INDEX SQL statements
///
/// Provides a fluent API for dropping database indexes. Use [`drop_index`] to construct.
///
/// ## Examples
///
/// ```rust,no_run
/// use switchy_database::schema::drop_index;
/// use switchy_database::Database;
///
/// # async fn example(db: &dyn Database) -> Result<(), switchy_database::DatabaseError> {
/// drop_index("idx_email", "users")
///     .if_exists()
///     .execute(db)
///     .await?;
/// # Ok(())
/// # }
/// ```
pub struct DropIndexStatement<'a> {
    pub index_name: &'a str,
    pub table_name: &'a str,
    pub if_exists: bool,
}

/// Creates a new DROP INDEX statement builder
///
/// # Arguments
///
/// * `index_name` - Name of the index to drop
/// * `table_name` - Name of the table containing the index
///
/// # Examples
///
/// ```rust
/// use switchy_database::schema::drop_index;
///
/// let stmt = drop_index("idx_user_email", "users")
///     .if_exists();
/// ```
#[must_use]
pub const fn drop_index<'a>(index_name: &'a str, table_name: &'a str) -> DropIndexStatement<'a> {
    DropIndexStatement {
        index_name,
        table_name,
        if_exists: false,
    }
}

impl DropIndexStatement<'_> {
    /// Sets the IF EXISTS clause to avoid errors if the index doesn't exist
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

/// Represents operations that can be performed in an ALTER TABLE statement
///
/// This enum defines the various table modification operations supported by the
/// database abstraction layer, including adding, dropping, renaming, and modifying columns.
///
/// ## Variants
///
/// * `AddColumn` - Add a new column to the table
/// * `DropColumn` - Remove an existing column (with optional CASCADE/RESTRICT behavior)
/// * `RenameColumn` - Rename an existing column
/// * `ModifyColumn` - Change column data type, nullability, or default value
///
/// ## Examples
///
/// ```rust
/// use switchy_database::schema::{AlterOperation, DataType};
/// use switchy_database::DatabaseValue;
///
/// // Add a new column
/// let add_op = AlterOperation::AddColumn {
///     name: "email".to_string(),
///     data_type: DataType::VarChar(255),
///     nullable: true,
///     default: None,
/// };
///
/// // Rename a column
/// let rename_op = AlterOperation::RenameColumn {
///     old_name: "name".to_string(),
///     new_name: "full_name".to_string(),
/// };
/// ```
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
        #[cfg(feature = "cascade")]
        behavior: DropBehavior,
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

/// Builder for ALTER TABLE SQL statements
///
/// Provides a fluent API for modifying existing database tables including adding,
/// dropping, renaming, and modifying columns. Use [`alter_table`] to construct.
///
/// ## Backend-Specific Notes
///
/// * **`SQLite`**: Limited ALTER TABLE support. Some operations use table recreation workarounds.
/// * **Column Order**: Modified columns may be added at the end of the table in `SQLite`.
/// * **Transactions**: All operations are wrapped in transactions for atomicity.
///
/// ## Examples
///
/// ```rust,no_run
/// use switchy_database::schema::{alter_table, DataType};
/// use switchy_database::{Database, DatabaseValue};
///
/// # async fn example(db: &dyn Database) -> Result<(), switchy_database::DatabaseError> {
/// // Add a column
/// alter_table("users")
///     .add_column(
///         "phone".to_string(),
///         DataType::VarChar(20),
///         true,
///         None
///     )
///     .execute(db)
///     .await?;
///
/// // Rename a column
/// alter_table("users")
///     .rename_column("name".to_string(), "full_name".to_string())
///     .execute(db)
///     .await?;
/// # Ok(())
/// # }
/// ```
pub struct AlterTableStatement<'a> {
    pub table_name: &'a str,
    pub operations: Vec<AlterOperation>,
}

/// Creates a new ALTER TABLE statement builder
///
/// # Examples
///
/// ```rust
/// use switchy_database::schema::{alter_table, DataType};
///
/// let stmt = alter_table("users")
///     .add_column("email".to_string(), DataType::VarChar(255), true, None);
/// ```
#[must_use]
pub const fn alter_table(table_name: &str) -> AlterTableStatement<'_> {
    AlterTableStatement {
        table_name,
        operations: vec![],
    }
}

impl AlterTableStatement<'_> {
    /// Adds a new column to the table
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

    /// Removes a column from the table
    #[must_use]
    pub fn drop_column(mut self, name: String) -> Self {
        self.operations.push(AlterOperation::DropColumn {
            name,
            #[cfg(feature = "cascade")]
            behavior: DropBehavior::Default,
        });
        self
    }

    /// Removes a column and all dependent objects recursively
    #[cfg(feature = "cascade")]
    #[must_use]
    pub fn drop_column_cascade(mut self, name: String) -> Self {
        self.operations.push(AlterOperation::DropColumn {
            name,
            behavior: DropBehavior::Cascade,
        });
        self
    }

    /// Removes a column, failing if any dependencies exist
    #[cfg(feature = "cascade")]
    #[must_use]
    pub fn drop_column_restrict(mut self, name: String) -> Self {
        self.operations.push(AlterOperation::DropColumn {
            name,
            behavior: DropBehavior::Restrict,
        });
        self
    }

    /// Renames an existing column
    #[must_use]
    pub fn rename_column(mut self, old_name: String, new_name: String) -> Self {
        self.operations
            .push(AlterOperation::RenameColumn { old_name, new_name });
        self
    }

    /// Modifies a column's data type, nullability, or default value
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
            AlterOperation::DropColumn {
                name,
                #[cfg(feature = "cascade")]
                behavior,
            } => {
                assert_eq!(name, "old_column");
                #[cfg(feature = "cascade")]
                assert_eq!(*behavior, DropBehavior::Default);
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
            Some(DatabaseValue::Int64(0)),
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
                assert!(matches!(new_default, Some(DatabaseValue::Int64(0))));
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
            AlterOperation::DropColumn {
                name,
                #[cfg(feature = "cascade")]
                behavior,
            } => {
                assert_eq!(name, "old_field");
                #[cfg(feature = "cascade")]
                assert_eq!(*behavior, DropBehavior::Default);
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
            default: Some(DatabaseValue::Int64(42)),
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
                        Some(DatabaseValue::Int64(42)),
                        Some(DatabaseValue::Int64(42))
                    )
                ));
            }
            _ => panic!("Clone should match original"),
        }
    }

    #[test]
    #[cfg(feature = "cascade")]
    fn test_drop_column_cascade() {
        let statement = alter_table("users").drop_column_cascade("old_column".to_string());

        assert_eq!(statement.table_name, "users");
        assert_eq!(statement.operations.len(), 1);
        match &statement.operations[0] {
            AlterOperation::DropColumn { name, behavior } => {
                assert_eq!(name, "old_column");
                assert_eq!(*behavior, DropBehavior::Cascade);
            }
            _ => panic!("Expected DropColumn operation with CASCADE behavior"),
        }
    }

    #[test]
    #[cfg(feature = "cascade")]
    fn test_drop_column_restrict() {
        let statement = alter_table("users").drop_column_restrict("old_column".to_string());

        assert_eq!(statement.table_name, "users");
        assert_eq!(statement.operations.len(), 1);
        match &statement.operations[0] {
            AlterOperation::DropColumn { name, behavior } => {
                assert_eq!(name, "old_column");
                assert_eq!(*behavior, DropBehavior::Restrict);
            }
            _ => panic!("Expected DropColumn operation with RESTRICT behavior"),
        }
    }

    #[test]
    #[cfg(feature = "cascade")]
    fn test_multiple_drop_column_behaviors() {
        let statement = alter_table("users")
            .drop_column("default_col".to_string())
            .drop_column_cascade("cascade_col".to_string())
            .drop_column_restrict("restrict_col".to_string());

        assert_eq!(statement.table_name, "users");
        assert_eq!(statement.operations.len(), 3);

        // Check default behavior
        match &statement.operations[0] {
            AlterOperation::DropColumn { name, behavior } => {
                assert_eq!(name, "default_col");
                assert_eq!(*behavior, DropBehavior::Default);
            }
            _ => panic!("Expected DropColumn operation with Default behavior"),
        }

        // Check CASCADE behavior
        match &statement.operations[1] {
            AlterOperation::DropColumn { name, behavior } => {
                assert_eq!(name, "cascade_col");
                assert_eq!(*behavior, DropBehavior::Cascade);
            }
            _ => panic!("Expected DropColumn operation with CASCADE behavior"),
        }

        // Check RESTRICT behavior
        match &statement.operations[2] {
            AlterOperation::DropColumn { name, behavior } => {
                assert_eq!(name, "restrict_col");
                assert_eq!(*behavior, DropBehavior::Restrict);
            }
            _ => panic!("Expected DropColumn operation with RESTRICT behavior"),
        }
    }
}

// Dependency management for CASCADE and RESTRICT operations
#[cfg(feature = "schema")]
pub mod dependencies;

#[cfg(feature = "schema")]
pub use dependencies::{
    ColumnDependencies, CycleError, DependencyGraph, DropPlan, get_column_dependencies,
};

#[cfg(feature = "auto-reverse")]
pub mod auto_reversible;

#[cfg(feature = "auto-reverse")]
pub use auto_reversible::AutoReversible;
