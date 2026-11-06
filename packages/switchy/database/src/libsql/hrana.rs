//! Hrana over HTTP: the protocol `libSQL` servers (`sqld`, Turso Cloud) speak.
//!
//! A client sends a *pipeline* of requests to `POST /v2/pipeline`. Requests in
//! one pipeline run in order on one server-side *stream* (a database
//! connection). The response carries a *baton* that continues the stream in
//! a later pipeline; a pipeline without a baton opens a new stream, and a
//! `close` request ends it. The server closes idle streams after a few
//! seconds, rolling back any transaction they hold.
//!
//! Only the parts of the protocol this backend uses are modelled here:
//! `execute`, `batch` (with step conditions), `describe` and `close`.

use base64::Engine as _;
use serde::{Deserialize, Serialize};

use crate::DatabaseValue;

use super::{LibsqlDatabaseError, error_chain};

/// Base64 that accepts padded and unpadded input (`sqld` omits the padding).
const BASE64: base64::engine::GeneralPurpose = base64::engine::GeneralPurpose::new(
    &base64::alphabet::STANDARD,
    base64::engine::GeneralPurposeConfig::new()
        .with_encode_padding(true)
        .with_decode_padding_mode(base64::engine::DecodePaddingMode::Indifferent),
);

/// A value as Hrana encodes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Value {
    /// SQL `NULL`.
    Null,
    /// A 64-bit integer, sent as a decimal string so no precision is lost.
    Integer {
        /// The integer in decimal.
        value: String,
    },
    /// A double.
    Float {
        /// The number.
        value: f64,
    },
    /// UTF-8 text.
    Text {
        /// The text.
        value: String,
    },
    /// Bytes.
    Blob {
        /// The bytes in base64.
        base64: String,
    },
}

impl Value {
    fn integer(value: i64) -> Self {
        Self::Integer {
            value: value.to_string(),
        }
    }

    fn text(value: impl Into<String>) -> Self {
        Self::Text {
            value: value.into(),
        }
    }

    /// Encode a parameter.
    ///
    /// # Errors
    ///
    /// * [`LibsqlDatabaseError::UnsupportedValue`] for an unsigned integer
    ///   above `i64::MAX`, a non-finite float, or `Now`/`NowPlus` (which are
    ///   rendered into the SQL text before binding)
    #[allow(clippy::too_many_lines)]
    pub fn from_database_value(value: &DatabaseValue) -> Result<Self, LibsqlDatabaseError> {
        if crate::query::Expression::is_null(value) {
            return Ok(Self::Null);
        }
        let float = |value: f64| {
            if value.is_finite() {
                Ok(Self::Float { value })
            } else {
                Err(LibsqlDatabaseError::UnsupportedValue(format!(
                    "{value} has no Hrana encoding"
                )))
            }
        };
        Ok(match value {
            DatabaseValue::String(text) | DatabaseValue::StringOpt(Some(text)) => {
                Self::text(text.clone())
            }
            DatabaseValue::Bool(value) | DatabaseValue::BoolOpt(Some(value)) => {
                Self::integer(i64::from(*value))
            }
            DatabaseValue::Int8(value) | DatabaseValue::Int8Opt(Some(value)) => {
                Self::integer(i64::from(*value))
            }
            DatabaseValue::Int16(value) | DatabaseValue::Int16Opt(Some(value)) => {
                Self::integer(i64::from(*value))
            }
            DatabaseValue::Int32(value) | DatabaseValue::Int32Opt(Some(value)) => {
                Self::integer(i64::from(*value))
            }
            DatabaseValue::Int64(value) | DatabaseValue::Int64Opt(Some(value)) => {
                Self::integer(*value)
            }
            DatabaseValue::UInt8(value) | DatabaseValue::UInt8Opt(Some(value)) => {
                Self::integer(i64::from(*value))
            }
            DatabaseValue::UInt16(value) | DatabaseValue::UInt16Opt(Some(value)) => {
                Self::integer(i64::from(*value))
            }
            DatabaseValue::UInt32(value) | DatabaseValue::UInt32Opt(Some(value)) => {
                Self::integer(i64::from(*value))
            }
            DatabaseValue::UInt64(value) | DatabaseValue::UInt64Opt(Some(value)) => {
                Self::integer(i64::try_from(*value).map_err(|_| {
                    LibsqlDatabaseError::UnsupportedValue(format!("{value} is above i64::MAX"))
                })?)
            }
            DatabaseValue::Real32(value) | DatabaseValue::Real32Opt(Some(value)) => {
                float(f64::from(*value))?
            }
            DatabaseValue::Real64(value) | DatabaseValue::Real64Opt(Some(value)) => float(*value)?,
            #[cfg(feature = "decimal")]
            DatabaseValue::Decimal(value) | DatabaseValue::DecimalOpt(Some(value)) => {
                Self::text(value.to_string())
            }
            #[cfg(feature = "uuid")]
            DatabaseValue::Uuid(value) | DatabaseValue::UuidOpt(Some(value)) => {
                Self::text(value.to_string())
            }
            #[cfg(feature = "json")]
            DatabaseValue::Json(value) | DatabaseValue::JsonOpt(Some(value)) => {
                Self::text(value.to_string())
            }
            DatabaseValue::DateTime(value) => {
                Self::text(value.format("%Y-%m-%d %H:%M:%S").to_string())
            }
            DatabaseValue::Date(value) | DatabaseValue::DateOpt(Some(value)) => {
                Self::text(value.format("%Y-%m-%d").to_string())
            }
            DatabaseValue::Time(value) | DatabaseValue::TimeOpt(Some(value)) => {
                Self::text(value.format("%H:%M:%S%.f").to_string())
            }
            DatabaseValue::DateTimeTz(value) | DatabaseValue::DateTimeTzOpt(Some(value)) => {
                Self::text(value.to_rfc3339())
            }
            DatabaseValue::Bytes(bytes) | DatabaseValue::BytesOpt(Some(bytes)) => Self::Blob {
                base64: BASE64.encode(bytes),
            },
            DatabaseValue::Other { text, .. } => Self::text(text.clone()),
            DatabaseValue::Now | DatabaseValue::NowPlus(_) => {
                return Err(LibsqlDatabaseError::UnsupportedValue(
                    "Now/NowPlus must be rendered into the SQL before binding".to_string(),
                ));
            }
            // Every remaining variant is a `None` option, handled above.
            _ => Self::Null,
        })
    }

    /// Decode a result value.
    ///
    /// # Errors
    ///
    /// * [`LibsqlDatabaseError::Protocol`] if an integer or blob is malformed
    pub fn into_database_value(self) -> Result<DatabaseValue, LibsqlDatabaseError> {
        Ok(match self {
            Self::Null => DatabaseValue::Null,
            Self::Integer { value } => DatabaseValue::Int64(value.parse().map_err(|_| {
                LibsqlDatabaseError::Protocol(format!("invalid integer {value:?}"))
            })?),
            Self::Float { value } => DatabaseValue::Real64(value),
            Self::Text { value } => DatabaseValue::String(value),
            Self::Blob { base64 } => {
                DatabaseValue::Bytes(BASE64.decode(base64.as_bytes()).map_err(|error| {
                    LibsqlDatabaseError::Protocol(format!("invalid blob: {error}"))
                })?)
            }
        })
    }
}

/// A statement with positional arguments.
#[derive(Debug, Clone, Serialize)]
pub struct Stmt {
    /// The SQL text (one statement).
    pub sql: String,
    /// Positional arguments for `?` placeholders.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<Value>,
    /// Whether the server should send result rows.
    pub want_rows: bool,
}

impl Stmt {
    /// A statement without arguments that returns its rows.
    #[must_use]
    pub fn new(sql: impl Into<String>) -> Self {
        Self {
            sql: sql.into(),
            args: Vec::new(),
            want_rows: true,
        }
    }

    /// A statement with arguments.
    ///
    /// # Errors
    ///
    /// * [`LibsqlDatabaseError::UnsupportedValue`] if an argument can't be encoded
    pub fn with_params(
        sql: impl Into<String>,
        params: &[DatabaseValue],
        want_rows: bool,
    ) -> Result<Self, LibsqlDatabaseError> {
        Ok(Self {
            sql: sql.into(),
            args: params
                .iter()
                .map(Value::from_database_value)
                .collect::<Result<_, _>>()?,
            want_rows,
        })
    }
}

/// A condition on earlier steps of a batch.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum BatchCond {
    /// The step succeeded.
    Ok {
        /// Index of the step.
        step: usize,
    },
}

/// A batch step.
#[derive(Debug, Clone, Serialize)]
pub struct BatchStep {
    /// Run only when this holds (always when absent).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub condition: Option<BatchCond>,
    /// The statement.
    pub stmt: Stmt,
}

/// A batch of statements.
#[derive(Debug, Clone, Serialize)]
pub struct Batch {
    /// The steps, run in order.
    pub steps: Vec<BatchStep>,
}

/// A request in a pipeline.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Request {
    /// Run one statement.
    Execute {
        /// The statement.
        stmt: Stmt,
    },
    /// Run statements with conditions.
    Batch {
        /// The batch.
        batch: Batch,
    },
    /// Parse a statement without running it.
    Describe {
        /// The SQL text.
        sql: String,
    },
    /// End the stream.
    Close,
}

#[derive(Debug, Serialize)]
struct PipelineRequest<'a> {
    baton: Option<&'a str>,
    requests: &'a [Request],
}

/// A result column.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Col {
    /// Column name, if the server reports one.
    pub name: Option<String>,
    /// Declared type of the source column, if any.
    pub decltype: Option<String>,
}

/// The result of one statement.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct StmtResult {
    /// Result columns.
    #[serde(default)]
    pub cols: Vec<Col>,
    /// Result rows (empty unless rows were wanted).
    #[serde(default)]
    pub rows: Vec<Vec<Value>>,
    /// Rows changed by an `INSERT`, `UPDATE` or `DELETE`.
    #[serde(default)]
    pub affected_row_count: u64,
}

/// The result of a batch.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct BatchResult {
    /// Each step's result (`None`: it failed or did not run).
    pub step_results: Vec<Option<StmtResult>>,
    /// Each step's error (`None`: it succeeded or did not run).
    pub step_errors: Vec<Option<ServerError>>,
}

/// The result of `describe`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct DescribeResult {
    /// Whether the statement leaves the database file unchanged
    /// (`sqlite3_stmt_readonly`).
    pub is_readonly: bool,
}

/// An error reported by the server.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ServerError {
    /// What went wrong.
    pub message: String,
    /// Machine-readable code (`SQLITE_CONSTRAINT`, `STREAM_EXPIRED`, ...).
    #[serde(default)]
    pub code: Option<String>,
}

/// A successful response to one request.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Response {
    /// To `execute`.
    Execute {
        /// The statement's result.
        result: StmtResult,
    },
    /// To `batch`.
    Batch {
        /// The batch's result.
        result: BatchResult,
    },
    /// To `describe`.
    Describe {
        /// The description.
        result: DescribeResult,
    },
    /// To `close`.
    Close,
}

/// The outcome of one request.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum StreamResult {
    /// It succeeded.
    Ok {
        /// The response.
        response: Response,
    },
    /// It failed.
    Error {
        /// Why.
        error: ServerError,
    },
}

impl StreamResult {
    /// The response, or the server's error.
    ///
    /// # Errors
    ///
    /// * [`LibsqlDatabaseError::Server`] if the request failed
    pub fn into_response(self) -> Result<Response, LibsqlDatabaseError> {
        match self {
            Self::Ok { response } => Ok(response),
            Self::Error { error } => Err(LibsqlDatabaseError::server(error)),
        }
    }
}

/// The response to a pipeline.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PipelineResponse {
    /// Continues the stream (`None` once it is closed).
    pub baton: Option<String>,
    /// Where to send the next pipeline of this stream, if not the same URL.
    pub base_url: Option<String>,
    /// One result per request.
    pub results: Vec<StreamResult>,
}

/// Where a stream continues.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StreamPosition {
    /// The baton from the last response.
    pub baton: Option<String>,
    /// The base URL from the last response.
    pub base_url: Option<String>,
}

/// How a [`HranaClient`] reaches its server.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct HttpOptions {
    /// How long to wait for the TCP/TLS connection.
    pub connect_timeout: Option<std::time::Duration>,
    /// How long to wait for each request's whole response.
    pub request_timeout: Option<std::time::Duration>,
    /// Connect here instead of to the address the URL's host resolves to,
    /// still naming the URL's host for TLS and in the `Host` header.
    pub connect_address: Option<std::net::SocketAddr>,
    /// PEM certificate authorities trusted in addition to the system's.
    pub root_certificates: Option<Vec<u8>>,
}

impl HttpOptions {
    /// No timeouts, the URL's own address and the system's trust store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the connect timeout.
    #[must_use]
    pub const fn connect_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.connect_timeout = Some(timeout);
        self
    }

    /// Set the request timeout.
    #[must_use]
    pub const fn request_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }

    /// Connect to `address` instead of the URL host's own address.
    #[must_use]
    pub const fn connect_address(mut self, address: std::net::SocketAddr) -> Self {
        self.connect_address = Some(address);
        self
    }

    /// Trust the certificate authorities in `pem` too.
    #[must_use]
    pub fn root_certificates(mut self, pem: Vec<u8>) -> Self {
        self.root_certificates = Some(pem);
        self
    }
}

/// Where requests for the server's own host and port go instead
/// ([`HttpOptions::connect_address`]).
#[derive(Debug, Clone)]
struct Route {
    /// The host as the URL names it (`db.example.com`, `[::1]`).
    host: String,
    /// The URL's port, or its scheme's default.
    port: u16,
    /// Whether the host is an IP address (reqwest never resolves those).
    ip_literal: bool,
    /// Where to connect.
    address: std::net::SocketAddr,
}

/// An HTTP client for one `libSQL` server.
#[derive(Clone)]
pub struct HranaClient {
    http: reqwest::Client,
    base_url: String,
    auth_token: Option<String>,
    request_timeout: Option<std::time::Duration>,
    route: Option<Route>,
}

impl std::fmt::Debug for HranaClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HranaClient")
            .field("base_url", &self.base_url)
            .field(
                "auth_token",
                &self.auth_token.as_ref().map(|_| "<redacted>"),
            )
            .field("request_timeout", &self.request_timeout)
            .field("route", &self.route)
            .finish_non_exhaustive()
    }
}

impl HranaClient {
    /// A client for the server at `base_url` (`http://` or `https://`).
    ///
    /// With [`HttpOptions::connect_address`], requests to the URL's host
    /// and port connect to that address instead (a tunnel's local end, a
    /// port forward), while TLS still checks the certificate against, and
    /// sends as SNI, the URL's host name, and the `Host` header still names
    /// it. An `https://` URL whose host is an IP address can't be checked
    /// that way and is refused.
    ///
    /// # Errors
    ///
    /// * [`LibsqlDatabaseError::InvalidUrl`] if `base_url` doesn't parse, or
    ///   names its TLS server by IP address with a connect address
    /// * [`LibsqlDatabaseError::InvalidConfig`] if the root certificates
    ///   aren't PEM certificates
    /// * [`LibsqlDatabaseError::Http`] if the HTTP client can't be built
    pub fn new(
        base_url: &str,
        auth_token: Option<String>,
        options: &HttpOptions,
    ) -> Result<Self, LibsqlDatabaseError> {
        let base_url = base_url.trim_end_matches('/').to_string();
        let mut builder = reqwest::Client::builder()
            .user_agent(concat!("switchy_database/", env!("CARGO_PKG_VERSION")));
        if let Some(timeout) = options.connect_timeout {
            builder = builder.connect_timeout(timeout);
        }
        if let Some(timeout) = options.request_timeout {
            builder = builder.timeout(timeout);
        }
        if let Some(pem) = &options.root_certificates {
            let certificates = reqwest::Certificate::from_pem_bundle(pem).map_err(|error| {
                LibsqlDatabaseError::InvalidConfig(format!(
                    "the CA certificate isn't usable: {}",
                    error_chain(&error)
                ))
            })?;
            if certificates.is_empty() {
                return Err(LibsqlDatabaseError::InvalidConfig(
                    "the CA certificate file holds no PEM certificate".to_string(),
                ));
            }
            builder = builder.tls_certs_merge(certificates);
        }
        let route = match options.connect_address {
            Some(address) => Some(route(&base_url, address)?),
            None => None,
        };
        if let Some(route) = route.as_ref().filter(|route| !route.ip_literal) {
            builder = builder.resolve(&route.host, route.address);
        }
        Ok(Self {
            http: builder.build().map_err(LibsqlDatabaseError::Http)?,
            base_url,
            auth_token: auth_token.filter(|token| !token.is_empty()),
            request_timeout: options.request_timeout,
            route,
        })
    }

    /// The URL to send a request for `url` to, and the `Host` header it
    /// carries: `url` itself unless it is the server's own host and port
    /// and a connect address is set. reqwest connects to the URL's port
    /// whenever the URL names one, even for a host it was told to resolve
    /// elsewhere, so the port moves from the URL to the `Host` header.
    fn routed(&self, url: &str) -> Result<(String, Option<String>), LibsqlDatabaseError> {
        let Some(route) = &self.route else {
            return Ok((url.to_string(), None));
        };
        let mut parsed = reqwest::Url::parse(url)
            .map_err(|error| LibsqlDatabaseError::InvalidUrl(format!("{url}: {error}")))?;
        if parsed.host_str() != Some(route.host.as_str())
            || parsed.port_or_known_default() != Some(route.port)
        {
            return Ok((url.to_string(), None));
        }
        let host = parsed.port().map_or_else(
            || route.host.clone(),
            |port| format!("{}:{port}", route.host),
        );
        let rewritten = if route.ip_literal {
            parsed.set_ip_host(route.address.ip()).is_ok()
                && parsed.set_port(Some(route.address.port())).is_ok()
        } else {
            parsed.set_port(None).is_ok()
        };
        if !rewritten {
            return Err(LibsqlDatabaseError::InvalidUrl(format!(
                "{url} can't be sent to {}",
                route.address
            )));
        }
        Ok((parsed.to_string(), Some(host)))
    }

    /// The server's base URL.
    #[must_use]
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Send `requests` on the stream at `position` (a new stream when it
    /// has no baton) and return one result per request, updating
    /// `position` to continue the stream.
    ///
    /// # Errors
    ///
    /// * [`LibsqlDatabaseError::Http`] if the server can't be reached
    /// * [`LibsqlDatabaseError::Unauthorized`] if the server refuses the auth token
    /// * [`LibsqlDatabaseError::Forbidden`] if the token may not do what was asked
    /// * [`LibsqlDatabaseError::StreamExpired`] if the stream was closed for inactivity
    /// * [`LibsqlDatabaseError::Server`] for other HTTP-level errors
    /// * [`LibsqlDatabaseError::Protocol`] if the response is malformed
    pub async fn pipeline(
        &self,
        position: &mut StreamPosition,
        requests: &[Request],
    ) -> Result<Vec<StreamResult>, LibsqlDatabaseError> {
        let base = position.base_url.as_deref().unwrap_or(&self.base_url);
        let url = format!("{}/v2/pipeline", base.trim_end_matches('/'));
        let (url, host) = self.routed(&url)?;
        let body = serde_json::to_vec(&PipelineRequest {
            baton: position.baton.as_deref(),
            requests,
        })
        .map_err(|error| LibsqlDatabaseError::Protocol(error.to_string()))?;
        let mut request = self
            .http
            .post(url)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body);
        if let Some(host) = host {
            request = request.header(reqwest::header::HOST, host);
        }
        if let Some(token) = &self.auth_token {
            request = request.bearer_auth(token);
        }
        let response = request
            .send()
            .await
            .map_err(|error| self.http_failure(error))?;
        let status = response.status();
        let bytes = response
            .bytes()
            .await
            .map_err(|error| self.http_failure(error))?;
        if !status.is_success() {
            return Err(http_error(status, &bytes));
        }
        let response: PipelineResponse = serde_json::from_slice(&bytes)
            .map_err(|error| LibsqlDatabaseError::Protocol(error.to_string()))?;
        if response.results.len() != requests.len() {
            return Err(LibsqlDatabaseError::Protocol(format!(
                "expected {} results, got {}",
                requests.len(),
                response.results.len()
            )));
        }
        position.baton = response.baton;
        position.base_url = response.base_url;
        Ok(response.results)
    }
}

impl HranaClient {
    /// The error a failed HTTP exchange stands for: a request that ran past
    /// the request timeout is [`LibsqlDatabaseError::Timeout`].
    fn http_failure(&self, error: reqwest::Error) -> LibsqlDatabaseError {
        match self.request_timeout {
            Some(timeout) if error.is_timeout() && !error.is_connect() => {
                LibsqlDatabaseError::Timeout(timeout)
            }
            _ => LibsqlDatabaseError::Http(error),
        }
    }
}

/// The [`Route`] for `base_url`'s host and port to `address`.
fn route(base_url: &str, address: std::net::SocketAddr) -> Result<Route, LibsqlDatabaseError> {
    let parsed = reqwest::Url::parse(base_url)
        .map_err(|error| LibsqlDatabaseError::InvalidUrl(format!("{base_url}: {error}")))?;
    let (Some(host), Some(port)) = (parsed.host_str(), parsed.port_or_known_default()) else {
        return Err(LibsqlDatabaseError::InvalidUrl(format!(
            "{base_url} has no host"
        )));
    };
    let ip_literal = parsed.domain().is_none();
    if ip_literal && parsed.scheme() == "https" {
        return Err(LibsqlDatabaseError::InvalidUrl(format!(
            "{base_url} names its server by IP address, so its certificate can't be checked \
             through another address; name the server by its host name"
        )));
    }
    Ok(Route {
        host: host.to_string(),
        port,
        ip_literal,
        address,
    })
}

/// An HTTP-level error body: `{"message", "code"}` (Hrana) or
/// `{"error"}` (`sqld`'s authentication errors).
#[derive(Debug, Deserialize)]
struct HttpErrorBody {
    #[serde(default)]
    message: Option<String>,
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    code: Option<String>,
}

/// The error an unsuccessful HTTP response stands for.
fn http_error(status: reqwest::StatusCode, body: &[u8]) -> LibsqlDatabaseError {
    let parsed: Option<HttpErrorBody> = serde_json::from_slice(body).ok();
    let (message, code) = match parsed {
        Some(parsed) => (
            parsed.message.or(parsed.error).unwrap_or_default(),
            parsed.code,
        ),
        None => (String::from_utf8_lossy(body).trim().to_string(), None),
    };
    let message = if message.is_empty() {
        status.to_string()
    } else {
        message
    };
    if status == reqwest::StatusCode::UNAUTHORIZED {
        return LibsqlDatabaseError::Unauthorized(message);
    }
    if status == reqwest::StatusCode::FORBIDDEN {
        return LibsqlDatabaseError::Forbidden(message);
    }
    if code.as_deref() == Some("STREAM_EXPIRED") {
        return LibsqlDatabaseError::StreamExpired;
    }
    LibsqlDatabaseError::Server {
        message: format!("{message} (HTTP {})", status.as_u16()),
        code,
        kind: None,
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;

    #[test]
    fn values_round_trip_through_hrana() {
        let values = [
            DatabaseValue::Null,
            DatabaseValue::Int64(i64::MIN),
            DatabaseValue::Real64(1.5),
            DatabaseValue::String("it's".to_string()),
            DatabaseValue::Bytes(vec![0, 1, 255]),
        ];
        for value in values {
            let encoded = Value::from_database_value(&value).unwrap();
            let json = serde_json::to_string(&encoded).unwrap();
            let decoded: Value = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded.into_database_value().unwrap(), value);
        }
    }

    #[test]
    fn integers_travel_as_strings_and_blobs_without_padding_decode() {
        let json =
            serde_json::to_string(&Value::from_database_value(&DatabaseValue::Int64(7)).unwrap())
                .unwrap();
        assert_eq!(json, r#"{"type":"integer","value":"7"}"#);
        let blob: Value = serde_json::from_str(r#"{"type":"blob","base64":"AAE"}"#).unwrap();
        assert_eq!(
            blob.into_database_value().unwrap(),
            DatabaseValue::Bytes(vec![0, 1])
        );
    }

    #[test]
    fn values_without_an_encoding_are_refused() {
        assert!(Value::from_database_value(&DatabaseValue::Real64(f64::NAN)).is_err());
        assert!(Value::from_database_value(&DatabaseValue::UInt64(u64::MAX)).is_err());
        assert!(Value::from_database_value(&DatabaseValue::Now).is_err());
        assert_eq!(
            Value::from_database_value(&DatabaseValue::Int64Opt(None)).unwrap(),
            Value::Null
        );
    }

    #[test]
    fn pipeline_requests_serialize_as_hrana_json() {
        let requests = [
            Request::Describe {
                sql: "SELECT 1".to_string(),
            },
            Request::Batch {
                batch: Batch {
                    steps: vec![
                        BatchStep {
                            condition: None,
                            stmt: Stmt::new("BEGIN"),
                        },
                        BatchStep {
                            condition: Some(BatchCond::Ok { step: 0 }),
                            stmt: Stmt::with_params("SELECT ?", &[DatabaseValue::Int64(1)], true)
                                .unwrap(),
                        },
                    ],
                },
            },
            Request::Close,
        ];
        let json = serde_json::to_string(&PipelineRequest {
            baton: None,
            requests: &requests,
        })
        .unwrap();
        assert_eq!(
            json,
            concat!(
                r#"{"baton":null,"requests":[{"type":"describe","sql":"SELECT 1"},"#,
                r#"{"type":"batch","batch":{"steps":[{"stmt":{"sql":"BEGIN","want_rows":true}},"#,
                r#"{"condition":{"type":"ok","step":0},"stmt":{"sql":"SELECT ?","#,
                r#""args":[{"type":"integer","value":"1"}],"want_rows":true}}]}},"#,
                r#"{"type":"close"}]}"#
            )
        );
    }

    #[test]
    fn http_errors_are_classified() {
        assert!(matches!(
            http_error(reqwest::StatusCode::UNAUTHORIZED, b"Unauthorized: missing token"),
            LibsqlDatabaseError::Unauthorized(message) if message.contains("missing token")
        ));
        assert!(matches!(
            http_error(
                reqwest::StatusCode::UNAUTHORIZED,
                br#"{"error":"Unauthorized: `The JWT is invalid`"}"#
            ),
            LibsqlDatabaseError::Unauthorized(message) if message.contains("JWT is invalid")
        ));
        assert!(matches!(
            http_error(
                reqwest::StatusCode::FORBIDDEN,
                br#"{"error":"Authorization forbidden: no Write permission"}"#
            ),
            LibsqlDatabaseError::Forbidden(message) if message.contains("Write permission")
        ));
        assert!(matches!(
            http_error(
                reqwest::StatusCode::BAD_REQUEST,
                br#"{"message":"The stream has expired due to inactivity","code":"STREAM_EXPIRED"}"#
            ),
            LibsqlDatabaseError::StreamExpired
        ));
        assert!(matches!(
            http_error(reqwest::StatusCode::BAD_REQUEST, b"Received an invalid baton"),
            LibsqlDatabaseError::Server { message, .. } if message.contains("invalid baton")
        ));
    }
}
