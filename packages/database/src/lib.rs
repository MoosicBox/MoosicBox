#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

pub mod config;
pub mod executable;
#[cfg(feature = "postgres-raw")]
pub mod postgres;
pub mod profiles;
#[cfg(feature = "sqlite-rusqlite")]
pub mod rusqlite;
#[cfg(feature = "simulator")]
pub mod simulator;
#[cfg(feature = "sqlx")]
pub mod sqlx;

pub mod query;

#[cfg(feature = "schema")]
pub mod schema;

use std::{num::TryFromIntError, sync::Arc};

use async_trait::async_trait;
use chrono::NaiveDateTime;
use query::{
    DeleteStatement, InsertStatement, SelectQuery, UpdateStatement, UpsertMultiStatement,
    UpsertStatement,
};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub enum DatabaseValue {
    Null,
    String(String),
    StringOpt(Option<String>),
    Bool(bool),
    BoolOpt(Option<bool>),
    Number(i64),
    NumberOpt(Option<i64>),
    UNumber(u64),
    UNumberOpt(Option<u64>),
    Real(f64),
    RealOpt(Option<f64>),
    NowAdd(String),
    Now,
    DateTime(NaiveDateTime),
}

impl DatabaseValue {
    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) | Self::StringOpt(Some(value)) => Some(value),
            _ => None,
        }
    }

    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Number(value) | Self::NumberOpt(Some(value)) => Some(*value),
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
            Self::UNumber(value) | Self::UNumberOpt(Some(value)) => Some(*value),
            Self::Number(value) | Self::NumberOpt(Some(value)) => Some(
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

    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::Real(value) | Self::RealOpt(Some(value)) => Some(*value),
            _ => None,
        }
    }

    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn as_datetime(&self) -> Option<NaiveDateTime> {
        match self {
            Self::DateTime(value) => Some(*value),
            _ => None,
        }
    }

    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) | Self::BoolOpt(Some(value)) => Some(*value),
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
        Self::String(val.to_string())
    }
}

impl From<String> for DatabaseValue {
    fn from(val: String) -> Self {
        Self::String(val)
    }
}

impl From<f32> for DatabaseValue {
    fn from(val: f32) -> Self {
        Self::Real(f64::from(val))
    }
}

impl From<f64> for DatabaseValue {
    fn from(val: f64) -> Self {
        Self::Real(val)
    }
}

impl From<i8> for DatabaseValue {
    fn from(val: i8) -> Self {
        Self::Number(i64::from(val))
    }
}

impl From<i16> for DatabaseValue {
    fn from(val: i16) -> Self {
        Self::Number(i64::from(val))
    }
}

impl From<i32> for DatabaseValue {
    fn from(val: i32) -> Self {
        Self::Number(i64::from(val))
    }
}

impl From<i64> for DatabaseValue {
    fn from(val: i64) -> Self {
        Self::Number(val)
    }
}

impl From<isize> for DatabaseValue {
    fn from(val: isize) -> Self {
        Self::Number(val as i64)
    }
}

impl From<u8> for DatabaseValue {
    fn from(val: u8) -> Self {
        Self::UNumber(u64::from(val))
    }
}

impl From<u16> for DatabaseValue {
    fn from(val: u16) -> Self {
        Self::UNumber(u64::from(val))
    }
}

impl From<u32> for DatabaseValue {
    fn from(val: u32) -> Self {
        Self::UNumber(u64::from(val))
    }
}

impl From<u64> for DatabaseValue {
    fn from(val: u64) -> Self {
        Self::UNumber(val)
    }
}

impl From<usize> for DatabaseValue {
    fn from(val: usize) -> Self {
        Self::UNumber(val as u64)
    }
}

pub trait AsId {
    fn as_id(&self) -> DatabaseValue;
}

#[derive(Debug, Error)]
pub enum TryFromError {
    #[error("Could not convert to type '{0}'")]
    CouldNotConvert(String),
    #[error(transparent)]
    TryFromInt(#[from] TryFromIntError),
}

impl TryFrom<DatabaseValue> for u64 {
    type Error = TryFromError;

    fn try_from(value: DatabaseValue) -> Result<Self, Self::Error> {
        match value {
            DatabaseValue::Number(value) | DatabaseValue::NumberOpt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::UNumber(value) | DatabaseValue::UNumberOpt(Some(value)) => Ok(value),
            _ => Err(TryFromError::CouldNotConvert("u64".into())),
        }
    }
}

impl TryFrom<DatabaseValue> for i32 {
    type Error = TryFromError;

    fn try_from(value: DatabaseValue) -> Result<Self, Self::Error> {
        match value {
            DatabaseValue::Number(value) | DatabaseValue::NumberOpt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            DatabaseValue::UNumber(value) | DatabaseValue::UNumberOpt(Some(value)) => {
                Ok(Self::try_from(value)?)
            }
            _ => Err(TryFromError::CouldNotConvert("i32".into())),
        }
    }
}

#[derive(Debug, Error)]
pub enum DatabaseError {
    #[cfg(feature = "sqlite-rusqlite")]
    #[error(transparent)]
    Rusqlite(rusqlite::RusqliteDatabaseError),
    #[cfg(feature = "mysql-sqlx")]
    #[error(transparent)]
    MysqlSqlx(sqlx::mysql::SqlxDatabaseError),
    #[cfg(feature = "sqlite-sqlx")]
    #[error(transparent)]
    SqliteSqlx(sqlx::sqlite::SqlxDatabaseError),
    #[cfg(feature = "postgres-raw")]
    #[error(transparent)]
    Postgres(postgres::postgres::PostgresDatabaseError),
    #[cfg(feature = "postgres-sqlx")]
    #[error(transparent)]
    PostgresSqlx(sqlx::postgres::SqlxDatabaseError),
    #[error("No row")]
    NoRow,
    #[cfg(feature = "schema")]
    #[error("Invalid schema: {0}")]
    InvalidSchema(String),
    #[error("Already in transaction - nested transactions not supported")]
    AlreadyInTransaction,
    #[error("Transaction already committed")]
    TransactionCommitted,
    #[error("Transaction already rolled back")]
    TransactionRolledBack,
    #[error("Transaction failed to start")]
    TransactionFailed,
    #[error("Unexpected result from operation")]
    UnexpectedResult,
}

impl DatabaseError {
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

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub columns: Vec<(String, DatabaseValue)>,
}

impl Row {
    #[must_use]
    pub fn get(&self, column_name: &str) -> Option<DatabaseValue> {
        self.columns
            .iter()
            .find(|c| c.0 == column_name)
            .map(|c| c.1.clone())
    }

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

    /// Begin a database transaction
    ///
    /// # Errors
    ///
    /// * If transaction creation fails
    /// * If called on a `DatabaseTransaction` (nested transactions not supported)
    async fn begin_transaction(&self) -> Result<Box<dyn DatabaseTransaction>, DatabaseError>;
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
