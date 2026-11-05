//! Transactions on a SQL Server database: one connection held from `BEGIN
//! TRANSACTION` to `COMMIT`/`ROLLBACK`.
//!
//! On a read-only database every statement must pass the read-only check
//! (`SET SHOWPLAN_XML ON`/`OFF` is accepted too), `@@TRANCOUNT` must still
//! be 1 after it, and committing rolls the transaction back. A connection
//! left with `SHOWPLAN_XML` on, or in any doubtful state, is closed instead
//! of going back to the pool.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use async_trait::async_trait;

use crate::{DatabaseError, DatabaseValue};

use super::pool::{Lease, Pool};
use super::run::{self, Mode, Outcome};
use super::tsql::{self, ReadOnlyStatement};
use super::{Executor, MssqlDatabaseError, with_timeout};

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
    /// `SET SHOWPLAN_XML ON` is in effect (read-only transactions).
    showplan: bool,
    /// The no-execute options writable statements turned on.
    modes: tsql::SessionModes,
}

impl State {
    /// Check that the transaction is still usable: open, and not left
    /// behind by a cancelled statement.
    fn check(&mut self) -> Result<(), MssqlDatabaseError> {
        if self.in_flight {
            self.lose(
                "a statement was cancelled part-way, so its connection was closed and the server \
                 rolled the transaction back",
            );
        }
        if let Some(reason) = &self.lost {
            return Err(MssqlDatabaseError::TransactionLost(reason.clone()));
        }
        if self.lease.is_none() {
            return Err(MssqlDatabaseError::TransactionLost(
                "it already ended".to_string(),
            ));
        }
        Ok(())
    }

    /// The connection (after [`Self::check`]).
    fn client(&mut self) -> Result<&mut super::pool::Client, MssqlDatabaseError> {
        self.lease
            .as_mut()
            .map(Lease::client)
            .ok_or_else(|| MssqlDatabaseError::TransactionLost("it already ended".to_string()))
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
    statement_timeout: Option<Duration>,
}

impl Shared {
    /// Run a statement the backend generated itself (savepoints), without
    /// the read-only check.
    async fn control(&self, sql: &str) -> Result<(), MssqlDatabaseError> {
        let mut state = self.state.lock().await;
        state.check()?;
        state.in_flight = true;
        let result = run::control(state.client()?, sql).await;
        state.in_flight = false;
        if let Err(error) = &result
            && error.breaks_connection()
        {
            state.lose(&error.to_string());
        }
        result
    }
}

/// A transaction on an [`MssqlDatabase`](super::MssqlDatabase).
#[derive(Debug)]
pub struct MssqlTransaction {
    shared: Arc<Shared>,
    finished: AtomicBool,
}

impl MssqlTransaction {
    /// Lease a connection and begin a transaction on it.
    pub(super) async fn begin(
        pool: &Arc<Pool>,
        statement_timeout: Option<Duration>,
    ) -> Result<Self, MssqlDatabaseError> {
        let mut lease = Box::pin(pool.acquire()).await?;
        run::control(lease.client(), "BEGIN TRANSACTION").await?;
        Ok(Self {
            shared: Arc::new(Shared {
                state: tokio::sync::Mutex::new(State {
                    lease: Some(lease),
                    ..State::default()
                }),
                read_only: pool.read_only(),
                statement_timeout,
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
        let (lease, lost, showplan, normal) = {
            let mut state = self.shared.state.lock().await;
            let lost = state.check().err();
            (
                state.lease.take(),
                lost,
                state.showplan,
                state.modes.is_normal(),
            )
        };
        let Some(mut lease) = lease else {
            // Nothing to roll back; a lost transaction can't be committed.
            return match (keep, lost) {
                (true, Some(error)) => Err(error.into()),
                _ => Ok(()),
            };
        };
        let keep = keep && !self.shared.read_only;
        if !normal {
            // Statements sent while the session only compiled them
            // (`SET NOEXEC ON`, ...) reported success without running, so
            // committing would keep a transaction that silently lacks them.
            // Dropping the connection rolls it back.
            drop(lease);
            return if keep {
                Err(MssqlDatabaseError::TransactionLost(
                    "the session was switched to a mode in which statements only compile (SET \
                     SHOWPLAN_*, NOEXEC, PARSEONLY or FMTONLY), so the transaction was rolled \
                     back instead of committed"
                        .to_string(),
                )
                .into())
            } else {
                Ok(())
            };
        }
        let sql = if keep {
            "COMMIT TRANSACTION"
        } else {
            "IF @@TRANCOUNT > 0 ROLLBACK TRANSACTION"
        };
        run::control(lease.client(), sql).await?;
        if !showplan {
            lease.release();
        }
        Ok(())
    }

    /// Run one statement in the transaction.
    async fn run_statement(
        &self,
        sql: &str,
        params: &[DatabaseValue],
        mode: Mode,
    ) -> Result<Outcome, MssqlDatabaseError> {
        let checked = if self.shared.read_only {
            Some(tsql::check_read_only(sql, true).map_err(MssqlDatabaseError::ReadOnly)?)
        } else {
            None
        };
        let mut state = self.shared.state.lock().await;
        state.check()?;
        state.in_flight = true;
        let showplan = state.showplan;
        let read_only = self.shared.read_only;
        let result = with_timeout(self.shared.statement_timeout, async {
            let client = state.client()?;
            match checked {
                None => run::statement(client, sql, params, mode, false).await,
                Some(ReadOnlyStatement::ShowplanOn) => {
                    run::control(client, "SET SHOWPLAN_XML ON").await?;
                    Ok(Outcome::default())
                }
                Some(ReadOnlyStatement::ShowplanOff) => {
                    run::control(client, "SET SHOWPLAN_XML OFF").await?;
                    Ok(Outcome::default())
                }
                // With SHOWPLAN_XML on the server compiles statements and
                // returns their plans without running them.
                Some(ReadOnlyStatement::Read) if showplan => {
                    run::statement(client, sql, params, Mode::Rows, false).await
                }
                Some(ReadOnlyStatement::Read) => {
                    let mode = if mode == Mode::Batch {
                        Mode::Count
                    } else {
                        mode
                    };
                    let outcome = run::statement(client, sql, params, mode, true).await?;
                    let open = run::trancount(client).await?;
                    if open == 1 {
                        Ok(outcome)
                    } else {
                        Err(MssqlDatabaseError::ReadOnlyViolation(format!(
                            "the statement ended the read-only transaction (@@TRANCOUNT was \
                             {open}, not 1), so its connection was closed"
                        )))
                    }
                }
            }
        })
        .await;
        state.in_flight = false;
        if !read_only {
            state.modes.apply(sql, result.is_ok());
        }
        match &result {
            Ok(_) => match checked {
                Some(ReadOnlyStatement::ShowplanOn) => state.showplan = true,
                Some(ReadOnlyStatement::ShowplanOff) => state.showplan = false,
                _ => {}
            },
            Err(error) if read_only || error.breaks_connection() => {
                state.lose(&format!("a statement failed: {error}"));
            }
            Err(_) => {
                // The server rolls the whole transaction back for some
                // errors (deadlocks, XACT_ABORT); then nothing more may run
                // as if it were still open.
                let open = match state.client() {
                    Ok(client) => run::trancount(client).await.ok(),
                    Err(_) => None,
                };
                if open != Some(1) {
                    state.lose("the server rolled the transaction back after a statement failed");
                }
            }
        }
        result
    }
}

impl Drop for MssqlTransaction {
    fn drop(&mut self) {
        // Dropped without commit or rollback: the connection (if any) is
        // closed with the shared state, and the server rolls back.
        if !self.finished.swap(true, Ordering::SeqCst) {
            log::debug!("mssql: a transaction was dropped without commit or rollback");
            if let Ok(mut state) = self.shared.state.try_lock() {
                state.lose("the transaction was dropped");
            }
        }
    }
}

#[async_trait]
impl Executor for MssqlTransaction {
    async fn run(
        &self,
        sql: &str,
        params: &[DatabaseValue],
        mode: Mode,
    ) -> Result<Outcome, DatabaseError> {
        self.check_open()?;
        Ok(self.run_statement(sql, params, mode).await?)
    }
}

super::impl_database!(MssqlTransaction, {
    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn crate::DatabaseTransaction>, crate::DatabaseError> {
        Err(crate::DatabaseError::AlreadyInTransaction)
    }
});

#[async_trait]
impl crate::DatabaseTransaction for MssqlTransaction {
    async fn commit(self: Box<Self>) -> Result<(), DatabaseError> {
        // A read-only transaction has nothing to keep: it rolls back.
        self.end(true).await
    }

    async fn rollback(self: Box<Self>) -> Result<(), DatabaseError> {
        self.end(false).await
    }

    async fn savepoint(&self, name: &str) -> Result<Box<dyn crate::Savepoint>, DatabaseError> {
        crate::validate_savepoint_name(name)?;
        self.check_open()?;
        // SQL Server savepoint names are at most 32 characters.
        if name.len() > 32 {
            return Err(DatabaseError::InvalidSavepointName(format!(
                "Savepoint name '{name}' is longer than SQL Server's 32 characters"
            )));
        }
        self.shared
            .control(&format!("SAVE TRANSACTION {name}"))
            .await?;
        Ok(Box::new(MssqlSavepoint {
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

/// A savepoint in an [`MssqlTransaction`].
#[derive(Debug)]
pub struct MssqlSavepoint {
    name: String,
    shared: Arc<Shared>,
}

#[async_trait]
impl crate::Savepoint for MssqlSavepoint {
    async fn release(self: Box<Self>) -> Result<(), DatabaseError> {
        // SQL Server has no RELEASE SAVEPOINT: the work stays in the
        // transaction, which is all releasing means. The transaction must
        // still be open, though.
        self.shared.state.lock().await.check()?;
        Ok(())
    }

    async fn rollback_to(self: Box<Self>) -> Result<(), DatabaseError> {
        Ok(self
            .shared
            .control(&format!("ROLLBACK TRANSACTION {}", self.name))
            .await?)
    }

    fn name(&self) -> &str {
        &self.name
    }
}
