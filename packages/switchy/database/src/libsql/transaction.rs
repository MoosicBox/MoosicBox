//! Transactions on a remote `libSQL` database: one server stream held from
//! `BEGIN` to `COMMIT`/`ROLLBACK`.
//!
//! The server closes a stream that sits idle for a few seconds (rolling its
//! transaction back), so while a transaction is open a background task sends
//! an empty request on its stream every few seconds. If the stream is closed
//! anyway, the next statement fails with
//! [`LibsqlDatabaseError::StreamExpired`].
//!
//! A request that gets no answer (it ran past the request timeout, or the
//! connection dropped mid-request) loses the stream: the next baton never
//! arrived, and the server either dropped the stream when the request went
//! away or will close it once it sits idle, rolling its transaction back
//! either way. Further statements then fail with
//! [`LibsqlDatabaseError::StreamLost`], committing does too, and rolling back
//! succeeds without sending anything. Rolling back a stream the server
//! closed for inactivity succeeds the same way.
//!
//! On a read-only database the server describes every statement first, in a
//! request of its own, and only a statement it reports read-only is sent to
//! run, inside a savepoint that is rolled back; committing rolls the
//! transaction back.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;

use crate::{DatabaseError, DatabaseValue};

use super::hrana::{HranaClient, Request, Stmt, StreamPosition};
use super::{
    Executor, LibsqlDatabaseError, Outcome, READ_ONLY_SAVEPOINT, describe_request, describe_result,
    execute_result, read_only_batch, read_only_result, statement,
};

/// How often an open transaction pings its stream (servers close idle
/// streams after about ten seconds).
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(4);

#[derive(Debug, Default)]
struct StreamState {
    position: StreamPosition,
    /// Whether the stream (and its transaction) is still open.
    open: bool,
    /// Whether the stream was closed because a request on it got no answer.
    lost: bool,
}

impl StreamState {
    /// Record what `result` (of a request on this stream) means for it.
    fn update<T>(&mut self, result: &Result<T, LibsqlDatabaseError>) {
        match result {
            Err(LibsqlDatabaseError::StreamExpired) => self.open = false,
            // The request may have reached the server, which then moved the
            // stream on to a baton that never arrived.
            Err(LibsqlDatabaseError::Timeout(_)) => self.lose(),
            Err(LibsqlDatabaseError::Http(error)) if !error.is_connect() => self.lose(),
            _ => {
                if self.position.baton.is_none() {
                    self.open = false;
                }
            }
        }
    }

    const fn lose(&mut self) {
        self.open = false;
        self.lost = true;
    }

    /// The error for a request on this stream once it is closed.
    const fn closed_error(&self) -> LibsqlDatabaseError {
        if self.lost {
            LibsqlDatabaseError::StreamLost
        } else {
            LibsqlDatabaseError::StreamExpired
        }
    }
}

#[derive(Debug)]
struct Shared {
    client: Arc<HranaClient>,
    read_only: bool,
    stream: tokio::sync::Mutex<StreamState>,
}

impl Shared {
    /// Send `requests` on the transaction's stream.
    async fn send(
        &self,
        requests: &[Request],
    ) -> Result<Vec<super::hrana::StreamResult>, LibsqlDatabaseError> {
        let mut stream = self.stream.lock().await;
        if !stream.open {
            return Err(stream.closed_error());
        }
        let result = self.client.pipeline(&mut stream.position, requests).await;
        stream.update(&result);
        result
    }

    /// Run a statement this backend generated itself (transaction and
    /// savepoint control), without the read-only wrapping.
    async fn control(&self, sql: &str) -> Result<(), LibsqlDatabaseError> {
        let requests = [Request::Execute {
            stmt: Stmt::new(sql),
        }];
        execute_result(self.send(&requests).await?.into_iter().next()).map(|_| ())
    }

    /// End the transaction with `sql` and close the stream. Rolling back a
    /// stream that is already closed (lost or expired) succeeds: the server
    /// rolls its transaction back without being asked.
    async fn finish(&self, sql: &str) -> Result<(), LibsqlDatabaseError> {
        if sql == "ROLLBACK" {
            let closed = {
                let stream = self.stream.lock().await;
                (!stream.open).then(|| stream.closed_error())
            };
            if let Some(closed) = closed {
                log::debug!(
                    "libsql: rolling back a closed stream; the server rolls it back itself ({closed})"
                );
                return Ok(());
            }
        }
        let requests = [
            Request::Execute {
                stmt: Stmt::new(sql),
            },
            Request::Close,
        ];
        let result = self.send(&requests).await;
        self.stream.lock().await.open = false;
        execute_result(result?.into_iter().next()).map(|_| ())
    }
}

/// A transaction on a [`super::LibsqlDatabase`].
#[derive(Debug)]
pub struct LibsqlTransaction {
    shared: Arc<Shared>,
    keepalive: Option<tokio::task::JoinHandle<()>>,
    finished: AtomicBool,
}

impl LibsqlTransaction {
    /// Open a stream and begin a transaction on it.
    pub(super) async fn begin(
        client: Arc<HranaClient>,
        read_only: bool,
    ) -> Result<Self, LibsqlDatabaseError> {
        let shared = Arc::new(Shared {
            client,
            read_only,
            stream: tokio::sync::Mutex::new(StreamState {
                position: StreamPosition::default(),
                open: true,
                lost: false,
            }),
        });
        shared.control("BEGIN").await?;
        let keepalive = tokio::spawn(keep_alive(Arc::downgrade(&shared)));
        Ok(Self {
            shared,
            keepalive: Some(keepalive),
            finished: AtomicBool::new(false),
        })
    }

    fn check_open(&self) -> Result<(), DatabaseError> {
        if self.finished.load(Ordering::SeqCst) {
            return Err(DatabaseError::TransactionCommitted);
        }
        Ok(())
    }

    async fn end(&self, sql: &str) -> Result<(), DatabaseError> {
        self.check_open()?;
        self.finished.store(true, Ordering::SeqCst);
        if let Some(keepalive) = &self.keepalive {
            keepalive.abort();
        }
        Ok(self.shared.finish(sql).await?)
    }
}

/// Ping the stream until the transaction ends.
async fn keep_alive(shared: std::sync::Weak<Shared>) {
    loop {
        tokio::time::sleep(KEEPALIVE_INTERVAL).await;
        let Some(shared) = shared.upgrade() else {
            return;
        };
        let mut stream = shared.stream.lock().await;
        if !stream.open {
            return;
        }
        let result = shared.client.pipeline(&mut stream.position, &[]).await;
        stream.update(&result);
        let open = stream.open;
        drop(stream);
        if let Err(error) = result {
            log::debug!("libsql: keeping the transaction's stream open failed: {error}");
        }
        if !open {
            return;
        }
    }
}

impl Drop for LibsqlTransaction {
    fn drop(&mut self) {
        if let Some(keepalive) = self.keepalive.take() {
            keepalive.abort();
        }
        if self.finished.swap(true, Ordering::SeqCst) {
            return;
        }
        // Dropped without commit or rollback: roll back now rather than
        // leaving the server to do it when the stream expires.
        if let Ok(runtime) = tokio::runtime::Handle::try_current() {
            let shared = Arc::clone(&self.shared);
            runtime.spawn(async move {
                if let Err(error) = shared.finish("ROLLBACK").await {
                    log::debug!("libsql: rolling back a dropped transaction failed: {error}");
                }
            });
        }
    }
}

#[async_trait]
impl Executor for LibsqlTransaction {
    async fn run(
        &self,
        sql: &str,
        params: &[DatabaseValue],
        want_rows: bool,
    ) -> Result<Outcome, DatabaseError> {
        self.check_open()?;
        let stmt = statement(sql, params, want_rows)?;
        log::trace!("libsql (transaction): {}", stmt.sql);
        let result = if self.shared.read_only {
            let release = format!("RELEASE {READ_ONLY_SAVEPOINT}");
            let rollback = format!("ROLLBACK TO {READ_ONLY_SAVEPOINT}");
            // The statement is sent only once the server described it as
            // read-only, in a request of its own.
            let check = [describe_request(&stmt.sql)];
            describe_result(self.shared.send(&check).await?.into_iter().next())?;
            let requests = [read_only_batch(
                stmt,
                &format!("SAVEPOINT {READ_ONLY_SAVEPOINT}"),
                &[&rollback, &release],
            )];
            read_only_result(self.shared.send(&requests).await?.into_iter().next())?
        } else {
            let requests = [Request::Execute { stmt }];
            execute_result(self.shared.send(&requests).await?.into_iter().next())?
        };
        Ok(Outcome::decode(result)?)
    }
}

super::impl_database!(LibsqlTransaction, {
    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn crate::DatabaseTransaction>, crate::DatabaseError> {
        Err(crate::DatabaseError::AlreadyInTransaction)
    }
});

#[async_trait]
impl crate::DatabaseTransaction for LibsqlTransaction {
    async fn commit(self: Box<Self>) -> Result<(), DatabaseError> {
        // A read-only transaction has nothing to keep.
        let sql = if self.shared.read_only {
            "ROLLBACK"
        } else {
            "COMMIT"
        };
        self.end(sql).await
    }

    async fn rollback(self: Box<Self>) -> Result<(), DatabaseError> {
        self.end("ROLLBACK").await
    }

    async fn savepoint(&self, name: &str) -> Result<Box<dyn crate::Savepoint>, DatabaseError> {
        crate::validate_savepoint_name(name)?;
        self.check_open()?;
        self.shared.control(&format!("SAVEPOINT {name}")).await?;
        Ok(Box::new(LibsqlSavepoint {
            name: name.to_string(),
            shared: Arc::clone(&self.shared),
        }))
    }

    #[cfg(feature = "cascade")]
    async fn find_cascade_targets(
        &self,
        table_name: &str,
    ) -> Result<crate::schema::DropPlan, DatabaseError> {
        Ok(crate::schema::DropPlan::Simple(
            super::schema::drop_order(self, table_name).await?,
        ))
    }

    #[cfg(feature = "cascade")]
    async fn has_any_dependents(&self, table_name: &str) -> Result<bool, DatabaseError> {
        Ok(!super::schema::direct_dependents(self, table_name)
            .await?
            .is_empty())
    }

    #[cfg(feature = "cascade")]
    async fn get_direct_dependents(
        &self,
        table_name: &str,
    ) -> Result<std::collections::BTreeSet<String>, DatabaseError> {
        super::schema::direct_dependents(self, table_name).await
    }
}

/// A savepoint in a [`LibsqlTransaction`].
#[derive(Debug)]
pub struct LibsqlSavepoint {
    name: String,
    shared: Arc<Shared>,
}

#[async_trait]
impl crate::Savepoint for LibsqlSavepoint {
    async fn release(self: Box<Self>) -> Result<(), DatabaseError> {
        Ok(self
            .shared
            .control(&format!("RELEASE {}", self.name))
            .await?)
    }

    async fn rollback_to(self: Box<Self>) -> Result<(), DatabaseError> {
        Ok(self
            .shared
            .control(&format!("ROLLBACK TO {}", self.name))
            .await?)
    }

    fn name(&self) -> &str {
        &self.name
    }
}
