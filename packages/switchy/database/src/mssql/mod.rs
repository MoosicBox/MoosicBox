//! Microsoft SQL Server backend (SQL Server 2012 and later, Azure SQL), over
//! TDS with the `tiberius` driver.
//!
//! [`MssqlDatabase`] keeps a small pool of connections (opened over TCP, with
//! TLS from `rustls` as [`Encryption`] asks). It authenticates with a SQL
//! Server login and password; Windows and Microsoft Entra authentication are
//! not supported. A named instance (`host\SQLEXPRESS`) is found through the
//! SQL Server Browser unless a port is given.
//!
//! # Statements
//!
//! Raw statements take `@P1`, `@P2`, ... placeholders, the convention of
//! `sp_executesql`, which runs every statement with parameters: values are
//! always sent as typed parameters, never written into the statement.
//! [`DatabaseValue::Now`] and [`DatabaseValue::NowPlus`] are the exception:
//! they are written as `SYSUTCDATETIME()` expressions. Statements without
//! parameters run as SQL text, so a batch may hold several statements and
//! session settings it makes (`SET IDENTITY_INSERT ... ON`) last for the
//! connection. Outside a transaction a statement runs on whichever pooled
//! connection is free, so such settings only reliably apply inside a
//! transaction; a statement that leaves a transaction open outside one is an
//! error, and that transaction is rolled back.
//!
//! The query builder writes T-SQL (`TOP`, `OUTPUT INSERTED.*` instead of
//! `RETURNING`), and the schema builder writes SQL Server DDL (`IDENTITY`
//! columns, named default constraints, `sp_rename`).
//!
//! # Transactions
//!
//! A transaction holds one connection from `BEGIN TRANSACTION` to `COMMIT` /
//! `ROLLBACK`. Savepoints are `SAVE TRANSACTION name` and `ROLLBACK
//! TRANSACTION name`; SQL Server has no `RELEASE`, so releasing a savepoint
//! keeps its work in the transaction and does nothing else. When a statement
//! fails and the server rolls the whole transaction back (as it does for
//! deadlocks and some errors), the transaction is over: later statements
//! fail instead of running outside it. A statement cancelled part-way closes
//! the connection, and the server rolls the transaction back.
//!
//! # Read-only connections
//!
//! SQL Server has no read-only session setting (`ApplicationIntent=ReadOnly`
//! only routes connections to readable replicas). A read-only database
//! ([`MssqlDatabaseBuilder::read_only`]) enforces read-only access itself:
//!
//! * every statement must pass a read-only check: one query
//!   (`SELECT`/`WITH`), or `EXEC` of a read-only catalog procedure with
//!   literal arguments, with no keyword that writes, changes the session,
//!   runs code, controls transactions or reaches outside the database, no
//!   `NEXT VALUE FOR`, and no linked-server names. A statement the check
//!   can't read to the end is refused;
//! * a statement that passed runs, with `SET XACT_ABORT ON`, inside a
//!   transaction that is always rolled back (through `sp_executesql`, on a
//!   connection used by nothing else meanwhile). `@@TRANCOUNT` must still be
//!   1 afterwards; if anything ended the transaction, the statement fails
//!   and its connection is closed rather than reused. This check runs after
//!   the statement, so it only *detects* an escape: a commit that happened
//!   has already taken effect. Preventing writes rests on the statement
//!   check above; the rollback undoes what a statement that passed it did
//!   inside the transaction.
//!
//! What a rollback can't undo, and what the check therefore keeps out:
//! identity and sequence values (`NEXT VALUE FOR`), work outside the
//! database (`xp_cmdshell`, mail, linked servers, `OPENROWSET`), server
//! administration (`BACKUP`, `ALTER DATABASE`, `KILL`, `DBCC`, `SHUTDOWN`),
//! and code that commits (procedures, triggers). The residual risk is code
//! already in the database that a query calls: a user-defined function
//! runs with the query, and SQL Server lets functions call extended stored
//! procedures (and CLR functions do what their assembly allows). For a
//! guarantee from the server itself, connect with a login that may only
//! read (the `db_datareader` role and no other permissions): this backend
//! can't enforce read-only access as strongly as the server's permissions.
//!
//! # Example
//!
//! ```rust,no_run
//! use switchy_database::{Database, DatabaseValue, mssql::MssqlDatabase};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let db = MssqlDatabase::builder("db.example")
//!     .user("shop")
//!     .password("secret")
//!     .database("music")
//!     .read_only(true)
//!     .build()
//!     .await?;
//! let rows = db
//!     .query_raw_params("SELECT title FROM dbo.albums WHERE id = @P1", &[DatabaseValue::Int64(1)])
//!     .await?;
//! # Ok(())
//! # }
//! ```

#[cfg(feature = "schema")]
mod introspection;
mod pool;
mod run;
#[cfg(feature = "schema")]
mod schema;
mod sql;
mod transaction;
mod tsql;
mod value;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;

use crate::{DatabaseError, DatabaseValue};

pub use transaction::{MssqlSavepoint, MssqlTransaction};
pub use tsql::ALLOWED_PROCEDURES as READ_ONLY_PROCEDURES;

use pool::{Lease, Pool};
use run::{Mode, Outcome};

/// Errors from the SQL Server backend.
#[derive(Debug, Error)]
pub enum MssqlDatabaseError {
    /// The server reported an error.
    #[error("{message} (SQL Server error {number}, state {state}, line {line}{})", procedure_suffix(.procedure))]
    Server {
        /// The error number (`2627` for a duplicate key, `547` for a
        /// constraint, `1205` for a deadlock, ...).
        number: u32,
        /// The error state.
        state: u8,
        /// The severity (class); 20 and above close the connection.
        class: u8,
        /// The line of the batch or procedure the error is on.
        line: u32,
        /// The procedure the error is in, if any.
        procedure: String,
        /// What went wrong.
        message: String,
    },
    /// The connection failed (network, TLS, protocol).
    #[error("{0}")]
    Driver(String),
    /// Connecting or a statement took longer than allowed; the connection
    /// was closed.
    #[error("timed out {0}")]
    Timeout(String),
    /// A read-only connection refused a statement.
    #[error("read-only connection: {0}")]
    ReadOnly(String),
    /// A statement on a read-only connection ended its rolled-back
    /// transaction; the connection was closed.
    #[error("read-only connection: {0}")]
    ReadOnlyViolation(String),
    /// The transaction is over (rolled back by the server after an error,
    /// or lost with its connection); nothing more runs in it.
    #[error("the transaction is no longer open: {0}")]
    TransactionLost(String),
    /// A statement outside a transaction left one open; it was rolled back.
    #[error(
        "the statement left a transaction open outside a transaction, so it was rolled back; \
         use a transaction (begin_transaction) for statements that span calls"
    )]
    OpenTransaction,
    /// The statement's placeholders don't match its parameters.
    #[error("invalid statement: {0}")]
    InvalidStatement(String),
    /// The server answered in an unexpected way.
    #[error("unexpected response from SQL Server: {0}")]
    Protocol(String),
    /// The configuration can't be used.
    #[error("invalid SQL Server configuration: {0}")]
    InvalidConfig(String),
    /// The database was closed.
    #[error("the database was closed")]
    Closed,
}

/// `, procedure p` for a server error in a procedure.
fn procedure_suffix(procedure: &str) -> String {
    if procedure.is_empty() {
        String::new()
    } else {
        format!(", procedure {procedure}")
    }
}

impl MssqlDatabaseError {
    /// The server's error number, if the server reported an error.
    #[must_use]
    pub const fn number(&self) -> Option<u32> {
        match self {
            Self::Server { number, .. } => Some(*number),
            _ => None,
        }
    }

    /// Whether the connection is gone (or can't be trusted any more).
    pub(crate) const fn is_connection_error(&self) -> bool {
        matches!(self, Self::Driver(_) | Self::Closed | Self::Timeout(_))
    }

    /// Whether the connection the error happened on can't be trusted any
    /// more (anything but an error the server reported for a statement).
    const fn breaks_connection(&self) -> bool {
        !matches!(
            self,
            Self::Server { class: ..20, .. }
                | Self::ReadOnly(_)
                | Self::InvalidStatement(_)
                | Self::OpenTransaction
        )
    }

    /// The failure category of a server error.
    pub(crate) const fn kind(&self) -> Option<crate::DatabaseErrorKind> {
        use crate::DatabaseErrorKind;
        match self {
            // Deadlock victim, lock request timeout.
            Self::Server { number: 1205, .. } => Some(DatabaseErrorKind::Busy),
            Self::Server { number: 1222, .. } => Some(DatabaseErrorKind::Locked),
            // Unique key/index, check/foreign key, NOT NULL.
            Self::Server {
                number: 2627 | 2601 | 547 | 515,
                ..
            } => Some(DatabaseErrorKind::Constraint),
            Self::Server {
                number: 823 | 824, ..
            } => Some(DatabaseErrorKind::Corrupt),
            _ => None,
        }
    }
}

impl From<tiberius::error::Error> for MssqlDatabaseError {
    fn from(error: tiberius::error::Error) -> Self {
        match error {
            tiberius::error::Error::Server(token) => Self::Server {
                number: token.code(),
                state: token.state(),
                class: token.class(),
                line: token.line(),
                procedure: token.procedure().to_string(),
                message: token.message().to_string(),
            },
            other => Self::Driver(other.to_string()),
        }
    }
}

impl From<MssqlDatabaseError> for DatabaseError {
    fn from(error: MssqlDatabaseError) -> Self {
        Self::Mssql(error)
    }
}

/// How much of a connection TLS protects.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Encryption {
    /// No TLS at all, not even for the login, which sends the password
    /// barely obscured. Only for servers that can't do TLS.
    Plaintext,
    /// TLS for the login only, then plain TCP (`Encrypt=false`; the server
    /// may still insist on encrypting everything).
    LoginOnly,
    /// TLS for the whole session when the server supports it.
    Preferred,
    /// TLS for the whole session, or no connection (`Encrypt=true`).
    #[default]
    Required,
    /// TDS 8.0 strict encryption: TLS before anything else is sent
    /// (`Encrypt=strict`; SQL Server 2022 and Azure SQL).
    Strict,
}

impl Encryption {
    const fn level(self) -> tiberius::EncryptionLevel {
        match self {
            Self::Plaintext => tiberius::EncryptionLevel::NotSupported,
            Self::LoginOnly => tiberius::EncryptionLevel::Off,
            Self::Preferred => tiberius::EncryptionLevel::On,
            Self::Required => tiberius::EncryptionLevel::Required,
            Self::Strict => tiberius::EncryptionLevel::Strict,
        }
    }
}

/// Builds an [`MssqlDatabase`].
#[derive(Clone)]
pub struct MssqlDatabaseBuilder {
    host: String,
    port: Option<u16>,
    instance: Option<String>,
    database: Option<String>,
    user: Option<String>,
    password: Option<String>,
    encryption: Encryption,
    trust_server_certificate: bool,
    ca_file: Option<PathBuf>,
    host_name_in_certificate: Option<String>,
    application_name: Option<String>,
    read_only: bool,
    pool_size: usize,
    connect_timeout: Duration,
    statement_timeout: Option<Duration>,
}

impl std::fmt::Debug for MssqlDatabaseBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MssqlDatabaseBuilder")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("instance", &self.instance)
            .field("database", &self.database)
            .field("user", &self.user)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("encryption", &self.encryption)
            .field("trust_server_certificate", &self.trust_server_certificate)
            .field("ca_file", &self.ca_file)
            .field("host_name_in_certificate", &self.host_name_in_certificate)
            .field("application_name", &self.application_name)
            .field("read_only", &self.read_only)
            .field("pool_size", &self.pool_size)
            .field("connect_timeout", &self.connect_timeout)
            .field("statement_timeout", &self.statement_timeout)
            .finish()
    }
}

impl MssqlDatabaseBuilder {
    /// A builder for the server at `host` (port 1433 unless set).
    #[must_use]
    pub fn new(host: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            port: None,
            instance: None,
            database: None,
            user: None,
            password: None,
            encryption: Encryption::default(),
            trust_server_certificate: false,
            ca_file: None,
            host_name_in_certificate: None,
            application_name: None,
            read_only: false,
            pool_size: 5,
            connect_timeout: Duration::from_secs(15),
            statement_timeout: None,
        }
    }

    /// The server's TCP port (default 1433, or the named instance's port).
    #[must_use]
    pub const fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// A named instance, found through the SQL Server Browser (UDP 1434)
    /// unless a port is set.
    #[must_use]
    pub fn instance(mut self, instance: impl Into<String>) -> Self {
        self.instance = Some(instance.into());
        self
    }

    /// The database to use (default: the login's default database).
    #[must_use]
    pub fn database(mut self, database: impl Into<String>) -> Self {
        self.database = Some(database.into());
        self
    }

    /// The SQL Server login.
    #[must_use]
    pub fn user(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }

    /// The login's password.
    #[must_use]
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    /// How much of the connection TLS protects (default
    /// [`Encryption::Required`]).
    #[must_use]
    pub const fn encryption(mut self, encryption: Encryption) -> Self {
        self.encryption = encryption;
        self
    }

    /// Accept any server certificate, without checking who issued it or
    /// whom it names (`TrustServerCertificate=true`). Only for servers on a
    /// network you trust.
    #[must_use]
    pub const fn trust_server_certificate(mut self, trust: bool) -> Self {
        self.trust_server_certificate = trust;
        self
    }

    /// Also trust the certificate authorities in `path` (PEM or DER) to
    /// verify the server.
    #[must_use]
    pub fn ca_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.ca_file = Some(path.into());
        self
    }

    /// The name the server's certificate must carry, when it differs from
    /// the host connected to (a tunnel, an IP address).
    #[must_use]
    pub fn host_name_in_certificate(mut self, name: impl Into<String>) -> Self {
        self.host_name_in_certificate = Some(name.into());
        self
    }

    /// The application name the server reports for the connections.
    #[must_use]
    pub fn application_name(mut self, name: impl Into<String>) -> Self {
        self.application_name = Some(name.into());
        self
    }

    /// Refuse writes (see the [module documentation](self)).
    #[must_use]
    pub const fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// The most connections to keep open at once (default 5).
    #[must_use]
    pub const fn pool_size(mut self, size: usize) -> Self {
        self.pool_size = size;
        self
    }

    /// How long to wait for the TCP connection and the login (default 15
    /// seconds).
    #[must_use]
    pub const fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// The longest a statement may run. SQL Server has no server-side
    /// statement timeout, so this is enforced by the client: a statement
    /// that runs longer fails and its connection is closed, which makes the
    /// server stop it.
    #[must_use]
    pub const fn statement_timeout(mut self, timeout: Duration) -> Self {
        self.statement_timeout = Some(timeout);
        self
    }

    /// The driver configuration.
    fn config(&self) -> Result<tiberius::Config, MssqlDatabaseError> {
        if self.host.trim().is_empty() {
            return Err(MssqlDatabaseError::InvalidConfig(
                "the server host is empty".to_string(),
            ));
        }
        if self.trust_server_certificate && self.ca_file.is_some() {
            return Err(MssqlDatabaseError::InvalidConfig(
                "trusting any server certificate and verifying it with a CA file contradict each \
                 other; choose one"
                    .to_string(),
            ));
        }
        let mut config = tiberius::Config::new();
        config.host(&self.host);
        if let Some(port) = self.port {
            config.port(port);
        }
        if let Some(instance) = &self.instance {
            config.instance_name(instance);
        }
        if let Some(database) = self.database.as_deref().filter(|name| !name.is_empty()) {
            config.database(database);
        }
        config.authentication(tiberius::AuthMethod::sql_server(
            self.user.as_deref().unwrap_or_default(),
            self.password.as_deref().unwrap_or_default(),
        ));
        config.encryption(self.encryption.level());
        if self.trust_server_certificate {
            config.trust_cert();
        } else if let Some(path) = &self.ca_file {
            config.trust_cert_ca(path.display());
        }
        if let Some(name) = &self.host_name_in_certificate {
            config.hostname_in_certificate(name);
        }
        if let Some(name) = &self.application_name {
            config.application_name(name);
        }
        Ok(config)
    }

    /// Connect, checking that the server accepts the login.
    ///
    /// # Errors
    ///
    /// * [`MssqlDatabaseError::InvalidConfig`] for an empty host, or both a
    ///   trusted server certificate and a CA file
    /// * [`MssqlDatabaseError::Driver`] if the server can't be reached or TLS fails
    /// * [`MssqlDatabaseError::Server`] if the server refuses the login or
    ///   the database
    /// * [`MssqlDatabaseError::Timeout`] if connecting takes too long
    pub async fn build(self) -> Result<MssqlDatabase, MssqlDatabaseError> {
        let config = self.config()?;
        let connector = pool::Connector {
            config,
            connect_timeout: self.connect_timeout,
            read_only: self.read_only,
        };
        let pool = Pool::new(connector, self.pool_size);
        // Open one connection now, so a bad login fails here.
        pool.acquire().await?.release();
        log::debug!(
            "mssql: connected to {} ({})",
            self.host,
            if self.read_only {
                "read-only"
            } else {
                "read-write"
            }
        );
        Ok(MssqlDatabase {
            pool,
            statement_timeout: self.statement_timeout,
            closed: AtomicBool::new(false),
        })
    }
}

/// A SQL Server database (see the [module documentation](self)).
#[derive(Debug)]
pub struct MssqlDatabase {
    pool: Arc<Pool>,
    statement_timeout: Option<Duration>,
    closed: AtomicBool,
}

impl MssqlDatabase {
    /// A builder for the server at `host`.
    #[must_use]
    pub fn builder(host: impl Into<String>) -> MssqlDatabaseBuilder {
        MssqlDatabaseBuilder::new(host)
    }

    /// Whether the database refuses writes.
    #[must_use]
    pub fn is_read_only(&self) -> bool {
        self.pool.read_only()
    }

    /// Run one statement outside a transaction.
    async fn run_autocommit(
        &self,
        sql: &str,
        params: &[DatabaseValue],
        mode: Mode,
    ) -> Result<Outcome, MssqlDatabaseError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(MssqlDatabaseError::Closed);
        }
        if self.pool.read_only() {
            tsql::check_read_only(sql, false).map_err(MssqlDatabaseError::ReadOnly)?;
            return with_timeout(
                self.statement_timeout,
                run::read_only(self.pool.acquire().await?, sql, params, mode),
            )
            .await;
        }
        with_timeout(
            self.statement_timeout,
            run_writable(self.pool.acquire().await?, sql, params, mode),
        )
        .await
    }
}

/// Run a statement on a writable connection outside a transaction, then
/// roll back (and report) any transaction it left open.
async fn run_writable(
    mut lease: Lease,
    sql: &str,
    params: &[DatabaseValue],
    mode: Mode,
) -> Result<Outcome, MssqlDatabaseError> {
    let client = lease.client();
    let result = run::statement(client, sql, params, mode, false).await;
    // A statement that changes (or may change) whether the session runs
    // statements at all (`SET NOEXEC ON`) leaves a connection the next user
    // can't trust: it is closed, which also rolls back anything open.
    if !tsql::no_execute_changes(sql).is_ok_and(|changes| changes.is_empty()) {
        return result;
    }
    match result {
        Ok(outcome) => {
            if run::rollback_reporting(client).await? > 0 {
                lease.release();
                return Err(MssqlDatabaseError::OpenTransaction);
            }
            lease.release();
            Ok(outcome)
        }
        Err(error) if !error.breaks_connection() => {
            if run::rollback_reporting(client).await.is_ok() {
                lease.release();
            }
            Err(error)
        }
        Err(error) => Err(error),
    }
}

/// `future`, failing with [`MssqlDatabaseError::Timeout`] after `timeout`
/// (dropping the future closes the connection it used).
async fn with_timeout<T>(
    timeout: Option<Duration>,
    future: impl std::future::Future<Output = Result<T, MssqlDatabaseError>>,
) -> Result<T, MssqlDatabaseError> {
    match timeout {
        Some(timeout) => tokio::time::timeout(timeout, future)
            .await
            .map_err(|_| MssqlDatabaseError::Timeout(format!("after {timeout:?}")))?,
        None => future.await,
    }
}

/// Runs statements: on pooled connections ([`MssqlDatabase`]) or on a
/// transaction's connection ([`MssqlTransaction`]).
#[async_trait]
trait Executor: Send + Sync {
    /// Run one statement with `@P1`... parameters.
    async fn run(
        &self,
        sql: &str,
        params: &[DatabaseValue],
        mode: Mode,
    ) -> Result<Outcome, DatabaseError>;
}

#[async_trait]
impl Executor for MssqlDatabase {
    async fn run(
        &self,
        sql: &str,
        params: &[DatabaseValue],
        mode: Mode,
    ) -> Result<Outcome, DatabaseError> {
        Ok(self.run_autocommit(sql, params, mode).await?)
    }
}

/// Implement [`crate::Database`] for an [`Executor`], with the given extra items.
macro_rules! impl_database {
    ($type:ty, { $($extra:tt)* }) => {
        #[async_trait::async_trait]
        impl $crate::Database for $type {
            $($extra)*

            async fn check_connection(&self) -> Result<(), $crate::DatabaseError> {
                $crate::mssql::Executor::run(self, "SELECT 1", &[], $crate::mssql::Mode::Rows)
                    .await
                    .map(|_| ())
            }

            #[cfg(feature = "raw-sql")]
            async fn query_raw(&self, query: &str) -> Result<Vec<$crate::Row>, $crate::DatabaseError> {
                Ok($crate::mssql::Executor::run(self, query, &[], $crate::mssql::Mode::Rows)
                    .await?
                    .into_rows())
            }

            #[cfg(feature = "raw-sql")]
            async fn query_raw_params(
                &self,
                query: &str,
                params: &[$crate::DatabaseValue],
            ) -> Result<Vec<$crate::Row>, $crate::DatabaseError> {
                Ok($crate::mssql::Executor::run(self, query, params, $crate::mssql::Mode::Rows)
                    .await?
                    .into_rows())
            }

            #[cfg(feature = "raw-sql")]
            async fn query_raw_result(
                &self,
                query: &str,
                params: &[$crate::DatabaseValue],
            ) -> Result<$crate::QueryResult, $crate::DatabaseError> {
                Ok($crate::mssql::Executor::run(self, query, params, $crate::mssql::Mode::Rows)
                    .await?
                    .into_result())
            }

            #[cfg(feature = "raw-sql")]
            async fn exec_raw(&self, statement: &str) -> Result<(), $crate::DatabaseError> {
                $crate::mssql::Executor::run(self, statement, &[], $crate::mssql::Mode::Batch)
                    .await
                    .map(|_| ())
            }

            #[cfg(feature = "raw-sql")]
            async fn exec_raw_params(
                &self,
                query: &str,
                params: &[$crate::DatabaseValue],
            ) -> Result<u64, $crate::DatabaseError> {
                Ok($crate::mssql::Executor::run(self, query, params, $crate::mssql::Mode::Count)
                    .await?
                    .affected)
            }

            async fn query(
                &self,
                query: &$crate::query::SelectQuery<'_>,
            ) -> Result<Vec<$crate::Row>, $crate::DatabaseError> {
                $crate::mssql::sql::select(self, query).await
            }

            async fn query_first(
                &self,
                query: &$crate::query::SelectQuery<'_>,
            ) -> Result<Option<$crate::Row>, $crate::DatabaseError> {
                $crate::mssql::sql::select_first(self, query).await
            }

            async fn exec_update(
                &self,
                statement: &$crate::query::UpdateStatement<'_>,
            ) -> Result<Vec<$crate::Row>, $crate::DatabaseError> {
                $crate::mssql::sql::update(
                    self,
                    statement.table_name,
                    &statement.values,
                    statement.filters.as_deref(),
                    statement.limit,
                )
                .await
            }

            async fn exec_update_first(
                &self,
                statement: &$crate::query::UpdateStatement<'_>,
            ) -> Result<Option<$crate::Row>, $crate::DatabaseError> {
                Ok($crate::mssql::sql::update(
                    self,
                    statement.table_name,
                    &statement.values,
                    statement.filters.as_deref(),
                    Some(1),
                )
                .await?
                .into_iter()
                .next())
            }

            async fn exec_insert(
                &self,
                statement: &$crate::query::InsertStatement<'_>,
            ) -> Result<$crate::Row, $crate::DatabaseError> {
                $crate::mssql::sql::insert(self, statement.table_name, &statement.values).await
            }

            async fn exec_upsert(
                &self,
                statement: &$crate::query::UpsertStatement<'_>,
            ) -> Result<Vec<$crate::Row>, $crate::DatabaseError> {
                $crate::mssql::sql::upsert(
                    self,
                    statement.table_name,
                    &statement.values,
                    statement.filters.as_deref(),
                    statement.limit,
                    statement.unique,
                )
                .await
            }

            async fn exec_upsert_first(
                &self,
                statement: &$crate::query::UpsertStatement<'_>,
            ) -> Result<$crate::Row, $crate::DatabaseError> {
                $crate::mssql::sql::upsert(
                    self,
                    statement.table_name,
                    &statement.values,
                    statement.filters.as_deref(),
                    Some(1),
                    statement.unique,
                )
                .await?
                .into_iter()
                .next()
                .ok_or($crate::DatabaseError::NoRow)
            }

            async fn exec_upsert_multi(
                &self,
                statement: &$crate::query::UpsertMultiStatement<'_>,
            ) -> Result<Vec<$crate::Row>, $crate::DatabaseError> {
                $crate::mssql::sql::upsert_multi(
                    self,
                    statement.table_name,
                    &statement.values,
                    statement.unique.as_deref(),
                )
                .await
            }

            async fn exec_delete(
                &self,
                statement: &$crate::query::DeleteStatement<'_>,
            ) -> Result<Vec<$crate::Row>, $crate::DatabaseError> {
                $crate::mssql::sql::delete(
                    self,
                    statement.table_name,
                    statement.filters.as_deref(),
                    statement.limit,
                )
                .await
            }

            async fn exec_delete_first(
                &self,
                statement: &$crate::query::DeleteStatement<'_>,
            ) -> Result<Option<$crate::Row>, $crate::DatabaseError> {
                Ok($crate::mssql::sql::delete(
                    self,
                    statement.table_name,
                    statement.filters.as_deref(),
                    Some(1),
                )
                .await?
                .into_iter()
                .next())
            }

            #[cfg(feature = "schema")]
            async fn exec_create_table(
                &self,
                statement: &$crate::schema::CreateTableStatement<'_>,
            ) -> Result<(), $crate::DatabaseError> {
                let sql = $crate::mssql::schema::create_table_sql(statement)?;
                $crate::mssql::Executor::run(self, &sql, &[], $crate::mssql::Mode::Batch)
                    .await
                    .map(|_| ())
            }

            #[cfg(feature = "schema")]
            async fn exec_drop_table(
                &self,
                statement: &$crate::schema::DropTableStatement<'_>,
            ) -> Result<(), $crate::DatabaseError> {
                $crate::mssql::schema::drop_table(self, statement).await
            }

            #[cfg(feature = "schema")]
            async fn exec_create_index(
                &self,
                statement: &$crate::schema::CreateIndexStatement<'_>,
            ) -> Result<(), $crate::DatabaseError> {
                let sql = $crate::mssql::schema::create_index_sql(statement)?;
                $crate::mssql::Executor::run(self, &sql, &[], $crate::mssql::Mode::Batch)
                    .await
                    .map(|_| ())
            }

            #[cfg(feature = "schema")]
            async fn exec_drop_index(
                &self,
                statement: &$crate::schema::DropIndexStatement<'_>,
            ) -> Result<(), $crate::DatabaseError> {
                let sql = $crate::mssql::schema::drop_index_sql(statement);
                $crate::mssql::Executor::run(self, &sql, &[], $crate::mssql::Mode::Batch)
                    .await
                    .map(|_| ())
            }

            #[cfg(feature = "schema")]
            async fn exec_alter_table(
                &self,
                statement: &$crate::schema::AlterTableStatement<'_>,
            ) -> Result<(), $crate::DatabaseError> {
                $crate::mssql::schema::alter_table(self, statement).await
            }

            #[cfg(feature = "schema")]
            async fn table_exists(&self, table: &str) -> Result<bool, $crate::DatabaseError> {
                $crate::mssql::introspection::table_exists(self, table).await
            }

            #[cfg(feature = "schema")]
            async fn list_tables(&self) -> Result<Vec<String>, $crate::DatabaseError> {
                $crate::mssql::introspection::list_tables(self).await
            }

            #[cfg(feature = "schema")]
            async fn get_table_info(
                &self,
                table: &str,
            ) -> Result<Option<$crate::schema::TableInfo>, $crate::DatabaseError> {
                $crate::mssql::introspection::relation_info(
                    self,
                    &$crate::schema::RelationRef::new(table),
                )
                .await
            }

            #[cfg(feature = "schema")]
            async fn get_table_columns(
                &self,
                table: &str,
            ) -> Result<Vec<$crate::schema::ColumnInfo>, $crate::DatabaseError> {
                $crate::mssql::introspection::columns(self, None, table).await
            }

            #[cfg(feature = "schema")]
            async fn column_exists(
                &self,
                table: &str,
                column: &str,
            ) -> Result<bool, $crate::DatabaseError> {
                Ok($crate::mssql::introspection::columns(self, None, table)
                    .await?
                    .iter()
                    .any(|info| info.name == column))
            }

            #[cfg(feature = "schema")]
            async fn list_schemas(
                &self,
            ) -> Result<Vec<$crate::schema::SchemaInfo>, $crate::DatabaseError> {
                $crate::mssql::introspection::list_schemas(self).await
            }

            #[cfg(feature = "schema")]
            async fn list_databases(
                &self,
            ) -> Result<Vec<$crate::schema::DatabaseInfo>, $crate::DatabaseError> {
                $crate::mssql::introspection::list_databases(self).await
            }

            #[cfg(feature = "schema")]
            async fn list_relations(
                &self,
                schema: Option<&str>,
            ) -> Result<Vec<$crate::schema::RelationSummary>, $crate::DatabaseError> {
                $crate::mssql::introspection::list_relations(self, schema).await
            }

            #[cfg(feature = "schema")]
            async fn get_relation_info(
                &self,
                relation: &$crate::schema::RelationRef,
            ) -> Result<Option<$crate::schema::TableInfo>, $crate::DatabaseError> {
                $crate::mssql::introspection::relation_info(self, relation).await
            }

            #[cfg(all(feature = "schema", feature = "raw-sql"))]
            async fn list_routines(
                &self,
                schema: Option<&str>,
            ) -> Result<Vec<$crate::schema::RoutineInfo>, $crate::DatabaseError> {
                $crate::mssql::introspection::list_routines(self, schema).await
            }

            #[cfg(feature = "schema")]
            async fn get_view_definition(
                &self,
                relation: &$crate::schema::RelationRef,
            ) -> Result<Option<String>, $crate::DatabaseError> {
                $crate::mssql::introspection::view_definition(self, relation).await
            }
        }
    };
}

pub(crate) use impl_database;

impl_database!(MssqlDatabase, {
    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn crate::DatabaseTransaction>, crate::DatabaseError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(crate::DatabaseError::ConnectionClosed);
        }
        Ok(Box::new(
            MssqlTransaction::begin(&self.pool, self.statement_timeout).await?,
        ))
    }

    async fn close(&self) -> Result<(), crate::DatabaseError> {
        self.closed.store(true, Ordering::SeqCst);
        self.pool.close();
        Ok(())
    }
});

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn builder_debug_hides_the_password() {
        let builder = MssqlDatabase::builder("db.example")
            .user("shop")
            .password("s3cret-pw");
        let debug = format!("{builder:?}");
        assert!(!debug.contains("s3cret-pw"), "{debug}");
        assert!(debug.contains("<redacted>"), "{debug}");
    }

    #[test]
    fn contradictory_trust_settings_are_refused() {
        let builder = MssqlDatabase::builder("db.example")
            .trust_server_certificate(true)
            .ca_file("/etc/ca.pem");
        assert!(matches!(
            builder.config(),
            Err(MssqlDatabaseError::InvalidConfig(_))
        ));
        assert!(matches!(
            MssqlDatabase::builder(" ").config(),
            Err(MssqlDatabaseError::InvalidConfig(_))
        ));
    }

    #[test]
    fn server_errors_name_their_number_and_line() {
        let error = MssqlDatabaseError::Server {
            number: 2627,
            state: 1,
            class: 14,
            line: 3,
            procedure: String::new(),
            message: "Violation of PRIMARY KEY constraint".to_string(),
        };
        assert_eq!(
            error.to_string(),
            "Violation of PRIMARY KEY constraint (SQL Server error 2627, state 1, line 3)"
        );
        assert_eq!(error.number(), Some(2627));
        assert_eq!(error.kind(), Some(crate::DatabaseErrorKind::Constraint));
        assert!(!error.breaks_connection());
        assert!(MssqlDatabaseError::Driver("reset".to_string()).breaks_connection());
    }

    #[test]
    fn encryption_levels_map_to_the_driver() {
        assert_eq!(
            Encryption::default().level(),
            tiberius::EncryptionLevel::Required
        );
        assert_eq!(
            Encryption::LoginOnly.level(),
            tiberius::EncryptionLevel::Off
        );
        assert_eq!(
            Encryption::Strict.level(),
            tiberius::EncryptionLevel::Strict
        );
    }
}
