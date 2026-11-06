//! Remote `libSQL` backend: `sqld`, `turso dev` and Turso Cloud databases,
//! reached with Hrana over HTTP.
//!
//! [`LibsqlDatabase`] sends every statement as an HTTP request to the
//! server's `/v2/pipeline` endpoint (see [`hrana`]), authenticating with a
//! bearer token when one is given. TLS comes from the URL: `https://` (and
//! `libsql://`, which means `https://`) use it, `http://` does not.
//!
//! # Statements and transactions
//!
//! Outside a transaction each statement runs on a fresh server stream that
//! is closed in the same request, so nothing (session settings, temporary
//! tables) carries over between statements. A transaction holds one stream
//! open from `BEGIN` to `COMMIT`/`ROLLBACK`; the server closes streams that
//! sit idle for a few seconds, so an open transaction pings its stream in the
//! background until it ends.
//!
//! # Read-only connections
//!
//! `libSQL` servers refuse `PRAGMA query_only` and have no read-only open
//! flag a client can set, so a read-only database
//! ([`LibsqlDatabaseBuilder::read_only`]) enforces read-only access on the
//! server in two ways:
//!
//! * the server first describes the statement, in a request of its own, and
//!   the statement is refused, without ever being sent to run, unless the
//!   server reports it read-only (`sqlite3_stmt_readonly`). The description
//!   has to be its own request: a server runs every request of a pipeline
//!   whatever the earlier ones returned;
//! * a statement that passed then runs inside a transaction (a savepoint
//!   inside a transaction) that is always rolled back in the same request,
//!   so even a statement the description missed changes nothing.
//!
//! This costs one extra round trip per statement on read-only databases.
//!
//! A read-only *auth token* (Turso's `--read-only` tokens, or a JWT with the
//! `ro` claim for `sqld`) adds the server's own authorization on top.
//!
//! # Example
//!
//! ```rust,no_run
//! use switchy_database::{Database, libsql::LibsqlDatabase};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let db = LibsqlDatabase::builder("libsql://shop-example.turso.io")
//!     .auth_token("eyJ...")
//!     .read_only(true)
//!     .build()
//!     .await?;
//! let rows = db.query_raw("SELECT name FROM sqlite_master").await?;
//! # Ok(())
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

pub mod hrana;
#[cfg(feature = "schema")]
mod introspection;
#[cfg(feature = "schema")]
mod schema;
mod sql;
mod transaction;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;

use crate::query_transform::{QuestionMarkHandler, transform_query_for_params};
use crate::sqlite_common::format_sqlite_interval;
use crate::{DatabaseError, DatabaseValue, QueryResult, ResultColumn, Row};

use hrana::{
    Batch, BatchCond, BatchStep, HranaClient, HttpOptions, Request, Response, StmtResult,
    StreamPosition, StreamResult,
};

pub use transaction::{LibsqlSavepoint, LibsqlTransaction};

/// Errors from the `libSQL` backend.
#[derive(Debug, Error)]
pub enum LibsqlDatabaseError {
    /// The server could not be reached, or the HTTP exchange failed.
    #[error("{}", error_chain(.0))]
    Http(#[source] reqwest::Error),
    /// The server refused the auth token (or wanted one).
    #[error("authentication failed: the server refused the auth token ({0})")]
    Unauthorized(String),
    /// The token may not do what was asked (a read-only token writing, say).
    #[error("the server refused: {0}")]
    Forbidden(String),
    /// The server closed the stream because it sat idle; a transaction it
    /// held was rolled back.
    #[error(
        "the server closed the connection after it sat idle; any open transaction was rolled back"
    )]
    StreamExpired,
    /// A request on a transaction's stream got no answer (it timed out, or
    /// the connection dropped), so the transaction can't go on; the server
    /// rolls it back.
    #[error(
        "the transaction was lost when a request in it got no answer; roll it back (the server \
         rolls it back itself)"
    )]
    StreamLost,
    /// The server reported an error (a SQL error, a constraint, ...).
    #[error("{message}")]
    Server {
        /// What went wrong.
        message: String,
        /// The server's error code (`SQLITE_CONSTRAINT_UNIQUE`, ...), if any.
        code: Option<String>,
        /// The failure category the code stands for, if any.
        kind: Option<crate::DatabaseErrorKind>,
    },
    /// A read-only database refused a statement the server reports as a write.
    #[error(
        "read-only connection: the server reports that this statement writes, so it was not run"
    )]
    ReadOnly,
    /// The server's response did not follow the protocol.
    #[error("invalid response from the libSQL server: {0}")]
    Protocol(String),
    /// A value has no Hrana encoding.
    #[error("unsupported value: {0}")]
    UnsupportedValue(String),
    /// The URL is not a `libSQL` server URL.
    #[error("invalid libSQL URL: {0}")]
    InvalidUrl(String),
    /// A setting can't be used (an unreadable CA certificate, say).
    #[error("{0}")]
    InvalidConfig(String),
    /// The server didn't answer a request within the request timeout. The
    /// client stopped waiting; the server may still be running the
    /// statement.
    #[error(
        "timed out: the server didn't answer within {}",
        format_timeout(*.0)
    )]
    Timeout(Duration),
}

/// A timeout for messages: whole seconds where it is a whole number of
/// them, otherwise milliseconds.
fn format_timeout(timeout: Duration) -> String {
    if timeout.subsec_millis() == 0 && timeout.as_secs() > 0 {
        format!("{}s", timeout.as_secs())
    } else {
        format!("{}ms", timeout.as_millis())
    }
}

impl LibsqlDatabaseError {
    fn server(error: hrana::ServerError) -> Self {
        if error.code.as_deref() == Some("STREAM_EXPIRED") {
            return Self::StreamExpired;
        }
        Self::Server {
            kind: error.code.as_deref().and_then(error_kind),
            message: error.message,
            code: error.code,
        }
    }

    /// The server's error code, if the server reported one.
    #[must_use]
    pub fn code(&self) -> Option<&str> {
        match self {
            Self::Server { code, .. } => code.as_deref(),
            _ => None,
        }
    }
}

/// The failure category of a server error code (`SQLite` result codes, as
/// `sqld` reports them).
fn error_kind(code: &str) -> Option<crate::DatabaseErrorKind> {
    use crate::DatabaseErrorKind;

    if code.starts_with("SQLITE_BUSY") {
        Some(DatabaseErrorKind::Busy)
    } else if code.starts_with("SQLITE_LOCKED") {
        Some(DatabaseErrorKind::Locked)
    } else if code.starts_with("SQLITE_CONSTRAINT") {
        Some(DatabaseErrorKind::Constraint)
    } else if code.starts_with("SQLITE_CORRUPT") || code == "SQLITE_NOTADB" {
        Some(DatabaseErrorKind::Corrupt)
    } else {
        None
    }
}

/// An error and its sources (reqwest keeps the useful part in the sources).
fn error_chain(error: &dyn std::error::Error) -> String {
    let mut message = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        let text = cause.to_string();
        if !message.contains(&text) {
            message.push_str(": ");
            message.push_str(&text);
        }
        source = cause.source();
    }
    message
}

impl From<LibsqlDatabaseError> for DatabaseError {
    fn from(error: LibsqlDatabaseError) -> Self {
        Self::Libsql(error)
    }
}

/// The `https://` or `http://` base URL of a `libSQL` server URL:
/// `libsql://` becomes `https://`, a trailing `/` and any query string are
/// dropped.
///
/// # Errors
///
/// * [`LibsqlDatabaseError::InvalidUrl`] for other schemes or a URL without a host
pub fn base_url(url: &str) -> Result<String, LibsqlDatabaseError> {
    let url = url.trim();
    let url = url.split(['?', '#']).next().unwrap_or(url);
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| LibsqlDatabaseError::InvalidUrl("expected scheme://host".to_string()))?;
    let scheme = match scheme.to_ascii_lowercase().as_str() {
        "libsql" | "https" | "wss" => "https",
        "http" | "ws" => "http",
        other => {
            return Err(LibsqlDatabaseError::InvalidUrl(format!(
                "unsupported scheme {other}:// (use libsql://, https:// or http://)"
            )));
        }
    };
    let rest = rest.trim_end_matches('/');
    if rest.is_empty() || rest.starts_with('/') {
        return Err(LibsqlDatabaseError::InvalidUrl(
            "the URL has no host".to_string(),
        ));
    }
    if rest.contains('@') {
        return Err(LibsqlDatabaseError::InvalidUrl(
            "credentials go in the auth token, not the URL's user info".to_string(),
        ));
    }
    Ok(format!("{scheme}://{rest}"))
}

/// Builds a [`LibsqlDatabase`].
#[derive(Clone)]
pub struct LibsqlDatabaseBuilder {
    url: String,
    auth_token: Option<String>,
    read_only: bool,
    connect_timeout: Option<Duration>,
    request_timeout: Option<Duration>,
    connect_address: Option<SocketAddr>,
    root_certificate: Option<PathBuf>,
}

impl std::fmt::Debug for LibsqlDatabaseBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LibsqlDatabaseBuilder")
            .field("url", &self.url)
            .field(
                "auth_token",
                &self.auth_token.as_ref().map(|_| "<redacted>"),
            )
            .field("read_only", &self.read_only)
            .field("connect_timeout", &self.connect_timeout)
            .field("request_timeout", &self.request_timeout)
            .field("connect_address", &self.connect_address)
            .field("root_certificate", &self.root_certificate)
            .finish()
    }
}

impl LibsqlDatabaseBuilder {
    /// A builder for the server at `url` (`libsql://`, `https://` or `http://`).
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            auth_token: None,
            read_only: false,
            connect_timeout: None,
            request_timeout: None,
            connect_address: None,
            root_certificate: None,
        }
    }

    /// Authenticate with `token` (sent as `Authorization: Bearer <token>`).
    #[must_use]
    pub fn auth_token(mut self, token: impl Into<String>) -> Self {
        self.auth_token = Some(token.into());
        self
    }

    /// Refuse writes (see the [module documentation](self)).
    #[must_use]
    pub const fn read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// How long to wait for the TCP/TLS connection to the server.
    #[must_use]
    pub const fn connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = Some(timeout);
        self
    }

    /// How long to wait for each request's whole response, so for each
    /// statement (Hrana has no server-side statement timeout). A request
    /// that runs longer fails with [`LibsqlDatabaseError::Timeout`]; the
    /// server is not told, and may keep running the statement.
    #[must_use]
    pub const fn request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }

    /// Connect to `address` (an SSH tunnel's local end, a port forward)
    /// instead of the address the URL's host resolves to. TLS still checks
    /// the server's certificate against the URL's host name and sends it as
    /// SNI, and the `Host` header still names the URL's host and port. An
    /// `https://` (or `libsql://`) URL must then name its server by host
    /// name, not IP address.
    #[must_use]
    pub const fn connect_address(mut self, address: SocketAddr) -> Self {
        self.connect_address = Some(address);
        self
    }

    /// Trust the certificate authorities in the PEM file at `path`, in
    /// addition to the system's, when checking the server's certificate
    /// (a server whose certificate a private CA issued). Needs TLS.
    #[must_use]
    pub fn root_certificate(mut self, path: impl Into<PathBuf>) -> Self {
        self.root_certificate = Some(path.into());
        self
    }

    /// Connect, checking that the server answers (and accepts the token).
    ///
    /// # Errors
    ///
    /// * [`LibsqlDatabaseError::InvalidUrl`] if the URL is not a `libSQL` server
    ///   URL, or names its TLS server by IP address with a connect address
    /// * [`LibsqlDatabaseError::InvalidConfig`] if the root certificate can't be
    ///   read or holds no PEM certificate, or is given for a plain `http://` URL
    /// * [`LibsqlDatabaseError::Http`] if the server can't be reached
    /// * [`LibsqlDatabaseError::Timeout`] if the server doesn't answer within the
    ///   request timeout
    /// * [`LibsqlDatabaseError::Unauthorized`] if the server refuses the token
    /// * [`LibsqlDatabaseError::Server`] / [`LibsqlDatabaseError::Protocol`] if
    ///   the server does not answer a `SELECT 1` as a `libSQL` server would
    pub async fn build(self) -> Result<LibsqlDatabase, LibsqlDatabaseError> {
        let base = base_url(&self.url)?;
        let mut options = HttpOptions::new();
        if let Some(timeout) = self.connect_timeout {
            options = options.connect_timeout(timeout);
        }
        if let Some(timeout) = self.request_timeout {
            options = options.request_timeout(timeout);
        }
        if let Some(address) = self.connect_address {
            options = options.connect_address(address);
        }
        if let Some(path) = &self.root_certificate {
            if !base.starts_with("https://") {
                return Err(LibsqlDatabaseError::InvalidConfig(
                    "a CA certificate needs TLS (an https:// or libsql:// URL)".to_string(),
                ));
            }
            let pem = std::fs::read(path).map_err(|error| {
                LibsqlDatabaseError::InvalidConfig(format!(
                    "can't read the CA certificate {}: {error}",
                    path.display()
                ))
            })?;
            options = options.root_certificates(pem);
        }
        let client = HranaClient::new(&base, self.auth_token, &options)?;
        let db = LibsqlDatabase {
            client: Arc::new(client),
            read_only: self.read_only,
            closed: AtomicBool::new(false),
        };
        run_autocommit(&db.client, db.read_only, hrana::Stmt::new("SELECT 1")).await?;
        log::debug!(
            "libsql: connected to {base} ({})",
            if db.read_only {
                "read-only"
            } else {
                "read-write"
            }
        );
        Ok(db)
    }
}

/// A remote `libSQL` database.
#[derive(Debug)]
pub struct LibsqlDatabase {
    client: Arc<HranaClient>,
    read_only: bool,
    closed: AtomicBool,
}

impl LibsqlDatabase {
    /// A builder for the server at `url`.
    #[must_use]
    pub fn builder(url: impl Into<String>) -> LibsqlDatabaseBuilder {
        LibsqlDatabaseBuilder::new(url)
    }

    /// Whether the database refuses writes.
    #[must_use]
    pub const fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// The server's base URL.
    #[must_use]
    pub fn base_url(&self) -> &str {
        self.client.base_url()
    }
}

/// A statement's decoded result.
#[derive(Debug, Default)]
pub(crate) struct Outcome {
    columns: Vec<ResultColumn>,
    rows: Vec<Vec<DatabaseValue>>,
    /// Rows changed; only `exec_raw_params` (a `raw-sql` method) reports it.
    #[cfg_attr(not(feature = "raw-sql"), allow(dead_code))]
    affected: u64,
}

impl Outcome {
    fn decode(result: StmtResult) -> Result<Self, LibsqlDatabaseError> {
        let columns = result
            .cols
            .into_iter()
            .map(|col| ResultColumn {
                name: col.name.unwrap_or_default(),
                type_name: col.decltype.filter(|decltype| !decltype.is_empty()),
            })
            .collect();
        let rows = result
            .rows
            .into_iter()
            .map(|row| {
                row.into_iter()
                    .map(hrana::Value::into_database_value)
                    .collect::<Result<Vec<_>, _>>()
            })
            .collect::<Result<_, _>>()?;
        Ok(Self {
            columns,
            rows,
            affected: result.affected_row_count,
        })
    }

    fn into_result(self) -> QueryResult {
        QueryResult {
            columns: self.columns,
            rows: self.rows,
        }
    }

    fn into_rows(self) -> Vec<Row> {
        self.into_result().into_rows()
    }
}

/// Runs statements: on fresh streams ([`LibsqlDatabase`]) or on a
/// transaction's stream ([`LibsqlTransaction`]).
#[async_trait]
pub(crate) trait Executor: Send + Sync {
    /// Run one statement with `?` (or `$n`) parameters.
    async fn run(
        &self,
        sql: &str,
        params: &[DatabaseValue],
        want_rows: bool,
    ) -> Result<Outcome, DatabaseError>;
}

/// `sql` and `params` as a Hrana statement: `$n` placeholders become `?`,
/// and `Now`/`NowPlus` become `SQLite` date expressions.
fn statement(
    sql: &str,
    params: &[DatabaseValue],
    want_rows: bool,
) -> Result<hrana::Stmt, DatabaseError> {
    let (sql, params) =
        transform_query_for_params(sql, params, &QuestionMarkHandler, |param| match param {
            DatabaseValue::Now => Some("datetime('now')".to_string()),
            DatabaseValue::NowPlus(interval) => Some(format!(
                "datetime('now', {})",
                format_sqlite_interval(interval)
                    .iter()
                    .map(|modifier| format!("'{modifier}'"))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
            _ => None,
        })
        .map_err(DatabaseError::QueryFailed)?;
    Ok(hrana::Stmt::with_params(sql, &params, want_rows)?)
}

/// The savepoint a read-only transaction runs each statement in.
const READ_ONLY_SAVEPOINT: &str = "switchy_read_only";

/// The request that asks the server to describe `sql`; see
/// [`describe_result`].
///
/// It is always sent in a pipeline of its own, before the statement: a
/// server runs every request of a pipeline whatever the earlier ones
/// returned, so a statement batched after a failed check would still run.
fn describe_request(sql: &str) -> Request {
    Request::Describe {
        sql: sql.to_string(),
    }
}

/// Whether the response to a `describe` request reports a read-only
/// statement.
fn describe_result(result: Option<StreamResult>) -> Result<(), LibsqlDatabaseError> {
    match result
        .ok_or_else(|| LibsqlDatabaseError::Protocol("missing result".to_string()))?
        .into_response()?
    {
        Response::Describe { result } if result.is_readonly => Ok(()),
        Response::Describe { .. } => Err(LibsqlDatabaseError::ReadOnly),
        other => Err(LibsqlDatabaseError::Protocol(format!(
            "expected a describe response, got {other:?}"
        ))),
    }
}

/// The request that runs `stmt` between `open` and the always-run `close`
/// statements (a transaction or savepoint that is rolled back), only once
/// `open` succeeded. Sent only after [`describe_result`] passed.
fn read_only_batch(stmt: hrana::Stmt, open: &str, close: &[&str]) -> Request {
    let mut steps = vec![
        BatchStep {
            condition: None,
            stmt: hrana::Stmt::new(open),
        },
        BatchStep {
            condition: Some(BatchCond::Ok { step: 0 }),
            stmt,
        },
    ];
    steps.extend(close.iter().map(|sql| BatchStep {
        condition: None,
        stmt: hrana::Stmt::new(*sql),
    }));
    Request::Batch {
        batch: Batch { steps },
    }
}

/// The statement's result from the response to a [`read_only_batch`].
fn read_only_result(result: Option<StreamResult>) -> Result<StmtResult, LibsqlDatabaseError> {
    let missing = || LibsqlDatabaseError::Protocol("missing result".to_string());
    let Response::Batch { result } = result.ok_or_else(missing)?.into_response()? else {
        return Err(LibsqlDatabaseError::Protocol(
            "expected a batch response".to_string(),
        ));
    };
    for step in 0..2 {
        if let Some(Some(error)) = result.step_errors.get(step) {
            return Err(LibsqlDatabaseError::server(error.clone()));
        }
    }
    result
        .step_results
        .into_iter()
        .nth(1)
        .flatten()
        .ok_or_else(missing)
}

/// The statement's result from the response to an `execute` request.
fn execute_result(result: Option<StreamResult>) -> Result<StmtResult, LibsqlDatabaseError> {
    match result
        .ok_or_else(|| LibsqlDatabaseError::Protocol("missing result".to_string()))?
        .into_response()?
    {
        Response::Execute { result } => Ok(result),
        other => Err(LibsqlDatabaseError::Protocol(format!(
            "expected an execute response, got {other:?}"
        ))),
    }
}

/// Run `stmt` on a fresh stream, closed in the same request.
async fn run_autocommit(
    client: &HranaClient,
    read_only: bool,
    stmt: hrana::Stmt,
) -> Result<StmtResult, LibsqlDatabaseError> {
    let mut position = StreamPosition::default();
    if read_only {
        // The check runs on a stream of its own, closed in the same request,
        // and the statement is sent only once it passed.
        let check = [describe_request(&stmt.sql), Request::Close];
        describe_result(
            client
                .pipeline(&mut StreamPosition::default(), &check)
                .await?
                .into_iter()
                .next(),
        )?;
        let requests = [
            read_only_batch(stmt, "BEGIN", &["ROLLBACK"]),
            Request::Close,
        ];
        read_only_result(
            client
                .pipeline(&mut position, &requests)
                .await?
                .into_iter()
                .next(),
        )
    } else {
        let requests = [Request::Execute { stmt }, Request::Close];
        execute_result(
            client
                .pipeline(&mut position, &requests)
                .await?
                .into_iter()
                .next(),
        )
    }
}

#[async_trait]
impl Executor for LibsqlDatabase {
    async fn run(
        &self,
        sql: &str,
        params: &[DatabaseValue],
        want_rows: bool,
    ) -> Result<Outcome, DatabaseError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(DatabaseError::ConnectionClosed);
        }
        let stmt = statement(sql, params, want_rows)?;
        log::trace!("libsql: {}", stmt.sql);
        let result = run_autocommit(&self.client, self.read_only, stmt).await?;
        Ok(Outcome::decode(result)?)
    }
}

/// Implement [`crate::Database`] for an [`Executor`], with the given extra items.
macro_rules! impl_database {
    ($type:ty, { $($extra:tt)* }) => {
        #[async_trait::async_trait]
        impl $crate::Database for $type {
            $($extra)*

            async fn check_connection(&self) -> Result<(), $crate::DatabaseError> {
                $crate::libsql::Executor::run(self, "SELECT 1", &[], true).await.map(|_| ())
            }

            #[cfg(feature = "raw-sql")]
            async fn query_raw(&self, query: &str) -> Result<Vec<$crate::Row>, $crate::DatabaseError> {
                Ok($crate::libsql::Executor::run(self, query, &[], true).await?.into_rows())
            }

            #[cfg(feature = "raw-sql")]
            async fn query_raw_params(
                &self,
                query: &str,
                params: &[$crate::DatabaseValue],
            ) -> Result<Vec<$crate::Row>, $crate::DatabaseError> {
                Ok($crate::libsql::Executor::run(self, query, params, true).await?.into_rows())
            }

            #[cfg(feature = "raw-sql")]
            async fn query_raw_result(
                &self,
                query: &str,
                params: &[$crate::DatabaseValue],
            ) -> Result<$crate::QueryResult, $crate::DatabaseError> {
                Ok($crate::libsql::Executor::run(self, query, params, true).await?.into_result())
            }

            #[cfg(feature = "raw-sql")]
            async fn exec_raw(&self, statement: &str) -> Result<(), $crate::DatabaseError> {
                $crate::libsql::Executor::run(self, statement, &[], false).await.map(|_| ())
            }

            #[cfg(feature = "raw-sql")]
            async fn exec_raw_params(
                &self,
                query: &str,
                params: &[$crate::DatabaseValue],
            ) -> Result<u64, $crate::DatabaseError> {
                Ok($crate::libsql::Executor::run(self, query, params, false).await?.affected)
            }

            async fn query(
                &self,
                query: &$crate::query::SelectQuery<'_>,
            ) -> Result<Vec<$crate::Row>, $crate::DatabaseError> {
                $crate::libsql::sql::select(self, query).await
            }

            async fn query_first(
                &self,
                query: &$crate::query::SelectQuery<'_>,
            ) -> Result<Option<$crate::Row>, $crate::DatabaseError> {
                $crate::libsql::sql::select_first(self, query).await
            }

            async fn exec_update(
                &self,
                statement: &$crate::query::UpdateStatement<'_>,
            ) -> Result<Vec<$crate::Row>, $crate::DatabaseError> {
                $crate::libsql::sql::update(
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
                Ok($crate::libsql::sql::update(
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
                $crate::libsql::sql::insert(self, statement.table_name, &statement.values).await
            }

            async fn exec_upsert(
                &self,
                statement: &$crate::query::UpsertStatement<'_>,
            ) -> Result<Vec<$crate::Row>, $crate::DatabaseError> {
                $crate::libsql::sql::upsert(
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
                $crate::libsql::sql::upsert(
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
                $crate::libsql::sql::upsert_multi(
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
                $crate::libsql::sql::delete(
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
                Ok($crate::libsql::sql::delete(self, statement.table_name, statement.filters.as_deref(), Some(1))
                    .await?
                    .into_iter()
                    .next())
            }

            #[cfg(feature = "schema")]
            async fn exec_create_table(
                &self,
                statement: &$crate::schema::CreateTableStatement<'_>,
            ) -> Result<(), $crate::DatabaseError> {
                let sql = $crate::libsql::schema::create_table_sql(statement)?;
                $crate::libsql::Executor::run(self, &sql, &[], false).await.map(|_| ())
            }

            #[cfg(feature = "schema")]
            async fn exec_drop_table(
                &self,
                statement: &$crate::schema::DropTableStatement<'_>,
            ) -> Result<(), $crate::DatabaseError> {
                $crate::libsql::schema::drop_table(self, statement).await
            }

            #[cfg(feature = "schema")]
            async fn exec_create_index(
                &self,
                statement: &$crate::schema::CreateIndexStatement<'_>,
            ) -> Result<(), $crate::DatabaseError> {
                let sql = $crate::libsql::schema::create_index_sql(statement)?;
                $crate::libsql::Executor::run(self, &sql, &[], false).await.map(|_| ())
            }

            #[cfg(feature = "schema")]
            async fn exec_drop_index(
                &self,
                statement: &$crate::schema::DropIndexStatement<'_>,
            ) -> Result<(), $crate::DatabaseError> {
                let sql = $crate::libsql::schema::drop_index_sql(statement);
                $crate::libsql::Executor::run(self, &sql, &[], false).await.map(|_| ())
            }

            #[cfg(feature = "schema")]
            async fn exec_alter_table(
                &self,
                statement: &$crate::schema::AlterTableStatement<'_>,
            ) -> Result<(), $crate::DatabaseError> {
                for sql in $crate::libsql::schema::alter_table_sql(statement)? {
                    $crate::libsql::Executor::run(self, &sql, &[], false).await?;
                }
                Ok(())
            }

            #[cfg(feature = "schema")]
            async fn table_exists(&self, table: &str) -> Result<bool, $crate::DatabaseError> {
                $crate::libsql::introspection::table_exists(self, table).await
            }

            #[cfg(feature = "schema")]
            async fn list_tables(&self) -> Result<Vec<String>, $crate::DatabaseError> {
                $crate::libsql::introspection::list_tables(self).await
            }

            #[cfg(feature = "schema")]
            async fn get_table_info(
                &self,
                table: &str,
            ) -> Result<Option<$crate::schema::TableInfo>, $crate::DatabaseError> {
                $crate::libsql::introspection::table_info(self, table).await
            }

            #[cfg(feature = "schema")]
            async fn get_table_columns(
                &self,
                table: &str,
            ) -> Result<Vec<$crate::schema::ColumnInfo>, $crate::DatabaseError> {
                $crate::libsql::introspection::columns(self, None, table).await
            }

            #[cfg(feature = "schema")]
            async fn column_exists(
                &self,
                table: &str,
                column: &str,
            ) -> Result<bool, $crate::DatabaseError> {
                Ok($crate::libsql::introspection::columns(self, None, table)
                    .await?
                    .iter()
                    .any(|info| info.name == column))
            }

            #[cfg(feature = "schema")]
            async fn list_schemas(
                &self,
            ) -> Result<Vec<$crate::schema::SchemaInfo>, $crate::DatabaseError> {
                $crate::libsql::introspection::list_schemas(self).await
            }

            #[cfg(feature = "schema")]
            async fn list_databases(
                &self,
            ) -> Result<Vec<$crate::schema::DatabaseInfo>, $crate::DatabaseError> {
                $crate::libsql::introspection::list_schemas(self)
                    .await
                    .map($crate::sqlite_common::sqlite_databases)
            }

            #[cfg(feature = "schema")]
            async fn list_relations(
                &self,
                schema: Option<&str>,
            ) -> Result<Vec<$crate::schema::RelationSummary>, $crate::DatabaseError> {
                $crate::libsql::introspection::list_relations(self, schema).await
            }

            #[cfg(feature = "schema")]
            async fn list_relation_statistics(
                &self,
                schema: Option<&str>,
            ) -> Result<Vec<$crate::schema::RelationStatistics>, $crate::DatabaseError> {
                $crate::libsql::introspection::list_relation_statistics(self, schema).await
            }

            #[cfg(feature = "schema")]
            async fn get_relation_info(
                &self,
                relation: &$crate::schema::RelationRef,
            ) -> Result<Option<$crate::schema::TableInfo>, $crate::DatabaseError> {
                $crate::libsql::introspection::relation_info(self, relation).await
            }

            #[cfg(all(feature = "schema", feature = "raw-sql"))]
            async fn list_routines(
                &self,
                schema: Option<&str>,
            ) -> Result<Vec<$crate::schema::RoutineInfo>, $crate::DatabaseError> {
                // SQLite functions are not per schema.
                let _ = schema;
                let result = $crate::libsql::Executor::run(
                    self,
                    &$crate::schema::routines_sql::sqlite(),
                    &[],
                    true,
                )
                .await?;
                Ok($crate::schema::routines_from_rows(&result.rows))
            }

            #[cfg(all(feature = "schema", feature = "raw-sql"))]
            async fn list_triggers(
                &self,
                schema: Option<&str>,
            ) -> Result<Vec<$crate::schema::TriggerInfo>, $crate::DatabaseError> {
                $crate::schema::objects::list(
                    self,
                    &$crate::schema::triggers_sql::sqlite(schema),
                    $crate::schema::triggers_from_rows,
                )
                .await
            }

            #[cfg(all(feature = "schema", feature = "raw-sql"))]
            async fn list_foreign_keys(
                &self,
                scope: &$crate::schema::ForeignKeyScope,
            ) -> Result<Vec<$crate::schema::TableForeignKey>, $crate::DatabaseError> {
                let (sql, params) = $crate::schema::foreign_keys_sql::sqlite(scope);
                let result = $crate::libsql::Executor::run(self, &sql, &params, true).await?;
                Ok($crate::schema::foreign_keys_sql::foreign_keys_from_rows(
                    &result.rows,
                ))
            }

            #[cfg(feature = "schema")]
            async fn get_view_definition(
                &self,
                relation: &$crate::schema::RelationRef,
            ) -> Result<Option<String>, $crate::DatabaseError> {
                $crate::libsql::introspection::view_definition(self, relation).await
            }
        }
    };
}

pub(crate) use impl_database;

impl_database!(LibsqlDatabase, {
    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn crate::DatabaseTransaction>, crate::DatabaseError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(crate::DatabaseError::ConnectionClosed);
        }
        Ok(Box::new(
            LibsqlTransaction::begin(Arc::clone(&self.client), self.read_only).await?,
        ))
    }

    async fn close(&self) -> Result<(), crate::DatabaseError> {
        self.closed.store(true, Ordering::SeqCst);
        Ok(())
    }
});

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[tokio::test]
    async fn tls_settings_are_checked_before_connecting() {
        let error = LibsqlDatabase::builder("https://192.0.2.10:8443")
            .connect_address("127.0.0.1:1".parse().unwrap())
            .build()
            .await
            .unwrap_err();
        assert!(
            matches!(&error, LibsqlDatabaseError::InvalidUrl(message) if message.contains("host name")),
            "{error}"
        );
        let error = LibsqlDatabase::builder("http://db.example.invalid")
            .root_certificate("/nonexistent/ca.pem")
            .build()
            .await
            .unwrap_err();
        assert!(error.to_string().contains("needs TLS"), "{error}");
        let error = LibsqlDatabase::builder("libsql://db.example.invalid")
            .root_certificate("/nonexistent/ca.pem")
            .build()
            .await
            .unwrap_err();
        assert!(error.to_string().contains("/nonexistent/ca.pem"), "{error}");
        let dir = std::env::temp_dir().join(format!("switchy-libsql-ca-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let not_pem = dir.join("ca.pem");
        std::fs::write(&not_pem, "not a certificate").unwrap();
        let error = LibsqlDatabase::builder("libsql://db.example.invalid")
            .root_certificate(&not_pem)
            .build()
            .await
            .unwrap_err();
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(
            matches!(error, LibsqlDatabaseError::InvalidConfig(_)),
            "{error}"
        );
    }

    #[test]
    fn timeouts_read_in_seconds_or_milliseconds() {
        assert_eq!(format_timeout(Duration::from_secs(30)), "30s");
        assert_eq!(format_timeout(Duration::from_millis(1500)), "1500ms");
        assert_eq!(format_timeout(Duration::from_millis(250)), "250ms");
    }

    #[test]
    fn libsql_urls_become_http_base_urls() {
        assert_eq!(
            base_url("libsql://shop-example.turso.io").unwrap(),
            "https://shop-example.turso.io"
        );
        assert_eq!(
            base_url("http://127.0.0.1:8080/?authToken=x").unwrap(),
            "http://127.0.0.1:8080"
        );
        assert_eq!(
            base_url("HTTPS://db.example:8443").unwrap(),
            "https://db.example:8443"
        );
        assert!(base_url("postgres://db").is_err());
        assert!(base_url("http://").is_err());
        assert!(base_url("https://user:pw@db.example").is_err());
    }

    #[test]
    fn read_only_statements_are_described_alone_then_rolled_back() {
        let describe = serde_json::to_string(&describe_request("DELETE FROM t")).unwrap();
        assert_eq!(describe, r#"{"type":"describe","sql":"DELETE FROM t"}"#);

        let batch = read_only_batch(hrana::Stmt::new("DELETE FROM t"), "BEGIN", &["ROLLBACK"]);
        let json = serde_json::to_string(&batch).unwrap();
        assert_eq!(
            json,
            concat!(
                r#"{"type":"batch","batch":{"steps":["#,
                r#"{"stmt":{"sql":"BEGIN","want_rows":true}},"#,
                r#"{"condition":{"type":"ok","step":0},"stmt":{"sql":"DELETE FROM t","want_rows":true}},"#,
                r#"{"stmt":{"sql":"ROLLBACK","want_rows":true}}]}}"#
            )
        );
    }

    fn parse(json: &str) -> StreamResult {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn describe_results_refuse_writes() {
        let write = parse(
            r#"{"type":"ok","response":{"type":"describe","result":{"params":[],"cols":[],"is_explain":false,"is_readonly":false}}}"#,
        );
        assert!(matches!(
            describe_result(Some(write)),
            Err(LibsqlDatabaseError::ReadOnly)
        ));

        let read = parse(
            r#"{"type":"ok","response":{"type":"describe","result":{"params":[],"cols":[],"is_explain":false,"is_readonly":true}}}"#,
        );
        assert!(describe_result(Some(read)).is_ok());

        let failed = parse(
            r#"{"type":"error","error":{"message":"SQL input error: near \"selec\"","code":"SQL_INPUT_ERROR"}}"#,
        );
        assert!(matches!(
            describe_result(Some(failed)),
            Err(LibsqlDatabaseError::Server { code: Some(code), .. }) if code == "SQL_INPUT_ERROR"
        ));
    }

    #[test]
    fn read_only_results_take_the_statement_step() {
        let read = parse(
            r#"{"type":"ok","response":{"type":"batch","result":{"step_results":[{"cols":[],"rows":[]},{"cols":[{"name":"n","decltype":null}],"rows":[[{"type":"integer","value":"3"}]]},null],"step_errors":[null,null,{"message":"no transaction is active"}]}}}"#,
        );
        let result = read_only_result(Some(read)).unwrap();
        assert_eq!(result.rows.len(), 1);

        let failed = parse(
            r#"{"type":"ok","response":{"type":"batch","result":{"step_results":[{"cols":[],"rows":[]},null,{"cols":[],"rows":[]}],"step_errors":[null,{"message":"no such table: t","code":"SQLITE_ERROR"},null]}}}"#,
        );
        assert!(matches!(
            read_only_result(Some(failed)),
            Err(LibsqlDatabaseError::Server { .. })
        ));
    }

    /// Read-only checks against a stand-in server (they run statements
    /// through the raw SQL API).
    #[cfg(feature = "raw-sql")]
    mod stub {
        use std::sync::Arc;

        use super::LibsqlDatabase;

        /// The result a stand-in server gives one request: `describe` reports
        /// only `SELECT`s as read-only.
        fn stub_result(request: &serde_json::Value) -> serde_json::Value {
            let response = match request["type"].as_str() {
                Some("describe") => {
                    let sql = request["sql"].as_str().unwrap_or_default();
                    let read_only = sql.trim_start().to_uppercase().starts_with("SELECT");
                    serde_json::json!({"type": "describe", "result": {
                        "params": [], "cols": [], "is_explain": false, "is_readonly": read_only
                    }})
                }
                Some("batch") => {
                    let steps = request["batch"]["steps"].as_array().map_or(0, Vec::len);
                    serde_json::json!({"type": "batch", "result": {
                        "step_results": vec![serde_json::json!({"cols": [], "rows": []}); steps],
                        "step_errors": vec![serde_json::Value::Null; steps],
                    }})
                }
                Some("execute") => {
                    serde_json::json!({"type": "execute", "result": {"cols": [], "rows": []}})
                }
                _ => serde_json::json!({"type": "close"}),
            };
            serde_json::json!({"type": "ok", "response": response})
        }

        /// What a stand-in server saw: every pipeline body, and the `Host`
        /// header of every request.
        #[derive(Default)]
        struct Seen {
            bodies: std::sync::Mutex<Vec<serde_json::Value>>,
            hosts: std::sync::Mutex<Vec<String>>,
        }

        /// A stand-in `libSQL` server on a free local port that answers with
        /// [`stub_result`] and records every pipeline it receives.
        async fn stub_server() -> (String, Arc<Seen>) {
            let (address, seen) = stub_server_with(std::time::Duration::ZERO).await;
            (format!("http://{address}"), seen)
        }

        /// [`stub_server`], answering any pipeline whose body mentions
        /// `slow` only after `delay`, and recording `Host` headers too.
        async fn stub_server_with(delay: std::time::Duration) -> (std::net::SocketAddr, Arc<Seen>) {
            use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let address = listener.local_addr().unwrap();
            let seen = Arc::new(Seen::default());
            let recorded = Arc::clone(&seen);
            tokio::spawn(async move {
                while let Ok((mut socket, _)) = listener.accept().await {
                    let recorded = Arc::clone(&recorded);
                    tokio::spawn(async move {
                        let mut buffer = Vec::new();
                        let mut chunk = [0_u8; 4096];
                        loop {
                            let Ok(read) = socket.read(&mut chunk).await else {
                                return;
                            };
                            if read == 0 {
                                return;
                            }
                            buffer.extend_from_slice(&chunk[..read]);
                            let Some(end) = buffer.windows(4).position(|w| w == b"\r\n\r\n") else {
                                continue;
                            };
                            let headers = String::from_utf8_lossy(&buffer[..end]).to_lowercase();
                            if let Some(host) =
                                headers.lines().find_map(|line| line.strip_prefix("host:"))
                            {
                                recorded.hosts.lock().unwrap().push(host.trim().to_string());
                            }
                            let length = headers
                                .lines()
                                .find_map(|line| line.strip_prefix("content-length:"))
                                .map_or(0, |value| value.trim().parse::<usize>().unwrap());
                            let start = end + 4;
                            if buffer.len() < start + length {
                                continue;
                            }
                            let body: serde_json::Value =
                                serde_json::from_slice(&buffer[start..start + length]).unwrap();
                            buffer.drain(..start + length);
                            let requests = body["requests"].as_array().cloned().unwrap_or_default();
                            let closes = requests.iter().any(|request| request["type"] == "close");
                            let results: Vec<_> = requests.iter().map(stub_result).collect();
                            if body.to_string().contains("slow") {
                                tokio::time::sleep(delay).await;
                            }
                            recorded.bodies.lock().unwrap().push(body);
                            let response = serde_json::json!({
                                "baton": if closes { None } else { Some("stub") },
                                "base_url": null,
                                "results": results,
                            })
                            .to_string();
                            let reply = format!(
                                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\n\
                                 content-length: {}\r\n\r\n{response}",
                                response.len()
                            );
                            if socket.write_all(reply.as_bytes()).await.is_err() {
                                return;
                            }
                        }
                    });
                }
            });
            (address, seen)
        }

        /// The SQL of every statement the server was asked to run (not just
        /// describe) in the recorded pipelines.
        fn run_statements(seen: &Seen) -> Vec<String> {
            let mut statements = Vec::new();
            for body in seen.bodies.lock().unwrap().iter() {
                for request in body["requests"].as_array().into_iter().flatten() {
                    let stmts = match request["type"].as_str() {
                        Some("execute") => vec![&request["stmt"]],
                        Some("batch") => request["batch"]["steps"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(|step| &step["stmt"])
                            .collect(),
                        _ => Vec::new(),
                    };
                    statements.extend(
                        stmts
                            .into_iter()
                            .filter_map(|stmt| stmt["sql"].as_str().map(str::to_string)),
                    );
                }
            }
            statements
        }

        #[tokio::test]
        async fn read_only_writes_the_server_describes_are_never_sent() {
            use crate::Database as _;

            let (url, seen) = stub_server().await;
            let db = LibsqlDatabase::builder(url)
                .read_only(true)
                .build()
                .await
                .unwrap();

            let error = db.exec_raw("DELETE FROM albums").await.unwrap_err();
            assert!(error.to_string().contains("read-only"), "{error}");

            let tx = db.begin_transaction().await.unwrap();
            assert!(tx.exec_raw("UPDATE albums SET price = 0").await.is_err());
            tx.query_raw("SELECT 2").await.unwrap();
            tx.rollback().await.unwrap();

            let statements = run_statements(&seen);
            assert!(
                statements.iter().all(|sql| !sql.contains("albums")),
                "a refused write was sent to run: {statements:?}"
            );
            // Reads described as read-only do run.
            assert!(
                statements.iter().any(|sql| sql == "SELECT 1"),
                "{statements:?}"
            );
            assert!(
                statements.iter().any(|sql| sql == "SELECT 2"),
                "{statements:?}"
            );
        }

        #[tokio::test]
        async fn a_connect_address_reaches_the_server_by_its_host_name() {
            use crate::Database as _;

            let (address, seen) = stub_server_with(std::time::Duration::ZERO).await;
            // Nothing resolves this name: every request must go to `address`.
            let db = LibsqlDatabase::builder("http://db.example.invalid:8080")
                .connect_address(address)
                .build()
                .await
                .unwrap();
            db.query_raw("SELECT 2").await.unwrap();
            assert_eq!(db.base_url(), "http://db.example.invalid:8080");
            let hosts = seen.hosts.lock().unwrap().clone();
            // The connect probe and the query.
            assert_eq!(hosts, ["db.example.invalid:8080"; 2]);

            // An IP address host is sent to the connect address too.
            let (address, seen) = stub_server_with(std::time::Duration::ZERO).await;
            LibsqlDatabase::builder("http://192.0.2.10:8080")
                .connect_address(address)
                .build()
                .await
                .unwrap();
            assert_eq!(
                seen.hosts.lock().unwrap().as_slice(),
                ["192.0.2.10:8080".to_string()]
            );
        }

        #[tokio::test]
        async fn requests_that_outlast_the_request_timeout_fail_as_timeouts() {
            use crate::Database as _;

            let (address, _) = stub_server_with(std::time::Duration::from_secs(5)).await;
            let db = LibsqlDatabase::builder(format!("http://{address}"))
                .request_timeout(std::time::Duration::from_millis(300))
                .build()
                .await
                .unwrap();
            let started = std::time::Instant::now();
            let error = db.query_raw("SELECT 'slow'").await.unwrap_err();
            assert!(started.elapsed() < std::time::Duration::from_secs(4));
            assert!(
                matches!(
                    &error,
                    crate::DatabaseError::Libsql(super::super::LibsqlDatabaseError::Timeout(t))
                        if *t == std::time::Duration::from_millis(300)
                ),
                "{error:?}"
            );
            assert!(error.to_string().contains("timed out"), "{error}");
            assert!(error.to_string().contains("300ms"), "{error}");
            // Quick statements still run.
            db.query_raw("SELECT 3").await.unwrap();
        }

        #[tokio::test]
        async fn a_transaction_a_request_timed_out_in_is_lost_and_rolls_back() {
            use crate::Database as _;

            let (address, seen) = stub_server_with(std::time::Duration::from_secs(5)).await;
            let db = LibsqlDatabase::builder(format!("http://{address}"))
                .request_timeout(std::time::Duration::from_millis(300))
                .build()
                .await
                .unwrap();
            let tx = db.begin_transaction().await.unwrap();
            let error = tx.query_raw("SELECT 'slow'").await.unwrap_err();
            assert!(error.to_string().contains("timed out"), "{error}");
            let error = tx.query_raw("SELECT 4").await.unwrap_err();
            assert!(
                matches!(
                    &error,
                    crate::DatabaseError::Libsql(super::super::LibsqlDatabaseError::StreamLost)
                ),
                "{error:?}"
            );
            assert!(error.is_connection_error(), "{error:?}");
            // Its baton never arrived, so rolling back sends nothing: the
            // server rolls the transaction back itself.
            tx.rollback().await.unwrap();
            let statements = run_statements(&seen);
            assert!(
                statements
                    .iter()
                    .all(|sql| sql != "ROLLBACK" && sql != "SELECT 4"),
                "{statements:?}"
            );

            // Committing a lost transaction fails.
            let tx = db.begin_transaction().await.unwrap();
            assert!(tx.query_raw("SELECT 'slow'").await.is_err());
            let error = tx.commit().await.unwrap_err();
            assert!(
                matches!(
                    &error,
                    crate::DatabaseError::Libsql(super::super::LibsqlDatabaseError::StreamLost)
                ),
                "{error:?}"
            );
        }
    }
}
