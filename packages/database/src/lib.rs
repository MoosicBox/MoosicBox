//! Database Abstraction Layer for Switchy Ecosystem
//!
//! This crate provides a unified interface for working with multiple database backends
//! including `SQLite` (rusqlite and sqlx), `PostgreSQL` (raw and sqlx), and `MySQL` (sqlx).
//!
//! ## Features
//!
//! * **Multiple Backend Support**: `SQLite`, `PostgreSQL`, `MySQL` with consistent APIs
//! * **Query Builder**: Type-safe query construction for common operations
//! * **Schema Management**: Create/alter tables, indexes with portable definitions
//! * **Schema Introspection**: Query existing database structure programmatically
//! * **Transaction Support**: Safe transaction handling across all backends
//! * **Connection Pooling**: Efficient connection management for concurrent operations
//!
//! ## Schema Introspection
//!
//! The schema introspection capabilities allow you to programmatically examine
//! your database structure. This is particularly useful for:
//!
//! * **Migration Systems**: Check if tables/columns exist before creating them
//! * **Dynamic Schema Validation**: Ensure your code matches the database structure
//! * **Database Documentation**: Generate schema documentation automatically
//! * **Cross-Backend Compatibility**: Handle backend differences transparently
//!
//! ### Core Introspection Methods
//!
//! ```rust,no_run
//! # #[cfg(feature = "schema")]
//! # {
//! # use switchy_database::{Database, DatabaseError};
//! # async fn example(db: &dyn Database) -> Result<(), DatabaseError> {
//! // Check if a table exists before creating it
//! if !db.table_exists("users").await? {
//!     // Create the table...
//! }
//!
//! // Check if a column exists before adding it
//! if !db.column_exists("users", "email").await? {
//!     // Add the column...
//! }
//!
//! // Get complete table information
//! if let Some(table_info) = db.get_table_info("users").await? {
//!     for (column_name, column_info) in &table_info.columns {
//!         println!("Column {}: {:?}", column_name, column_info.data_type);
//!     }
//! }
//!
//! // Get just the columns
//! let columns = db.get_table_columns("users").await?;
//! for column in columns {
//!     println!("Column: {} ({})", column.name,
//!              if column.nullable { "NULL" } else { "NOT NULL" });
//! }
//! # Ok(())
//! # }
//! # }
//! ```
//!
//! ### Backend-Specific Type Mappings
//!
//! Each database backend maps its native types to our common [`schema::DataType`] enum:
//!
//! | `DataType` | `SQLite` | `PostgreSQL` | `MySQL` |
//! |----------|--------|------------|-------|
//! | `Text` | `TEXT` | `TEXT` | `TEXT` |
//! | `VarChar(n)` | `VARCHAR(n)` | `VARCHAR(n)` | `VARCHAR(n)` |
//! | `Bool` | `BOOLEAN` | `BOOLEAN` | `BOOLEAN` |
//! | `Int` | `INTEGER` | `INTEGER` | `INT` |
//! | `BigInt` | `BIGINT` | `BIGINT` | `BIGINT` |
//! | `Real` | `REAL` | `REAL` | `FLOAT` |
//! | `Double` | `DOUBLE` | `DOUBLE PRECISION` | `DOUBLE` |
//! | `DateTime` | `DATETIME` | `TIMESTAMP` | `DATETIME` |
//!
//! ### Known Limitations
//!
//! Schema introspection has some limitations that vary by backend:
//!
//! * **Computed/Generated Columns**: Not currently supported for introspection
//! * **Complex Default Values**: Function calls and expressions may not be parsed correctly
//! * **Custom Types**: User-defined types map to closest standard type
//! * **Views**: Currently not supported - only tables are introspected
//! * **Triggers**: Trigger information is not included in table info
//!
//! ### Common Pitfalls
//!
//! * **Case Sensitivity**: `PostgreSQL` folds unquoted identifiers to lowercase
//! * **Schema Awareness**: `PostgreSQL` searches `search_path`, others use default schema
//! * **Auto-increment Detection**: Implementation varies significantly between backends
//! * **NULL vs NOT NULL**: `SQLite` PRIMARY KEY doesn't imply NOT NULL (unlike other DBs)
//!
//! ## Example: Migration-Safe Table Creation
//!
//!
//! ```rust,no_run
//! # #[cfg(feature = "schema")]
//! # {
//! use switchy_database::{Database, DatabaseError, schema::{create_table, Column, DataType}};
//!
//! async fn ensure_users_table(db: &dyn Database) -> Result<(), DatabaseError> {
//!     // Check if table exists first
//!     if db.table_exists("users").await? {
//!         // Table exists - check if we need to add columns
//!         if !db.column_exists("users", "email").await? {
//!             // Add email column - you'd use ALTER TABLE here
//!         }
//!         return Ok(());
//!     }
//!
//!     // Create the table from scratch
//!     create_table("users")
//!         .column(Column {
//!             name: "id".to_string(),
//!             nullable: false,
//!             auto_increment: true,
//!             data_type: DataType::BigInt,
//!             default: None,
//!         })
//!         .column(Column {
//!             name: "username".to_string(),
//!             nullable: false,
//!             auto_increment: false,
//!             data_type: DataType::VarChar(50),
//!             default: None,
//!         })
//!         .column(Column {
//!             name: "email".to_string(),
//!             nullable: true,
//!             auto_increment: false,
//!             data_type: DataType::VarChar(255),
//!             default: None,
//!         })
//!         .primary_key("id")
//!         .execute(db)
//!         .await
//! }
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

/// Database configuration and initialization
pub mod config;
/// Trait for executing database operations
pub mod executable;
#[cfg(feature = "postgres-raw")]
/// `PostgreSQL` database backend implementation
pub mod postgres;
/// Database profiles management for multi-database support
pub mod profiles;
#[cfg(feature = "sqlite-rusqlite")]
/// SQLite database backend using rusqlite
pub mod rusqlite;
#[cfg(feature = "simulator")]
/// Database simulator for testing
pub mod simulator;
#[cfg(feature = "sqlx")]
/// Database backends using `SQLx` library
pub mod sqlx;
#[cfg(feature = "turso")]
/// Turso database backend
pub mod turso;

/// SQL query builder types and builders
pub mod query;
pub mod query_transform;
pub mod sql_interval;
pub mod value_builders;

#[cfg(feature = "schema")]
pub mod schema;

use std::{num::TryFromIntError, sync::Arc};

use async_trait::async_trait;
use chrono::NaiveDateTime;

use crate::sql_interval::SqlInterval;
use query::{
    DeleteStatement, InsertStatement, SelectQuery, UpdateStatement, UpsertMultiStatement,
    UpsertStatement,
};
use thiserror::Error;

/// Represents values that can be stored in or retrieved from a database
///
/// This enum provides a unified type for all database values, handling both
/// nullable and non-nullable variants of each type. It supports conversion
/// to and from Rust primitive types.
#[derive(Debug, Clone, PartialEq)]
pub enum DatabaseValue {
    /// SQL NULL value
    Null,
    /// Non-nullable string
    String(String),
    /// Nullable string
    StringOpt(Option<String>),
    /// Non-nullable boolean
    Bool(bool),
    /// Nullable boolean
    BoolOpt(Option<bool>),
    /// Non-nullable 8-bit signed integer
    Int8(i8),
    /// Nullable 8-bit signed integer
    Int8Opt(Option<i8>),
    /// Non-nullable 16-bit signed integer
    Int16(i16),
    /// Nullable 16-bit signed integer
    Int16Opt(Option<i16>),
    /// Non-nullable 32-bit signed integer
    Int32(i32),
    /// Nullable 32-bit signed integer
    Int32Opt(Option<i32>),
    /// Non-nullable 64-bit signed integer
    Int64(i64),
    /// Nullable 64-bit signed integer
    Int64Opt(Option<i64>),
    /// Non-nullable 8-bit unsigned integer
    UInt8(u8),
    /// Nullable 8-bit unsigned integer
    UInt8Opt(Option<u8>),
    /// Non-nullable 16-bit unsigned integer
    UInt16(u16),
    /// Nullable 16-bit unsigned integer
    UInt16Opt(Option<u16>),
    /// Non-nullable 32-bit unsigned integer
    UInt32(u32),
    /// Nullable 32-bit unsigned integer
    UInt32Opt(Option<u32>),
    /// Non-nullable 64-bit unsigned integer
    UInt64(u64),
    /// Nullable 64-bit unsigned integer
    UInt64Opt(Option<u64>),
    /// Non-nullable 64-bit floating point
    Real64(f64),
    /// Nullable 64-bit floating point
    Real64Opt(Option<f64>),
    /// Non-nullable 32-bit floating point
    Real32(f32),
    /// Nullable 32-bit floating point
    Real32Opt(Option<f32>),
    #[cfg(feature = "decimal")]
    /// Non-nullable decimal number (requires decimal feature)
    Decimal(rust_decimal::Decimal),
    #[cfg(feature = "decimal")]
    /// Nullable decimal number (requires decimal feature)
    DecimalOpt(Option<rust_decimal::Decimal>),
    #[cfg(feature = "uuid")]
    /// Non-nullable UUID (requires uuid feature)
    Uuid(uuid::Uuid),
    #[cfg(feature = "uuid")]
    /// Nullable UUID (requires uuid feature)
    UuidOpt(Option<uuid::Uuid>),
    /// Current timestamp plus an interval (generates SQL expression)
    NowPlus(SqlInterval),
    /// Current timestamp (generates SQL `NOW()` function)
    Now,
    /// Specific date/time value
    DateTime(NaiveDateTime),
}

impl DatabaseValue {
    /// Extracts a string reference if this value is a string type
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) | Self::StringOpt(Some(value)) => Some(value),
            _ => None,
        }
    }

    /// Extracts an i8 value if this value is an `Int8` type
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn as_i8(&self) -> Option<i8> {
        match self {
            Self::Int8(value) | Self::Int8Opt(Some(value)) => Some(*value),
            _ => None,
        }
    }

    /// Extracts an i16 value if this value is an integer type (coerces i8 to i16)
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn as_i16(&self) -> Option<i16> {
        match self {
            Self::Int8(value) | Self::Int8Opt(Some(value)) => Some(i16::from(*value)),
            Self::Int16(value) | Self::Int16Opt(Some(value)) => Some(*value),
            _ => None,
        }
    }

    /// Extracts an i32 value if this value is an integer type (coerces smaller integers to i32)
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn as_i32(&self) -> Option<i32> {
        match self {
            Self::Int8(value) | Self::Int8Opt(Some(value)) => Some(i32::from(*value)),
            Self::Int16(value) | Self::Int16Opt(Some(value)) => Some(i32::from(*value)),
            Self::Int32(value) | Self::Int32Opt(Some(value)) => Some(*value),
            _ => None,
        }
    }

    /// Extracts an i64 value if this value is an integer type (coerces smaller integers to i64)
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int8(value) | Self::Int8Opt(Some(value)) => Some(i64::from(*value)),
            Self::Int16(value) | Self::Int16Opt(Some(value)) => Some(i64::from(*value)),
            Self::Int32(value) | Self::Int32Opt(Some(value)) => Some(i64::from(*value)),
            Self::Int64(value) | Self::Int64Opt(Some(value)) => Some(*value),
            _ => None,
        }
    }

    /// # Panics
    ///
    /// * If the value is an i64 and is negative
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::UInt8(value) | Self::UInt8Opt(Some(value)) => Some(u64::from(*value)),
            Self::UInt16(value) | Self::UInt16Opt(Some(value)) => Some(u64::from(*value)),
            Self::UInt32(value) | Self::UInt32Opt(Some(value)) => Some(u64::from(*value)),
            Self::UInt64(value) | Self::UInt64Opt(Some(value)) => Some(*value),
            Self::Int8(value) | Self::Int8Opt(Some(value)) => Some(
                #[allow(clippy::cast_sign_loss)]
                if *value >= 0 {
                    *value as u64
                } else {
                    panic!("DatabaseValue::as_u64: value is negative")
                },
            ),
            Self::Int16(value) | Self::Int16Opt(Some(value)) => Some(
                #[allow(clippy::cast_sign_loss)]
                if *value >= 0 {
                    *value as u64
                } else {
                    panic!("DatabaseValue::as_u64: value is negative")
                },
            ),
            Self::Int32(value) | Self::Int32Opt(Some(value)) => Some(
                #[allow(clippy::cast_sign_loss)]
                if *value >= 0 {
                    *value as u64
                } else {
                    panic!("DatabaseValue::as_u64: value is negative")
                },
            ),
            Self::Int64(value) | Self::Int64Opt(Some(value)) => Some(
                #[allow(clippy::cast_sign_loss)]
                if *value >= 0 {
                    *value as u64
                } else {
                    panic!("DatabaseValue::as_u64: value is negative")
                },
            ),
            _ => None,
        }
    }

    /// Extracts an f64 value if this value is a floating point type (coerces f32 to f64)
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Real32(value) | Self::Real32Opt(Some(value)) => Some(f64::from(*value)),
            Self::Real64(value) | Self::Real64Opt(Some(value)) => Some(*value),
            _ => None,
        }
    }

    /// Extracts an f32 value if this value is a `Real32` type
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn as_f32(&self) -> Option<f32> {
        match self {
            Self::Real32(value) | Self::Real32Opt(Some(value)) => Some(*value),
            _ => None,
        }
    }

    /// Extracts a `Decimal` value if this value is a decimal type or can be parsed as one
    #[cfg(feature = "decimal")]
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn as_decimal(&self) -> Option<rust_decimal::Decimal> {
        match self {
            Self::String(value) | Self::StringOpt(Some(value)) => {
                value.parse::<rust_decimal::Decimal>().ok()
            }
            Self::Decimal(value) | Self::DecimalOpt(Some(value)) => Some(*value),
            _ => None,
        }
    }

    /// Extracts a `UUID` value if this value is a `UUID` type or can be parsed as one
    #[cfg(feature = "uuid")]
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn as_uuid(&self) -> Option<uuid::Uuid> {
        match self {
            Self::String(value) | Self::StringOpt(Some(value)) => value.parse::<uuid::Uuid>().ok(),
            Self::Uuid(value) | Self::UuidOpt(Some(value)) => Some(*value),
            _ => None,
        }
    }

    /// Extracts a `NaiveDateTime` value if this value is a `DateTime` type
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn as_datetime(&self) -> Option<NaiveDateTime> {
        match self {
            Self::DateTime(value) => Some(*value),
            _ => None,
        }
    }

    /// Extracts a boolean value if this value is a `Bool` type
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) | Self::BoolOpt(Some(value)) => Some(*value),
            _ => None,
        }
    }

    /// Extracts a u8 value if this value is a `UInt8` type
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn as_u8(&self) -> Option<u8> {
        match self {
            Self::UInt8(value) | Self::UInt8Opt(Some(value)) => Some(*value),
            _ => None,
        }
    }

    /// Extracts a u16 value if this value is an unsigned integer type (coerces u8 to u16)
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn as_u16(&self) -> Option<u16> {
        match self {
            Self::UInt8(value) | Self::UInt8Opt(Some(value)) => Some(u16::from(*value)),
            Self::UInt16(value) | Self::UInt16Opt(Some(value)) => Some(*value),
            _ => None,
        }
    }

    /// Extracts a u32 value if this value is an unsigned integer type (coerces smaller unsigned integers to u32)
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn as_u32(&self) -> Option<u32> {
        match self {
            Self::UInt8(value) | Self::UInt8Opt(Some(value)) => Some(u32::from(*value)),
            Self::UInt16(value) | Self::UInt16Opt(Some(value)) => Some(u32::from(*value)),
            Self::UInt32(value) | Self::UInt32Opt(Some(value)) => Some(*value),
            _ => None,
        }
    }
}

impl<T: Into<Self>> From<Option<T>> for DatabaseValue {
    fn from(val: Option<T>) -> Self {
        val.map_or(Self::Null, std::convert::Into::into)
    }
}

impl From<bool> for DatabaseValue {
    fn from(val: bool) -> Self {
        Self::Bool(val)
    }
}

impl From<&str> for DatabaseValue {
    fn from(val: &str) -> Self {
        Self::String(val.to_string())
    }
}

impl From<&String> for DatabaseValue {
    fn from(val: &String) -> Self {
        Self::String(val.clone())
    }
}

impl From<String> for DatabaseValue {
    fn from(val: String) -> Self {
        Self::String(val)
    }
}

impl From<f32> for DatabaseValue {
    fn from(val: f32) -> Self {
        Self::Real32(val)
    }
}

impl From<f64> for DatabaseValue {
    fn from(val: f64) -> Self {
        Self::Real64(val)
    }
}

#[cfg(feature = "decimal")]
impl From<rust_decimal::Decimal> for DatabaseValue {
    fn from(val: rust_decimal::Decimal) -> Self {
        Self::Decimal(val)
    }
}

impl From<i8> for DatabaseValue {
    fn from(val: i8) -> Self {
        Self::Int8(val)
    }
}

#[cfg(feature = "uuid")]
impl From<uuid::Uuid> for DatabaseValue {
    fn from(val: uuid::Uuid) -> Self {
        Self::Uuid(val)
    }
}

impl From<i16> for DatabaseValue {
    fn from(val: i16) -> Self {
        Self::Int16(val)
    }
}

impl From<i32> for DatabaseValue {
    fn from(val: i32) -> Self {
        Self::Int32(val)
    }
}

impl From<i64> for DatabaseValue {
    fn from(val: i64) -> Self {
        Self::Int64(val)
    }
}

impl From<isize> for DatabaseValue {
    fn from(val: isize) -> Self {
        Self::Int64(val as i64)
    }
}

impl From<u8> for DatabaseValue {
    fn from(val: u8) -> Self {
        Self::UInt8(val)
    }
}

impl From<u16> for DatabaseValue {
    fn from(val: u16) -> Self {
        Self::UInt16(val)
    }
}

impl From<u32> for DatabaseValue {
    fn from(val: u32) -> Self {
        Self::UInt32(val)
    }
}

impl From<u64> for DatabaseValue {
    fn from(val: u64) -> Self {
        Self::UInt64(val)
    }
}

impl From<usize> for DatabaseValue {
    fn from(val: usize) -> Self {
        Self::UInt64(val as u64)
    }
}

/// Trait for types that can be converted to a database ID value
pub trait AsId {
    /// Converts this value to a `DatabaseValue` representing an ID
    fn as_id(&self) -> DatabaseValue;
}

/// Error type for `DatabaseValue` type conversions
#[derive(Debug, Error)]
pub enum TryFromError {
    /// Failed to convert `DatabaseValue` to target type
    #[error("Could not convert to type '{0}'")]
    CouldNotConvert(String),
    /// Integer conversion overflow error
    #[error(transparent)]
    TryFromInt(#[from] TryFromIntError),
}

impl TryFrom<DatabaseValue> for u8 {
    type Error = TryFromError;

    fn try_from(value: DatabaseValue) -> Result<Self, Self::Error> {
        match value {
            DatabaseValue::UInt8(value) | DatabaseValue::UInt8Opt(Some(value)) => Ok(value),
            DatabaseValue::UInt16(value) | DatabaseValue::UInt16Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::UInt32(value) | DatabaseValue::UInt32Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::UInt64(value) | DatabaseValue::UInt64Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::Int8(value) | DatabaseValue::Int8Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::Int16(value) | DatabaseValue::Int16Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::Int32(value) | DatabaseValue::Int32Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::Int64(value) | DatabaseValue::Int64Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            _ => Err(TryFromError::CouldNotConvert("u8".into())),
        }
    }
}

impl TryFrom<DatabaseValue> for u16 {
    type Error = TryFromError;

    fn try_from(value: DatabaseValue) -> Result<Self, Self::Error> {
        match value {
            DatabaseValue::UInt8(value) | DatabaseValue::UInt8Opt(Some(value)) => {
                Ok(Self::from(value))
            }
            DatabaseValue::UInt16(value) | DatabaseValue::UInt16Opt(Some(value)) => Ok(value),
            DatabaseValue::UInt32(value) | DatabaseValue::UInt32Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::UInt64(value) | DatabaseValue::UInt64Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::Int8(value) | DatabaseValue::Int8Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::Int16(value) | DatabaseValue::Int16Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::Int32(value) | DatabaseValue::Int32Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::Int64(value) | DatabaseValue::Int64Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            _ => Err(TryFromError::CouldNotConvert("u16".into())),
        }
    }
}

impl TryFrom<DatabaseValue> for u32 {
    type Error = TryFromError;

    fn try_from(value: DatabaseValue) -> Result<Self, Self::Error> {
        match value {
            DatabaseValue::UInt8(value) | DatabaseValue::UInt8Opt(Some(value)) => {
                Ok(Self::from(value))
            }
            DatabaseValue::UInt16(value) | DatabaseValue::UInt16Opt(Some(value)) => {
                Ok(Self::from(value))
            }
            DatabaseValue::UInt32(value) | DatabaseValue::UInt32Opt(Some(value)) => Ok(value),
            DatabaseValue::UInt64(value) | DatabaseValue::UInt64Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::Int8(value) | DatabaseValue::Int8Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::Int16(value) | DatabaseValue::Int16Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::Int32(value) | DatabaseValue::Int32Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::Int64(value) | DatabaseValue::Int64Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            _ => Err(TryFromError::CouldNotConvert("u32".into())),
        }
    }
}

impl TryFrom<DatabaseValue> for u64 {
    type Error = TryFromError;

    fn try_from(value: DatabaseValue) -> Result<Self, Self::Error> {
        match value {
            DatabaseValue::UInt8(value) | DatabaseValue::UInt8Opt(Some(value)) => {
                Ok(Self::from(value))
            }
            DatabaseValue::UInt16(value) | DatabaseValue::UInt16Opt(Some(value)) => {
                Ok(Self::from(value))
            }
            DatabaseValue::UInt32(value) | DatabaseValue::UInt32Opt(Some(value)) => {
                Ok(Self::from(value))
            }
            DatabaseValue::UInt64(value) | DatabaseValue::UInt64Opt(Some(value)) => Ok(value),
            DatabaseValue::Int8(value) | DatabaseValue::Int8Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::Int16(value) | DatabaseValue::Int16Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::Int32(value) | DatabaseValue::Int32Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::Int64(value) | DatabaseValue::Int64Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            _ => Err(TryFromError::CouldNotConvert("u64".into())),
        }
    }
}

impl TryFrom<DatabaseValue> for i64 {
    type Error = TryFromError;

    fn try_from(value: DatabaseValue) -> Result<Self, Self::Error> {
        match value {
            DatabaseValue::Int8(value) | DatabaseValue::Int8Opt(Some(value)) => {
                Ok(Self::from(value))
            }
            DatabaseValue::Int16(value) | DatabaseValue::Int16Opt(Some(value)) => {
                Ok(Self::from(value))
            }
            DatabaseValue::Int32(value) | DatabaseValue::Int32Opt(Some(value)) => {
                Ok(Self::from(value))
            }
            DatabaseValue::Int64(value) | DatabaseValue::Int64Opt(Some(value)) => Ok(value),
            DatabaseValue::UInt64(value) | DatabaseValue::UInt64Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            _ => Err(TryFromError::CouldNotConvert("i64".into())),
        }
    }
}

impl TryFrom<DatabaseValue> for i8 {
    type Error = TryFromError;

    fn try_from(value: DatabaseValue) -> Result<Self, Self::Error> {
        match value {
            DatabaseValue::Int8(value) | DatabaseValue::Int8Opt(Some(value)) => Ok(value),
            DatabaseValue::Int16(value) | DatabaseValue::Int16Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::Int32(value) | DatabaseValue::Int32Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::Int64(value) | DatabaseValue::Int64Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::UInt64(value) | DatabaseValue::UInt64Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            _ => Err(TryFromError::CouldNotConvert("i8".into())),
        }
    }
}

impl TryFrom<DatabaseValue> for i16 {
    type Error = TryFromError;

    fn try_from(value: DatabaseValue) -> Result<Self, Self::Error> {
        match value {
            DatabaseValue::Int8(value) | DatabaseValue::Int8Opt(Some(value)) => {
                Ok(Self::from(value))
            }
            DatabaseValue::Int16(value) | DatabaseValue::Int16Opt(Some(value)) => Ok(value),
            DatabaseValue::Int32(value) | DatabaseValue::Int32Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::Int64(value) | DatabaseValue::Int64Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::UInt64(value) | DatabaseValue::UInt64Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            _ => Err(TryFromError::CouldNotConvert("i16".into())),
        }
    }
}

impl TryFrom<DatabaseValue> for i32 {
    type Error = TryFromError;

    fn try_from(value: DatabaseValue) -> Result<Self, Self::Error> {
        match value {
            DatabaseValue::Int8(value) | DatabaseValue::Int8Opt(Some(value)) => {
                Ok(Self::from(value))
            }
            DatabaseValue::Int16(value) | DatabaseValue::Int16Opt(Some(value)) => {
                Ok(Self::from(value))
            }
            DatabaseValue::Int32(value) | DatabaseValue::Int32Opt(Some(value)) => Ok(value),
            DatabaseValue::Int64(value) | DatabaseValue::Int64Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::UInt64(value) | DatabaseValue::UInt64Opt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            _ => Err(TryFromError::CouldNotConvert("i32".into())),
        }
    }
}

impl TryFrom<DatabaseValue> for f32 {
    type Error = TryFromError;

    fn try_from(value: DatabaseValue) -> Result<Self, Self::Error> {
        match value {
            DatabaseValue::Real32(value) | DatabaseValue::Real32Opt(Some(value)) => Ok(value),
            #[allow(clippy::cast_possible_truncation)]
            DatabaseValue::Real64(value) | DatabaseValue::Real64Opt(Some(value)) => {
                Ok(value as Self)
            }
            _ => Err(TryFromError::CouldNotConvert("f32".into())),
        }
    }
}

impl TryFrom<DatabaseValue> for f64 {
    type Error = TryFromError;

    fn try_from(value: DatabaseValue) -> Result<Self, Self::Error> {
        match value {
            DatabaseValue::Real64(value) | DatabaseValue::Real64Opt(Some(value)) => Ok(value),
            DatabaseValue::Real32(value) | DatabaseValue::Real32Opt(Some(value)) => {
                Ok(Self::from(value))
            }
            _ => Err(TryFromError::CouldNotConvert("f64".into())),
        }
    }
}

#[cfg(feature = "decimal")]
impl TryFrom<DatabaseValue> for rust_decimal::Decimal {
    type Error = TryFromError;

    fn try_from(value: DatabaseValue) -> Result<Self, Self::Error> {
        match value {
            DatabaseValue::Decimal(value) | DatabaseValue::DecimalOpt(Some(value)) => Ok(value),
            _ => Err(TryFromError::CouldNotConvert("Decimal".into())),
        }
    }
}

#[cfg(feature = "uuid")]
impl TryFrom<DatabaseValue> for uuid::Uuid {
    type Error = TryFromError;

    fn try_from(value: DatabaseValue) -> Result<Self, Self::Error> {
        match value {
            DatabaseValue::Uuid(value) | DatabaseValue::UuidOpt(Some(value)) => Ok(value),
            _ => Err(TryFromError::CouldNotConvert("Uuid".into())),
        }
    }
}

/// Errors that can occur during database operations
#[derive(Debug, Error)]
pub enum DatabaseError {
    #[cfg(feature = "sqlite-rusqlite")]
    /// Error from rusqlite `SQLite` backend
    #[error(transparent)]
    Rusqlite(rusqlite::RusqliteDatabaseError),
    #[cfg(feature = "mysql-sqlx")]
    /// Error from sqlx `MySQL` backend
    #[error(transparent)]
    MysqlSqlx(sqlx::mysql::SqlxDatabaseError),
    #[cfg(feature = "sqlite-sqlx")]
    /// Error from sqlx `SQLite` backend
    #[error(transparent)]
    SqliteSqlx(sqlx::sqlite::SqlxDatabaseError),
    #[cfg(feature = "postgres-raw")]
    /// Error from raw `PostgreSQL` backend
    #[error(transparent)]
    Postgres(postgres::postgres::PostgresDatabaseError),
    #[cfg(feature = "postgres-sqlx")]
    /// Error from sqlx `PostgreSQL` backend
    #[error(transparent)]
    PostgresSqlx(sqlx::postgres::SqlxDatabaseError),
    #[cfg(feature = "turso")]
    /// Error from Turso backend
    #[error(transparent)]
    Turso(#[from] turso::TursoDatabaseError),
    /// Query returned no rows when at least one was expected
    #[error("No row")]
    NoRow,
    #[cfg(feature = "schema")]
    /// Schema definition is invalid
    #[error("Invalid schema: {0}")]
    InvalidSchema(String),
    /// Attempted to start a transaction while already in one
    #[error("Already in transaction - nested transactions not supported")]
    AlreadyInTransaction,
    /// Transaction has already been committed
    #[error("Transaction already committed")]
    TransactionCommitted,
    /// Transaction has already been rolled back
    #[error("Transaction already rolled back")]
    TransactionRolledBack,
    /// Transaction failed to start
    #[error("Transaction failed to start")]
    TransactionFailed,
    /// Operation returned unexpected result
    #[error("Unexpected result from operation")]
    UnexpectedResult,
    /// Database backend does not support this data type
    #[error("Unsupported data type: {0}")]
    UnsupportedDataType(String),
    /// Invalid savepoint name (contains invalid characters or empty)
    #[error("Invalid savepoint name: {0}")]
    InvalidSavepointName(String),
    /// Savepoint with this name already exists
    #[error("Savepoint already exists: {0}")]
    SavepointExists(String),
    /// Savepoint not found for rollback/release
    #[error("Savepoint not found: {0}")]
    SavepointNotFound(String),
    /// Raw SQL query execution failed
    #[error("Query failed: {0}")]
    QueryFailed(String),
    /// Foreign key constraint violation
    #[error("Foreign key violation: {0}")]
    ForeignKeyViolation(String),
    /// Invalid query syntax
    #[error("Invalid query: {0}")]
    InvalidQuery(String),
    /// Unsupported operation by backend
    #[error("Unsupported operation: {0}")]
    UnsupportedOperation(String),
    /// `UInt8` value overflow (value > `i8::MAX` on `PostgreSQL`/`SQLite`)
    #[error("UInt8 overflow: value {0} exceeds i8::MAX (127) for this database backend")]
    UInt8Overflow(u8),
    /// `UInt16` value overflow (value > `i16::MAX` on `PostgreSQL`/`SQLite`)
    #[error("UInt16 overflow: value {0} exceeds i16::MAX (32767) for this database backend")]
    UInt16Overflow(u16),
    /// `UInt32` value overflow (value > `i32::MAX` on `PostgreSQL`/`SQLite`)
    #[error("UInt32 overflow: value {0} exceeds i32::MAX (2147483647) for this database backend")]
    UInt32Overflow(u32),
}

impl DatabaseError {
    /// Checks if this error is a database connection error
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn is_connection_error(&self) -> bool {
        match &self {
            #[cfg(feature = "postgres-sqlx")]
            Self::PostgresSqlx(sqlx::postgres::SqlxDatabaseError::Sqlx(::sqlx::Error::Io(
                _io_err,
            ))) => true,
            #[cfg(feature = "mysql-sqlx")]
            Self::MysqlSqlx(sqlx::mysql::SqlxDatabaseError::Sqlx(::sqlx::Error::Io(_io_err))) => {
                true
            }
            #[cfg(feature = "sqlite-sqlx")]
            Self::SqliteSqlx(sqlx::sqlite::SqlxDatabaseError::Sqlx(::sqlx::Error::Io(_io_err))) => {
                true
            }
            #[cfg(feature = "postgres-raw")]
            Self::Postgres(postgres::postgres::PostgresDatabaseError::Postgres(pg_err)) => {
                pg_err.to_string().as_str() == "connection closed"
            }
            #[cfg(feature = "sqlite-rusqlite")]
            Self::Rusqlite(rusqlite::RusqliteDatabaseError::Rusqlite(
                ::rusqlite::Error::SqliteFailure(_, _),
            )) => true,
            _ => false,
        }
    }
}

/// Validate savepoint name follows SQL identifier rules
#[allow(unused)]
pub(crate) fn validate_savepoint_name(name: &str) -> Result<(), DatabaseError> {
    if name.is_empty() {
        return Err(DatabaseError::InvalidSavepointName(
            "Savepoint name cannot be empty".to_string(),
        ));
    }

    // Check for valid SQL identifier characters
    if !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Err(DatabaseError::InvalidSavepointName(format!(
            "Savepoint name '{name}' contains invalid characters"
        )));
    }

    // Check doesn't start with number
    if name.chars().next().is_some_and(char::is_numeric) {
        return Err(DatabaseError::InvalidSavepointName(format!(
            "Savepoint name '{name}' cannot start with a number"
        )));
    }

    Ok(())
}

/// Represents a row of data returned from a database query
///
/// Each row contains named columns with their associated values.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// Column name-value pairs in this row
    pub columns: Vec<(String, DatabaseValue)>,
}

impl Row {
    /// Gets the value of a column by name
    #[must_use]
    pub fn get(&self, column_name: &str) -> Option<DatabaseValue> {
        self.columns
            .iter()
            .find(|c| c.0 == column_name)
            .map(|c| c.1.clone())
    }

    /// Convenience method to get the "id" column value
    #[must_use]
    pub fn id(&self) -> Option<DatabaseValue> {
        self.get("id")
    }
}

#[async_trait]
pub trait Database: Send + Sync + std::fmt::Debug {
    fn select<'a>(&self, table_name: &'a str) -> SelectQuery<'a> {
        query::select(table_name)
    }
    fn update<'a>(&self, table_name: &'a str) -> UpdateStatement<'a> {
        query::update(table_name)
    }
    fn insert<'a>(&self, table_name: &'a str) -> InsertStatement<'a> {
        query::insert(table_name)
    }
    fn upsert<'a>(&self, table_name: &'a str) -> UpsertStatement<'a> {
        query::upsert(table_name)
    }
    fn upsert_first<'a>(&self, table_name: &'a str) -> UpsertStatement<'a> {
        query::upsert(table_name)
    }
    fn upsert_multi<'a>(&self, table_name: &'a str) -> UpsertMultiStatement<'a> {
        query::upsert_multi(table_name)
    }
    fn delete<'a>(&self, table_name: &'a str) -> DeleteStatement<'a> {
        query::delete(table_name)
    }

    #[cfg(feature = "schema")]
    fn create_table<'a>(&self, table_name: &'a str) -> schema::CreateTableStatement<'a> {
        schema::create_table(table_name)
    }

    #[cfg(feature = "schema")]
    fn drop_table<'a>(&self, table_name: &'a str) -> schema::DropTableStatement<'a> {
        schema::drop_table(table_name)
    }

    #[cfg(feature = "schema")]
    fn create_index<'a>(&self, index_name: &'a str) -> schema::CreateIndexStatement<'a> {
        schema::create_index(index_name)
    }

    #[cfg(feature = "schema")]
    fn drop_index<'a>(
        &self,
        index_name: &'a str,
        table_name: &'a str,
    ) -> schema::DropIndexStatement<'a> {
        schema::drop_index(index_name, table_name)
    }

    #[cfg(feature = "schema")]
    fn alter_table<'a>(&self, table_name: &'a str) -> schema::AlterTableStatement<'a> {
        schema::alter_table(table_name)
    }

    async fn query(&self, query: &SelectQuery<'_>) -> Result<Vec<Row>, DatabaseError>;
    async fn query_first(&self, query: &SelectQuery<'_>) -> Result<Option<Row>, DatabaseError>;
    async fn exec_update(&self, statement: &UpdateStatement<'_>)
    -> Result<Vec<Row>, DatabaseError>;
    async fn exec_update_first(
        &self,
        statement: &UpdateStatement<'_>,
    ) -> Result<Option<Row>, DatabaseError>;
    async fn exec_insert(&self, statement: &InsertStatement<'_>) -> Result<Row, DatabaseError>;
    async fn exec_upsert(&self, statement: &UpsertStatement<'_>)
    -> Result<Vec<Row>, DatabaseError>;
    async fn exec_upsert_first(
        &self,
        statement: &UpsertStatement<'_>,
    ) -> Result<Row, DatabaseError>;
    async fn exec_upsert_multi(
        &self,
        statement: &UpsertMultiStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError>;
    async fn exec_delete(&self, statement: &DeleteStatement<'_>)
    -> Result<Vec<Row>, DatabaseError>;
    async fn exec_delete_first(
        &self,
        statement: &DeleteStatement<'_>,
    ) -> Result<Option<Row>, DatabaseError>;

    async fn exec_raw(&self, statement: &str) -> Result<(), DatabaseError>;

    /// # Errors
    ///
    /// Will return `Err` if the close failed to trigger.
    fn trigger_close(&self) -> Result<(), DatabaseError> {
        Ok(())
    }

    async fn close(&self) -> Result<(), DatabaseError> {
        self.trigger_close()
    }

    #[cfg(feature = "schema")]
    async fn exec_create_table(
        &self,
        statement: &schema::CreateTableStatement<'_>,
    ) -> Result<(), DatabaseError>;

    #[cfg(feature = "schema")]
    async fn exec_drop_table(
        &self,
        statement: &schema::DropTableStatement<'_>,
    ) -> Result<(), DatabaseError>;

    #[cfg(feature = "schema")]
    async fn exec_create_index(
        &self,
        statement: &schema::CreateIndexStatement<'_>,
    ) -> Result<(), DatabaseError>;

    #[cfg(feature = "schema")]
    async fn exec_drop_index(
        &self,
        statement: &schema::DropIndexStatement<'_>,
    ) -> Result<(), DatabaseError>;

    #[cfg(feature = "schema")]
    async fn exec_alter_table(
        &self,
        statement: &schema::AlterTableStatement<'_>,
    ) -> Result<(), DatabaseError>;

    /// Check if a table exists in the database
    ///
    /// This method queries the appropriate system catalog for each database backend:
    /// - **SQLite**: Queries `sqlite_master` table for table existence
    /// - **PostgreSQL**: Queries `information_schema.tables` with schema awareness (defaults to 'public')
    /// - **MySQL**: Queries `information_schema.tables` for current database
    ///
    /// # Backend-Specific Behavior
    ///
    /// - **PostgreSQL**: Only searches in 'public' schema by default
    /// - **MySQL**: Uses `DATABASE()` function to limit to current database
    /// - **SQLite**: Searches all attached databases
    ///
    /// # Errors
    ///
    /// * If the database query fails
    #[cfg(feature = "schema")]
    async fn table_exists(&self, table_name: &str) -> Result<bool, DatabaseError>;

    /// Get a list of all table names in the database
    ///
    /// This method enumerates all user tables in the database, excluding system tables
    /// and other database objects like views, indexes, or sequences.
    ///
    /// # Backend-Specific Behavior
    ///
    /// - **`SQLite`**: Queries `sqlite_master` table, excludes tables starting with `sqlite_`
    /// - **`PostgreSQL`**: Queries `pg_tables` for tables in the 'public' schema
    /// - **`MySQL`**: Queries `information_schema.tables` for the current database
    ///
    /// # Errors
    ///
    /// * If the database query fails
    /// * If there are permission issues accessing system catalogs
    #[cfg(feature = "schema")]
    async fn list_tables(&self) -> Result<Vec<String>, DatabaseError>;

    /// Get complete information about a table including columns, indexes, and foreign keys
    ///
    /// Returns `None` if the table doesn't exist. Provides comprehensive metadata including:
    /// - All column information with data types, constraints, and defaults
    /// - All indexes with column lists and uniqueness constraints
    /// - All foreign key relationships with referential actions
    ///
    /// # Data Type Mappings
    ///
    /// Each backend maps its native types to our common [`schema::DataType`] enum:
    ///
    /// | Common Type | SQLite | `PostgreSQL` | `MySQL` |
    /// |-------------|---------|------------|-------|
    /// | `BigInt` | `INTEGER` | `BIGINT`, `INT8` | `BIGINT` |
    /// | `Int` | - | `INTEGER`, `INT4` | `INT`, `INTEGER`, `MEDIUMINT` |
    /// | `SmallInt` | - | `SMALLINT`, `INT2` | `TINYINT`, `SMALLINT` |
    /// | `Text` | `TEXT` | `TEXT` | `TEXT`, `TINYTEXT`, `MEDIUMTEXT`, `LONGTEXT` |
    /// | `VarChar(n)` | - | `VARCHAR(n)` | `VARCHAR(n)`, `CHAR(n)` |
    /// | `Bool` | `BOOLEAN` | `BOOLEAN`, `BOOL` | `BOOLEAN`, `BOOL` |
    /// | `Real` | - | `REAL`, `FLOAT4` | `FLOAT` |
    /// | `Double` | `REAL` | `DOUBLE PRECISION`, `FLOAT8` | `DOUBLE`, `REAL` |
    /// | `DateTime` | - | `TIMESTAMP` (without time zone) | `DATETIME`, `TIMESTAMP`, `DATE`, `TIME` |
    /// | `Decimal(p,s)` | - | `NUMERIC`, `DECIMAL` | `DECIMAL`, `NUMERIC` |
    ///
    /// # Limitations
    ///
    /// - **Computed/Generated Columns**: Not supported for introspection
    /// - **Complex Default Values**: Function calls and expressions may not parse correctly
    /// - **Custom Types**: User-defined types map to closest standard type or return `UnsupportedDataType` error
    /// - **Views**: Only tables are introspected, not views
    /// - **Auto-increment Detection**: Limited implementation (`SQLite` requires additional parsing)
    ///
    /// # Errors
    ///
    /// * If the database query fails
    /// * If an unsupported data type is encountered (`DatabaseError::UnsupportedDataType`)
    #[cfg(feature = "schema")]
    async fn get_table_info(
        &self,
        table_name: &str,
    ) -> Result<Option<schema::TableInfo>, DatabaseError>;

    /// Get all columns for a table
    ///
    /// Returns an empty Vec if the table doesn't exist. This is a lighter-weight alternative
    /// to [`get_table_info`](Self::get_table_info) when you only need column information.
    ///
    /// # Column Information Provided
    ///
    /// Each [`schema::ColumnInfo`] includes:
    /// - **name**: Column name as stored in database
    /// - **`data_type`**: Mapped to common [`schema::DataType`] enum (see [`get_table_info`](Self::get_table_info) for mapping table)
    /// - **nullable**: Whether column allows NULL values
    /// - **`is_primary_key`**: Whether column is part of primary key
    /// - **`auto_increment`**: Whether column has auto-increment behavior (limited detection)
    /// - **`default_value`**: Parsed default value as [`DatabaseValue`] (where possible)
    /// - **`ordinal_position`**: 1-based position of column in table definition
    ///
    /// # Backend-Specific Parsing
    ///
    /// - **`SQLite`**: Uses `PRAGMA table_info()`, limited auto-increment detection
    /// - **`PostgreSQL`**: Queries `information_schema.columns` with type casting awareness
    /// - **`MySQL`**: Queries `information_schema.columns` with `EXTRA` field for auto-increment
    ///
    /// # Errors
    ///
    /// * If the database query fails
    /// * If an unsupported data type is encountered
    #[cfg(feature = "schema")]
    async fn get_table_columns(
        &self,
        table_name: &str,
    ) -> Result<Vec<schema::ColumnInfo>, DatabaseError>;

    /// Check if a column exists in a table
    ///
    /// This is a convenience method that's more efficient than getting all columns
    /// when you only need to check for existence of a specific column.
    ///
    /// # Implementation Details
    ///
    /// - **`SQLite`**: Uses `PRAGMA table_info()` and searches results
    /// - **`PostgreSQL`**: Queries `information_schema.columns` with column name filter
    /// - **`MySQL`**: Queries `information_schema.columns` with column name filter
    ///
    /// Returns `false` if either the table or column doesn't exist.
    ///
    /// # Errors
    ///
    /// * If the database query fails (but not if table/column doesn't exist)
    #[cfg(feature = "schema")]
    async fn column_exists(
        &self,
        table_name: &str,
        column_name: &str,
    ) -> Result<bool, DatabaseError>;

    /// Execute raw SQL query and return results
    /// Available on both Database and `DatabaseTransaction` traits for flexibility
    ///
    /// # Safety and Scope
    ///
    /// This method is intended for internal framework use only for performance optimization.
    /// Uses string interpolation for simplicity - parameterized queries added in Phase 15.1.5.
    ///
    /// # Backend Support
    ///
    /// All backends must implement this method. This is core Database functionality
    /// without fallback or default behavior.
    ///
    /// # Errors
    ///
    /// * Returns `DatabaseError::QueryFailed` if query execution fails
    /// * Returns `DatabaseError::InvalidQuery` for malformed SQL
    async fn query_raw(&self, query: &str) -> Result<Vec<Row>, DatabaseError>;

    /// Execute raw SQL with parameters
    /// Parameters are safely bound, preventing SQL injection
    ///
    /// # Parameters Format
    ///
    /// Parameter syntax varies by backend implementation, not just database type:
    /// * rusqlite: Uses ? placeholders (e.g., "SELECT * FROM users WHERE id = ?")
    /// * sqlx-sqlite: Uses $1, $2 placeholders (e.g., "SELECT * FROM users WHERE id = $1")
    /// * `PostgreSQL` (both native and sqlx): Uses $1, $2 placeholders (e.g., "SELECT * FROM users WHERE id = $1")
    /// * `MySQL` (sqlx): Uses ? placeholders (e.g., "SELECT * FROM users WHERE id = ?")
    ///
    /// # Errors
    ///
    /// * Returns `DatabaseError::UnsupportedOperation` if not implemented
    /// * Returns `DatabaseError::QueryFailed` if execution fails
    /// * Returns `DatabaseError::InvalidQuery` for parameter count mismatch
    async fn exec_raw_params(
        &self,
        _query: &str,
        _params: &[DatabaseValue],
    ) -> Result<u64, DatabaseError> {
        Err(DatabaseError::UnsupportedOperation(
            "exec_raw_params not implemented for this backend".to_string(),
        ))
    }

    /// Query raw SQL with parameters and return results
    ///
    /// # Safety
    ///
    /// Parameters are safely bound by the database driver,
    /// preventing SQL injection attacks.
    ///
    /// # Errors
    ///
    /// * Returns `DatabaseError::UnsupportedOperation` if not implemented
    /// * Returns `DatabaseError::QueryFailed` if query fails
    /// * Returns `DatabaseError::InvalidQuery` for parameter count mismatch
    async fn query_raw_params(
        &self,
        _query: &str,
        _params: &[DatabaseValue],
    ) -> Result<Vec<Row>, DatabaseError> {
        Err(DatabaseError::UnsupportedOperation(
            "query_raw_params not implemented for this backend".to_string(),
        ))
    }

    /// Begin a database transaction
    ///
    /// # Errors
    ///
    /// * If transaction creation fails
    /// * If called on a `DatabaseTransaction` (nested transactions not supported)
    async fn begin_transaction(&self) -> Result<Box<dyn DatabaseTransaction>, DatabaseError>;
}

/// Savepoint within a transaction for nested transaction support
///
/// Savepoints allow creating nested transaction boundaries within a main transaction.
/// They enable partial rollback without losing the entire transaction, useful for
/// implementing complex business logic with conditional rollback points.
///
/// # Usage Example
///
/// ```rust,ignore
/// let tx = db.begin_transaction().await?;
///
/// // Do some work
/// tx.insert("users").value("name", "Alice").execute(&*tx).await?;
///
/// // Create a savepoint before risky operation
/// let sp = tx.savepoint("before_risky_op").await?;
///
/// // Try risky operation
/// match risky_operation(&*tx).await {
///     Ok(_) => sp.release().await?, // Success: merge into transaction
///     Err(_) => sp.rollback_to().await?, // Error: rollback to savepoint
/// }
///
/// // Continue with transaction
/// tx.commit().await?;
/// ```
///
/// # Database Support
///
/// All supported databases (`SQLite`, `PostgreSQL`, `MySQL`) implement savepoints using
/// standard SQL commands:
/// - `SAVEPOINT name` - Create savepoint
/// - `RELEASE SAVEPOINT name` - Commit savepoint changes
/// - `ROLLBACK TO SAVEPOINT name` - Rollback to savepoint
#[async_trait]
pub trait Savepoint: Send + Sync {
    /// Release (commit) this savepoint, merging changes into parent transaction
    ///
    /// This consumes the savepoint and makes all changes since the savepoint
    /// permanent within the parent transaction. The parent transaction can
    /// still be rolled back.
    ///
    /// # Errors
    ///
    /// * If the savepoint was already released or rolled back
    /// * If the underlying database operation fails
    async fn release(self: Box<Self>) -> Result<(), DatabaseError>;

    /// Rollback to this savepoint, undoing all changes after it
    ///
    /// This consumes the savepoint and undoes all changes made since the
    /// savepoint was created. The transaction remains active and can continue.
    ///
    /// # Errors
    ///
    /// * If the savepoint was already released or rolled back
    /// * If the underlying database operation fails
    async fn rollback_to(self: Box<Self>) -> Result<(), DatabaseError>;

    /// Get the name of this savepoint
    fn name(&self) -> &str;
}

/// Database transaction trait that extends Database functionality
///
/// Transactions provide ACID properties for database operations. All Database trait
/// methods are available within transactions, plus commit/rollback operations.
///
/// # Transaction Architecture
///
/// Each database backend implements transactions using connection pooling for isolation:
/// - **SQLite**: Connection pool with shared in-memory databases for true concurrency
/// - **PostgreSQL**: deadpool-postgres or native sqlx pools with dedicated connections
/// - **MySQL**: Native sqlx pools with transaction-per-connection isolation
/// - **Database Simulator**: Simple delegation to underlying backend
///
/// This architecture provides:
/// - **Natural isolation** - Each transaction gets dedicated connection from pool
/// - **No deadlocks** - No manual locking or complex synchronization required
/// - **Concurrent transactions** - Multiple transactions can run simultaneously
/// - **Production ready** - Uses mature, battle-tested connection pooling libraries
///
/// # Usage Pattern - The Execute Pattern
///
/// The key pattern is using `execute(&*tx)` to run operations within a transaction:
///
/// ```rust,ignore
/// let tx = db.begin_transaction().await?;
///
/// // The execute pattern: stmt.execute(&*tx).await?
/// tx.insert("users")
///     .value("name", "Alice")
///     .value("email", "alice@example.com")
///     .execute(&*tx)  // Execute on transaction, not original database
///     .await?;
///
/// tx.update("posts")
///     .set("author_id", user_id)
///     .where_eq("status", "draft")
///     .execute(&*tx)  // Same transaction ensures consistency
///     .await?;
///
/// // Commit consumes the transaction - prevents further use
/// tx.commit().await?;
/// // tx is no longer usable here - compile error if attempted!
/// ```
///
/// # Complete Transaction Lifecycle Example
///
/// ```rust,ignore
/// use switchy_database::{Database, DatabaseError};
///
/// async fn transfer_funds(
///     db: &dyn Database,
///     from_account: u64,
///     to_account: u64,
///     amount: i64
/// ) -> Result<(), DatabaseError> {
///     // Begin transaction for atomic transfer
///     let tx = db.begin_transaction().await?;
///
///     // Check source account balance
///     let balance_rows = tx.select("accounts")
///         .columns(&["balance"])
///         .where_eq("id", from_account)
///         .execute(&*tx)
///         .await?;
///
///     if balance_rows.is_empty() {
///         return tx.rollback().await;
///     }
///
///     let current_balance: i64 = balance_rows[0].get("balance")?.try_into()?;
///     if current_balance < amount {
///         // Insufficient funds - rollback transaction
///         return tx.rollback().await;
///     }
///
///     // Debit source account
///     tx.update("accounts")
///         .set("balance", current_balance - amount)
///         .where_eq("id", from_account)
///         .execute(&*tx)
///         .await?;
///
///     // Credit destination account
///     tx.update("accounts")
///         .set("balance", tx.select("accounts")
///             .columns(&["balance"])
///             .where_eq("id", to_account)
///             .execute(&*tx)
///             .await?[0]
///             .get("balance")?
///             .try_into::<i64>()? + amount)
///         .where_eq("id", to_account)
///         .execute(&*tx)
///         .await?;
///
///     // All operations succeeded - commit atomically
///     tx.commit().await?;
///     Ok(())
/// }
/// ```
///
/// # Error Handling Best Practices
///
/// Transactions do not have "poisoned" state - they remain usable after errors.
/// Users decide whether to continue operations or rollback after failures:
///
/// ```rust,ignore
/// let tx = db.begin_transaction().await?;
///
/// // Attempt risky operation
/// let result = tx.insert("users")
///     .value("email", potentially_duplicate_email)
///     .execute(&*tx)
///     .await;
///
/// match result {
///     Ok(_) => {
///         // Success - continue with more operations
///         tx.update("user_stats")
///             .set("total_users", "total_users + 1")
///             .execute(&*tx)
///             .await?;
///
///         tx.commit().await?;
///     }
///     Err(DatabaseError::Constraint(_)) => {
///         // Expected error - rollback gracefully
///         tx.rollback().await?;
///     }
///     Err(e) => {
///         // Unexpected error - rollback and propagate
///         tx.rollback().await.ok(); // Don't mask original error
///         return Err(e);
///     }
/// }
/// ```
///
/// # Connection Pool Benefits and Behavior
///
/// The connection pool architecture provides several benefits:
///
/// - **Efficient Resource Usage**: Connections are reused across transactions
/// - **Concurrent Transactions**: Multiple transactions can run simultaneously without blocking
/// - **Automatic Cleanup**: Connections return to pool when transactions complete
/// - **Isolation Guarantees**: Each transaction gets dedicated connection ensuring isolation
/// - **Scalability**: Pool size can be tuned for optimal performance under load
///
/// Pool behavior varies by backend:
/// - **SQLite**: 5-connection pool with shared in-memory databases using URI syntax
/// - **PostgreSQL**: deadpool-postgres with configurable pool size and timeouts
/// - **MySQL**: Native sqlx pools with connection lifecycle management
/// - **`SqlX` backends**: Use native `pool.begin()` API for optimal transaction handling
///
/// # Manual Rollback Required
///
/// Transactions do NOT auto-rollback on drop. Users must explicitly call
/// `commit()` or `rollback()` to avoid leaking database connections.
///
/// ```rust,ignore
/// let tx = db.begin_transaction().await?;
///
/// // Do work...
/// tx.insert("data").value("key", "value").execute(&*tx).await?;
///
/// // MUST explicitly commit or rollback - no auto-cleanup!
/// if success_condition {
///     tx.commit().await?;
/// } else {
///     tx.rollback().await?;
/// }
/// // Connection properly returned to pool
/// ```
///
/// # Common Pitfalls
///
/// ## Forgetting to Commit or Rollback
/// ```rust,ignore
/// let tx = db.begin_transaction().await?;
/// tx.insert("users").value("name", "Alice").execute(&*tx).await?;
/// // BUG: Transaction never committed or rolled back!
/// // This leaks a pooled connection until the function returns
/// ```
///
/// ## Using Database Instead of Transaction
/// ```rust,ignore
/// let tx = db.begin_transaction().await?;
///
/// // BUG: This executes outside the transaction!
/// db.insert("users").value("name", "Alice").execute(db).await?;
/// //  ^^                                        ^^
/// // Should be tx                            Should be &*tx
///
/// tx.commit().await?; // Commits empty transaction
/// ```
///
/// ## Attempting to Use Transaction After Commit
/// ```rust,ignore
/// let tx = db.begin_transaction().await?;
/// tx.insert("users").value("name", "Alice").execute(&*tx).await?;
/// tx.commit().await?;
///
/// // COMPILE ERROR: tx was consumed by commit()
/// tx.insert("posts").value("title", "Hello").execute(&*tx).await?;
/// ```
///
/// ## Nested Transaction Attempts
/// ```rust,ignore
/// let tx = db.begin_transaction().await?;
///
/// // ERROR: DatabaseError::AlreadyInTransaction
/// let nested_tx = tx.begin_transaction().await?;
/// ```
///
/// ## Pool Exhaustion Scenarios
/// ```rust,ignore
/// // Creating many transactions without committing/rolling back
/// for i in 0..100 {
///     let tx = db.begin_transaction().await?; // Eventually fails when pool exhausted
///     // BUG: Never commit or rollback - connections accumulate
/// }
/// ```
///
/// # Migration Safety with Savepoints
///
/// Savepoints are useful for testing migrations safely in production:
///
/// ```rust,ignore
/// async fn safe_migration(db: &Database) -> Result<(), DatabaseError> {
///     let tx = db.begin_transaction().await?;
///
///     // Create savepoint before potentially dangerous schema change
///     let sp = tx.savepoint("before_migration").await?;
///
///     // Apply risky migration
///     tx.execute("ALTER TABLE users ADD COLUMN new_field TEXT")
///         .await?;
///
///     // Test the migration with sample data
///     match test_migration(&*tx).await {
///         Ok(_) => {
///             // Migration successful, keep changes
///             sp.release().await?;
///             tx.commit().await?;
///         }
///         Err(e) => {
///             // Migration failed, rollback to savepoint
///             sp.rollback_to().await?;
///
///             // Could try alternative migration approach
///             tx.execute("ALTER TABLE users ADD COLUMN new_field VARCHAR(255)")
///                 .await?;
///             tx.commit().await?;
///         }
///     }
///     Ok(())
/// }
/// ```
///
/// # Backend-Specific Behavior
///
/// ## PostgreSQL
/// - After any error in a transaction, `PostgreSQL` enters an "aborted" state
/// - No further operations (including savepoint creation) are allowed until rollback
/// - Best practice: Create savepoints BEFORE operations that might fail
/// - Example:
/// ```rust,ignore
/// // PostgreSQL - Create savepoint first
/// let sp = tx.savepoint("before_risky").await?;
/// match risky_operation(&*tx).await {
///     Err(_) => sp.rollback_to().await?, // Can recover
///     Ok(_) => sp.release().await?,
/// }
/// ```
///
/// ## `SQLite` & `MySQL`
/// - Allow savepoint creation after errors within a transaction
/// - More forgiving error recovery model
/// - Can create savepoints reactively after errors occur
///
/// ## Savepoint Name Restrictions
/// - All backends: Names must be alphanumeric with underscores only
/// - Cannot start with numbers
/// - No spaces or special characters allowed
/// - Maximum length varies by backend (typically 63-128 characters)
#[async_trait]
pub trait DatabaseTransaction: Database + Send + Sync {
    /// Commit the transaction, consuming it
    ///
    /// # Errors
    ///
    /// * If the commit operation fails
    /// * If the transaction was already committed or rolled back
    async fn commit(self: Box<Self>) -> Result<(), DatabaseError>;

    /// Rollback the transaction, consuming it
    ///
    /// # Errors
    ///
    /// * If the rollback operation fails
    /// * If the transaction was already committed or rolled back
    async fn rollback(self: Box<Self>) -> Result<(), DatabaseError>;

    /// Create a savepoint within this transaction
    ///
    /// # Errors
    ///
    /// * If the savepoint creation fails
    /// * If a savepoint with this name already exists
    /// * If the savepoint name is invalid
    async fn savepoint(&self, name: &str) -> Result<Box<dyn Savepoint>, DatabaseError>;

    /// CASCADE-specific methods (feature-gated)
    /// Find all tables that would be affected by CASCADE deletion of the specified table
    /// Returns a `DropPlan` which handles both simple and circular dependencies
    ///
    /// # Performance
    ///
    /// Time: O(d * f) where d = dependent tables, f = foreign keys per table
    /// Space: O(d) for visited set and results
    /// Note: Optimized for targeted discovery instead of analyzing all tables
    ///
    /// # Errors
    ///
    /// * Returns `DatabaseError` if dependency discovery fails
    #[cfg(feature = "cascade")]
    async fn find_cascade_targets(
        &self,
        table_name: &str,
    ) -> Result<crate::schema::DropPlan, DatabaseError>;

    /// Check if a table has any dependents (for RESTRICT validation)
    /// Returns immediately upon finding first dependent for efficiency
    ///
    /// # Performance
    ///
    /// Best case: O(1) - stops at first dependent found
    /// Worst case: O(n) - only when table has no dependents
    ///
    /// # Errors
    ///
    /// * Returns `DatabaseError` if introspection fails
    #[cfg(feature = "cascade")]
    async fn has_any_dependents(&self, table_name: &str) -> Result<bool, DatabaseError>;

    /// Get direct dependents of a table (one level only, no recursion)
    ///
    /// # Errors
    ///
    /// * Returns `DatabaseError` if table introspection fails
    #[cfg(feature = "cascade")]
    async fn get_direct_dependents(
        &self,
        table_name: &str,
    ) -> Result<std::collections::BTreeSet<String>, DatabaseError>;
}

#[async_trait]
pub trait TryFromDb<T>
where
    Self: Sized,
{
    type Error;

    async fn try_from_db(value: T, db: Arc<Box<dyn Database>>) -> Result<Self, Self::Error>;
}

#[async_trait]
impl<T, U: Send + 'static> TryFromDb<Vec<U>> for Vec<T>
where
    T: TryFromDb<U> + Send,
{
    type Error = T::Error;

    async fn try_from_db(value: Vec<U>, db: Arc<Box<dyn Database>>) -> Result<Self, T::Error> {
        let mut converted = Self::with_capacity(value.len());

        for x in value {
            converted.push(T::try_from_db(x, db.clone()).await?);
        }

        Ok(converted)
    }
}

#[async_trait]
impl<T, U: Send + 'static> TryFromDb<Option<U>> for Option<T>
where
    T: TryFromDb<U>,
{
    type Error = T::Error;

    async fn try_from_db(value: Option<U>, db: Arc<Box<dyn Database>>) -> Result<Self, T::Error> {
        Ok(match value {
            Some(x) => Some(T::try_from_db(x, db).await?),
            None => None,
        })
    }
}

#[async_trait]
pub trait TryIntoDb<T>
where
    Self: Sized,
{
    type Error;

    async fn try_into_db(self, db: Arc<Box<dyn Database>>) -> Result<T, Self::Error>;
}

#[async_trait]
impl<T: Send, U> TryIntoDb<U> for T
where
    U: TryFromDb<T>,
{
    type Error = U::Error;

    async fn try_into_db(self, db: Arc<Box<dyn Database>>) -> Result<U, U::Error> {
        U::try_from_db(self, db).await
    }
}

// Re-export Executable trait
pub use executable::Executable;
