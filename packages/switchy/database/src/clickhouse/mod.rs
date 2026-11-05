//! `ClickHouse` backend over the server's HTTP interface.
//!
//! [`ClickHouseDatabase`](crate::clickhouse::ClickHouseDatabase) sends every statement as an HTTP `POST` to the
//! server (`http://host:8123` or `https://host:8443`), authenticating with
//! the `X-ClickHouse-User` / `X-ClickHouse-Key` headers. Results come back in
//! the `TabSeparatedWithNamesAndTypes` format and are decoded by each
//! column's type. A statement that names another output
//! format (`... FORMAT JSON`) returns that output as text, one row per line,
//! in a column named after the format.
//!
//! # Parameters
//!
//! Statements take `$1`, `$2`, ... placeholders (`?` is `ClickHouse`'s
//! ternary operator). Each becomes one of `ClickHouse`'s own typed
//! substitutions (`{p1:String}`) and its value is sent as the `param_p1`
//! HTTP parameter, which the server parses as that type; values are never
//! written into the statement.
//!
//! # Transactions
//!
//! `ClickHouse` has no transactions (only an experimental mode this backend
//! does not use): every statement takes effect on its own.
//! [`begin_transaction`](crate::Database::begin_transaction) returns
//! [`DatabaseError::UnsupportedOperation`] rather than pretending.
//!
//! # Read-only connections
//!
//! A read-only database
//! ([`ClickHouseDatabaseBuilder::read_only`](crate::clickhouse::ClickHouseDatabaseBuilder::read_only))
//! sends
//! `readonly=1` with every request, the server's own read-only mode: it
//! refuses `INSERT`, `ALTER`, `CREATE`, `DROP`, `TRUNCATE`, `OPTIMIZE`,
//! `SYSTEM`, table functions, and changing any setting (`SET`, `SETTINGS`).
//! It is sent as the last HTTP parameter, and settings given to the builder
//! may not name it, so nothing overrides it. `KILL QUERY` of the same user's
//! queries is still allowed in this mode, as `ClickHouse` allows it to any
//! user.
//!
//! # Example
//!
//! ```rust,no_run
//! use switchy_database::{Database, DatabaseValue, clickhouse::ClickHouseDatabase};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let db = ClickHouseDatabase::builder("http://127.0.0.1:8123")
//!     .user("default")
//!     .password("secret")
//!     .database("shop")
//!     .read_only(true)
//!     .build()
//!     .await?;
//! let rows = db
//!     .query_raw_params("SELECT title FROM albums WHERE id = $1", &[DatabaseValue::Int64(1)])
//!     .await?;
//! # Ok(())
//! # }
//! ```

#[cfg(feature = "schema")]
mod introspection;
mod params;
mod tsv;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;

use crate::{DatabaseError, DatabaseValue, QueryResult, ResultColumn, Row};

/// The output format requested for every statement.
const FORMAT: &str = "TabSeparatedWithNamesAndTypes";

/// HTTP parameters the backend sets itself, which settings may not name.
const RESERVED_PARAMS: &[&str] = &[
    "readonly",
    "query",
    "database",
    "default_format",
    "user",
    "password",
    "quota_key",
    "query_id",
    "session_id",
    "session_timeout",
    "session_check",
    "wait_end_of_query",
    "role",
    "compress",
    "decompress",
];

/// Errors from the `ClickHouse` backend.
#[derive(Debug, Error)]
pub enum ClickHouseDatabaseError {
    /// The server could not be reached, or the HTTP exchange failed.
    #[error("{}", error_chain(.0))]
    Http(#[source] reqwest::Error),
    /// The server refused the user or password.
    #[error("authentication failed: {0}")]
    Unauthorized(String),
    /// The user may not do what was asked.
    #[error("the server refused: {0}")]
    Forbidden(String),
    /// The server refused a write because the connection is read-only.
    #[error("read-only connection: {0}")]
    ReadOnly(String),
    /// The server reported an error (a SQL error, an unknown table, ...).
    #[error("{message}")]
    Server {
        /// What went wrong.
        message: String,
        /// The server's numeric error code, if it sent one.
        code: Option<u32>,
    },
    /// The server's response could not be read.
    #[error("invalid response from the ClickHouse server: {0}")]
    Protocol(String),
    /// The URL or a setting is not usable.
    #[error("invalid ClickHouse configuration: {0}")]
    InvalidConfig(String),
}

impl ClickHouseDatabaseError {
    /// The server's numeric error code, if it reported one.
    #[must_use]
    pub const fn code(&self) -> Option<u32> {
        match self {
            Self::Server { code, .. } => *code,
            _ => None,
        }
    }

    /// The error for a failed response: the body is the server's message
    /// (`Code: 60. DB::Exception: ...`), `code` its error code header.
    fn from_response(status: reqwest::StatusCode, code: Option<u32>, body: &str) -> Self {
        let message = server_message(body);
        let message = if message.is_empty() {
            format!("HTTP {status}")
        } else {
            message
        };
        match code {
            // UNKNOWN_USER, WRONG_PASSWORD, REQUIRED_PASSWORD,
            // AUTHENTICATION_FAILED
            Some(192 | 193 | 194 | 516) => Self::Unauthorized(message),
            // READONLY
            Some(164) => Self::ReadOnly(message),
            // ACCESS_DENIED, NOT_ENOUGH_PRIVILEGES
            Some(291 | 497) => Self::Forbidden(message),
            _ if status == reqwest::StatusCode::UNAUTHORIZED => Self::Unauthorized(message),
            _ if status == reqwest::StatusCode::FORBIDDEN => Self::Forbidden(message),
            _ => Self::Server { message, code },
        }
    }
}

/// The server's error message without its `Code: N. DB::Exception: ` prefix
/// and version suffix.
fn server_message(body: &str) -> String {
    let message = body.trim();
    let message = message
        .split_once("DB::Exception: ")
        .map_or(message, |(_, rest)| rest);
    let message = message
        .rfind(" (version ")
        .map_or(message, |end| &message[..end]);
    message.trim().to_string()
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

impl From<ClickHouseDatabaseError> for DatabaseError {
    fn from(error: ClickHouseDatabaseError) -> Self {
        Self::ClickHouse(error)
    }
}

/// The `http://` or `https://` base URL of a server URL: a trailing `/` and
/// any query string are dropped.
///
/// # Errors
///
/// * [`ClickHouseDatabaseError::InvalidConfig`] for other schemes, a URL
///   without a host, or user info in the URL (credentials go to
///   [`ClickHouseDatabaseBuilder::user`] and
///   [`ClickHouseDatabaseBuilder::password`])
pub fn base_url(url: &str) -> Result<String, ClickHouseDatabaseError> {
    let invalid = |message: &str| ClickHouseDatabaseError::InvalidConfig(message.to_string());
    let url = url.trim();
    let url = url.split(['?', '#']).next().unwrap_or(url);
    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| invalid("expected http://host or https://host"))?;
    let scheme = match scheme.to_ascii_lowercase().as_str() {
        "http" => "http",
        "https" => "https",
        other => {
            return Err(ClickHouseDatabaseError::InvalidConfig(format!(
                "unsupported scheme {other}:// (use http:// or https://)"
            )));
        }
    };
    let rest = rest.trim_end_matches('/');
    if rest.is_empty() || rest.starts_with('/') {
        return Err(invalid("the URL has no host"));
    }
    if rest
        .split('/')
        .next()
        .is_some_and(|host| host.contains('@'))
    {
        return Err(invalid(
            "the user and password go to the builder, not the URL",
        ));
    }
    Ok(format!("{scheme}://{rest}"))
}

/// Whether `name` may be sent as a setting: a plain identifier the backend
/// doesn't set itself.
fn valid_setting(name: &str) -> bool {
    !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !name.to_ascii_lowercase().starts_with("param_")
        && !RESERVED_PARAMS.contains(&name.to_ascii_lowercase().as_str())
}

/// Builds a [`ClickHouseDatabase`].
#[derive(Clone)]
pub struct ClickHouseDatabaseBuilder {
    url: String,
    user: Option<String>,
    password: Option<String>,
    database: Option<String>,
    read_only: bool,
    connect_timeout: Option<Duration>,
    settings: BTreeMap<String, String>,
}

impl std::fmt::Debug for ClickHouseDatabaseBuilder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClickHouseDatabaseBuilder")
            .field("url", &self.url)
            .field("user", &self.user)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("database", &self.database)
            .field("read_only", &self.read_only)
            .field("connect_timeout", &self.connect_timeout)
            .field("settings", &self.settings)
            .finish()
    }
}

impl ClickHouseDatabaseBuilder {
    /// A builder for the server at `url` (`http://host:8123` or
    /// `https://host:8443`).
    #[must_use]
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            user: None,
            password: None,
            database: None,
            read_only: false,
            connect_timeout: None,
            settings: BTreeMap::new(),
        }
    }

    /// Log in as `user` (the server's `default` user when not given).
    #[must_use]
    pub fn user(mut self, user: impl Into<String>) -> Self {
        self.user = Some(user.into()).filter(|user| !user.is_empty());
        self
    }

    /// Log in with `password`.
    #[must_use]
    pub fn password(mut self, password: impl Into<String>) -> Self {
        self.password = Some(password.into());
        self
    }

    /// Use `database` for unqualified names (the user's default database
    /// when not given).
    #[must_use]
    pub fn database(mut self, database: impl Into<String>) -> Self {
        self.database = Some(database.into()).filter(|database| !database.is_empty());
        self
    }

    /// Send `readonly=1` with every request (see the
    /// [module documentation](self)).
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

    /// Send the setting `name=value` with every request (`max_execution_time`,
    /// `max_result_rows`, ...). Read-only mode allows settings sent this way.
    #[must_use]
    pub fn setting(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.settings.insert(name.into(), value.into());
        self
    }

    /// Connect, checking that the server answers (and accepts the login).
    ///
    /// # Errors
    ///
    /// * [`ClickHouseDatabaseError::InvalidConfig`] if the URL is not an
    ///   `http(s)://` URL or a setting names a parameter the backend sets
    ///   itself (`readonly`, `database`, `param_*`, ...)
    /// * [`ClickHouseDatabaseError::Http`] if the server can't be reached
    /// * [`ClickHouseDatabaseError::Unauthorized`] if the server refuses the
    ///   user or password
    /// * [`ClickHouseDatabaseError::Server`] if the server does not answer a
    ///   `SELECT 1` (an unknown database, say)
    pub async fn build(self) -> Result<ClickHouseDatabase, ClickHouseDatabaseError> {
        let base = base_url(&self.url)?;
        if let Some(name) = self.settings.keys().find(|name| !valid_setting(name)) {
            return Err(ClickHouseDatabaseError::InvalidConfig(format!(
                "{name} can't be given as a setting"
            )));
        }
        let mut http = reqwest::Client::builder()
            .user_agent(concat!("switchy_database/", env!("CARGO_PKG_VERSION")));
        if let Some(timeout) = self.connect_timeout {
            http = http.connect_timeout(timeout);
        }
        let db = ClickHouseDatabase {
            http: http.build().map_err(ClickHouseDatabaseError::Http)?,
            base_url: base,
            user: self.user,
            password: self.password,
            database: self.database,
            read_only: self.read_only,
            settings: self.settings,
            closed: AtomicBool::new(false),
        };
        db.send("SELECT 1", &[])
            .await
            .map_err(|error| match error {
                DatabaseError::ClickHouse(error) => error,
                other => ClickHouseDatabaseError::Protocol(other.to_string()),
            })?;
        log::debug!(
            "clickhouse: connected to {} ({})",
            db.base_url,
            if db.read_only {
                "read-only"
            } else {
                "read-write"
            }
        );
        Ok(db)
    }
}

/// A `ClickHouse` server, reached over HTTP.
pub struct ClickHouseDatabase {
    http: reqwest::Client,
    base_url: String,
    user: Option<String>,
    password: Option<String>,
    database: Option<String>,
    read_only: bool,
    settings: BTreeMap<String, String>,
    closed: AtomicBool,
}

impl std::fmt::Debug for ClickHouseDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ClickHouseDatabase")
            .field("base_url", &self.base_url)
            .field("user", &self.user)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("database", &self.database)
            .field("read_only", &self.read_only)
            .field("settings", &self.settings)
            .finish_non_exhaustive()
    }
}

/// A statement's decoded result (read through the `raw-sql` methods and
/// introspection).
#[derive(Debug, Default)]
#[cfg_attr(not(any(feature = "raw-sql", feature = "schema")), allow(dead_code))]
pub(crate) struct Outcome {
    columns: Vec<ResultColumn>,
    rows: Vec<Vec<DatabaseValue>>,
    /// Rows written, from the server's summary; only `exec_raw_params` (a
    /// `raw-sql` method) reports it.
    #[cfg_attr(not(feature = "raw-sql"), allow(dead_code))]
    affected: u64,
}

#[cfg_attr(not(feature = "raw-sql"), allow(dead_code))]
impl Outcome {
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

impl ClickHouseDatabase {
    /// A builder for the server at `url`.
    #[must_use]
    pub fn builder(url: impl Into<String>) -> ClickHouseDatabaseBuilder {
        ClickHouseDatabaseBuilder::new(url)
    }

    /// Whether the database sends `readonly=1` with every request.
    #[must_use]
    pub const fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// The server's base URL.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// The query string of a request: the database, the format, the
    /// settings, the parameters, and last `readonly=1` on read-only
    /// databases (the server takes the last of repeated parameters).
    fn query_string(&self, params: &[(String, Vec<u8>)]) -> String {
        let mut pairs: Vec<(String, Vec<u8>)> = Vec::new();
        if let Some(database) = &self.database {
            pairs.push(("database".to_string(), database.as_bytes().to_vec()));
        }
        pairs.push(("default_format".to_string(), FORMAT.as_bytes().to_vec()));
        // Buffer the result, so an error part-way through is reported as
        // one instead of truncating it.
        pairs.push(("wait_end_of_query".to_string(), b"1".to_vec()));
        for (name, value) in &self.settings {
            pairs.push((name.clone(), value.as_bytes().to_vec()));
        }
        for (name, value) in params {
            pairs.push((format!("param_{name}"), value.clone()));
        }
        if self.read_only {
            pairs.push(("readonly".to_string(), b"1".to_vec()));
        }
        pairs
            .iter()
            .map(|(name, value)| {
                format!(
                    "{}={}",
                    percent_encode(name.as_bytes()),
                    percent_encode(value)
                )
            })
            .collect::<Vec<_>>()
            .join("&")
    }

    /// Run one statement with `$n` parameters.
    async fn send(&self, sql: &str, params: &[DatabaseValue]) -> Result<Outcome, DatabaseError> {
        if self.closed.load(Ordering::SeqCst) {
            return Err(DatabaseError::ConnectionClosed);
        }
        let bound = params::bind(sql, params)?;
        log::trace!("clickhouse: {}", bound.sql);
        let url = format!("{}/?{}", self.base_url, self.query_string(&bound.params));
        let mut request = self.http.post(url).body(bound.sql);
        if let Some(user) = &self.user {
            request = request.header("X-ClickHouse-User", user);
        }
        if let Some(password) = &self.password {
            request = request.header("X-ClickHouse-Key", password);
        }
        let response = request
            .send()
            .await
            .map_err(ClickHouseDatabaseError::Http)?;
        let status = response.status();
        let headers = response.headers().clone();
        let header = |name: &str| {
            headers
                .get(name)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string)
        };
        let body = response
            .bytes()
            .await
            .map_err(ClickHouseDatabaseError::Http)?;
        let code = header("X-ClickHouse-Exception-Code").and_then(|code| code.parse().ok());
        if !status.is_success() || code.is_some() {
            return Err(ClickHouseDatabaseError::from_response(
                status,
                code,
                &String::from_utf8_lossy(&body),
            )
            .into());
        }
        let affected = header("X-ClickHouse-Summary")
            .and_then(|summary| written_rows(&summary))
            .unwrap_or(0);
        let format = header("X-ClickHouse-Format");
        let (columns, rows) = match format.as_deref() {
            None | Some("") if body.is_empty() => (Vec::new(), Vec::new()),
            Some(FORMAT | "") | None => {
                let table = tsv::decode(&body);
                (table.columns, table.rows)
            }
            Some(other) => other_format(other, &body),
        };
        Ok(Outcome {
            columns,
            rows,
            affected,
        })
    }
}

/// Output a statement asked for in another format: one text row per line,
/// in a column named after the format.
fn other_format(format: &str, body: &[u8]) -> (Vec<ResultColumn>, Vec<Vec<DatabaseValue>>) {
    let text = String::from_utf8_lossy(body);
    let rows = text
        .strip_suffix('\n')
        .unwrap_or(&text)
        .split('\n')
        .map(|line| vec![DatabaseValue::String(line.to_string())])
        .collect();
    (
        vec![ResultColumn {
            name: format.to_string(),
            type_name: Some("String".to_string()),
        }],
        rows,
    )
}

/// `written_rows` from an `X-ClickHouse-Summary` header.
fn written_rows(summary: &str) -> Option<u64> {
    let summary: serde_json::Value = serde_json::from_str(summary).ok()?;
    match &summary["written_rows"] {
        serde_json::Value::String(rows) => rows.parse().ok(),
        serde_json::Value::Number(rows) => rows.as_u64(),
        _ => None,
    }
}

/// Percent-encode bytes for a URL query (everything but unreserved
/// characters).
fn percent_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for &byte in bytes {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'~') {
            out.push(char::from(byte));
        } else {
            let _ = write!(out, "%{byte:02X}");
        }
    }
    out
}

/// Why the query builder isn't available.
fn builder_unsupported(what: &str) -> DatabaseError {
    DatabaseError::UnsupportedOperation(format!(
        "{what}: the ClickHouse backend runs raw SQL (query_raw_params, exec_raw_params) and \
         schema introspection; it has no query or schema builder, since ClickHouse has no \
         RETURNING, changes rows through asynchronous mutations, and needs a table engine"
    ))
}

#[async_trait]
impl crate::Database for ClickHouseDatabase {
    async fn check_connection(&self) -> Result<(), DatabaseError> {
        self.send("SELECT 1", &[]).await.map(|_| ())
    }

    #[cfg(feature = "raw-sql")]
    async fn query_raw(&self, query: &str) -> Result<Vec<Row>, DatabaseError> {
        Ok(self.send(query, &[]).await?.into_rows())
    }

    #[cfg(feature = "raw-sql")]
    async fn query_raw_params(
        &self,
        query: &str,
        params: &[DatabaseValue],
    ) -> Result<Vec<Row>, DatabaseError> {
        Ok(self.send(query, params).await?.into_rows())
    }

    #[cfg(feature = "raw-sql")]
    async fn query_raw_result(
        &self,
        query: &str,
        params: &[DatabaseValue],
    ) -> Result<QueryResult, DatabaseError> {
        Ok(self.send(query, params).await?.into_result())
    }

    #[cfg(feature = "raw-sql")]
    async fn exec_raw(&self, statement: &str) -> Result<(), DatabaseError> {
        self.send(statement, &[]).await.map(|_| ())
    }

    #[cfg(feature = "raw-sql")]
    async fn exec_raw_params(
        &self,
        query: &str,
        params: &[DatabaseValue],
    ) -> Result<u64, DatabaseError> {
        Ok(self.send(query, params).await?.affected)
    }

    async fn query(
        &self,
        _query: &crate::query::SelectQuery<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        Err(builder_unsupported("query"))
    }

    async fn query_first(
        &self,
        _query: &crate::query::SelectQuery<'_>,
    ) -> Result<Option<Row>, DatabaseError> {
        Err(builder_unsupported("query_first"))
    }

    async fn exec_update(
        &self,
        _statement: &crate::query::UpdateStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        Err(builder_unsupported("exec_update"))
    }

    async fn exec_update_first(
        &self,
        _statement: &crate::query::UpdateStatement<'_>,
    ) -> Result<Option<Row>, DatabaseError> {
        Err(builder_unsupported("exec_update_first"))
    }

    async fn exec_insert(
        &self,
        _statement: &crate::query::InsertStatement<'_>,
    ) -> Result<Row, DatabaseError> {
        Err(builder_unsupported("exec_insert"))
    }

    async fn exec_upsert(
        &self,
        _statement: &crate::query::UpsertStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        Err(builder_unsupported("exec_upsert"))
    }

    async fn exec_upsert_first(
        &self,
        _statement: &crate::query::UpsertStatement<'_>,
    ) -> Result<Row, DatabaseError> {
        Err(builder_unsupported("exec_upsert_first"))
    }

    async fn exec_upsert_multi(
        &self,
        _statement: &crate::query::UpsertMultiStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        Err(builder_unsupported("exec_upsert_multi"))
    }

    async fn exec_delete(
        &self,
        _statement: &crate::query::DeleteStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        Err(builder_unsupported("exec_delete"))
    }

    async fn exec_delete_first(
        &self,
        _statement: &crate::query::DeleteStatement<'_>,
    ) -> Result<Option<Row>, DatabaseError> {
        Err(builder_unsupported("exec_delete_first"))
    }

    #[cfg(feature = "schema")]
    async fn exec_create_table(
        &self,
        _statement: &crate::schema::CreateTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        Err(builder_unsupported("exec_create_table"))
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_table(
        &self,
        _statement: &crate::schema::DropTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        Err(builder_unsupported("exec_drop_table"))
    }

    #[cfg(feature = "schema")]
    async fn exec_create_index(
        &self,
        _statement: &crate::schema::CreateIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        Err(builder_unsupported("exec_create_index"))
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_index(
        &self,
        _statement: &crate::schema::DropIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        Err(builder_unsupported("exec_drop_index"))
    }

    #[cfg(feature = "schema")]
    async fn exec_alter_table(
        &self,
        _statement: &crate::schema::AlterTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        Err(builder_unsupported("exec_alter_table"))
    }

    #[cfg(feature = "schema")]
    async fn table_exists(&self, table: &str) -> Result<bool, DatabaseError> {
        introspection::table_exists(self, table).await
    }

    #[cfg(feature = "schema")]
    async fn list_tables(&self) -> Result<Vec<String>, DatabaseError> {
        introspection::list_tables(self).await
    }

    #[cfg(feature = "schema")]
    async fn get_table_info(
        &self,
        table: &str,
    ) -> Result<Option<crate::schema::TableInfo>, DatabaseError> {
        introspection::relation_info(self, &crate::schema::RelationRef::new(table)).await
    }

    #[cfg(feature = "schema")]
    async fn get_table_columns(
        &self,
        table: &str,
    ) -> Result<Vec<crate::schema::ColumnInfo>, DatabaseError> {
        Ok(
            introspection::relation_info(self, &crate::schema::RelationRef::new(table))
                .await?
                .map(|info| {
                    let mut columns: Vec<_> = info.columns.into_values().collect();
                    columns.sort_by_key(|column| column.ordinal_position);
                    columns
                })
                .unwrap_or_default(),
        )
    }

    #[cfg(feature = "schema")]
    async fn column_exists(&self, table: &str, column: &str) -> Result<bool, DatabaseError> {
        Ok(self
            .get_table_columns(table)
            .await?
            .iter()
            .any(|info| info.name == column))
    }

    #[cfg(feature = "schema")]
    async fn list_schemas(&self) -> Result<Vec<crate::schema::SchemaInfo>, DatabaseError> {
        introspection::list_schemas(self).await
    }

    #[cfg(feature = "schema")]
    async fn list_databases(&self) -> Result<Vec<crate::schema::DatabaseInfo>, DatabaseError> {
        Ok(introspection::list_schemas(self)
            .await?
            .into_iter()
            .map(|schema| crate::schema::DatabaseInfo {
                name: schema.name,
                is_current: schema.is_default,
                is_system: schema.is_system,
            })
            .collect())
    }

    #[cfg(feature = "schema")]
    async fn list_relations(
        &self,
        schema: Option<&str>,
    ) -> Result<Vec<crate::schema::RelationSummary>, DatabaseError> {
        introspection::list_relations(self, schema).await
    }

    #[cfg(feature = "schema")]
    async fn get_relation_info(
        &self,
        relation: &crate::schema::RelationRef,
    ) -> Result<Option<crate::schema::TableInfo>, DatabaseError> {
        introspection::relation_info(self, relation).await
    }

    #[cfg(all(feature = "schema", feature = "raw-sql"))]
    async fn list_routines(
        &self,
        schema: Option<&str>,
    ) -> Result<Vec<crate::schema::RoutineInfo>, DatabaseError> {
        // ClickHouse functions are server-wide, not per database.
        let _ = schema;
        introspection::list_routines(self).await
    }

    #[cfg(feature = "schema")]
    async fn get_view_definition(
        &self,
        relation: &crate::schema::RelationRef,
    ) -> Result<Option<String>, DatabaseError> {
        introspection::view_definition(self, relation).await
    }

    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn crate::DatabaseTransaction>, DatabaseError> {
        Err(DatabaseError::UnsupportedOperation(
            "ClickHouse has no transactions: every statement takes effect on its own".to_string(),
        ))
    }

    async fn close(&self) -> Result<(), DatabaseError> {
        self.closed.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[cfg(test)]
// `{p1:String}` is a ClickHouse query parameter, not a format argument.
#[allow(clippy::literal_string_with_formatting_args)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn server_urls_become_base_urls() {
        assert_eq!(
            base_url("http://127.0.0.1:8123/?x=1").unwrap(),
            "http://127.0.0.1:8123"
        );
        assert_eq!(
            base_url("HTTPS://ch.example:8443").unwrap(),
            "https://ch.example:8443"
        );
        assert!(base_url("clickhouse://db").is_err());
        assert!(base_url("http://").is_err());
        assert!(base_url("https://user:pw@ch.example").is_err());
    }

    #[test]
    fn settings_cannot_name_reserved_parameters() {
        assert!(valid_setting("max_execution_time"));
        for name in [
            "readonly", "READONLY", "param_p1", "database", "", "a=b", "user",
        ] {
            assert!(!valid_setting(name), "{name}");
        }
    }

    #[test]
    fn server_messages_lose_their_prefix_and_version() {
        assert_eq!(
            server_message(
                "Code: 60. DB::Exception: Unknown table expression identifier 'x'. (UNKNOWN_TABLE) (version 26.8.7.19)\n"
            ),
            "Unknown table expression identifier 'x'. (UNKNOWN_TABLE)"
        );
        assert!(matches!(
            ClickHouseDatabaseError::from_response(
                reqwest::StatusCode::INTERNAL_SERVER_ERROR,
                Some(164),
                "Code: 164. DB::Exception: u: Cannot execute query in readonly mode. (READONLY)"
            ),
            ClickHouseDatabaseError::ReadOnly(_)
        ));
        assert!(matches!(
            ClickHouseDatabaseError::from_response(
                reqwest::StatusCode::UNAUTHORIZED,
                Some(516),
                "Code: 516. DB::Exception: u: Authentication failed"
            ),
            ClickHouseDatabaseError::Unauthorized(_)
        ));
    }

    #[test]
    fn summaries_report_written_rows() {
        assert_eq!(
            written_rows(r#"{"read_rows":"2","written_rows":"2","written_bytes":"15"}"#),
            Some(2)
        );
        assert_eq!(written_rows("nonsense"), None);
    }

    #[test]
    fn debug_output_hides_the_password() {
        let builder = ClickHouseDatabase::builder("http://127.0.0.1:8123")
            .user("shop")
            .password("hunter2");
        let debug = format!("{builder:?}");
        assert!(!debug.contains("hunter2"), "{debug}");
        assert!(debug.contains("<redacted>"), "{debug}");
    }

    /// Requests against a stand-in server.
    #[cfg(feature = "raw-sql")]
    mod stub {
        use std::sync::Arc;

        use super::super::ClickHouseDatabase;
        use crate::{Database as _, DatabaseValue};

        /// A request the stand-in server received.
        #[derive(Debug, Clone)]
        struct Seen {
            target: String,
            headers: String,
            body: String,
        }

        /// A stand-in `ClickHouse` server on a free local port that answers
        /// every request with one `UInt8` row and records what it received.
        async fn stub_server() -> (String, Arc<std::sync::Mutex<Vec<Seen>>>) {
            use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("http://{}", listener.local_addr().unwrap());
            let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
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
                            let head = String::from_utf8_lossy(&buffer[..end]).to_string();
                            let length = head
                                .to_lowercase()
                                .lines()
                                .find_map(|line| {
                                    line.strip_prefix("content-length:")
                                        .map(|value| value.trim().parse::<usize>().unwrap())
                                })
                                .unwrap_or(0);
                            let start = end + 4;
                            if buffer.len() < start + length {
                                continue;
                            }
                            let body =
                                String::from_utf8_lossy(&buffer[start..start + length]).to_string();
                            buffer.drain(..start + length);
                            let target = head
                                .lines()
                                .next()
                                .and_then(|line| line.split(' ').nth(1))
                                .unwrap_or_default()
                                .to_string();
                            recorded.lock().unwrap().push(Seen {
                                target,
                                headers: head,
                                body,
                            });
                            let response = "x\nUInt8\n1\n";
                            let reply = format!(
                                "HTTP/1.1 200 OK\r\nX-ClickHouse-Format: TabSeparatedWithNamesAndTypes\r\n\
                                 X-ClickHouse-Summary: {{\"written_rows\":\"0\"}}\r\n\
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
            (url, seen)
        }

        #[tokio::test]
        async fn read_only_requests_end_with_readonly_and_bind_values_as_parameters() {
            let (url, seen) = stub_server().await;
            let db = ClickHouseDatabase::builder(url)
                .user("shop")
                .password("hunter2")
                .database("music")
                .setting("max_execution_time", "5")
                .read_only(true)
                .build()
                .await
                .unwrap();
            let rows = db
                .query_raw_params(
                    "SELECT x FROM albums WHERE title = $1",
                    &[DatabaseValue::String("'; DROP TABLE albums --".to_string())],
                )
                .await
                .unwrap();
            assert_eq!(rows.len(), 1);
            assert!(db.begin_transaction().await.is_err());

            let seen = seen.lock().unwrap().clone();
            assert_eq!(seen.len(), 2, "{seen:?}");
            for request in &seen {
                assert!(request.target.ends_with("&readonly=1"), "{request:?}");
                assert!(request.target.contains("database=music"), "{request:?}");
                assert!(
                    request.target.contains("max_execution_time=5"),
                    "{request:?}"
                );
                let headers = request.headers.to_lowercase();
                assert!(headers.contains("x-clickhouse-user: shop"), "{request:?}");
                assert!(!request.target.contains("hunter2"), "{request:?}");
            }
            let query = &seen[1];
            assert_eq!(query.body, "SELECT x FROM albums WHERE title = {p1:String}");
            assert!(!query.body.contains("DROP"), "{query:?}");
            assert!(
                query
                    .target
                    .contains("param_p1=%27%3B%20DROP%20TABLE%20albums%20--"),
                "{query:?}"
            );
        }

        #[tokio::test]
        async fn readonly_cannot_be_given_as_a_setting() {
            let (url, seen) = stub_server().await;
            let error = ClickHouseDatabase::builder(url)
                .setting("readonly", "0")
                .build()
                .await
                .unwrap_err();
            assert!(error.to_string().contains("readonly"), "{error}");
            assert_eq!(seen.lock().unwrap().len(), 0);
        }
    }
}
