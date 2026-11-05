//! Running statements on a connection (on a blocking thread), and the
//! read-only and autocommit wrapping.

use oracle::Connection;
use oracle::sql_type::ToSql;

use crate::{DatabaseValue, QueryResult, ResultColumn, Row};

use super::OracleDatabaseError;
use super::lexer;
use super::pool::ServerInfo;
use super::value::{Bind, decode, now_sql, type_name};

/// A statement's decoded result.
#[derive(Debug, Default)]
pub(super) struct Outcome {
    pub(super) columns: Vec<ResultColumn>,
    pub(super) rows: Vec<Vec<DatabaseValue>>,
    /// Rows a DML statement changed (or the rows a query returned).
    // Only the raw-SQL API reports affected rows.
    #[cfg_attr(not(feature = "raw-sql"), allow(dead_code))]
    pub(super) affected: u64,
    /// The `ROWID`s a `RETURNING ROWID INTO` statement returned.
    pub(super) rowids: Vec<String>,
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

/// The output variable a `RETURNING ROWID INTO` statement names.
pub(super) const ROWID_OUTPUT: &str = "switchy_rowid";

/// A statement to run: its text, parameters, and whether it ends with
/// `RETURNING ROWID INTO` an output placeholder the backend binds itself.
#[derive(Debug, Clone)]
pub(super) struct Request {
    pub(super) sql: String,
    pub(super) params: Vec<DatabaseValue>,
    pub(super) returning_rowids: bool,
}

impl Request {
    pub(super) fn new(sql: &str, params: &[DatabaseValue]) -> Self {
        Self {
            sql: sql.to_string(),
            params: params.to_vec(),
            returning_rowids: false,
        }
    }
}

/// Run `request` on `conn` (blocking).
///
/// # Errors
///
/// * [`OracleDatabaseError::InvalidStatement`] if the placeholders don't
///   match the parameters
/// * The driver's or the server's error
pub(super) fn execute(
    conn: &Connection,
    server: ServerInfo,
    request: &Request,
) -> Result<Outcome, OracleDatabaseError> {
    let params = &request.params;
    let output = request.returning_rowids.then_some(ROWID_OUTPUT);
    let bound = lexer::bind_placeholders(&request.sql, params.len(), output, |index| {
        now_sql(&params[index])
    })
    .map_err(OracleDatabaseError::InvalidStatement)?;
    let text = lexer::strip_terminator(&bound.sql);
    let binds: Vec<Bind> = bound
        .params
        .iter()
        .map(|&index| Bind::new(&params[index], server.boolean))
        .collect();
    let values: Vec<Box<dyn ToSql + '_>> = binds.iter().map(Bind::to_sql).collect();
    let mut refs: Vec<&dyn ToSql> = values.iter().map(AsRef::as_ref).collect();
    log::trace!("oracle: {text} ({} parameters)", refs.len());
    let mut statement = conn.statement(text).build()?;
    if statement.is_query() {
        let result = statement.query(&refs).map_err(unsupported_type)?;
        let columns = result
            .column_info()
            .iter()
            .map(|info| ResultColumn {
                name: info.name().to_string(),
                type_name: Some(type_name(info.oracle_type())),
            })
            .collect();
        let mut rows = Vec::new();
        for row in result {
            rows.push(row?.sql_values().iter().map(decode).collect());
        }
        let affected = u64::try_from(rows.len()).unwrap_or(u64::MAX);
        return Ok(Outcome {
            columns,
            rows,
            affected,
            rowids: Vec::new(),
        });
    }
    let rowid_type = oracle::sql_type::OracleType::Varchar2(4000);
    if request.returning_rowids {
        refs.push(&rowid_type);
    }
    statement.execute(&refs)?;
    let rowids = if request.returning_rowids {
        statement
            .returned_values::<_, Option<String>>(refs.len())?
            .into_iter()
            .flatten()
            .collect()
    } else {
        Vec::new()
    };
    Ok(Outcome {
        affected: if statement.is_dml() {
            statement.row_count()?
        } else {
            0
        },
        rowids,
        ..Outcome::default()
    })
}

/// The driver's refusal of a column type it can't read, with what to do
/// instead (Oracle 23ai's native `JSON` type).
fn unsupported_type(error: oracle::Error) -> OracleDatabaseError {
    let text = error.to_string();
    if text.contains("unsupported Oracle type JSON") {
        return OracleDatabaseError::Unsupported(
            "a column is Oracle's native JSON type, which the driver can't read; select \
             JSON_SERIALIZE(column RETURNING CLOB) instead"
                .to_string(),
        );
    }
    error.into()
}

/// Whether `conn` has a transaction open (known to the client without a
/// round trip).
pub(super) fn in_transaction(conn: &Connection) -> bool {
    // If the attribute can't be read, assume a transaction is open, so it
    // is ended explicitly.
    conn.oci_attr::<oracle::oci_attr::TransactionInProgress>()
        .unwrap_or(true)
}

/// Run `request` on a writable connection outside a transaction: commit
/// what it did, or roll it back if it failed.
///
/// # Errors
///
/// * The statement's error (after the rollback)
pub(super) fn autocommit(
    conn: &Connection,
    server: ServerInfo,
    request: &Request,
) -> Result<Outcome, OracleDatabaseError> {
    match execute(conn, server, request) {
        Ok(outcome) => {
            if in_transaction(conn) {
                conn.commit()?;
            }
            Ok(outcome)
        }
        Err(error) => {
            if in_transaction(conn) {
                conn.rollback()?;
            }
            Err(error)
        }
    }
}

/// Start a read-only statement: re-assert the session's read-only mode
/// where the server has one (something the last statement ran may have
/// switched it off), which refuses DML, DDL, locks and writes in
/// autonomous transactions; before 23ai, `SET TRANSACTION READ ONLY`, which
/// refuses DML and locks in the statement's transaction. (With the session
/// mode, a read-only transaction would add nothing but `ORA-01466` for
/// tables created moments before.)
///
/// # Errors
///
/// * The server refuses either (a transaction is already open, say); the
///   caller closes the connection
pub(super) fn begin_read_only(
    conn: &Connection,
    server: ServerInfo,
) -> Result<(), OracleDatabaseError> {
    if server.session_read_only {
        conn.execute("ALTER SESSION SET READ_ONLY = TRUE", &[])?;
    } else {
        conn.execute("SET TRANSACTION READ ONLY", &[])?;
    }
    Ok(())
}

/// Run a statement that passed the read-only check on a read-only
/// connection: in a read-only session (23ai) or a read-only transaction
/// (before), rolled back afterwards.
///
/// # Errors
///
/// * The statement's error, or the setup's; the caller closes the
///   connection after any error
pub(super) fn read_only(
    conn: &Connection,
    server: ServerInfo,
    request: &Request,
) -> Result<Outcome, OracleDatabaseError> {
    begin_read_only(conn, server)?;
    let result = execute(conn, server, request);
    let rolled_back = conn.rollback();
    let outcome = result?;
    rolled_back?;
    Ok(outcome)
}
