//! Running statements on a connection, and the read-only wrapping.

use futures::TryStreamExt as _;
use tiberius::{QueryItem, ToSql};

use crate::{DatabaseValue, QueryResult, ResultColumn, Row};

use super::MssqlDatabaseError;
use super::pool::{Client, Lease};
use super::tsql;
use super::value::{Param, decode, now_sql, type_name};

/// What a statement is run for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Mode {
    /// Its first result set (columns and rows).
    Rows,
    /// The number of rows it changed.
    Count,
    /// Nothing: a raw batch (`exec_raw`), run as SQL text so that session
    /// settings it makes (`SET IDENTITY_INSERT`) last.
    Batch,
}

/// A statement's decoded result.
#[derive(Debug, Default)]
pub(super) struct Outcome {
    pub(super) columns: Vec<ResultColumn>,
    pub(super) rows: Vec<Vec<DatabaseValue>>,
    /// Rows changed (`Mode::Count` only).
    // Only the raw-SQL API reports affected rows.
    #[cfg_attr(not(feature = "raw-sql"), allow(dead_code))]
    pub(super) affected: u64,
}

impl Outcome {
    pub(super) fn into_result(self) -> QueryResult {
        QueryResult {
            columns: self.columns,
            rows: self.rows,
        }
    }

    pub(super) fn into_rows(self) -> Vec<Row> {
        self.into_result().into_rows()
    }
}

/// Run a statement the backend wrote itself (transaction control, session
/// setup), reading every result to the end.
pub(super) async fn control(client: &mut Client, sql: &str) -> Result<(), MssqlDatabaseError> {
    log::trace!("mssql: {sql}");
    client.simple_query(sql).await?.into_results().await?;
    Ok(())
}

/// `@@TRANCOUNT`: how many transactions the connection has open.
pub(super) async fn trancount(client: &mut Client) -> Result<i64, MssqlDatabaseError> {
    let row = client
        .simple_query("SELECT CAST(@@TRANCOUNT AS bigint) AS n")
        .await?
        .into_row()
        .await?;
    count_of(row)
}

/// The `bigint` count in the first column of `row`. A session that only
/// compiles statements (`SET SHOWPLAN_XML ON`) answers with a plan instead,
/// which is an error here, never a panic.
fn count_of(row: Option<tiberius::Row>) -> Result<i64, MssqlDatabaseError> {
    let row =
        row.ok_or_else(|| MssqlDatabaseError::Protocol("@@TRANCOUNT returned no row".to_string()))?;
    row.try_get::<i64, _>(0)
        .map_err(|error| {
            MssqlDatabaseError::Protocol(format!("@@TRANCOUNT returned something else: {error}"))
        })?
        .ok_or_else(|| MssqlDatabaseError::Protocol("@@TRANCOUNT returned NULL".to_string()))
}

/// Run `sql` with `params` (`@P1`, `@P2`, ...) on `client`.
///
/// `rpc` forces the statement through `sp_executesql` even without
/// parameters (so it runs in a scope of its own). Without parameters and
/// without `rpc`, rows and batches are sent as SQL text.
pub(super) async fn statement(
    client: &mut Client,
    sql: &str,
    params: &[DatabaseValue],
    mode: Mode,
    rpc: bool,
) -> Result<Outcome, MssqlDatabaseError> {
    let (sql, kept) = tsql::inline_placeholders(sql, params.len(), |index| now_sql(&params[index]))
        .map_err(MssqlDatabaseError::InvalidStatement)?;
    let bound: Vec<Param<'_>> = kept.iter().map(|&index| Param(&params[index])).collect();
    let refs: Vec<&dyn ToSql> = bound.iter().map(|param| param as &dyn ToSql).collect();
    log::trace!("mssql: {sql} ({} parameters)", refs.len());
    let text = refs.is_empty() && !rpc;
    match mode {
        Mode::Count => {
            let result = client.execute(sql, &refs).await?;
            Ok(Outcome {
                affected: result.total(),
                ..Outcome::default()
            })
        }
        Mode::Batch if text => {
            client.simple_query(sql).await?.into_results().await?;
            Ok(Outcome::default())
        }
        Mode::Batch => {
            client.execute(sql, &refs).await?;
            Ok(Outcome::default())
        }
        Mode::Rows if text => first_result(client.simple_query(sql).await?).await,
        Mode::Rows => first_result(client.query(sql, &refs).await?).await,
    }
}

/// The first result set of a stream (columns even without rows), reading
/// the rest of the stream so that errors from later statements surface.
async fn first_result(
    mut stream: tiberius::QueryStream<'_>,
) -> Result<Outcome, MssqlDatabaseError> {
    let mut outcome = Outcome::default();
    let mut types = Vec::new();
    let mut sets = 0_usize;
    while let Some(item) = stream.try_next().await? {
        match item {
            QueryItem::Metadata(metadata) => {
                sets += 1;
                if sets == 1 {
                    for column in metadata.columns() {
                        outcome.columns.push(ResultColumn {
                            name: column.name().to_string(),
                            type_name: type_name(column.column_type()).map(str::to_string),
                        });
                        types.push(column.column_type());
                    }
                }
            }
            QueryItem::Row(row) if sets <= 1 => {
                outcome.rows.push(
                    row.cells()
                        .zip(&types)
                        .map(|((_, data), column_type)| decode(data, *column_type))
                        .collect(),
                );
            }
            QueryItem::Row(_) => {}
        }
    }
    Ok(outcome)
}

/// Run a statement that passed the read-only check inside a transaction
/// that is always rolled back, on a connection leased for it alone.
///
/// The statement runs through `sp_executesql`; afterwards `@@TRANCOUNT`
/// must still be 1 (the statement neither committed nor rolled back the
/// transaction), and the transaction is rolled back. The connection goes
/// back to the pool only when all of that succeeded: after an error or an
/// unexpected `@@TRANCOUNT` it is closed, which also makes the server roll
/// back anything still open.
///
/// The `@@TRANCOUNT` check detects an escape; it can't undo a commit the
/// statement already made. Keeping such statements out is the job of
/// [`tsql::check_read_only`], which callers run first.
///
/// # Errors
///
/// * The statement fails
/// * [`MssqlDatabaseError::ReadOnlyViolation`] if the statement ended the transaction
pub(super) async fn read_only(
    mut lease: Lease,
    sql: &str,
    params: &[DatabaseValue],
    mode: Mode,
) -> Result<Outcome, MssqlDatabaseError> {
    let client = lease.client();
    control(client, "BEGIN TRANSACTION").await?;
    let mode = if mode == Mode::Batch {
        Mode::Count
    } else {
        mode
    };
    let outcome = match statement(client, sql, params, mode, true).await {
        Ok(outcome) => outcome,
        Err(error) => {
            // Best effort: the connection is closed anyway.
            let _ = control(client, "IF @@TRANCOUNT > 0 ROLLBACK TRANSACTION").await;
            return Err(error);
        }
    };
    let open = rollback_reporting(client).await?;
    if open != 1 {
        return Err(MssqlDatabaseError::ReadOnlyViolation(format!(
            "the statement ended the read-only transaction (@@TRANCOUNT was {open}, not 1), so \
             its connection was closed"
        )));
    }
    lease.release();
    Ok(outcome)
}

/// Roll back every open transaction, returning how many were open.
pub(super) async fn rollback_reporting(client: &mut Client) -> Result<i64, MssqlDatabaseError> {
    let row = client
        .simple_query(
            "SELECT CAST(@@TRANCOUNT AS bigint) AS n; IF @@TRANCOUNT > 0 ROLLBACK TRANSACTION",
        )
        .await?
        .into_row()
        .await?;
    count_of(row)
}
