//! Connection targets, connection URLs and backend-neutral connection options.
//!
//! [`ConnectionTarget::from_url`] parses a database URL into a typed target that
//! [`init_with_options`](crate::init_with_options) can open with the best
//! enabled backend, applying [`ConnectOptions`] natively.

use std::{
    collections::BTreeMap, net::SocketAddr, num::NonZeroUsize, path::PathBuf, time::Duration,
};

use thiserror::Error;

use crate::Credentials;

/// Whether a connection may modify the database.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AccessMode {
    /// Reads and writes are allowed (the default).
    #[default]
    ReadWrite,
    /// The connection is opened so that the database rejects writes.
    ///
    /// This is enforced by the database itself (read-only open flags, read-only
    /// sessions or read-only default transactions), not by this crate.
    ReadOnly,
}

/// Backend-neutral options applied when opening a connection.
///
/// See [`init_with_options`](crate::init_with_options) for how each option is
/// applied by each backend.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ConnectOptions {
    /// Read/write access mode.
    pub access: AccessMode,
    /// Server-side limit on statement execution time, where supported.
    pub statement_timeout: Option<Duration>,
    /// Application name reported to the server, where supported.
    pub application_name: Option<String>,
    /// Maximum number of pooled connections (defaults to 5).
    pub pool_size: Option<NonZeroUsize>,
    /// Connect to this address instead of the one the target's host
    /// resolves to (an SSH tunnel's local end, a port forward), still
    /// naming the target's host for TLS, where supported.
    pub connect_address: Option<SocketAddr>,
}

impl ConnectOptions {
    /// Default options: read-write, no timeout, no application name, default pool size.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the access mode.
    #[must_use]
    pub const fn access(mut self, access: AccessMode) -> Self {
        self.access = access;
        self
    }

    /// Open the connection read-only.
    #[must_use]
    pub const fn read_only(self) -> Self {
        self.access(AccessMode::ReadOnly)
    }

    /// Set the server-side statement timeout.
    #[must_use]
    pub const fn statement_timeout(mut self, timeout: Duration) -> Self {
        self.statement_timeout = Some(timeout);
        self
    }

    /// Set the application name reported to the server.
    #[must_use]
    pub fn application_name(mut self, name: impl Into<String>) -> Self {
        self.application_name = Some(name.into());
        self
    }

    /// Set the maximum number of pooled connections.
    #[must_use]
    pub const fn pool_size(mut self, size: NonZeroUsize) -> Self {
        self.pool_size = Some(size);
        self
    }

    /// Connect to `address` instead of the target host's own address.
    #[must_use]
    pub const fn connect_address(mut self, address: SocketAddr) -> Self {
        self.connect_address = Some(address);
        self
    }

    /// Whether [`AccessMode::ReadOnly`] was requested.
    #[must_use]
    pub const fn is_read_only(&self) -> bool {
        matches!(self.access, AccessMode::ReadOnly)
    }
}

/// TLS negotiation mode, following libpq `sslmode` semantics.
///
/// `MySQL` `ssl-mode` values map as `DISABLED` → [`SslMode::Disable`],
/// `PREFERRED` → [`SslMode::Prefer`], `REQUIRED` → [`SslMode::Require`],
/// `VERIFY_CA` → [`SslMode::VerifyCa`] and `VERIFY_IDENTITY` → [`SslMode::VerifyFull`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SslMode {
    /// Never use TLS.
    Disable,
    /// Try without TLS first, then with TLS (backends without this mode use `Prefer`).
    Allow,
    /// Try TLS first, falling back to plaintext.
    Prefer,
    /// Require TLS without verifying the server certificate.
    Require,
    /// Require TLS and verify the server certificate chain.
    VerifyCa,
    /// Require TLS and verify the certificate chain and host name.
    VerifyFull,
}

impl std::str::FromStr for SslMode {
    type Err = ConnectionUrlError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().replace('_', "-").as_str() {
            "disable" | "disabled" | "false" | "0" => Ok(Self::Disable),
            "allow" => Ok(Self::Allow),
            "prefer" | "preferred" => Ok(Self::Prefer),
            "require" | "required" | "true" | "1" => Ok(Self::Require),
            "verify-ca" => Ok(Self::VerifyCa),
            "verify-full" | "verify-identity" => Ok(Self::VerifyFull),
            _ => Err(ConnectionUrlError::InvalidSslMode(value.to_string())),
        }
    }
}

/// How much of a SQL Server connection TLS protects (the `encrypt` URL
/// parameter, as SQL Server's own drivers spell it).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MssqlEncrypt {
    /// No TLS at all, not even for the login (`encrypt=danger_plaintext`).
    Plaintext,
    /// TLS for the login only (`encrypt=false`, `no` or `optional`); the
    /// server may still insist on encrypting everything.
    LoginOnly,
    /// TLS for the whole session if the server supports it
    /// (`encrypt=prefer`).
    Preferred,
    /// TLS for the whole session, or no connection (`encrypt=true`, `yes`
    /// or `mandatory`; the default).
    #[default]
    Required,
    /// TDS 8.0 strict encryption: TLS before anything else
    /// (`encrypt=strict`; SQL Server 2022 and Azure SQL).
    Strict,
}

impl std::str::FromStr for MssqlEncrypt {
    type Err = ConnectionUrlError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "danger_plaintext" => Ok(Self::Plaintext),
            "false" | "no" | "optional" | "0" => Ok(Self::LoginOnly),
            "prefer" | "preferred" => Ok(Self::Preferred),
            "true" | "yes" | "mandatory" | "required" | "1" => Ok(Self::Required),
            "strict" => Ok(Self::Strict),
            _ => Err(ConnectionUrlError::InvalidUrl(format!(
                "encrypt must be strict, true, false or danger_plaintext, not {value:?}"
            ))),
        }
    }
}

/// TLS certificate and key files used when connecting to a network database.
///
/// Every path is optional. Files are read by the driver when a connection is
/// opened, so a missing or unreadable file surfaces as a connection error.
///
/// [`ConnectionTarget::from_url`] fills these from the libpq parameters
/// `sslrootcert`, `sslcert` and `sslkey` (`PostgreSQL`) or from `ssl-ca`,
/// `ssl-cert` and `ssl-key` (`MySQL`; the `ssl_ca`/`sslca` spellings are also
/// accepted).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct TlsFiles {
    /// PEM file of certificate authorities used to verify the server certificate.
    pub root_cert: Option<PathBuf>,
    /// PEM client certificate presented to the server.
    pub client_cert: Option<PathBuf>,
    /// PEM private key for [`client_cert`](Self::client_cert).
    pub client_key: Option<PathBuf>,
}

impl TlsFiles {
    /// No certificate files.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the certificate-authority file used to verify the server.
    #[must_use]
    pub fn root_cert(mut self, path: impl Into<PathBuf>) -> Self {
        self.root_cert = Some(path.into());
        self
    }

    /// Set the client certificate file.
    #[must_use]
    pub fn client_cert(mut self, path: impl Into<PathBuf>) -> Self {
        self.client_cert = Some(path.into());
        self
    }

    /// Set the client private key file.
    #[must_use]
    pub fn client_key(mut self, path: impl Into<PathBuf>) -> Self {
        self.client_key = Some(path.into());
        self
    }

    /// Whether no certificate file is set.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.root_cert.is_none() && self.client_cert.is_none() && self.client_key.is_none()
    }
}

/// Remove the first of `keys` present in `params`, ignoring empty values.
fn take_path_param(params: &mut BTreeMap<String, String>, keys: &[&str]) -> Option<PathBuf> {
    let mut found = None;
    for key in keys {
        if let Some(value) = params.remove(*key)
            && found.is_none()
            && !value.is_empty()
        {
            found = Some(PathBuf::from(value));
        }
    }
    found
}

/// Errors produced while parsing a connection URL.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[non_exhaustive]
pub enum ConnectionUrlError {
    /// The URL has no `scheme:` prefix.
    #[error("Connection URL is missing a scheme (e.g. postgres://, sqlite:)")]
    MissingScheme,
    /// The URL scheme is not a supported database driver.
    #[error("Unsupported connection URL scheme: {0}")]
    UnsupportedScheme(String),
    /// A network URL is not of the form `scheme://...`.
    #[error("Invalid connection URL: {0}")]
    InvalidUrl(String),
    /// The port is not a valid `u16`.
    #[error("Invalid port: {0}")]
    InvalidPort(String),
    /// A percent-encoded component is malformed or not UTF-8.
    #[error("Invalid percent-encoding in {0}")]
    InvalidPercentEncoding(&'static str),
    /// The `sslmode`/`ssl-mode` parameter has an unknown value.
    #[error("Invalid SSL mode: {0}")]
    InvalidSslMode(String),
}

/// A database to connect to, independent of which backend crate opens it.
///
/// Its `Debug` output redacts secrets (passwords and auth tokens).
#[derive(Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConnectionTarget {
    /// A `PostgreSQL` server.
    ///
    /// An empty user or database name in `creds` means "driver default"
    /// (libpq semantics: the OS user / the user name). A host starting with
    /// `/` is a Unix-domain socket directory.
    Postgres {
        /// Host, port, database, user and password.
        creds: Credentials,
        /// TLS mode from `sslmode`, if given.
        ssl_mode: Option<SslMode>,
        /// TLS files from `sslrootcert`, `sslcert` and `sslkey`.
        tls_files: TlsFiles,
        /// Remaining (percent-decoded) query parameters, e.g. `application_name`.
        params: BTreeMap<String, String>,
    },
    /// A `MySQL`/`MariaDB` server.
    ///
    /// An empty user means "driver default"; an empty database name means no
    /// default database is selected. A host starting with `/` (or a `socket`
    /// parameter) is a Unix-domain socket path.
    MySql {
        /// Host, port, database, user and password.
        creds: Credentials,
        /// TLS mode from `ssl-mode`/`sslmode`, if given.
        ssl_mode: Option<SslMode>,
        /// TLS files from `ssl-ca`, `ssl-cert` and `ssl-key`.
        tls_files: TlsFiles,
        /// Remaining (percent-decoded) query parameters.
        params: BTreeMap<String, String>,
    },
    /// A `SQLite` database file, or an in-memory database when `path` is `None`.
    Sqlite {
        /// Database file path; `None` for a private in-memory database.
        path: Option<PathBuf>,
    },
    /// A `DuckDB` database file, or an in-memory database when `path` is `None`.
    DuckDb {
        /// Database file path; `None` for an in-memory database.
        path: Option<PathBuf>,
    },
    /// A remote `libSQL` server (`sqld`, `turso dev`, Turso Cloud), reached
    /// with Hrana over HTTP.
    Libsql {
        /// The server's base URL: `https://...` or `http://...`, without a
        /// query string (`libsql://` URLs are given as `https://`, or
        /// `http://` with `tls=0`).
        url: String,
        /// Auth token from the `authToken` parameter, sent as a bearer token.
        auth_token: Option<String>,
        /// TLS files from `sslrootcert` (a certificate authority trusted in
        /// addition to the system's; it needs TLS).
        tls_files: TlsFiles,
        /// Remaining (percent-decoded) query parameters.
        params: BTreeMap<String, String>,
    },
    /// A `ClickHouse` server, reached over its HTTP interface.
    ClickHouse {
        /// Host, port, database (empty: the user's default database), user
        /// (empty: the server's `default` user) and password.
        creds: Credentials,
        /// Whether to use HTTPS.
        tls: bool,
        /// TLS files from `sslrootcert` (a certificate authority trusted in
        /// addition to the system's), `sslcert` and `sslkey` (a client
        /// certificate and its key); they need HTTPS.
        tls_files: TlsFiles,
        /// Remaining (percent-decoded) query parameters.
        params: BTreeMap<String, String>,
    },
    /// A Microsoft SQL Server (or Azure SQL) database, reached over TDS.
    Mssql {
        /// Host, port (empty: 1433, or a named instance's port), database
        /// (empty: the login's default database), login and password.
        creds: Credentials,
        /// A named instance (`host\NAME` or `instance=NAME`), found through
        /// the SQL Server Browser unless a port is given.
        instance: Option<String>,
        /// How much of the connection TLS protects (`encrypt=`).
        encrypt: MssqlEncrypt,
        /// Accept any server certificate without verifying it
        /// (`trustServerCertificate=true`).
        trust_server_certificate: bool,
        /// TLS files: a certificate authority to verify the server with
        /// (`certificate=`, `trustServerCertificateCA=` or `sslrootcert=`).
        /// SQL Server doesn't use client certificates.
        tls_files: TlsFiles,
        /// The name the server's certificate must carry, when it differs
        /// from the host (`hostNameInCertificate=`).
        host_name_in_certificate: Option<String>,
        /// Remaining (percent-decoded) query parameters, with lowercase
        /// names (SQL Server's drivers match them case-insensitively).
        params: BTreeMap<String, String>,
    },
    /// An Oracle database, reached through Oracle Instant Client.
    Oracle {
        /// Host, port (empty: 1521, or 2484 with TLS), database (the service
        /// name, or the SID when [`sid`](Self::Oracle::sid) is set; empty:
        /// the listener's default service), user and password. The host is
        /// empty when a connect string is given.
        creds: Credentials,
        /// The database is an instance's SID (`?sid=NAME`) rather than a
        /// service name.
        sid: bool,
        /// A net service name (`tnsnames.ora` alias), Easy Connect string or
        /// connect descriptor to give the client as it is (`?tns=...`),
        /// instead of a host and service.
        connect_string: Option<String>,
        /// Connect over TLS (`oracle+tcps://`, `?protocol=tcps`).
        tls: bool,
        /// The wallet directory TLS trusts (`?wallet_location=`).
        wallet_location: Option<PathBuf>,
        /// The distinguished name the server's certificate must have
        /// (`?ssl_server_cert_dn=`).
        server_cert_dn: Option<String>,
        /// Remaining (percent-decoded) query parameters, with lowercase
        /// names.
        params: BTreeMap<String, String>,
    },
}

impl std::fmt::Debug for ConnectionTarget {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Postgres {
                creds,
                ssl_mode,
                tls_files,
                params,
            } => f
                .debug_struct("Postgres")
                .field("creds", creds)
                .field("ssl_mode", ssl_mode)
                .field("tls_files", tls_files)
                .field("params", params)
                .finish(),
            Self::MySql {
                creds,
                ssl_mode,
                tls_files,
                params,
            } => f
                .debug_struct("MySql")
                .field("creds", creds)
                .field("ssl_mode", ssl_mode)
                .field("tls_files", tls_files)
                .field("params", params)
                .finish(),
            Self::Sqlite { path } => f.debug_struct("Sqlite").field("path", path).finish(),
            Self::DuckDb { path } => f.debug_struct("DuckDb").field("path", path).finish(),
            Self::Libsql {
                url,
                auth_token,
                tls_files,
                params,
            } => f
                .debug_struct("Libsql")
                .field("url", url)
                .field("auth_token", &auth_token.as_ref().map(|_| "<redacted>"))
                .field("tls_files", tls_files)
                .field("params", params)
                .finish(),
            Self::ClickHouse {
                creds,
                tls,
                tls_files,
                params,
            } => f
                .debug_struct("ClickHouse")
                .field("creds", creds)
                .field("tls", tls)
                .field("tls_files", tls_files)
                .field("params", params)
                .finish(),
            Self::Mssql {
                creds,
                instance,
                encrypt,
                trust_server_certificate,
                tls_files,
                host_name_in_certificate,
                params,
            } => f
                .debug_struct("Mssql")
                .field("creds", creds)
                .field("instance", instance)
                .field("encrypt", encrypt)
                .field("trust_server_certificate", trust_server_certificate)
                .field("tls_files", tls_files)
                .field("host_name_in_certificate", host_name_in_certificate)
                .field("params", params)
                .finish(),
            Self::Oracle {
                creds,
                sid,
                connect_string,
                tls,
                wallet_location,
                server_cert_dn,
                params,
            } => f
                .debug_struct("Oracle")
                .field("creds", creds)
                .field("sid", sid)
                .field("connect_string", connect_string)
                .field("tls", tls)
                .field("wallet_location", wallet_location)
                .field("server_cert_dn", server_cert_dn)
                .field("params", params)
                .finish(),
        }
    }
}

impl ConnectionTarget {
    /// Parse a connection URL.
    ///
    /// Supported forms:
    ///
    /// * `postgres://[user[:password]@][host][:port][/database][?params]`
    ///   (also `postgresql://`); IPv6 hosts use brackets (`[::1]`); a Unix socket
    ///   directory may be given as `?host=/var/run/postgresql`
    /// * `mysql://...` / `mariadb://...` with the same shape; `?socket=/path`
    ///   selects a Unix socket
    /// * `sqlite:path`, `sqlite://path`, `sqlite:///abs/path`, `file:path`,
    ///   `sqlite::memory:`, `sqlite://:memory:`, `:memory:`
    /// * `duckdb:path`, `duckdb://path`, `duckdb:///abs/path`, `duckdb::memory:`
    /// * `libsql://host[:port][?authToken=...]` (TLS; `tls=0` turns it off),
    ///   `https://...` and `http://...` for a remote `libSQL` server
    /// * `clickhouse://[user[:password]@][host][:port][/database][?params]`
    ///   for a `ClickHouse` server's HTTP interface (plain HTTP unless
    ///   `secure=true` or `tls=1`), and `clickhouse+http://...` /
    ///   `clickhouse+https://...`, whose scheme says whether TLS is used (a
    ///   `secure` parameter must agree with it)
    /// * `sqlserver://[user[:password]@]host[\instance][:port][/database][?params]`
    ///   (also `mssql://`) for Microsoft SQL Server and Azure SQL
    /// * `oracle://[user[:password]@]host[:port][/service_name][?params]` for
    ///   Oracle Database (`oracle+tcps://` for TLS), with `?sid=NAME` for an
    ///   instance's SID instead of a service name, or
    ///   `oracle://[user[:password]@]?tns=NAME` for a net service name (or an
    ///   Easy Connect string or connect descriptor) given to the client as it
    ///   is
    ///
    /// User, password, host, database and query parameters are percent-decoded.
    /// Reserved characters (`@`, `:`, `/`, `?`) in a password should be
    /// percent-encoded; an unencoded `@` is tolerated because the last `@`
    /// delimits the user info. `sslmode` (`PostgreSQL`) and `ssl-mode`/`sslmode`
    /// (`MySQL`) are parsed into `ssl_mode`, and certificate parameters into
    /// [`TlsFiles`]: `sslrootcert`/`sslcert`/`sslkey` (`PostgreSQL`; the libpq
    /// value `sslrootcert=system` means the default trust store and sets no file)
    /// and `ssl-ca`/`ssl-cert`/`ssl-key` (`MySQL`, also spelled `ssl_ca`/`sslca`,
    /// `ssl_cert`/`sslcert` and `ssl_key`/`sslkey`). Empty values are ignored. For
    /// `PostgreSQL` the libpq
    /// parameters `host`, `port`, `user`, `password` and `dbname` override the
    /// URL components. For file databases the query string is ignored, except
    /// `mode=memory` which selects an in-memory database. For `libSQL` the
    /// `authToken` (or `auth_token`) parameter becomes the auth token and
    /// `sslrootcert` names a CA file. For
    /// `ClickHouse` the `secure`/`tls` parameter selects HTTPS on
    /// `clickhouse://` URLs (and must agree with a `clickhouse+http(s)://`
    /// scheme), and `sslrootcert`, `sslcert` and `sslkey` name a CA file and
    /// a client certificate and key. For SQL Server, parameter names are case-insensitive:
    /// `encrypt` (`strict`, `true`, `false`, `danger_plaintext`; default
    /// `true`), `trustServerCertificate`, a CA file in `certificate` (or
    /// `trustServerCertificateCA`, `sslrootcert`), `hostNameInCertificate`,
    /// `instance` (or a `host\instance` host, `%5C` encoded) and `database`
    /// (overriding the path). For Oracle, parameter names are
    /// case-insensitive: `sid`, `tns`, `protocol` (`tcp` or `tcps`; also
    /// `ssl=true`), `wallet_location` (or `wallet`) and `ssl_server_cert_dn`.
    ///
    /// # Errors
    ///
    /// * [`ConnectionUrlError::MissingScheme`] if there is no `scheme:` prefix
    /// * [`ConnectionUrlError::UnsupportedScheme`] for unknown schemes
    /// * [`ConnectionUrlError::InvalidUrl`] for a network URL without `//`
    /// * [`ConnectionUrlError::InvalidPort`] if the port is not a valid number
    /// * [`ConnectionUrlError::InvalidPercentEncoding`] for malformed `%XX` escapes
    /// * [`ConnectionUrlError::InvalidSslMode`] for an unknown `sslmode` value
    pub fn from_url(url: &str) -> Result<Self, ConnectionUrlError> {
        let url = url.trim();
        if url == ":memory:" {
            return Ok(Self::Sqlite { path: None });
        }
        let (scheme, rest) = url
            .split_once(':')
            .ok_or(ConnectionUrlError::MissingScheme)?;
        if scheme.is_empty() {
            return Err(ConnectionUrlError::MissingScheme);
        }
        match scheme.to_ascii_lowercase().as_str() {
            "postgres" | "postgresql" => {
                let parts = parse_network(rest)?;
                Self::postgres_from_parts(parts)
            }
            "mysql" | "mariadb" => {
                let parts = parse_network(rest)?;
                Self::mysql_from_parts(parts)
            }
            "sqlite" | "sqlite3" | "file" => Ok(Self::Sqlite {
                path: parse_file_path(rest)?,
            }),
            "duckdb" => Ok(Self::DuckDb {
                path: parse_file_path(rest)?,
            }),
            "libsql" | "http" | "https" => Self::libsql_from_url(scheme, rest),
            "clickhouse" | "clickhouse+http" | "clickhouse+https" => {
                Self::clickhouse_from_parts(&scheme.to_ascii_lowercase(), parse_network(rest)?)
            }
            "sqlserver" | "mssql" => Self::mssql_from_parts(parse_network(rest)?),
            "oracle" | "oracle+tcp" | "oracle+tcps" => {
                Self::oracle_from_parts(&scheme.to_ascii_lowercase(), parse_network(rest)?)
            }
            other => Err(ConnectionUrlError::UnsupportedScheme(other.to_string())),
        }
    }

    fn postgres_from_parts(mut parts: NetworkParts) -> Result<Self, ConnectionUrlError> {
        let ssl_mode = parts
            .params
            .remove("sslmode")
            .map(|mode| mode.parse())
            .transpose()?;
        let mut tls_files = TlsFiles {
            root_cert: take_path_param(&mut parts.params, &["sslrootcert"]),
            client_cert: take_path_param(&mut parts.params, &["sslcert"]),
            client_key: take_path_param(&mut parts.params, &["sslkey"]),
        };
        if tls_files
            .root_cert
            .as_deref()
            .is_some_and(|path| path == std::path::Path::new("system"))
        {
            tls_files.root_cert = None;
        }
        if let Some(host) = parts.params.remove("host") {
            parts.host = host;
        }
        if let Some(port) = parts.params.remove("port") {
            parts.port = Some(parse_port(&port)?);
        }
        if let Some(user) = parts.params.remove("user") {
            parts.user = user;
        }
        if let Some(password) = parts.params.remove("password") {
            parts.password = Some(password);
        }
        if let Some(dbname) = parts.params.remove("dbname") {
            parts.database = dbname;
        }
        if parts.host.is_empty() {
            "localhost".clone_into(&mut parts.host);
        }
        let params = std::mem::take(&mut parts.params);
        Ok(Self::Postgres {
            creds: parts.credentials(),
            ssl_mode,
            tls_files,
            params,
        })
    }

    fn mysql_from_parts(mut parts: NetworkParts) -> Result<Self, ConnectionUrlError> {
        let ssl_mode = ["ssl-mode", "sslmode", "ssl_mode"]
            .iter()
            .find_map(|key| parts.params.remove(*key))
            .map(|mode| mode.parse())
            .transpose()?;
        let tls_files = TlsFiles {
            root_cert: take_path_param(&mut parts.params, &["ssl-ca", "ssl_ca", "sslca"]),
            client_cert: take_path_param(&mut parts.params, &["ssl-cert", "ssl_cert", "sslcert"]),
            client_key: take_path_param(&mut parts.params, &["ssl-key", "ssl_key", "sslkey"]),
        };
        if let Some(socket) = parts.params.remove("socket") {
            parts.host = socket;
        }
        if parts.host.is_empty() {
            "localhost".clone_into(&mut parts.host);
        }
        let params = std::mem::take(&mut parts.params);
        Ok(Self::MySql {
            creds: parts.credentials(),
            ssl_mode,
            tls_files,
            params,
        })
    }

    fn clickhouse_from_parts(
        scheme: &str,
        mut parts: NetworkParts,
    ) -> Result<Self, ConnectionUrlError> {
        let secure = ["secure", "tls"]
            .iter()
            .filter_map(|key| parts.params.remove(*key))
            .next_back();
        let secure = match secure.as_deref() {
            None => None,
            Some("1" | "true" | "on") => Some(true),
            Some("0" | "false" | "off") => Some(false),
            Some(other) => {
                return Err(ConnectionUrlError::InvalidUrl(format!(
                    "secure must be true or false, not {other:?}"
                )));
            }
        };
        let tls = match (scheme, secure) {
            ("clickhouse", secure) => secure.unwrap_or(false),
            (other, secure) => {
                let https = other == "clickhouse+https";
                if secure.is_some_and(|secure| secure != https) {
                    return Err(ConnectionUrlError::InvalidUrl(format!(
                        "{other}:// URLs set TLS by their scheme, which secure= contradicts"
                    )));
                }
                https
            }
        };
        if parts.host.is_empty() {
            "localhost".clone_into(&mut parts.host);
        }
        let tls_files = TlsFiles {
            root_cert: take_path_param(&mut parts.params, &["sslrootcert"]),
            client_cert: take_path_param(&mut parts.params, &["sslcert"]),
            client_key: take_path_param(&mut parts.params, &["sslkey"]),
        };
        let params = std::mem::take(&mut parts.params);
        Ok(Self::ClickHouse {
            creds: parts.credentials(),
            tls,
            tls_files,
            params,
        })
    }

    fn mssql_from_parts(mut parts: NetworkParts) -> Result<Self, ConnectionUrlError> {
        // SQL Server's drivers match parameter names case-insensitively.
        let mut params: BTreeMap<String, String> = std::mem::take(&mut parts.params)
            .into_iter()
            .map(|(key, value)| (key.to_ascii_lowercase(), value))
            .collect();
        let flag = |name: &str, value: Option<String>| -> Result<bool, ConnectionUrlError> {
            match value.as_deref().map(str::to_ascii_lowercase).as_deref() {
                None | Some("false" | "no" | "0") => Ok(false),
                Some("true" | "yes" | "1") => Ok(true),
                Some(other) => Err(ConnectionUrlError::InvalidUrl(format!(
                    "{name} must be true or false, not {other:?}"
                ))),
            }
        };
        let encrypt = params
            .remove("encrypt")
            .map(|value| value.parse())
            .transpose()?
            .unwrap_or_default();
        let trust_server_certificate = flag(
            "trustServerCertificate",
            params
                .remove("trustservercertificate")
                .or_else(|| params.remove("trust_server_certificate")),
        )?;
        let tls_files = TlsFiles {
            root_cert: take_path_param(
                &mut params,
                &["certificate", "trustservercertificateca", "sslrootcert"],
            ),
            client_cert: None,
            client_key: None,
        };
        if ["sslcert", "sslkey"]
            .iter()
            .any(|key| params.contains_key(*key))
        {
            return Err(ConnectionUrlError::InvalidUrl(
                "SQL Server doesn't use client certificates".to_string(),
            ));
        }
        if trust_server_certificate && tls_files.root_cert.is_some() {
            return Err(ConnectionUrlError::InvalidUrl(
                "trustServerCertificate=true accepts any certificate, so a CA file to verify \
                 it with contradicts it"
                    .to_string(),
            ));
        }
        // A password in the query string would be kept with the other
        // parameters (and shown with them); it belongs in the user info.
        if ["password", "pwd"]
            .iter()
            .any(|key| params.contains_key(*key))
        {
            return Err(ConnectionUrlError::InvalidUrl(
                "give the password in the URL's user info (sqlserver://user:password@host) or \
                 separately, not as a query parameter"
                    .to_string(),
            ));
        }
        let host_name_in_certificate = params
            .remove("hostnameincertificate")
            .filter(|name| !name.is_empty());
        let mut instance = params
            .remove("instance")
            .or_else(|| params.remove("instancename"))
            .filter(|name| !name.is_empty());
        if let Some((host, name)) = parts.host.split_once('\\') {
            if !name.is_empty() {
                instance = Some(name.to_string());
            }
            parts.host = host.to_string();
        }
        if let Some(database) = params.remove("database") {
            parts.database = database;
        }
        if parts.host.is_empty() {
            "localhost".clone_into(&mut parts.host);
        }
        Ok(Self::Mssql {
            creds: parts.credentials(),
            instance,
            encrypt,
            trust_server_certificate,
            tls_files,
            host_name_in_certificate,
            params,
        })
    }

    fn oracle_from_parts(
        scheme: &str,
        mut parts: NetworkParts,
    ) -> Result<Self, ConnectionUrlError> {
        let invalid = |message: String| Err(ConnectionUrlError::InvalidUrl(message));
        let mut params: BTreeMap<String, String> = std::mem::take(&mut parts.params)
            .into_iter()
            .map(|(key, value)| (key.to_ascii_lowercase(), value))
            .collect();
        if ["password", "pwd"]
            .iter()
            .any(|key| params.contains_key(*key))
        {
            return invalid(
                "give the password in the URL's user info (oracle://user:password@host) or \
                 separately, not as a query parameter"
                    .to_string(),
            );
        }
        let mut tls = scheme == "oracle+tcps";
        if let Some(protocol) = params.remove("protocol") {
            match protocol.to_ascii_lowercase().as_str() {
                "tcps" => tls = true,
                "tcp" if scheme != "oracle+tcps" => tls = false,
                other => {
                    return invalid(format!(
                        "protocol must be tcp or tcps (agreeing with the scheme), not {other:?}"
                    ));
                }
            }
        }
        if let Some(ssl) = params.remove("ssl") {
            match ssl.to_ascii_lowercase().as_str() {
                "true" | "1" | "yes" => tls = true,
                "false" | "0" | "no" if scheme != "oracle+tcps" => tls = false,
                other => return invalid(format!("ssl must be true or false, not {other:?}")),
            }
        }
        let wallet_location = take_path_param(&mut params, &["wallet_location", "wallet"]);
        let server_cert_dn = params
            .remove("ssl_server_cert_dn")
            .filter(|dn| !dn.is_empty());
        if !tls && (wallet_location.is_some() || server_cert_dn.is_some()) {
            return invalid(
                "a wallet or certificate DN needs TLS (oracle+tcps:// or protocol=tcps)"
                    .to_string(),
            );
        }
        let connect_string = params.remove("tns").filter(|tns| !tns.is_empty());
        if connect_string.is_some() {
            if !parts.host.is_empty() || parts.port.is_some() || !parts.database.is_empty() {
                return invalid(
                    "tns= names the database by itself; leave out the host, port and service"
                        .to_string(),
                );
            }
            if tls || wallet_location.is_some() || server_cert_dn.is_some() {
                return invalid(
                    "with tns=, TLS settings belong in the net service's own configuration"
                        .to_string(),
                );
            }
        } else if parts.host.is_empty() {
            "localhost".clone_into(&mut parts.host);
        }
        let sid = if let Some(name) = params.remove("sid").filter(|name| !name.is_empty()) {
            if !parts.database.is_empty() {
                return invalid(
                    "give either a service name (the path) or a SID (sid=), not both".to_string(),
                );
            }
            parts.database = name;
            true
        } else {
            false
        };
        Ok(Self::Oracle {
            creds: parts.credentials(),
            sid,
            connect_string,
            tls,
            wallet_location,
            server_cert_dn,
            params,
        })
    }

    fn libsql_from_url(scheme: &str, rest: &str) -> Result<Self, ConnectionUrlError> {
        let rest = rest
            .strip_prefix("//")
            .ok_or_else(|| ConnectionUrlError::InvalidUrl("expected scheme://".to_string()))?;
        let (base, query) = rest.split_once('?').unwrap_or((rest, ""));
        let base = base.trim_end_matches('/');
        let authority = base.split('/').next().unwrap_or(base);
        if authority.is_empty() {
            return Err(ConnectionUrlError::InvalidUrl(
                "the URL has no host".to_string(),
            ));
        }
        if authority.contains('@') {
            return Err(ConnectionUrlError::InvalidUrl(
                "libSQL takes an auth token (authToken=...), not a user and password".to_string(),
            ));
        }
        split_host_port(authority)?;
        let mut params = parse_query(query)?;
        let auth_token = ["authToken", "auth_token"]
            .iter()
            .filter_map(|key| params.remove(*key))
            .find(|token| !token.is_empty());
        let tls_files = TlsFiles {
            root_cert: take_path_param(&mut params, &["sslrootcert"]),
            ..TlsFiles::default()
        };
        let tls = params.remove("tls");
        let scheme = match scheme.to_ascii_lowercase().as_str() {
            "libsql" => match tls.as_deref() {
                None | Some("1" | "true" | "on") => "https",
                Some("0" | "false" | "off") => "http",
                Some(other) => {
                    return Err(ConnectionUrlError::InvalidUrl(format!(
                        "tls must be 0 or 1, not {other:?}"
                    )));
                }
            },
            other => {
                if tls.is_some() {
                    return Err(ConnectionUrlError::InvalidUrl(format!(
                        "{other}:// URLs set TLS by their scheme; tls= is for libsql:// URLs"
                    )));
                }
                if other == "https" { "https" } else { "http" }
            }
        };
        Ok(Self::Libsql {
            url: format!("{scheme}://{base}"),
            auth_token,
            tls_files,
            params,
        })
    }

    /// Short name of the target's driver family (`postgres`, `mysql`,
    /// `sqlite`, `duckdb`, `libsql`, `clickhouse`, `mssql`, `oracle`).
    #[must_use]
    pub const fn driver_name(&self) -> &'static str {
        match self {
            Self::Postgres { .. } => "postgres",
            Self::MySql { .. } => "mysql",
            Self::Sqlite { .. } => "sqlite",
            Self::DuckDb { .. } => "duckdb",
            Self::Libsql { .. } => "libsql",
            Self::ClickHouse { .. } => "clickhouse",
            Self::Mssql { .. } => "mssql",
            Self::Oracle { .. } => "oracle",
        }
    }

    /// The credentials of a network target (`None` for file databases and
    /// `libSQL`, which authenticates with a token).
    #[must_use]
    pub const fn credentials(&self) -> Option<&Credentials> {
        match self {
            Self::Postgres { creds, .. }
            | Self::MySql { creds, .. }
            | Self::ClickHouse { creds, .. }
            | Self::Mssql { creds, .. }
            | Self::Oracle { creds, .. } => Some(creds),
            Self::Sqlite { .. } | Self::DuckDb { .. } | Self::Libsql { .. } => None,
        }
    }

    /// The effective port of a network target, applying the driver default
    /// (5432 for `PostgreSQL`, 3306 for `MySQL`, 443 or 80 for `libSQL`
    /// over `https://` or `http://`, 8443 or 8123 for `ClickHouse` with or
    /// without TLS, 1433 for SQL Server, 1521 or 2484 for Oracle without or
    /// with TLS; `None` for a SQL Server named instance without a port,
    /// whose port the SQL Server Browser assigns, and for an Oracle net
    /// service name).
    #[must_use]
    pub fn port_or_default(&self) -> Option<u16> {
        match self {
            Self::Postgres { creds, .. } => Some(creds.port().unwrap_or(5432)),
            Self::MySql { creds, .. } => Some(creds.port().unwrap_or(3306)),
            Self::ClickHouse { creds, tls, .. } => {
                Some(creds.port().unwrap_or(if *tls { 8443 } else { 8123 }))
            }
            Self::Mssql {
                creds, instance, ..
            } => creds.port().or_else(|| instance.is_none().then_some(1433)),
            Self::Oracle {
                creds,
                connect_string,
                tls,
                ..
            } => creds.port().or_else(|| {
                connect_string
                    .is_none()
                    .then_some(if *tls { 2484 } else { 1521 })
            }),
            Self::Libsql { url, .. } => {
                let (scheme, rest) = url.split_once("://")?;
                let authority = rest.split('/').next().unwrap_or(rest);
                let port = split_host_port(authority).ok()?.1;
                Some(port.unwrap_or(if scheme == "https" { 443 } else { 80 }))
            }
            Self::Sqlite { .. } | Self::DuckDb { .. } => None,
        }
    }

    /// The TLS certificate files of a network target (`None` for file
    /// databases and Oracle, which trusts a wallet; SQL Server's and
    /// `libSQL`'s carry a root certificate only).
    #[must_use]
    pub const fn tls_files(&self) -> Option<&TlsFiles> {
        match self {
            Self::Postgres { tls_files, .. }
            | Self::MySql { tls_files, .. }
            | Self::ClickHouse { tls_files, .. }
            | Self::Libsql { tls_files, .. }
            | Self::Mssql { tls_files, .. } => Some(tls_files),
            Self::Sqlite { .. } | Self::DuckDb { .. } | Self::Oracle { .. } => None,
        }
    }

    /// Whether this target is an in-memory file database.
    #[must_use]
    pub const fn is_in_memory(&self) -> bool {
        matches!(
            self,
            Self::Sqlite { path: None } | Self::DuckDb { path: None }
        )
    }
}

impl std::str::FromStr for ConnectionTarget {
    type Err = ConnectionUrlError;

    fn from_str(url: &str) -> Result<Self, Self::Err> {
        Self::from_url(url)
    }
}

/// Decomposed `scheme://user:pass@host:port/db?params` URL (percent-decoded).
#[derive(Debug, Default)]
pub(crate) struct NetworkParts {
    pub(crate) user: String,
    pub(crate) password: Option<String>,
    pub(crate) host: String,
    pub(crate) port: Option<u16>,
    pub(crate) database: String,
    pub(crate) params: BTreeMap<String, String>,
}

impl NetworkParts {
    fn credentials(self) -> Credentials {
        Credentials::new(
            self.host,
            self.port,
            self.database,
            self.user,
            self.password,
        )
    }
}

/// Parse the part of a network URL after `scheme:`.
pub(crate) fn parse_network(rest: &str) -> Result<NetworkParts, ConnectionUrlError> {
    let rest = rest
        .strip_prefix("//")
        .ok_or_else(|| ConnectionUrlError::InvalidUrl("expected scheme://".to_string()))?;

    // The last '@' delimits user info, so unencoded '@' in passwords still works.
    let (userinfo, after_userinfo) = rest
        .rfind('@')
        .map_or((None, rest), |at| (Some(&rest[..at]), &rest[at + 1..]));

    let authority_end = after_userinfo
        .find(['/', '?'])
        .unwrap_or(after_userinfo.len());
    let host_port = &after_userinfo[..authority_end];
    let after_authority = &after_userinfo[authority_end..];

    let (path, query) = match after_authority.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (after_authority, None),
    };

    let mut parts = NetworkParts::default();

    if let Some(userinfo) = userinfo {
        let (user, password) = match userinfo.split_once(':') {
            Some((user, password)) => (user, Some(password)),
            None => (userinfo, None),
        };
        parts.user = percent_decode(user, "user")?;
        parts.password = password
            .map(|password| percent_decode(password, "password"))
            .transpose()?;
    }

    let (host, port) = split_host_port(host_port)?;
    parts.host = percent_decode(host, "host")?;
    parts.port = port;

    parts.database = percent_decode(path.trim_start_matches('/'), "database")?;

    if let Some(query) = query {
        parts.params = parse_query(query)?;
    }

    Ok(parts)
}

fn split_host_port(host_port: &str) -> Result<(&str, Option<u16>), ConnectionUrlError> {
    if let Some(bracketed) = host_port.strip_prefix('[') {
        let (host, after) = bracketed
            .split_once(']')
            .ok_or_else(|| ConnectionUrlError::InvalidUrl("unterminated IPv6 host".into()))?;
        return match after.strip_prefix(':') {
            Some(port) if !port.is_empty() => Ok((host, Some(parse_port(port)?))),
            Some(_) => Ok((host, None)),
            None if after.is_empty() => Ok((host, None)),
            None => Err(ConnectionUrlError::InvalidUrl(format!(
                "unexpected characters after IPv6 host: {after}"
            ))),
        };
    }
    match host_port.rsplit_once(':') {
        Some((host, "")) => Ok((host, None)),
        Some((host, port)) => Ok((host, Some(parse_port(port)?))),
        None => Ok((host_port, None)),
    }
}

fn parse_port(port: &str) -> Result<u16, ConnectionUrlError> {
    port.parse::<u16>()
        .map_err(|_| ConnectionUrlError::InvalidPort(port.to_string()))
}

fn parse_query(query: &str) -> Result<BTreeMap<String, String>, ConnectionUrlError> {
    let mut params = BTreeMap::new();
    for pair in query.split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        params.insert(
            percent_decode(key, "query parameter")?,
            percent_decode(value, "query parameter")?,
        );
    }
    Ok(params)
}

/// Parse the part of a file-database URL after `scheme:` into a path (`None` = in-memory).
fn parse_file_path(rest: &str) -> Result<Option<PathBuf>, ConnectionUrlError> {
    let (path, query) = match rest.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (rest, None),
    };
    if let Some(query) = query
        && parse_query(query)?
            .get("mode")
            .is_some_and(|mode| mode == "memory")
    {
        return Ok(None);
    }
    // `scheme://path` → `path` (so `scheme:///abs` → `/abs`); `scheme:path` → `path`.
    let path = path.strip_prefix("//").unwrap_or(path);
    let path = percent_decode(path, "path")?;
    if path.is_empty() || path == ":memory:" {
        return Ok(None);
    }
    Ok(Some(PathBuf::from(path)))
}

/// Decode `%XX` escapes (no `+` → space translation).
pub(crate) fn percent_decode(
    value: &str,
    component: &'static str,
) -> Result<String, ConnectionUrlError> {
    if !value.contains('%') {
        return Ok(value.to_string());
    }
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let hex = bytes
                .get(i + 1..i + 3)
                .and_then(|hex| std::str::from_utf8(hex).ok())
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
                .ok_or(ConnectionUrlError::InvalidPercentEncoding(component))?;
            decoded.push(hex);
            i += 3;
        } else {
            decoded.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(decoded).map_err(|_| ConnectionUrlError::InvalidPercentEncoding(component))
}

#[cfg(test)]
mod tests {
    use super::*;

    use pretty_assertions::assert_eq;

    #[test_log::test]
    fn debug_redacts_secrets() {
        let target =
            ConnectionTarget::from_url("libsql://shop.example.turso.io?authToken=secret-token")
                .unwrap();
        let debug = format!("{target:?}");
        assert!(!debug.contains("secret-token"), "{debug}");
        assert!(debug.contains("<redacted>"), "{debug}");

        let target = ConnectionTarget::from_url("postgres://u:secret-pw@db/shop").unwrap();
        let debug = format!("{target:?}");
        assert!(!debug.contains("secret-pw"), "{debug}");
    }

    fn creds_of(target: &ConnectionTarget) -> &Credentials {
        target.credentials().expect("network target")
    }

    #[test_log::test]
    fn postgres_full_url() {
        let target =
            ConnectionTarget::from_url("postgres://alice:secret@db.example.com:6543/app").unwrap();
        let creds = creds_of(&target);
        assert_eq!(target.driver_name(), "postgres");
        assert_eq!(creds.user(), "alice");
        assert_eq!(creds.password(), Some("secret"));
        assert_eq!(creds.host(), "db.example.com");
        assert_eq!(creds.port(), Some(6543));
        assert_eq!(creds.name(), "app");
        assert_eq!(target.port_or_default(), Some(6543));
    }

    #[test_log::test]
    fn postgresql_scheme_and_default_port() {
        let target = ConnectionTarget::from_url("postgresql://u@localhost/db").unwrap();
        assert!(matches!(target, ConnectionTarget::Postgres { .. }));
        assert_eq!(creds_of(&target).port(), None);
        assert_eq!(target.port_or_default(), Some(5432));
    }

    #[test_log::test]
    fn percent_decoding_of_components() {
        let target = ConnectionTarget::from_url(
            "postgres://us%40er:p%40ss%3Aw%2Fo%25rd%3F@localhost/my%20db?application_name=my%20app",
        )
        .unwrap();
        let creds = creds_of(&target);
        assert_eq!(creds.user(), "us@er");
        assert_eq!(creds.password(), Some("p@ss:w/o%rd?"));
        assert_eq!(creds.name(), "my db");
        let ConnectionTarget::Postgres { params, .. } = &target else {
            panic!("expected postgres")
        };
        assert_eq!(
            params.get("application_name").map(String::as_str),
            Some("my app")
        );
    }

    #[test_log::test]
    fn unencoded_at_and_colon_in_password() {
        let target =
            ConnectionTarget::from_url("postgres://user:p@ss:w0rd!@localhost:5432/mydb").unwrap();
        let creds = creds_of(&target);
        assert_eq!(creds.user(), "user");
        assert_eq!(creds.password(), Some("p@ss:w0rd!"));
        assert_eq!(creds.host(), "localhost");
    }

    #[test_log::test]
    fn invalid_percent_encoding() {
        assert_eq!(
            ConnectionTarget::from_url("postgres://u:%zz@localhost/db"),
            Err(ConnectionUrlError::InvalidPercentEncoding("password"))
        );
        assert_eq!(
            ConnectionTarget::from_url("postgres://u:%ff@localhost/db"),
            Err(ConnectionUrlError::InvalidPercentEncoding("password"))
        );
    }

    #[test_log::test]
    fn ipv6_hosts() {
        let target = ConnectionTarget::from_url("postgres://u:p@[::1]:5433/db").unwrap();
        assert_eq!(creds_of(&target).host(), "::1");
        assert_eq!(creds_of(&target).port(), Some(5433));
        let target = ConnectionTarget::from_url("mysql://u@[fe80::1]/db").unwrap();
        assert_eq!(creds_of(&target).host(), "fe80::1");
        assert_eq!(creds_of(&target).port(), None);
        assert!(ConnectionTarget::from_url("postgres://u@[::1/db").is_err());
    }

    #[test_log::test]
    fn invalid_port() {
        assert_eq!(
            ConnectionTarget::from_url("postgres://u@localhost:99999/db"),
            Err(ConnectionUrlError::InvalidPort("99999".to_string()))
        );
        assert_eq!(
            ConnectionTarget::from_url("mysql://u@localhost:abc/db"),
            Err(ConnectionUrlError::InvalidPort("abc".to_string()))
        );
    }

    #[test_log::test]
    fn missing_user_database_and_host_are_driver_defaults() {
        let target = ConnectionTarget::from_url("postgres://localhost").unwrap();
        let creds = creds_of(&target);
        assert_eq!(creds.user(), "");
        assert_eq!(creds.password(), None);
        assert_eq!(creds.name(), "");
        assert_eq!(creds.host(), "localhost");

        let target = ConnectionTarget::from_url("postgres:///mydb").unwrap();
        assert_eq!(creds_of(&target).host(), "localhost");
        assert_eq!(creds_of(&target).name(), "mydb");
    }

    #[test_log::test]
    fn postgres_unix_socket_and_libpq_overrides() {
        let target = ConnectionTarget::from_url(
            "postgres:///app?host=%2Fvar%2Frun%2Fpostgresql&port=5433&user=bob&sslmode=disable&connect_timeout=5",
        )
        .unwrap();
        let ConnectionTarget::Postgres {
            creds,
            ssl_mode,
            tls_files,
            params,
        } = &target
        else {
            panic!("expected postgres")
        };
        assert_eq!(creds.host(), "/var/run/postgresql");
        assert_eq!(creds.port(), Some(5433));
        assert_eq!(creds.user(), "bob");
        assert_eq!(creds.name(), "app");
        assert_eq!(*ssl_mode, Some(SslMode::Disable));
        assert!(tls_files.is_empty());
        assert_eq!(
            params.iter().collect::<Vec<_>>(),
            vec![(&"connect_timeout".to_string(), &"5".to_string())]
        );

        let target = ConnectionTarget::from_url("postgres://u@h/db?host=/tmp").unwrap();
        assert_eq!(creds_of(&target).host(), "/tmp");
    }

    #[test_log::test]
    fn ssl_modes() {
        for (value, expected) in [
            ("disable", SslMode::Disable),
            ("allow", SslMode::Allow),
            ("prefer", SslMode::Prefer),
            ("require", SslMode::Require),
            ("verify-ca", SslMode::VerifyCa),
            ("verify-full", SslMode::VerifyFull),
        ] {
            let target =
                ConnectionTarget::from_url(&format!("postgres://u@h/db?sslmode={value}")).unwrap();
            let ConnectionTarget::Postgres { ssl_mode, .. } = target else {
                panic!("expected postgres")
            };
            assert_eq!(ssl_mode, Some(expected));
        }
        let target = ConnectionTarget::from_url("mysql://u@h/db?ssl-mode=VERIFY_IDENTITY").unwrap();
        let ConnectionTarget::MySql { ssl_mode, .. } = target else {
            panic!("expected mysql")
        };
        assert_eq!(ssl_mode, Some(SslMode::VerifyFull));
        assert_eq!(
            ConnectionTarget::from_url("postgres://u@h/db?sslmode=bogus"),
            Err(ConnectionUrlError::InvalidSslMode("bogus".to_string()))
        );
    }

    #[test_log::test]
    fn postgres_tls_files() {
        let target = ConnectionTarget::from_url(
            "postgres://u@h/db?sslmode=verify-full&sslrootcert=%2Fcerts%2Fca.pem\
             &sslcert=/certs/client.pem&sslkey=/certs/client.key&connect_timeout=5",
        )
        .unwrap();
        let ConnectionTarget::Postgres {
            tls_files, params, ..
        } = &target
        else {
            panic!("expected postgres")
        };
        assert_eq!(
            *tls_files,
            TlsFiles::new()
                .root_cert("/certs/ca.pem")
                .client_cert("/certs/client.pem")
                .client_key("/certs/client.key")
        );
        assert_eq!(params.keys().collect::<Vec<_>>(), vec!["connect_timeout"]);
        assert_eq!(target.tls_files(), Some(tls_files));

        // `system` selects the default trust store; empty values are ignored.
        let target =
            ConnectionTarget::from_url("postgres://u@h/db?sslrootcert=system&sslcert=").unwrap();
        assert!(target.tls_files().unwrap().is_empty());
        let ConnectionTarget::Postgres { params, .. } = &target else {
            panic!("expected postgres")
        };
        assert!(params.is_empty());
    }

    #[test_log::test]
    fn mysql_tls_files() {
        for url in [
            "mysql://u@h/db?ssl-ca=/c/ca.pem&ssl-cert=/c/cl.pem&ssl-key=/c/cl.key",
            "mysql://u@h/db?ssl_ca=/c/ca.pem&ssl_cert=/c/cl.pem&ssl_key=/c/cl.key",
            "mysql://u@h/db?sslca=/c/ca.pem&sslcert=/c/cl.pem&sslkey=/c/cl.key",
        ] {
            let target = ConnectionTarget::from_url(url).unwrap();
            let ConnectionTarget::MySql {
                tls_files, params, ..
            } = &target
            else {
                panic!("expected mysql")
            };
            assert_eq!(
                *tls_files,
                TlsFiles::new()
                    .root_cert("/c/ca.pem")
                    .client_cert("/c/cl.pem")
                    .client_key("/c/cl.key"),
                "{url}"
            );
            assert!(params.is_empty(), "{url}");
        }
        assert!(
            ConnectionTarget::from_url("sqlite::memory:")
                .unwrap()
                .tls_files()
                .is_none()
        );
    }

    #[test_log::test]
    fn mysql_and_mariadb() {
        let target = ConnectionTarget::from_url("mariadb://root:pw@127.0.0.1:3307/shop").unwrap();
        assert_eq!(target.driver_name(), "mysql");
        let creds = creds_of(&target);
        assert_eq!(creds.host(), "127.0.0.1");
        assert_eq!(creds.port(), Some(3307));
        assert_eq!(creds.name(), "shop");

        let target =
            ConnectionTarget::from_url("mysql://root@localhost?socket=%2Ftmp%2Fmysql.sock")
                .unwrap();
        assert_eq!(creds_of(&target).host(), "/tmp/mysql.sock");
        assert_eq!(creds_of(&target).name(), "");
        assert_eq!(target.port_or_default(), Some(3306));
    }

    #[test_log::test]
    fn sqlite_forms() {
        let path = |url: &str| match ConnectionTarget::from_url(url).unwrap() {
            ConnectionTarget::Sqlite { path } => path,
            other => panic!("expected sqlite, got {other:?}"),
        };
        assert_eq!(path("sqlite:data.db"), Some(PathBuf::from("data.db")));
        assert_eq!(path("sqlite://data.db"), Some(PathBuf::from("data.db")));
        assert_eq!(
            path("sqlite://dir/data.db"),
            Some(PathBuf::from("dir/data.db"))
        );
        assert_eq!(
            path("sqlite:///abs/data.db"),
            Some(PathBuf::from("/abs/data.db"))
        );
        assert_eq!(
            path("sqlite:/abs/data.db"),
            Some(PathBuf::from("/abs/data.db"))
        );
        assert_eq!(path("file:data.db"), Some(PathBuf::from("data.db")));
        assert_eq!(
            path("file:///abs/my%20data.db"),
            Some(PathBuf::from("/abs/my data.db"))
        );
        assert_eq!(
            path("sqlite:data.db?mode=ro"),
            Some(PathBuf::from("data.db"))
        );
        assert_eq!(path("sqlite::memory:"), None);
        assert_eq!(path("sqlite://:memory:"), None);
        assert_eq!(path(":memory:"), None);
        assert_eq!(path("sqlite:"), None);
        assert_eq!(path("file:shared?mode=memory&cache=shared"), None);
        assert!(
            ConnectionTarget::from_url("sqlite::memory:")
                .unwrap()
                .is_in_memory()
        );
    }

    #[test_log::test]
    fn duckdb_forms() {
        let path = |url: &str| match ConnectionTarget::from_url(url).unwrap() {
            ConnectionTarget::DuckDb { path } => path,
            other => panic!("expected duckdb, got {other:?}"),
        };
        assert_eq!(
            path("duckdb:data.duckdb"),
            Some(PathBuf::from("data.duckdb"))
        );
        assert_eq!(
            path("duckdb://data.duckdb"),
            Some(PathBuf::from("data.duckdb"))
        );
        assert_eq!(
            path("duckdb:///abs/x.duckdb"),
            Some(PathBuf::from("/abs/x.duckdb"))
        );
        assert_eq!(path("duckdb::memory:"), None);
        assert_eq!(path("duckdb:"), None);
    }

    #[test_log::test]
    fn scheme_errors() {
        assert_eq!(
            ConnectionTarget::from_url("no-scheme-here"),
            Err(ConnectionUrlError::MissingScheme)
        );
        assert_eq!(
            ConnectionTarget::from_url("mongodb://u@h/db"),
            Err(ConnectionUrlError::UnsupportedScheme("mongodb".to_string()))
        );
        assert!(matches!(
            ConnectionTarget::from_url("postgres:localhost/db"),
            Err(ConnectionUrlError::InvalidUrl(_))
        ));
        assert!("postgres://u@h/db".parse::<ConnectionTarget>().is_ok());
    }

    #[test_log::test]
    fn libsql_urls_move_the_token_out_of_the_url() {
        let target =
            ConnectionTarget::from_url("libsql://shop-example.turso.io?authToken=abc.def&x=1")
                .unwrap();
        assert_eq!(
            target,
            ConnectionTarget::Libsql {
                url: "https://shop-example.turso.io".to_string(),
                auth_token: Some("abc.def".to_string()),
                tls_files: TlsFiles::default(),
                params: BTreeMap::from([("x".to_string(), "1".to_string())]),
            }
        );
        assert_eq!(target.driver_name(), "libsql");
        assert_eq!(target.port_or_default(), Some(443));
        assert_eq!(target.credentials(), None);

        let local = ConnectionTarget::from_url("http://127.0.0.1:8080/").unwrap();
        assert_eq!(
            local,
            ConnectionTarget::Libsql {
                url: "http://127.0.0.1:8080".to_string(),
                auth_token: None,
                tls_files: TlsFiles::default(),
                params: BTreeMap::new(),
            }
        );
        assert_eq!(local.port_or_default(), Some(8080));
        assert!(matches!(
            ConnectionTarget::from_url("libsql://localhost:8080?tls=0"),
            Ok(ConnectionTarget::Libsql { url, .. }) if url == "http://localhost:8080"
        ));
        assert!(matches!(
            ConnectionTarget::from_url("https://db.example?auth_token=t"),
            Ok(ConnectionTarget::Libsql { auth_token: Some(token), .. }) if token == "t"
        ));
        let private =
            ConnectionTarget::from_url("https://db.example?sslrootcert=/etc/ca.pem").unwrap();
        assert_eq!(
            private
                .tls_files()
                .and_then(|files| files.root_cert.as_deref()),
            Some(std::path::Path::new("/etc/ca.pem"))
        );
    }

    #[test_log::test]
    fn clickhouse_urls_carry_credentials_and_tls() {
        let target =
            ConnectionTarget::from_url("clickhouse://shop:s%40cret@ch.example/music?x=1").unwrap();
        let ConnectionTarget::ClickHouse {
            creds,
            tls,
            tls_files,
            params,
        } = &target
        else {
            panic!("{target:?}");
        };
        assert_eq!(
            (creds.host(), creds.user(), creds.password(), creds.name()),
            ("ch.example", "shop", Some("s@cret"), "music")
        );
        assert!(!tls);
        assert!(tls_files.is_empty());
        assert_eq!(
            params,
            &BTreeMap::from([("x".to_string(), "1".to_string())])
        );
        assert_eq!(target.driver_name(), "clickhouse");
        assert_eq!(target.port_or_default(), Some(8123));
        assert!(!format!("{target:?}").contains("s@cret"));

        let secure = ConnectionTarget::from_url("clickhouse://ch.example?secure=true").unwrap();
        assert!(matches!(
            secure,
            ConnectionTarget::ClickHouse { tls: true, .. }
        ));
        assert_eq!(secure.port_or_default(), Some(8443));
        let https = ConnectionTarget::from_url("clickhouse+https://ch.example:9443").unwrap();
        assert!(matches!(
            https,
            ConnectionTarget::ClickHouse { tls: true, .. }
        ));
        assert_eq!(https.port_or_default(), Some(9443));
        let local = ConnectionTarget::from_url("clickhouse+http://").unwrap();
        assert!(matches!(
            &local,
            ConnectionTarget::ClickHouse { creds, tls: false, .. } if creds.host() == "localhost"
        ));
        let files = ConnectionTarget::from_url(
            "clickhouse+https://ch.example?sslrootcert=/c/ca.pem&sslcert=/c/cl.pem&sslkey=/c/cl.key&x=1",
        )
        .unwrap();
        assert_eq!(
            files.tls_files(),
            Some(
                &TlsFiles::new()
                    .root_cert("/c/ca.pem")
                    .client_cert("/c/cl.pem")
                    .client_key("/c/cl.key")
            )
        );
        assert!(matches!(
            &files,
            ConnectionTarget::ClickHouse { params, .. } if params.len() == 1
        ));
        assert!(matches!(
            ConnectionTarget::from_url("clickhouse+https://ch.example?secure=true"),
            Ok(ConnectionTarget::ClickHouse { tls: true, params, .. }) if params.is_empty()
        ));
        for url in [
            "clickhouse://ch.example?secure=maybe",
            "clickhouse+https://ch.example?secure=false",
            "clickhouse+http://ch.example?tls=1",
        ] {
            assert!(
                matches!(
                    ConnectionTarget::from_url(url),
                    Err(ConnectionUrlError::InvalidUrl(_))
                ),
                "{url}"
            );
        }
    }

    #[test_log::test]
    fn mssql_urls_carry_instances_and_tls_settings() {
        let target = ConnectionTarget::from_url(
            "sqlserver://shop:s%40cret@db.example:14330/music?Encrypt=strict&TrustServerCertificate=false&applicationName=shop",
        )
        .unwrap();
        let ConnectionTarget::Mssql {
            creds,
            instance,
            encrypt,
            trust_server_certificate,
            tls_files,
            host_name_in_certificate,
            params,
        } = &target
        else {
            panic!("{target:?}");
        };
        assert_eq!(
            (creds.host(), creds.user(), creds.password(), creds.name()),
            ("db.example", "shop", Some("s@cret"), "music")
        );
        assert_eq!(*instance, None);
        assert_eq!(*encrypt, MssqlEncrypt::Strict);
        assert!(!trust_server_certificate);
        assert!(tls_files.is_empty());
        assert_eq!(*host_name_in_certificate, None);
        assert_eq!(
            params,
            &BTreeMap::from([("applicationname".to_string(), "shop".to_string())])
        );
        assert_eq!(target.driver_name(), "mssql");
        assert_eq!(target.port_or_default(), Some(14330));
        assert!(!format!("{target:?}").contains("s@cret"));

        let named =
            ConnectionTarget::from_url("mssql://sa@db.example%5CSQLEXPRESS?database=shop").unwrap();
        assert!(matches!(
            &named,
            ConnectionTarget::Mssql { creds, instance: Some(instance), encrypt: MssqlEncrypt::Required, .. }
                if creds.host() == "db.example" && instance == "SQLEXPRESS" && creds.name() == "shop"
        ));
        assert_eq!(named.port_or_default(), None);
        let local = ConnectionTarget::from_url(
            "sqlserver://?encrypt=false&certificate=/etc/ca.pem&hostNameInCertificate=db",
        )
        .unwrap();
        assert!(matches!(
            &local,
            ConnectionTarget::Mssql {
                creds,
                encrypt: MssqlEncrypt::LoginOnly,
                tls_files,
                host_name_in_certificate: Some(name),
                ..
            } if creds.host() == "localhost"
                && tls_files.root_cert.as_deref() == Some(std::path::Path::new("/etc/ca.pem"))
                && name == "db"
        ));
        assert_eq!(local.port_or_default(), Some(1433));
        for url in [
            "sqlserver://db?encrypt=maybe",
            "sqlserver://db?trustServerCertificate=perhaps",
            "sqlserver://db?trustServerCertificate=true&certificate=/etc/ca.pem",
            "sqlserver://db?sslcert=/etc/client.pem",
        ] {
            assert!(
                matches!(
                    ConnectionTarget::from_url(url),
                    Err(ConnectionUrlError::InvalidUrl(_))
                ),
                "{url}"
            );
        }
    }

    #[test_log::test]
    fn libsql_urls_reject_user_info_and_misplaced_tls() {
        for url in [
            "libsql://user:pw@db.example",
            "libsql://",
            "https://db.example?tls=0",
            "libsql://db.example?tls=maybe",
        ] {
            assert!(
                matches!(
                    ConnectionTarget::from_url(url),
                    Err(ConnectionUrlError::InvalidUrl(_))
                ),
                "{url}"
            );
        }
    }

    #[test_log::test]
    fn connect_options_builder() {
        let options = ConnectOptions::new()
            .read_only()
            .statement_timeout(Duration::from_secs(3))
            .application_name("viewer")
            .pool_size(NonZeroUsize::new(2).unwrap());
        assert!(options.is_read_only());
        assert_eq!(options.access, AccessMode::ReadOnly);
        assert_eq!(options.statement_timeout, Some(Duration::from_secs(3)));
        assert_eq!(options.application_name.as_deref(), Some("viewer"));
        assert_eq!(options.pool_size.map(NonZeroUsize::get), Some(2));
        assert_eq!(ConnectOptions::default().access, AccessMode::ReadWrite);
        assert!(
            !ConnectOptions::new()
                .access(AccessMode::ReadWrite)
                .is_read_only()
        );
    }
}
#[test]
fn oracle_urls_carry_services_sids_tns_and_tls() {
    let target =
        ConnectionTarget::from_url("oracle://shop:s%40cret@db.example:1522/FREEPDB1").unwrap();
    let ConnectionTarget::Oracle {
        creds,
        sid,
        connect_string,
        tls,
        ..
    } = &target
    else {
        panic!("{target:?}");
    };
    assert_eq!(
        (creds.host(), creds.port(), creds.name(), creds.user()),
        ("db.example", Some(1522), "FREEPDB1", "shop")
    );
    assert_eq!(creds.password(), Some("s@cret"));
    assert!(!*sid && connect_string.is_none() && !*tls);
    assert_eq!(target.driver_name(), "oracle");
    assert_eq!(target.port_or_default(), Some(1522));
    let debug = format!("{target:?}");
    assert!(!debug.contains("s@cret"), "{debug}");

    let target = ConnectionTarget::from_url(
        "oracle+tcps://shop@db.example?SID=ORCL&wallet_location=/etc/wallet",
    )
    .unwrap();
    let ConnectionTarget::Oracle {
        creds,
        sid,
        tls,
        wallet_location,
        ..
    } = &target
    else {
        panic!("{target:?}");
    };
    assert!(*sid && *tls);
    assert_eq!(creds.name(), "ORCL");
    assert_eq!(
        wallet_location.as_deref(),
        Some(std::path::Path::new("/etc/wallet"))
    );
    assert_eq!(target.port_or_default(), Some(2484));

    let target = ConnectionTarget::from_url("oracle://shop@?tns=shop_high").unwrap();
    let ConnectionTarget::Oracle { connect_string, .. } = &target else {
        panic!("{target:?}");
    };
    assert_eq!(connect_string.as_deref(), Some("shop_high"));
    assert_eq!(target.port_or_default(), None);
    assert!(matches!(
        ConnectionTarget::from_url("oracle://h/db?protocol=tcps").unwrap(),
        ConnectionTarget::Oracle { tls: true, .. }
    ));
    for url in [
        "oracle://h/db?password=hunter2",
        "oracle://h/db?sid=ORCL",
        "oracle://h/db?wallet_location=/w",
        "oracle://h/db?tns=alias",
        "oracle+tcps://h/db?protocol=tcp",
        "oracle://h/db?protocol=udp",
    ] {
        let error = ConnectionTarget::from_url(url).unwrap_err().to_string();
        assert!(!error.contains("hunter2"), "{error}");
    }
}

#[test]
fn mssql_refuses_passwords_in_the_query_string() {
    for url in [
        "sqlserver://h/db?password=hunter2",
        "mssql://h/db?PWD=hunter2",
        "sqlserver://h/db?Password=hunter2",
    ] {
        let error = ConnectionTarget::from_url(url).unwrap_err().to_string();
        assert!(!error.contains("hunter2"), "{error}");
    }
}
