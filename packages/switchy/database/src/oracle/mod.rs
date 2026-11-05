//! Oracle Database backend (12c and later, including 23ai and Autonomous
//! Database), over ODPI-C with the `oracle` driver.
//!
//! The driver loads Oracle Instant Client (`libclntsh`) at run time, so
//! building needs no Oracle software; connecting fails with
//! [`OracleDatabaseError::ClientLibrary`] when the client can't be found (it
//! is looked for on the platform's library path: `LD_LIBRARY_PATH`,
//! `DYLD_LIBRARY_PATH` or `PATH`).
//!
//! [`OracleDatabase`] keeps a small pool of connections, authenticated with
//! a user name and password (external, wallet-only and Kerberos
//! authentication are not supported). A server is reached by host, port and
//! service name or SID, over TCP or TLS (`TCPS`, verified against a wallet
//! or the client's defaults), or through a net service name (a
//! `tnsnames.ora` alias) or a connect descriptor given whole.
//!
//! # Statements
//!
//! Raw statements take Oracle's own numbered placeholders, `:1`, `:2`, ...
//! (a number may repeat); values are always bound, never written into the
//! statement. [`DatabaseValue::Now`] and [`DatabaseValue::NowPlus`] are the
//! exception: they are written as `SYS_EXTRACT_UTC(SYSTIMESTAMP)`
//! expressions. Without parameters a statement is sent as it is, so
//! `:NEW`/`:OLD` in a trigger body are untouched. A trailing `;` is dropped
//! from SQL statements (Oracle refuses it from client libraries) and kept
//! on PL/SQL, whose `END;` needs it. Oracle runs one statement per call.
//!
//! Oracle treats an empty string as `NULL`: binding `""` stores `NULL`.
//!
//! Oracle 23ai's native `JSON` columns can't be read by the driver: a
//! query returning one fails with [`OracleDatabaseError::Unsupported`]
//! (select `JSON_SERIALIZE(column RETURNING CLOB)` instead). JSON kept in
//! `CLOB`/`VARCHAR2` columns reads as text.
//!
//! Sessions convert text to dates, timestamps and numbers (and back) with
//! ISO formats (`NLS_DATE_FORMAT = 'YYYY-MM-DD HH24:MI:SS'`, timestamps with
//! fractional seconds and `TZH:TZM` offsets, `.` as the decimal point), set
//! when each connection opens, rather than the client's locale's.
//!
//! The query builder writes Oracle SQL (`FETCH FIRST n ROWS ONLY`,
//! `MERGE` for upserts); since Oracle's `RETURNING ... INTO` returns values
//! into variables rather than rows, changed rows are read back by `ROWID`.
//! The schema builder writes Oracle DDL (identity columns, `MODIFY`).
//!
//! # Transactions
//!
//! Oracle opens a transaction with the first change and keeps it until
//! `COMMIT` or `ROLLBACK`. Outside a [transaction](crate::Database::begin_transaction)
//! every statement is committed (or, if it failed, rolled back) as soon as
//! it ran. A transaction holds one connection; savepoints are `SAVEPOINT
//! name` and `ROLLBACK TO SAVEPOINT name` (Oracle has no `RELEASE`, so
//! releasing one keeps its work in the transaction and does nothing else).
//!
//! **DDL commits.** Oracle commits the open transaction before and after
//! every DDL statement (`CREATE`, `ALTER`, `DROP`, `TRUNCATE`, `GRANT`,
//! ...), so DDL inside a transaction commits the work before it and can't
//! be rolled back.
//!
//! # Read-only connections
//!
//! A read-only database ([`OracleDatabaseBuilder::read_only`]) enforces
//! read-only access itself, with three measures:
//!
//! * every statement must pass a conservative check of its text: one query
//!   (`SELECT`/`WITH`), with no keyword that writes, runs code, changes the
//!   session or locks rows, no sequence values (`NEXTVAL`), no database
//!   links, no names of Oracle's side-effect packages (`DBMS_*`, `UTL_*`),
//!   and calls written with parentheses only to Oracle's built-in SQL
//!   functions (see [`READ_ONLY_FUNCTIONS`]). A keyword followed by `(` is
//!   accepted only if Oracle reserves it ([`READ_ONLY_RESERVED_PAREN_WORDS`],
//!   which can't name a function) or, for an unreserved one
//!   ([`READ_ONLY_CONTEXT_PAREN_KEYWORDS`], which can), only in the clauses
//!   where the grammar makes it a keyword (`OVER (` after a `)`, `PIVOT (`
//!   after a table, `CAST(x AS TIMESTAMP(6))`); anywhere else it is a call,
//!   and refused. A statement the check can't read the way Oracle does is
//!   refused;
//! * on Oracle 23ai and later the session is read-only (`ALTER SESSION SET
//!   READ_ONLY = TRUE`, re-asserted before every statement), so the server
//!   refuses DML, DDL, locking and writes in autonomous transactions; before
//!   23ai, every statement runs in a `SET TRANSACTION READ ONLY`
//!   transaction, which refuses DML and locking in it (but not DDL, which
//!   commits and runs, nor autonomous transactions);
//! * every statement is rolled back afterwards, inside a
//!   [transaction](crate::Database::begin_transaction) too (Oracle can't
//!   re-assert the session's read-only mode while a transaction is open, so
//!   a read-only transaction is a connection whose statements each run
//!   alone).
//!
//! A connection that fails any of this, or any statement, is closed rather
//! than reused.
//!
//! What remains is code already in the database that a query runs without
//! the check seeing a call: a function called without parentheses (Oracle
//! lets a function without arguments be called as `name` or
//! `schema.name`, which reads like a column), functions a view calls, an
//! object type's `MAP`/`ORDER` method, policy functions. Such code can
//! write in an autonomous transaction, which `SET TRANSACTION READ ONLY`
//! doesn't cover; on 23ai the read-only session makes those writes fail,
//! unless the code first switches the session's read-only mode off (any
//! session may). For a guarantee from the server itself, connect as a
//! read-only user (Oracle 23ai `ALTER USER name READ ONLY`, which code
//! can't switch off), or as a user that holds only `CREATE SESSION` and
//! `SELECT`/`READ` grants.
//!
//! # Example
//!
//! ```rust,no_run
//! use switchy_database::{Database, DatabaseValue, oracle::OracleDatabase};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let db = OracleDatabase::builder("db.example")
//!     .service_name("FREEPDB1")
//!     .user("shop")
//!     .password("secret")
//!     .read_only(true)
//!     .build()
//!     .await?;
//! let rows = db
//!     .query_raw_params("SELECT title FROM albums WHERE id = :1", &[DatabaseValue::Int64(1)])
//!     .await?;
//! # Ok(())
//! # }
//! ```

#[cfg(feature = "schema")]
mod introspection;
mod lexer;
mod pool;
mod run;
#[cfg(feature = "schema")]
mod schema;
mod sql;
mod transaction;
mod value;

use std::fmt::Write as _;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;

use crate::{DatabaseError, DatabaseValue};

pub use lexer::{
    BUILTIN_FUNCTIONS as READ_ONLY_FUNCTIONS,
    CONTEXT_PAREN_KEYWORDS as READ_ONLY_CONTEXT_PAREN_KEYWORDS,
    DENIED_PREFIXES as READ_ONLY_DENIED_PREFIXES, DENIED_WORDS as READ_ONLY_DENIED_WORDS,
    RESERVED_PAREN_WORDS as READ_ONLY_RESERVED_PAREN_WORDS,
};
pub use transaction::{OracleSavepoint, OracleTransaction};

use pool::Pool;
use run::{Outcome, Request};

/// Errors from the Oracle backend.
#[derive(Debug, Error)]
pub enum OracleDatabaseError {
    /// The server reported an error (`ORA-nnnnn`).
    #[error("{message}")]
    Server {
        /// The error number (`1` for a unique key, `2291` for a missing
        /// parent key, `60` for a deadlock, ...).
        code: i32,
        /// Where in the statement the error is (a byte offset), if known.
        offset: u32,
        /// The server's message, starting with `ORA-nnnnn:`.
        message: String,
    },
    /// Oracle Instant Client couldn't be loaded.
    #[error(
        "Oracle Instant Client could not be loaded ({0}); install it and put its directory on \
         the library path (LD_LIBRARY_PATH on Linux, DYLD_LIBRARY_PATH on macOS, PATH on \
         Windows)"
    )]
    ClientLibrary(String),
    /// The driver failed (network, protocol, a value it can't convert).
    #[error("{0}")]
    Driver(String),
    /// Connecting or a statement took longer than allowed; the connection
    /// was closed.
    #[error("timed out {0}")]
    Timeout(String),
    /// A read-only connection refused a statement.
    #[error("read-only connection: {0}")]
    ReadOnly(String),
    /// The transaction is over (lost with its connection); nothing more
    /// runs in it.
    #[error("the transaction is no longer open: {0}")]
    TransactionLost(String),
    /// The statement's placeholders don't match its parameters.
    #[error("invalid statement: {0}")]
    InvalidStatement(String),
    /// The driver can't do what the statement needs (read a column type).
    #[error("{0}")]
    Unsupported(String),
    /// The configuration can't be used.
    #[error("invalid Oracle configuration: {0}")]
    InvalidConfig(String),
    /// The database was closed.
    #[error("the database was closed")]
    Closed,
}

/// Server errors after which the session is gone or can't be trusted:
/// end-of-file on the channel, not connected, lost contact, session
/// killed, call timeout, cancelled call.
const CONNECTION_ERRORS: &[i32] = &[28, 1012, 1013, 2396, 3113, 3114, 3135, 3156, 12_537];

impl OracleDatabaseError {
    /// The server's error number (`ORA-nnnnn`), if the server reported an
    /// error.
    #[must_use]
    pub const fn code(&self) -> Option<i32> {
        match self {
            Self::Server { code, .. } => Some(*code),
            _ => None,
        }
    }

    /// An error from connecting: a missing client library is told apart.
    fn connect(error: oracle::Error) -> Self {
        let text = error.to_string();
        if text.contains("DPI-1047") || text.contains("DPI-1072") {
            Self::ClientLibrary(text)
        } else {
            error.into()
        }
    }

    /// Whether the connection is gone (or can't be trusted any more).
    pub(crate) fn is_connection_error(&self) -> bool {
        match self {
            Self::Driver(_) | Self::Closed | Self::Timeout(_) => true,
            Self::Server { code, .. } => CONNECTION_ERRORS.contains(code),
            _ => false,
        }
    }

    /// Whether the connection the error happened on can't be trusted any
    /// more (anything but an error the server reported for a statement).
    fn breaks_connection(&self) -> bool {
        match self {
            Self::Server { code, .. } => CONNECTION_ERRORS.contains(code),
            Self::ReadOnly(_) | Self::InvalidStatement(_) | Self::Unsupported(_) => false,
            _ => true,
        }
    }

    /// The failure category of a server error.
    pub(crate) const fn kind(&self) -> Option<crate::DatabaseErrorKind> {
        use crate::DatabaseErrorKind;
        match self {
            // Deadlock; resource busy (NOWAIT / WAIT n).
            Self::Server { code: 60, .. } => Some(DatabaseErrorKind::Busy),
            Self::Server {
                code: 54 | 30_006, ..
            } => Some(DatabaseErrorKind::Locked),
            // Unique key, check, missing parent, child records, NOT NULL.
            Self::Server {
                code: 1 | 2290 | 2291 | 2292 | 1400 | 1407,
                ..
            } => Some(DatabaseErrorKind::Constraint),
            Self::Server {
                code: 1578 | 8103, ..
            } => Some(DatabaseErrorKind::Corrupt),
            _ => None,
        }
    }
}

impl From<oracle::Error> for OracleDatabaseError {
    fn from(error: oracle::Error) -> Self {
        if let Some(db_error) = error.db_error()
            && error.oci_code().is_some()
        {
            let code = db_error.code();
            // DPI-1067 (call timeout) and DPI-1080 (connection closed by
            // ORA-n) carry the ORA code of what ended the call.
            return Self::Server {
                code,
                offset: db_error.offset(),
                message: db_error
                    .message()
                    .lines()
                    .filter(|line| !line.starts_with("Help: https://"))
                    .collect::<Vec<_>>()
                    .join("\n"),
            };
        }
        Self::Driver(error.to_string())
    }
}

impl From<OracleDatabaseError> for DatabaseError {
    fn from(error: OracleDatabaseError) -> Self {
        Self::Oracle(error)
    }
}

/// How the server is named.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Address {
    /// Host, port and database service.
    Server {
        host: String,
        port: Option<u16>,
        service: Service,
    },
    /// A net service name (`tnsnames.ora` alias) or an Easy Connect string,
    /// passed to the client as it is.
    ConnectString(String),
}

/// The database service on a server.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
enum Service {
    /// The listener's default service.
    #[default]
    Default,
    /// A service name (`FREEPDB1`, `orclpdb.example.com`).
    Name(String),
    /// An instance's SID (older configurations).
    Sid(String),
}

/// Builds an [`OracleDatabase`].
#[derive(Clone)]
pub struct OracleDatabaseBuilder {
    address: Address,
    user: Option<String>,
    password: Option<String>,
    tls: bool,
    wallet_location: Option<PathBuf>,
    server_cert_dn: Option<String>,
    read_only: bool,
    pool_size: usize,
    connect_timeout: Duration,
    statement_timeout: Option<Duration>,
}

impl std::fmt::Debug for OracleDatabaseBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OracleDatabaseBuilder")
            .field("address", &self.address)
            .field("user", &self.user)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("tls", &self.tls)
            .field("wallet_location", &self.wallet_location)
            .field("server_cert_dn", &self.server_cert_dn)
            .field("read_only", &self.read_only)
            .field("pool_size", &self.pool_size)
            .field("connect_timeout", &self.connect_timeout)
            .field("statement_timeout", &self.statement_timeout)
            .finish()
    }
}

/// Characters with a meaning in a connect descriptor.
fn descriptor_safe(value: &str) -> bool {
    !value.is_empty()
        && !value.chars().any(|c| {
            matches!(c, '(' | ')' | '=' | '"' | '\'' | '#' | '\\')
                || c.is_whitespace()
                || c.is_control()
        })
}

impl OracleDatabaseBuilder {
    /// A builder for the server at `host` (port 1521 unless set).
    #[must_use]
    pub fn new(host: impl Into<String>) -> Self {
        Self {
            address: Address::Server {
                host: host.into(),
                port: None,
                service: Service::Default,
            },
            user: None,
            password: None,
            tls: false,
            wallet_location: None,
            server_cert_dn: None,
            read_only: false,
            pool_size: 5,
            connect_timeout: Duration::from_secs(15),
            statement_timeout: None,
        }
    }

    /// A builder for a net service name (a `tnsnames.ora` alias, found
    /// through `TNS_ADMIN`), an Easy Connect string, or a full connect
    /// descriptor, passed to the client as it is.
    #[must_use]
    pub fn connect_string(connect_string: impl Into<String>) -> Self {
        Self {
            address: Address::ConnectString(connect_string.into()),
            ..Self::new("")
        }
    }

    /// The listener's port (default 1521, or 2484 with TLS).
    #[must_use]
    pub const fn port(mut self, port: u16) -> Self {
        if let Address::Server { port: target, .. } = &mut self.address {
            *target = Some(port);
        }
        self
    }

    /// The database service name (a pluggable database's service, such as
    /// `FREEPDB1`).
    #[must_use]
    pub fn service_name(mut self, name: impl Into<String>) -> Self {
        if let Address::Server { service, .. } = &mut self.address {
            *service = Service::Name(name.into());
        }
        self
    }

    /// An instance's SID, for configurations without service names.
    #[must_use]
    pub fn sid(mut self, sid: impl Into<String>) -> Self {
        if let Address::Server { service, .. } = &mut self.address {
            *service = Service::Sid(sid.into());
        }
        self
    }

    /// The user to log in as.
    #[must_use]
    pub fn user(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into());
        self
    }

    /// The user's password.
    #[must_use]
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    /// Connect over TLS (`TCPS`), verifying the server's certificate and
    /// that it names the host.
    #[must_use]
    pub const fn tls(mut self, tls: bool) -> Self {
        self.tls = tls;
        self
    }

    /// The directory of the wallet (`cwallet.sso`/`ewallet.p12`) holding
    /// the certificates TLS trusts (and a client certificate, if the server
    /// asks for one). Without it, the client's own configuration decides.
    #[must_use]
    pub fn wallet_location(mut self, path: impl Into<PathBuf>) -> Self {
        self.wallet_location = Some(path.into());
        self
    }

    /// The distinguished name the server's certificate must have, when it
    /// doesn't name the host connected to (a tunnel, an IP address).
    #[must_use]
    pub fn server_cert_dn(mut self, dn: impl Into<String>) -> Self {
        self.server_cert_dn = Some(dn.into());
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

    /// How long to wait for the listener and the login (default 15
    /// seconds).
    #[must_use]
    pub const fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    /// The longest a call may take; a statement that runs longer is
    /// interrupted, and its connection closed.
    #[must_use]
    pub const fn statement_timeout(mut self, timeout: Duration) -> Self {
        self.statement_timeout = Some(timeout);
        self
    }

    /// The connect string the client is given.
    fn descriptor(&self) -> Result<String, OracleDatabaseError> {
        let invalid = |message: String| Err(OracleDatabaseError::InvalidConfig(message));
        let (host, port, service) = match &self.address {
            Address::ConnectString(text) => {
                if text.trim().is_empty() {
                    return invalid("the connect string is empty".to_string());
                }
                if self.tls || self.wallet_location.is_some() || self.server_cert_dn.is_some() {
                    return invalid(
                        "TLS settings can't be added to a connect string given whole; put them \
                         in the descriptor or tnsnames.ora entry"
                            .to_string(),
                    );
                }
                return Ok(text.clone());
            }
            Address::Server {
                host,
                port,
                service,
            } => (host, port, service),
        };
        if host.trim().is_empty() {
            return invalid("the server host is empty".to_string());
        }
        // An IPv6 address keeps its colons, which a descriptor allows.
        let bare_host = host.trim_start_matches('[').trim_end_matches(']');
        if !descriptor_safe(bare_host) {
            return invalid(format!(
                "the host {host:?} can't be used in a connect descriptor"
            ));
        }
        let port = port.unwrap_or(if self.tls { 2484 } else { 1521 });
        let connect_data = match service {
            Service::Default => String::new(),
            Service::Name(name) | Service::Sid(name) if !descriptor_safe(name) => {
                return invalid(format!(
                    "the service {name:?} can't be used in a connect descriptor"
                ));
            }
            Service::Name(name) => format!("(SERVICE_NAME={name})"),
            Service::Sid(sid) => format!("(SID={sid})"),
        };
        let mut security = String::new();
        if self.tls {
            security.push_str("(SSL_SERVER_DN_MATCH=ON)");
            if let Some(dn) = &self.server_cert_dn {
                if dn.contains('"') || dn.chars().any(char::is_control) {
                    return invalid("the certificate DN can't contain quotes".to_string());
                }
                let _ = write!(security, "(SSL_SERVER_CERT_DN=\"{dn}\")");
            }
            if let Some(path) = &self.wallet_location {
                let path = path.display().to_string();
                if path.contains('"') || path.chars().any(char::is_control) {
                    return invalid("the wallet location can't contain quotes".to_string());
                }
                let _ = write!(security, "(MY_WALLET_DIRECTORY=\"{path}\")");
            }
            security = format!("(SECURITY={security})");
        } else if self.wallet_location.is_some() || self.server_cert_dn.is_some() {
            return invalid("a wallet or certificate DN needs TLS (tcps)".to_string());
        }
        Ok(format!(
            "(DESCRIPTION=(CONNECT_TIMEOUT={})(RETRY_COUNT=0)(ADDRESS=(PROTOCOL={})(HOST={bare_host})(PORT={port}))(CONNECT_DATA={connect_data}){security})",
            self.connect_timeout.as_secs().max(1),
            if self.tls { "TCPS" } else { "TCP" },
        ))
    }

    /// Connect, checking that the server accepts the login.
    ///
    /// # Errors
    ///
    /// * [`OracleDatabaseError::InvalidConfig`] for an empty host, a host or
    ///   service a connect descriptor can't hold, or TLS settings without
    ///   TLS
    /// * [`OracleDatabaseError::ClientLibrary`] if Oracle Instant Client
    ///   can't be loaded
    /// * [`OracleDatabaseError::Server`] if the server refuses the login
    ///   (`ORA-01017`) or the service is unknown (`ORA-12514`)
    /// * [`OracleDatabaseError::Driver`] if the server can't be reached
    /// * [`OracleDatabaseError::Timeout`] if connecting takes too long
    pub async fn build(self) -> Result<OracleDatabase, OracleDatabaseError> {
        let connect_string = self.descriptor()?;
        let connector = pool::Connector {
            user: self.user.clone().unwrap_or_default(),
            password: self.password.clone().unwrap_or_default(),
            connect_string,
            connect_timeout: self.connect_timeout,
            statement_timeout: self.statement_timeout,
            read_only: self.read_only,
        };
        let pool = Pool::new(connector, self.pool_size);
        // Open one connection now, so a bad login fails here.
        pool.acquire().await?.release();
        log::debug!(
            "oracle: connected ({})",
            if self.read_only {
                "read-only"
            } else {
                "read-write"
            }
        );
        Ok(OracleDatabase {
            pool,
            closed: AtomicBool::new(false),
        })
    }
}

/// An Oracle database (see the [module documentation](self)).
#[derive(Debug)]
pub struct OracleDatabase {
    pool: Arc<Pool>,
    closed: AtomicBool,
}

impl OracleDatabase {
    /// A builder for the server at `host`.
    #[must_use]
    pub fn builder(host: impl Into<String>) -> OracleDatabaseBuilder {
        OracleDatabaseBuilder::new(host)
    }

    /// Whether the database refuses writes.
    #[must_use]
    pub fn is_read_only(&self) -> bool {
        self.pool.read_only()
    }

    /// Run one statement outside a transaction.
    async fn run_autocommit(&self, request: Request) -> Result<Outcome, OracleDatabaseError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(OracleDatabaseError::Closed);
        }
        let read_only = self.pool.read_only();
        if read_only {
            lexer::check_read_only(&request.sql).map_err(OracleDatabaseError::ReadOnly)?;
        }
        let lease = self.pool.acquire().await?;
        let session = lease.session().clone();
        let server = session.server;
        let result = pool::blocking(&session.conn, move |conn| {
            if read_only {
                run::read_only(conn, server, &request)
            } else {
                run::autocommit(conn, server, &request)
            }
        })
        .await;
        match &result {
            Ok(_) => lease.release(),
            // A read-only connection is never reused after an error.
            Err(error) if !read_only && !error.breaks_connection() => lease.release(),
            Err(_) => {}
        }
        result
    }
}

/// Runs statements: on pooled connections ([`OracleDatabase`]) or on a
/// transaction's connection ([`OracleTransaction`]).
#[async_trait]
trait Executor: Send + Sync {
    /// Run one statement with `:1`... parameters.
    async fn run(&self, request: Request) -> Result<Outcome, DatabaseError>;

    /// Run one statement with `:1`... parameters.
    async fn run_sql(&self, sql: &str, params: &[DatabaseValue]) -> Result<Outcome, DatabaseError> {
        self.run(Request::new(sql, params)).await
    }
}

#[async_trait]
impl Executor for OracleDatabase {
    async fn run(&self, request: Request) -> Result<Outcome, DatabaseError> {
        Ok(self.run_autocommit(request).await?)
    }
}

/// Implement [`crate::Database`] for an [`Executor`], with the given extra items.
macro_rules! impl_database {
    ($type:ty, { $($extra:tt)* }) => {
        #[async_trait::async_trait]
        impl $crate::Database for $type {
            $($extra)*

            async fn check_connection(&self) -> Result<(), $crate::DatabaseError> {
                $crate::oracle::Executor::run_sql(self, "SELECT 1 FROM dual", &[])
                    .await
                    .map(|_| ())
            }

            #[cfg(feature = "raw-sql")]
            async fn query_raw(&self, query: &str) -> Result<Vec<$crate::Row>, $crate::DatabaseError> {
                Ok($crate::oracle::Executor::run_sql(self, query, &[]).await?.into_rows())
            }

            #[cfg(feature = "raw-sql")]
            async fn query_raw_params(
                &self,
                query: &str,
                params: &[$crate::DatabaseValue],
            ) -> Result<Vec<$crate::Row>, $crate::DatabaseError> {
                Ok($crate::oracle::Executor::run_sql(self, query, params).await?.into_rows())
            }

            #[cfg(feature = "raw-sql")]
            async fn query_raw_result(
                &self,
                query: &str,
                params: &[$crate::DatabaseValue],
            ) -> Result<$crate::QueryResult, $crate::DatabaseError> {
                Ok($crate::oracle::Executor::run_sql(self, query, params).await?.into_result())
            }

            #[cfg(feature = "raw-sql")]
            async fn exec_raw(&self, statement: &str) -> Result<(), $crate::DatabaseError> {
                $crate::oracle::Executor::run_sql(self, statement, &[]).await.map(|_| ())
            }

            #[cfg(feature = "raw-sql")]
            async fn exec_raw_params(
                &self,
                query: &str,
                params: &[$crate::DatabaseValue],
            ) -> Result<u64, $crate::DatabaseError> {
                Ok($crate::oracle::Executor::run_sql(self, query, params).await?.affected)
            }

            async fn query(
                &self,
                query: &$crate::query::SelectQuery<'_>,
            ) -> Result<Vec<$crate::Row>, $crate::DatabaseError> {
                $crate::oracle::sql::select(self, query).await
            }

            async fn query_first(
                &self,
                query: &$crate::query::SelectQuery<'_>,
            ) -> Result<Option<$crate::Row>, $crate::DatabaseError> {
                $crate::oracle::sql::select_first(self, query).await
            }

            async fn exec_update(
                &self,
                statement: &$crate::query::UpdateStatement<'_>,
            ) -> Result<Vec<$crate::Row>, $crate::DatabaseError> {
                $crate::oracle::sql::update(
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
                Ok($crate::oracle::sql::update(
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
                $crate::oracle::sql::insert(self, statement.table_name, &statement.values).await
            }

            async fn exec_upsert(
                &self,
                statement: &$crate::query::UpsertStatement<'_>,
            ) -> Result<Vec<$crate::Row>, $crate::DatabaseError> {
                $crate::oracle::sql::upsert(
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
                $crate::oracle::sql::upsert(
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
                $crate::oracle::sql::upsert_multi(
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
                $crate::oracle::sql::delete(
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
                Ok($crate::oracle::sql::delete(
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
                $crate::oracle::schema::create_table(self, statement).await
            }

            #[cfg(feature = "schema")]
            async fn exec_drop_table(
                &self,
                statement: &$crate::schema::DropTableStatement<'_>,
            ) -> Result<(), $crate::DatabaseError> {
                $crate::oracle::schema::drop_table(self, statement).await
            }

            #[cfg(feature = "schema")]
            async fn exec_create_index(
                &self,
                statement: &$crate::schema::CreateIndexStatement<'_>,
            ) -> Result<(), $crate::DatabaseError> {
                $crate::oracle::schema::create_index(self, statement).await
            }

            #[cfg(feature = "schema")]
            async fn exec_drop_index(
                &self,
                statement: &$crate::schema::DropIndexStatement<'_>,
            ) -> Result<(), $crate::DatabaseError> {
                $crate::oracle::schema::drop_index(self, statement).await
            }

            #[cfg(feature = "schema")]
            async fn exec_alter_table(
                &self,
                statement: &$crate::schema::AlterTableStatement<'_>,
            ) -> Result<(), $crate::DatabaseError> {
                $crate::oracle::schema::alter_table(self, statement).await
            }

            #[cfg(feature = "schema")]
            async fn table_exists(&self, table: &str) -> Result<bool, $crate::DatabaseError> {
                $crate::oracle::introspection::table_exists(self, table).await
            }

            #[cfg(feature = "schema")]
            async fn list_tables(&self) -> Result<Vec<String>, $crate::DatabaseError> {
                $crate::oracle::introspection::list_tables(self).await
            }

            #[cfg(feature = "schema")]
            async fn get_table_info(
                &self,
                table: &str,
            ) -> Result<Option<$crate::schema::TableInfo>, $crate::DatabaseError> {
                $crate::oracle::introspection::relation_info(
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
                $crate::oracle::introspection::columns(self, None, table).await
            }

            #[cfg(feature = "schema")]
            async fn column_exists(
                &self,
                table: &str,
                column: &str,
            ) -> Result<bool, $crate::DatabaseError> {
                Ok($crate::oracle::introspection::columns(self, None, table)
                    .await?
                    .iter()
                    .any(|info| info.name == column))
            }

            #[cfg(feature = "schema")]
            async fn list_schemas(
                &self,
            ) -> Result<Vec<$crate::schema::SchemaInfo>, $crate::DatabaseError> {
                $crate::oracle::introspection::list_schemas(self).await
            }

            #[cfg(feature = "schema")]
            async fn list_databases(
                &self,
            ) -> Result<Vec<$crate::schema::DatabaseInfo>, $crate::DatabaseError> {
                $crate::oracle::introspection::list_databases(self).await
            }

            #[cfg(feature = "schema")]
            async fn list_relations(
                &self,
                schema: Option<&str>,
            ) -> Result<Vec<$crate::schema::RelationSummary>, $crate::DatabaseError> {
                $crate::oracle::introspection::list_relations(self, schema).await
            }

            #[cfg(feature = "schema")]
            async fn get_relation_info(
                &self,
                relation: &$crate::schema::RelationRef,
            ) -> Result<Option<$crate::schema::TableInfo>, $crate::DatabaseError> {
                $crate::oracle::introspection::relation_info(self, relation).await
            }

            #[cfg(all(feature = "schema", feature = "raw-sql"))]
            async fn list_routines(
                &self,
                schema: Option<&str>,
            ) -> Result<Vec<$crate::schema::RoutineInfo>, $crate::DatabaseError> {
                $crate::oracle::introspection::list_routines(self, schema).await
            }

            #[cfg(feature = "schema")]
            async fn get_view_definition(
                &self,
                relation: &$crate::schema::RelationRef,
            ) -> Result<Option<String>, $crate::DatabaseError> {
                $crate::oracle::introspection::view_definition(self, relation).await
            }
        }
    };
}

pub(crate) use impl_database;

impl_database!(OracleDatabase, {
    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn crate::DatabaseTransaction>, crate::DatabaseError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(crate::DatabaseError::ConnectionClosed);
        }
        Ok(Box::new(OracleTransaction::begin(&self.pool).await?))
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
        let builder = OracleDatabase::builder("db.example")
            .user("shop")
            .password("s3cret-pw");
        let debug = format!("{builder:?}");
        assert!(!debug.contains("s3cret-pw"), "{debug}");
        assert!(debug.contains("<redacted>"), "{debug}");
    }

    #[test]
    fn descriptors_name_the_service_and_tls() {
        let builder = OracleDatabase::builder("db.example")
            .port(1522)
            .service_name("FREEPDB1");
        assert_eq!(
            builder.descriptor().unwrap(),
            "(DESCRIPTION=(CONNECT_TIMEOUT=15)(RETRY_COUNT=0)(ADDRESS=(PROTOCOL=TCP)(HOST=db.example)(PORT=1522))(CONNECT_DATA=(SERVICE_NAME=FREEPDB1)))"
        );
        let builder = OracleDatabase::builder("db.example")
            .sid("ORCL")
            .tls(true)
            .wallet_location("/etc/wallet");
        assert_eq!(
            builder.descriptor().unwrap(),
            "(DESCRIPTION=(CONNECT_TIMEOUT=15)(RETRY_COUNT=0)(ADDRESS=(PROTOCOL=TCPS)(HOST=db.example)(PORT=2484))(CONNECT_DATA=(SID=ORCL))(SECURITY=(SSL_SERVER_DN_MATCH=ON)(MY_WALLET_DIRECTORY=\"/etc/wallet\")))"
        );
        assert_eq!(
            OracleDatabaseBuilder::connect_string("shop_tns")
                .descriptor()
                .unwrap(),
            "shop_tns"
        );
    }

    #[test]
    fn descriptor_injection_is_refused() {
        for builder in [
            OracleDatabase::builder("db)(HOST=evil"),
            OracleDatabase::builder("db").service_name("x)(SERVER=DEDICATED"),
            OracleDatabase::builder(" "),
            OracleDatabase::builder("db").wallet_location("/w"),
            OracleDatabase::builder("db")
                .tls(true)
                .server_cert_dn("CN=a\")(x"),
            OracleDatabaseBuilder::connect_string("alias").tls(true),
        ] {
            assert!(
                matches!(
                    builder.descriptor(),
                    Err(OracleDatabaseError::InvalidConfig(_))
                ),
                "{builder:?}"
            );
        }
    }

    #[test]
    fn server_errors_keep_their_code() {
        let error = OracleDatabaseError::Server {
            code: 1,
            offset: 0,
            message: "ORA-00001: unique constraint (SHOP.PK) violated".to_string(),
        };
        assert_eq!(error.code(), Some(1));
        assert_eq!(error.kind(), Some(crate::DatabaseErrorKind::Constraint));
        assert!(!error.breaks_connection());
        let lost = OracleDatabaseError::Server {
            code: 3113,
            offset: 0,
            message: "ORA-03113: end-of-file on communication channel".to_string(),
        };
        assert!(lost.breaks_connection() && lost.is_connection_error());
        assert!(OracleDatabaseError::Driver("x".to_string()).breaks_connection());
    }
}
