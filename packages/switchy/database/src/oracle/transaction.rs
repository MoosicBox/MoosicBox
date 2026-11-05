//! Transactions on an Oracle database: one connection held until `COMMIT`
//! or `ROLLBACK`.
//!
//! Oracle opens a transaction implicitly with the first change, so
//! beginning one only leases a connection. A failed statement rolls back
//! only itself; the transaction stays open. DDL commits the transaction
//! (Oracle commits before and after every DDL statement).
//!
//! On a read-only database the transaction holds one connection, but every
//! statement runs exactly as outside one: it must pass the read-only check,
//! and runs in a read-only transaction of its own that is rolled back, after
//! the session's read-only mode (23ai) is re-asserted. Oracle can't
//! re-assert that mode while a transaction is open, so one long read-only
//! transaction would let code a statement ran (an autonomous function that
//! switches the mode off) leave the next statement unprotected. Reads
//! therefore don't share one snapshot; committing and rolling back end
//! nothing, savepoints keep nothing, and any failure closes the
//! connection.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;

use crate::DatabaseError;

use super::pool::{self, Lease, Pool};
use super::run::{self, Outcome, Request};
use super::{Executor, OracleDatabaseError, lexer};

/// The connection of a transaction, and what is known about it.
#[derive(Debug, Default)]
struct State {
    /// The connection; `None` once it was closed or given back.
    lease: Option<Lease>,
    /// Why the transaction is over, if it is.
    lost: Option<String>,
    /// A statement started and hasn't finished (its future was dropped if
    /// this is still set when the next one starts).
    in_flight: bool,
}

impl State {
    /// Check that the transaction is still usable: open, and not left
    /// behind by a cancelled statement.
    fn check(&mut self) -> Result<(), OracleDatabaseError> {
        if self.in_flight {
            self.lose(
                "a statement was cancelled part-way, so its connection was closed and the server \
                 rolled the transaction back",
            );
        }
        if let Some(reason) = &self.lost {
            return Err(OracleDatabaseError::TransactionLost(reason.clone()));
        }
        if self.lease.is_none() {
            return Err(OracleDatabaseError::TransactionLost(
                "it already ended".to_string(),
            ));
        }
        Ok(())
    }

    /// The connection (after [`Self::check`]).
    fn session(&self) -> Result<pool::Session, OracleDatabaseError> {
        self.lease
            .as_ref()
            .map(|lease| lease.session().clone())
            .ok_or_else(|| OracleDatabaseError::TransactionLost("it already ended".to_string()))
    }

    /// Take the connection to end the transaction, with the reason it is
    /// lost, if it is.
    fn take(&mut self) -> (Option<Lease>, Option<OracleDatabaseError>) {
        let lost = self.check().err();
        (self.lease.take(), lost)
    }

    /// End the transaction for `reason`, closing its connection (the server
    /// rolls back whatever was open on it).
    fn lose(&mut self, reason: &str) {
        self.lease = None;
        self.in_flight = false;
        if self.lost.is_none() {
            self.lost = Some(reason.to_string());
        }
    }
}

#[derive(Debug)]
struct Shared {
    state: tokio::sync::Mutex<State>,
    read_only: bool,
}

impl Shared {
    /// Run `call` on the transaction's connection, marking the transaction
    /// lost if it fails in a way that leaves the connection in doubt.
    async fn call<T, F>(&self, call: F) -> Result<T, OracleDatabaseError>
    where
        T: Send + 'static,
        F: FnOnce(&oracle::Connection, pool::ServerInfo) -> Result<T, OracleDatabaseError>
            + Send
            + 'static,
    {
        let mut state = self.state.lock().await;
        state.check()?;
        let session = state.session()?;
        state.in_flight = true;
        let server = session.server;
        let result = pool::blocking(&session.conn, move |conn| call(conn, server)).await;
        state.in_flight = false;
        if let Err(error) = &result
            && (self.read_only || error.breaks_connection())
        {
            state.lose(&format!("a statement failed: {error}"));
        }
        drop(state);
        result
    }
}

/// A transaction on an [`OracleDatabase`](super::OracleDatabase).
#[derive(Debug)]
pub struct OracleTransaction {
    shared: Arc<Shared>,
    finished: AtomicBool,
}

impl OracleTransaction {
    /// Lease a connection for a transaction.
    pub(super) async fn begin(pool: &Arc<Pool>) -> Result<Self, OracleDatabaseError> {
        let lease = Box::pin(pool.acquire()).await?;
        let read_only = pool.read_only();
        let session = lease.session().clone();
        // A pooled connection has no transaction open; make sure.
        pool::blocking(&session.conn, move |conn| {
            if run::in_transaction(conn) {
                conn.rollback()?;
            }
            Ok(())
        })
        .await?;
        Ok(Self {
            shared: Arc::new(Shared {
                state: tokio::sync::Mutex::new(State {
                    lease: Some(lease),
                    ..State::default()
                }),
                read_only,
            }),
            finished: AtomicBool::new(false),
        })
    }

    fn check_open(&self) -> Result<(), DatabaseError> {
        if self.finished.load(Ordering::SeqCst) {
            return Err(DatabaseError::TransactionCommitted);
        }
        Ok(())
    }

    /// End the transaction: commit (`keep`) or roll back.
    async fn end(&self, keep: bool) -> Result<(), DatabaseError> {
        self.check_open()?;
        self.finished.store(true, Ordering::SeqCst);
        let (lease, lost) = self.shared.state.lock().await.take();
        let Some(lease) = lease else {
            // Nothing to roll back; a lost transaction can't be committed.
            return match (keep, lost) {
                (true, Some(error)) => Err(error.into()),
                _ => Ok(()),
            };
        };
        // A read-only transaction has nothing to keep: it rolls back.
        let keep = keep && !self.shared.read_only;
        let session = lease.session().clone();
        pool::blocking(&session.conn, move |conn| {
            if keep {
                conn.commit()?;
            } else {
                conn.rollback()?;
            }
            Ok(())
        })
        .await?;
        lease.release();
        Ok(())
    }
}

impl Drop for OracleTransaction {
    fn drop(&mut self) {
        // Dropped without commit or rollback: the connection (if any) is
        // closed with the shared state, and the server rolls back.
        if !self.finished.swap(true, Ordering::SeqCst) {
            log::debug!("oracle: a transaction was dropped without commit or rollback");
            if let Ok(mut state) = self.shared.state.try_lock() {
                state.lose("the transaction was dropped");
            }
        }
    }
}

#[async_trait]
impl Executor for OracleTransaction {
    async fn run(&self, request: Request) -> Result<Outcome, DatabaseError> {
        self.check_open()?;
        if self.shared.read_only {
            lexer::check_read_only(&request.sql).map_err(OracleDatabaseError::ReadOnly)?;
        }
        let read_only = self.shared.read_only;
        Ok(self
            .shared
            .call(move |conn, server| {
                if read_only {
                    run::read_only(conn, server, &request)
                } else {
                    run::execute(conn, server, &request)
                }
            })
            .await?)
    }
}

super::impl_database!(OracleTransaction, {
    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn crate::DatabaseTransaction>, crate::DatabaseError> {
        Err(crate::DatabaseError::AlreadyInTransaction)
    }
});

#[async_trait]
impl crate::DatabaseTransaction for OracleTransaction {
    async fn commit(self: Box<Self>) -> Result<(), DatabaseError> {
        self.end(true).await
    }

    async fn rollback(self: Box<Self>) -> Result<(), DatabaseError> {
        self.end(false).await
    }

    async fn savepoint(&self, name: &str) -> Result<Box<dyn crate::Savepoint>, DatabaseError> {
        crate::validate_savepoint_name(name)?;
        self.check_open()?;
        // Oracle identifiers are at most 128 bytes (30 before 12.2).
        if name.len() > 30 {
            return Err(DatabaseError::InvalidSavepointName(format!(
                "Savepoint name '{name}' is longer than 30 characters"
            )));
        }
        // A read-only transaction keeps nothing a savepoint could mark.
        if !self.shared.read_only {
            let sql = format!("SAVEPOINT {name}");
            self.shared
                .call(move |conn, _| {
                    conn.execute(&sql, &[])?;
                    Ok(())
                })
                .await?;
        }
        Ok(Box::new(OracleSavepoint {
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

/// A savepoint in an [`OracleTransaction`].
#[derive(Debug)]
pub struct OracleSavepoint {
    name: String,
    shared: Arc<Shared>,
}

#[async_trait]
impl crate::Savepoint for OracleSavepoint {
    async fn release(self: Box<Self>) -> Result<(), DatabaseError> {
        // Oracle has no RELEASE SAVEPOINT: the work stays in the
        // transaction, which is all releasing means. The transaction must
        // still be open, though.
        self.shared.state.lock().await.check()?;
        Ok(())
    }

    async fn rollback_to(self: Box<Self>) -> Result<(), DatabaseError> {
        if self.shared.read_only {
            self.shared.state.lock().await.check()?;
            return Ok(());
        }
        let sql = format!("ROLLBACK TO SAVEPOINT {}", self.name);
        Ok(self
            .shared
            .call(move |conn, _| {
                conn.execute(&sql, &[])?;
                Ok(())
            })
            .await?)
    }

    fn name(&self) -> &str {
        &self.name
    }
}
