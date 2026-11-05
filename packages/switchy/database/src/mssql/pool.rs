//! Opening SQL Server connections and keeping a small pool of them.
//!
//! A connection taken from the [`Pool`] is a [`Lease`]. It goes back to the
//! pool only when [`Lease::release`] is called, which the backend does once a
//! statement finished cleanly; a lease dropped any other way (an error, a
//! timeout, a cancelled future) closes its connection, so a connection in an
//! unknown state is never reused. Closing a connection makes the server roll
//! back any transaction it had open.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tiberius::SqlBrowser as _;
use tokio::net::TcpStream;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::compat::{Compat, TokioAsyncWriteCompatExt as _};

use super::MssqlDatabaseError;

/// A connection to a SQL Server.
pub(super) type Client = tiberius::Client<Compat<TcpStream>>;

/// Session options every connection starts with: the ANSI settings SQL
/// Server tools use (indexes on computed columns and indexed views require
/// them), and unlimited `text`/`ntext` reads.
const SESSION_SETUP: &str = "SET QUOTED_IDENTIFIER ON; SET ANSI_NULLS ON; SET ANSI_PADDING ON; \
     SET ANSI_WARNINGS ON; SET ANSI_NULL_DFLT_ON ON; SET CONCAT_NULL_YIELDS_NULL ON; \
     SET ARITHABORT ON; SET NUMERIC_ROUNDABORT OFF; SET TEXTSIZE 2147483647";

/// Idle connections unused for longer than this are checked with a
/// `SELECT 1` before they are handed out again.
const IDLE_CHECK_AFTER: Duration = Duration::from_secs(60);

/// Opens connections with one configuration.
pub(super) struct Connector {
    pub(super) config: tiberius::Config,
    pub(super) connect_timeout: Duration,
    pub(super) read_only: bool,
}

impl std::fmt::Debug for Connector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Connector")
            .field("addr", &self.config.get_addr())
            .field("connect_timeout", &self.connect_timeout)
            .field("read_only", &self.read_only)
            .finish_non_exhaustive()
    }
}

impl Connector {
    /// Open and set up a connection, following one routing redirect (Azure
    /// SQL's gateway sends clients on to the database's node).
    pub(super) async fn connect(&self) -> Result<Client, MssqlDatabaseError> {
        let mut config = self.config.clone();
        let mut redirected = false;
        loop {
            let tcp = tokio::time::timeout(self.connect_timeout, TcpStream::connect_named(&config))
                .await
                .map_err(|_| MssqlDatabaseError::Timeout("connecting".to_string()))?
                .map_err(MssqlDatabaseError::from)?;
            let connected = tokio::time::timeout(
                self.connect_timeout,
                Box::pin(tiberius::Client::connect(
                    config.clone(),
                    tcp.compat_write(),
                )),
            )
            .await
            .map_err(|_| MssqlDatabaseError::Timeout("logging in".to_string()))?;
            match connected {
                Ok(mut client) => {
                    let setup = if self.read_only {
                        format!("{SESSION_SETUP}; SET XACT_ABORT ON")
                    } else {
                        SESSION_SETUP.to_string()
                    };
                    client.simple_query(setup).await?.into_results().await?;
                    return Ok(client);
                }
                Err(tiberius::error::Error::Routing { host, port }) if !redirected => {
                    log::debug!("mssql: the server routed the connection to {host}:{port}");
                    config.host(host);
                    config.port(port);
                    redirected = true;
                }
                Err(error) => return Err(error.into()),
            }
        }
    }
}

/// A pool of up to `size` connections.
pub(super) struct Pool {
    connector: Connector,
    idle: std::sync::Mutex<Vec<(Client, Instant)>>,
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

impl std::fmt::Debug for Lease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Lease")
            .field("keep", &self.keep)
            .finish_non_exhaustive()
    }
}

/// A connection taken from a [`Pool`] (see the [module documentation](self)).
pub(super) struct Lease {
    client: Option<Client>,
    pool: Arc<Pool>,
    keep: bool,
    _permit: OwnedSemaphorePermit,
}

impl Lease {
    /// The connection.
    pub(super) fn client(&mut self) -> &mut Client {
        self.client
            .as_mut()
            .unwrap_or_else(|| unreachable!("a lease holds its connection until it is dropped"))
    }

    /// Give the connection back to the pool for reuse.
    pub(super) fn release(mut self) {
        self.keep = true;
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        let Some(client) = self.client.take() else {
            return;
        };
        if self.keep && !self.pool.permits.is_closed() {
            if let Ok(mut idle) = self.pool.idle.lock() {
                idle.push((client, Instant::now()));
            }
        } else {
            log::trace!("mssql: closing a connection that wasn't released");
        }
    }
}

impl Pool {
    /// A pool of up to `size` connections opened by `connector`.
    pub(super) fn new(connector: Connector, size: usize) -> Arc<Self> {
        Arc::new(Self {
            connector,
            idle: std::sync::Mutex::new(Vec::new()),
            permits: Arc::new(Semaphore::new(size.max(1))),
        })
    }

    /// Whether connections refuse writes (see the read-only check).
    pub(super) const fn read_only(&self) -> bool {
        self.connector.read_only
    }

    /// Take a connection: an idle one (checked first if it sat unused for a
    /// while), or a new one.
    pub(super) async fn acquire(self: &Arc<Self>) -> Result<Lease, MssqlDatabaseError> {
        let permit = Arc::clone(&self.permits)
            .acquire_owned()
            .await
            .map_err(|_| MssqlDatabaseError::Closed)?;
        loop {
            let idle = self.idle.lock().ok().and_then(|mut idle| idle.pop());
            let Some((mut client, since)) = idle else {
                break;
            };
            if since.elapsed() < IDLE_CHECK_AFTER {
                return Ok(self.lease(client, permit));
            }
            match ping(&mut client).await {
                Ok(()) => return Ok(self.lease(client, permit)),
                Err(error) => log::debug!("mssql: dropping a stale idle connection: {error}"),
            }
        }
        let client = Box::pin(self.connector.connect()).await?;
        Ok(self.lease(client, permit))
    }

    fn lease(self: &Arc<Self>, client: Client, permit: OwnedSemaphorePermit) -> Lease {
        Lease {
            client: Some(client),
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

/// Check that a connection still answers.
async fn ping(client: &mut Client) -> Result<(), MssqlDatabaseError> {
    client
        .simple_query("SELECT 1")
        .await?
        .into_results()
        .await?;
    Ok(())
}
