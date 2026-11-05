//! Opening Oracle connections and keeping a small pool of them.
//!
//! The driver is synchronous, so every call runs on tokio's blocking
//! threads ([`blocking`]). A connection taken from the [`Pool`] is a
//! [`Lease`]; it goes back to the pool only when [`Lease::release`] is
//! called, which the backend does once a statement finished cleanly. A
//! lease dropped any other way (an error, a timeout, a cancelled future)
//! closes its connection, so a connection in an unknown state is never
//! reused; closing a connection makes the server roll back whatever it had
//! open. A call whose future is dropped while it runs is interrupted
//! (`OCIBreak`), and its connection is closed when the call returns.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use oracle::Connection;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::OracleDatabaseError;

/// Idle connections unused for longer than this are pinged before they are
/// handed out again.
const IDLE_CHECK_AFTER: Duration = Duration::from_secs(60);

/// What a server supports, found when connecting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct ServerInfo {
    /// The server's major version (`19`, `21`, `23`).
    pub major: i32,
    /// SQL has a `BOOLEAN` type (23ai and later).
    pub boolean: bool,
    /// `ALTER SESSION SET READ_ONLY = TRUE` is available (23ai and later)
    /// and was set (read-only connections).
    pub session_read_only: bool,
}

/// An open connection and what its server supports.
#[derive(Debug, Clone)]
pub(super) struct Session {
    pub conn: Arc<Connection>,
    pub server: ServerInfo,
}

/// Opens connections with one configuration.
pub(super) struct Connector {
    pub(super) user: String,
    pub(super) password: String,
    pub(super) connect_string: String,
    pub(super) connect_timeout: Duration,
    pub(super) statement_timeout: Option<Duration>,
    pub(super) read_only: bool,
}

impl std::fmt::Debug for Connector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connector")
            .field("user", &self.user)
            .field("password", &"<redacted>")
            .field("connect_string", &self.connect_string)
            .field("connect_timeout", &self.connect_timeout)
            .field("statement_timeout", &self.statement_timeout)
            .field("read_only", &self.read_only)
            .finish()
    }
}

/// Session settings every connection starts with: ISO formats for text
/// converted to and from dates, timestamps and numbers (Oracle's defaults
/// depend on the client's language and territory, `DD-MON-RR` among them),
/// so values bound as text convert the same way everywhere.
const SESSION_SETUP: &str = "ALTER SESSION SET \
     NLS_DATE_FORMAT = 'YYYY-MM-DD HH24:MI:SS' \
     NLS_TIMESTAMP_FORMAT = 'YYYY-MM-DD HH24:MI:SS.FF' \
     NLS_TIMESTAMP_TZ_FORMAT = 'YYYY-MM-DD HH24:MI:SS.FF TZH:TZM' \
     NLS_NUMERIC_CHARACTERS = '.,'";

/// `ORA-02248: invalid option for ALTER SESSION`: the server has no
/// session-level read-only mode (before 23ai).
const INVALID_SESSION_OPTION: i32 = 2248;

impl Connector {
    /// Open and set up a connection (on a blocking thread).
    pub(super) fn connect_blocking(&self) -> Result<Session, OracleDatabaseError> {
        let mut connector =
            oracle::Connector::new(&self.user, &self.password, &self.connect_string);
        connector.driver_name("switchy_database");
        let conn = connector.connect().map_err(OracleDatabaseError::connect)?;
        let (version, _) = conn.server_version()?;
        let major = version.major();
        let mut server = ServerInfo {
            major,
            boolean: major >= 23,
            session_read_only: false,
        };
        conn.execute(SESSION_SETUP, &[])?;
        if self.read_only {
            match conn.execute("ALTER SESSION SET READ_ONLY = TRUE", &[]) {
                Ok(_) => server.session_read_only = true,
                Err(error) if error.oci_code() == Some(INVALID_SESSION_OPTION) => {
                    log::debug!(
                        "oracle: the server (version {major}) has no read-only session mode; \
                         read-only statements rely on the statement check and read-only \
                         transactions"
                    );
                }
                Err(error) => return Err(error.into()),
            }
        }
        conn.set_call_timeout(self.statement_timeout)?;
        Ok(Session {
            conn: Arc::new(conn),
            server,
        })
    }

    /// Open and set up a connection.
    async fn connect(self: &Arc<Self>) -> Result<Session, OracleDatabaseError> {
        let connector = Arc::clone(self);
        let task = tokio::task::spawn_blocking(move || connector.connect_blocking());
        tokio::time::timeout(self.connect_timeout + Duration::from_secs(1), task)
            .await
            .map_err(|_| OracleDatabaseError::Timeout("connecting".to_string()))?
            .map_err(|error| OracleDatabaseError::Driver(format!("connecting failed: {error}")))?
    }
}

/// A pool of up to `size` connections.
pub(super) struct Pool {
    connector: Arc<Connector>,
    idle: std::sync::Mutex<Vec<(Session, Instant)>>,
    permits: Arc<Semaphore>,
}

impl std::fmt::Debug for Pool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pool")
            .field("connector", &self.connector)
            .field("available", &self.permits.available_permits())
            .finish_non_exhaustive()
    }
}

/// A connection taken from a [`Pool`] (see the [module documentation](self)).
pub(super) struct Lease {
    session: Option<Session>,
    pool: Arc<Pool>,
    keep: bool,
    _permit: OwnedSemaphorePermit,
}

impl std::fmt::Debug for Lease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lease")
            .field("keep", &self.keep)
            .finish_non_exhaustive()
    }
}

impl Lease {
    /// The connection.
    pub(super) fn session(&self) -> &Session {
        self.session
            .as_ref()
            .unwrap_or_else(|| unreachable!("a lease holds its connection until it is dropped"))
    }

    /// Give the connection back to the pool for reuse.
    pub(super) fn release(mut self) {
        self.keep = true;
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        let Some(session) = self.session.take() else {
            return;
        };
        if self.keep && !self.pool.permits.is_closed() {
            if let Ok(mut idle) = self.pool.idle.lock() {
                idle.push((session, Instant::now()));
            }
        } else {
            log::trace!("oracle: closing a connection that wasn't released");
        }
    }
}

impl Pool {
    /// A pool of up to `size` connections opened by `connector`.
    pub(super) fn new(connector: Connector, size: usize) -> Arc<Self> {
        Arc::new(Self {
            connector: Arc::new(connector),
            idle: std::sync::Mutex::new(Vec::new()),
            permits: Arc::new(Semaphore::new(size.max(1))),
        })
    }

    /// Whether connections refuse writes.
    pub(super) fn read_only(&self) -> bool {
        self.connector.read_only
    }

    /// Take a connection: an idle one (pinged first if it sat unused for a
    /// while), or a new one.
    pub(super) async fn acquire(self: &Arc<Self>) -> Result<Lease, OracleDatabaseError> {
        let permit = Arc::clone(&self.permits)
            .acquire_owned()
            .await
            .map_err(|_| OracleDatabaseError::Closed)?;
        loop {
            let idle = self.idle.lock().ok().and_then(|mut idle| idle.pop());
            let Some((session, since)) = idle else {
                break;
            };
            if since.elapsed() < IDLE_CHECK_AFTER {
                return Ok(self.lease(session, permit));
            }
            let conn = Arc::clone(&session.conn);
            match blocking(&session.conn, move |_| Ok(conn.ping()?)).await {
                Ok(()) => return Ok(self.lease(session, permit)),
                Err(error) => log::debug!("oracle: dropping a stale idle connection: {error}"),
            }
        }
        let session = self.connector.connect().await?;
        Ok(self.lease(session, permit))
    }

    fn lease(self: &Arc<Self>, session: Session, permit: OwnedSemaphorePermit) -> Lease {
        Lease {
            session: Some(session),
            pool: Arc::clone(self),
            keep: false,
            _permit: permit,
        }
    }

    /// Close every idle connection and refuse new leases.
    pub(super) fn close(&self) {
        self.permits.close();
        if let Ok(mut idle) = self.idle.lock() {
            idle.clear();
        }
    }
}

/// Interrupts a call (`OCIBreak`) when dropped before [`Self::disarm`].
struct BreakOnDrop {
    conn: Option<Arc<Connection>>,
    done: Arc<AtomicBool>,
}

impl BreakOnDrop {
    fn disarm(mut self) {
        self.conn = None;
    }
}

impl Drop for BreakOnDrop {
    fn drop(&mut self) {
        let Some(conn) = self.conn.take() else {
            return;
        };
        if self.done.load(Ordering::SeqCst) {
            return;
        }
        log::debug!("oracle: interrupting a call whose caller went away");
        // The break is a network round trip: run it off the async thread.
        let interrupt = move || {
            if let Err(error) = conn.break_execution() {
                log::debug!("oracle: interrupting the call failed: {error}");
            }
        };
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            drop(runtime.spawn_blocking(interrupt));
        } else {
            interrupt();
        }
    }
}

/// Run `call` with `conn` on a blocking thread. If the returned future is
/// dropped before the call ends, the call is interrupted.
///
/// # Errors
///
/// * Whatever `call` returns
/// * [`OracleDatabaseError::Driver`] if the blocking task panicked
pub(super) async fn blocking<T, F>(
    conn: &Arc<Connection>,
    call: F,
) -> Result<T, OracleDatabaseError>
where
    T: Send + 'static,
    F: FnOnce(&Connection) -> Result<T, OracleDatabaseError> + Send + 'static,
{
    let done = Arc::new(AtomicBool::new(false));
    let guard = BreakOnDrop {
        conn: Some(Arc::clone(conn)),
        done: Arc::clone(&done),
    };
    let shared = Arc::clone(conn);
    let task = tokio::task::spawn_blocking(move || {
        let result = call(&shared);
        done.store(true, Ordering::SeqCst);
        result
    });
    let result = task
        .await
        .map_err(|error| OracleDatabaseError::Driver(format!("a driver call failed: {error}")));
    guard.disarm();
    result?
}
