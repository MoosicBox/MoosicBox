//! Oracle SQL for the query builder, and the statements that run it on an
//! [`Executor`].
//!
//! Identifiers are quoted with `"`, so they keep their case (Oracle folds
//! unquoted names to upper case); values become `:1`, `:2`, ... bind
//! variables in order, except the null-like values
//! [`Expression::params`](crate::query::Expression::params) leaves out,
//! which render as `NULL`, and "now", which renders as
//! `SYS_EXTRACT_UTC(SYSTIMESTAMP)`. Row limits are `FETCH FIRST n ROWS ONLY`
//! (`ROWNUM <= n` in `UPDATE`/`DELETE`). Oracle's `RETURNING ... INTO`
//! fills variables rather than returning rows, so inserted and updated rows
//! are read back by the `ROWID`s it returns, and deleted rows are read
//! (with their `ROWID`s) before they are deleted. Upserts on a unique key
//! are `MERGE`, after which the rows are read back by that key.
//!
//! The `SQLite`-specific expressions (JSON type predicates, storage classes
//! and casts, zero blobs, bounded text, schema predicates), table aliases,
//! index hints and `INSERT`s without values are refused with
//! [`DatabaseError::UnsupportedOperation`].

use std::fmt::Write as _;

use crate::query::{BooleanExpression, Expression};
use crate::{DatabaseError, DatabaseValue, Row};

use super::Executor;
use super::run::Request;
use super::value::now_sql;

/// The most bind variables in one `IN` list here.
const ROWID_CHUNK: usize = 500;

/// Rendering state: what couldn't be rendered.
#[derive(Debug, Default)]
pub(super) struct Render {
    unsupported: Option<&'static str>,
}

impl Render {
    fn refuse(&mut self, what: &'static str) -> String {
        self.unsupported.get_or_insert(what);
        String::new()
    }

    /// `sql`, or the refusal of what couldn't be rendered.
    fn finish(self, sql: &str) -> Result<String, DatabaseError> {
        self.unsupported.map_or_else(
            || Ok(number_placeholders(sql)),
            |what| {
                Err(DatabaseError::UnsupportedOperation(format!(
                    "{what} on Oracle"
                )))
            },
        )
    }
}

/// Number the `?` placeholders (outside literals) `:1`, `:2`, ...
fn number_placeholders(sql: &str) -> String {
    let Ok(tokens) = super::lexer::tokenize(sql) else {
        return sql.to_string();
    };
    let mut out = String::with_capacity(sql.len());
    let mut copied = 0;
    let mut number = 0;
    for token in tokens {
        if token.is_symbol('?') {
            number += 1;
            out.push_str(&sql[copied..token.start]);
            let _ = write!(out, ":{number}");
            copied = token.start + 1;
        }
    }
    out.push_str(&sql[copied..]);
    out
}

/// Quote an identifier with `"`.
fn ident(name: &str) -> String {
    crate::query::render_identifier(name, '"')
}

/// Oracle rendering of query builder expressions.
trait ToOracle {
    /// Render (with `?` placeholders, numbered by [`Render::finish`]).
    fn oracle(&self, cx: &mut Render) -> String;
}

impl<T: Expression + ?Sized> ToOracle for T {
    fn oracle(&self, cx: &mut Render) -> String {
        to_sql(self, cx)
    }
}

/// `(left op right)`, with `IS [NOT] NULL` for a null right side.
fn comparison(left: &str, right: &str, op: &str, right_null: bool) -> String {
    match (op, right_null) {
        ("=", true) => format!("({left} IS NULL)"),
        ("<>", true) => format!("({left} IS NOT NULL)"),
        _ => format!("({left} {op} {right})"),
    }
}

/// Render `expression` (with `?` placeholders, numbered by [`Render::finish`]).
#[allow(clippy::too_many_lines)]
fn to_sql<E: Expression + ?Sized>(expression: &E, cx: &mut Render) -> String {
    use crate::query::{ExpressionType, SortDirection};
    let list = |values: &[Box<dyn Expression>], cx: &mut Render, separator: &str| {
        values
            .iter()
            .map(|value| value.oracle(cx))
            .collect::<Vec<_>>()
            .join(separator)
    };
    match expression.expression_type() {
        #[cfg(feature = "schema")]
        ExpressionType::SchemaPredicate(_) => cx.refuse("schema predicates"),
        ExpressionType::JsonTypeIs(_) => cx.refuse("SQLite JSON type predicates"),
        ExpressionType::StorageType(_) => cx.refuse("SQLite storage classes"),
        ExpressionType::ZeroBlob(_) => cx.refuse("SQLite zero blobs"),
        ExpressionType::StorageCast(_) => cx.refuse("SQLite storage casts"),
        ExpressionType::BoundedText(_) => cx.refuse("SQLite bounded text"),
        ExpressionType::Add(value) => {
            format!("({} + {})", value.left.oracle(cx), value.right.oracle(cx))
        }
        ExpressionType::GtExpression(value) => {
            format!("({} > {})", value.left.oracle(cx), value.right.oracle(cx))
        }
        ExpressionType::EqExpression(value) => {
            let (left, right) = (value.left.oracle(cx), value.right.oracle(cx));
            comparison(&left, &right, "=", value.right.is_null())
        }
        ExpressionType::NotEqExpression(value) => {
            let (left, right) = (value.left.oracle(cx), value.right.oracle(cx));
            comparison(&left, &right, "<>", value.right.is_null())
        }
        ExpressionType::Eq(value) => {
            let (left, right) = (value.left.oracle(cx), value.right.oracle(cx));
            comparison(&left, &right, "=", value.right.is_null())
        }
        ExpressionType::NotEq(value) => {
            let (left, right) = (value.left.oracle(cx), value.right.oracle(cx));
            comparison(&left, &right, "<>", value.right.is_null())
        }
        ExpressionType::Gt(value) => {
            assert!(!value.right.is_null(), "Invalid > comparison with NULL");
            format!("({} > {})", value.left.oracle(cx), value.right.oracle(cx))
        }
        ExpressionType::Gte(value) => {
            assert!(!value.right.is_null(), "Invalid >= comparison with NULL");
            format!("({} >= {})", value.left.oracle(cx), value.right.oracle(cx))
        }
        ExpressionType::Lt(value) => {
            assert!(!value.right.is_null(), "Invalid < comparison with NULL");
            format!("({} < {})", value.left.oracle(cx), value.right.oracle(cx))
        }
        ExpressionType::Lte(value) => {
            assert!(!value.right.is_null(), "Invalid <= comparison with NULL");
            format!("({} <= {})", value.left.oracle(cx), value.right.oracle(cx))
        }
        ExpressionType::In(value) => {
            format!("{} IN ({})", value.left.oracle(cx), value.values.oracle(cx))
        }
        ExpressionType::NotIn(value) => {
            format!(
                "{} NOT IN ({})",
                value.left.oracle(cx),
                value.values.oracle(cx)
            )
        }
        ExpressionType::Or(value) => {
            let conditions = value
                .conditions
                .iter()
                .map(|condition| condition.oracle(cx))
                .collect::<Vec<_>>();
            format!("({})", conditions.join(" OR "))
        }
        ExpressionType::And(value) => {
            let conditions = value
                .conditions
                .iter()
                .map(|condition| condition.oracle(cx))
                .collect::<Vec<_>>();
            format!("({})", conditions.join(" AND "))
        }
        ExpressionType::Join(value) => format!(
            "{} JOIN {} ON {}",
            if value.left { "LEFT" } else { "INNER" },
            value.render_table('"'),
            value.on.render('"')
        ),
        ExpressionType::Sort(value) => format!(
            "({}) {}",
            value.expression.oracle(cx),
            match value.direction {
                SortDirection::Asc => "ASC",
                SortDirection::Desc => "DESC",
            }
        ),
        ExpressionType::NotLike(value) => format!(
            "({} NOT LIKE {})",
            value.left.oracle(cx),
            value.right.oracle(cx)
        ),
        ExpressionType::InList(value) => list(&value.values, cx, ","),
        ExpressionType::Coalesce(value) => format!("COALESCE({})", list(&value.values, cx, ",")),
        #[cfg(feature = "raw-sql")]
        ExpressionType::Literal(value) => value.value.clone(),
        ExpressionType::Max(value) => format!("MAX({})", value.expression.oracle(cx)),
        ExpressionType::Min(value) => format!("MIN({})", value.expression.oracle(cx)),
        ExpressionType::Count(value) => format!(
            "COUNT({})",
            value
                .expression
                .as_ref()
                .map_or_else(|| "*".to_owned(), |expression| expression.oracle(cx))
        ),
        // Bytes of the text in the database character set (UTF-8 in
        // current databases).
        ExpressionType::ByteLength(value) => {
            format!("LENGTHB(TO_CHAR({}))", value.expression.oracle(cx))
        }
        ExpressionType::QualifiedColumn(value) => value.render('"'),
        ExpressionType::Identifier(value) => ident(&value.value),
        ExpressionType::SelectQuery(value) => select_sql(value, value.limit, cx),
        ExpressionType::DatabaseValue(value) => match value {
            // Render NULL inline exactly for the values `Expression::params`
            // excludes from binding, so placeholders and bound values stay
            // aligned.
            value if crate::query::Expression::is_null(value) => "NULL".to_string(),
            value => now_sql(value).unwrap_or_else(|| "?".to_string()),
        },
    }
}

/// A builder `SELECT` limited to `limit` rows.
fn select_sql(
    query: &crate::query::SelectQuery<'_>,
    limit: Option<usize>,
    cx: &mut Render,
) -> String {
    if query.table_alias.is_some() {
        cx.refuse("table aliases");
    }
    if query.indexed_by.is_some() {
        cx.refuse("index hints");
    }
    let columns = if query.projections.is_empty() {
        crate::query::render_columns(query.columns, '"')
    } else {
        query
            .projections
            .iter()
            .map(|projection| {
                let expression = projection.expression.oracle(cx);
                projection.alias.as_ref().map_or_else(
                    || expression.clone(),
                    |alias| format!("{expression} AS {}", ident(alias)),
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let mut sql = String::from("SELECT ");
    if query.distinct {
        sql.push_str("DISTINCT ");
    }
    let _ = write!(sql, "{columns} FROM {}", ident(query.table_name));
    for join in query.joins.iter().flatten() {
        let _ = write!(sql, " {}", join.oracle(cx));
    }
    let filters = query
        .filters
        .iter()
        .flatten()
        .map(|filter| filter.oracle(cx))
        .collect::<Vec<_>>();
    if !filters.is_empty() {
        let _ = write!(sql, " WHERE {}", filters.join(" AND "));
    }
    let groups = query
        .groups
        .iter()
        .map(|group| group.oracle(cx))
        .collect::<Vec<_>>();
    if !groups.is_empty() {
        let _ = write!(sql, " GROUP BY {}", groups.join(", "));
    }
    let sorts = query
        .sorts
        .iter()
        .flatten()
        .map(|sort| sort.oracle(cx))
        .collect::<Vec<_>>();
    if !sorts.is_empty() {
        let _ = write!(sql, " ORDER BY {}", sorts.join(", "));
    }
    if let Some(limit) = limit {
        let _ = write!(sql, " FETCH FIRST {limit} ROWS ONLY");
    }
    sql
}

type Values<'a> = [(&'a str, Box<dyn Expression>)];
type Filters = [Box<dyn BooleanExpression>];

/// The parameters of `values`, in order.
fn value_params(values: &Values<'_>) -> Vec<DatabaseValue> {
    values
        .iter()
        .flat_map(|(_, value)| value.params().unwrap_or_default().into_iter().cloned())
        .collect()
}

/// The parameters of `filters`, in order.
fn filter_params(filters: Option<&Filters>) -> Vec<DatabaseValue> {
    filters
        .into_iter()
        .flatten()
        .flat_map(|filter| filter.params().unwrap_or_default().into_iter().cloned())
        .collect()
}

/// The conditions of `filters` (empty without any).
fn conditions(filters: Option<&Filters>, cx: &mut Render) -> Vec<String> {
    filters
        .into_iter()
        .flatten()
        .map(|filter| filter.oracle(cx))
        .collect()
}

/// ` WHERE ...` for `conditions` (empty without any).
fn where_clause(conditions: &[String]) -> String {
    if conditions.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", conditions.join(" AND "))
    }
}

/// Run `sql` and return its rows.
async fn rows(
    executor: &dyn Executor,
    sql: &str,
    params: &[DatabaseValue],
) -> Result<Vec<Row>, DatabaseError> {
    Ok(executor.run_sql(sql, params).await?.into_rows())
}

/// Run `sql`, which ends with `RETURNING ROWID INTO :switchy_rowid` (an
/// output variable the executor binds), and return the `ROWID`s.
async fn returning_rowids(
    executor: &dyn Executor,
    sql: &str,
    params: Vec<DatabaseValue>,
) -> Result<Vec<String>, DatabaseError> {
    Ok(executor
        .run(Request {
            sql: sql.to_string(),
            params,
            returning_rowids: true,
        })
        .await?
        .rowids)
}

/// The rows of `table_name` with these `ROWID`s.
async fn rows_by_rowid(
    executor: &dyn Executor,
    table_name: &str,
    rowids: &[String],
) -> Result<Vec<Row>, DatabaseError> {
    let mut found = Vec::new();
    for chunk in rowids.chunks(ROWID_CHUNK) {
        let list = (1..=chunk.len())
            .map(|number| format!("CHARTOROWID(:{number})"))
            .collect::<Vec<_>>()
            .join(", ");
        let params = chunk
            .iter()
            .map(|rowid| DatabaseValue::String(rowid.clone()))
            .collect::<Vec<_>>();
        found.extend(
            rows(
                executor,
                &format!(
                    "SELECT * FROM {} WHERE ROWID IN ({list})",
                    ident(table_name)
                ),
                &params,
            )
            .await?,
        );
    }
    Ok(found)
}

/// The parameters of a builder `SELECT`, in order.
fn select_params(query: &crate::query::SelectQuery<'_>) -> Vec<DatabaseValue> {
    query
        .params()
        .unwrap_or_default()
        .into_iter()
        .cloned()
        .collect()
}

/// Run a builder `SELECT`.
pub(super) async fn select(
    executor: &dyn Executor,
    query: &crate::query::SelectQuery<'_>,
) -> Result<Vec<Row>, DatabaseError> {
    let mut cx = Render::default();
    let sql = select_sql(query, query.limit, &mut cx);
    rows(executor, &cx.finish(&sql)?, &select_params(query)).await
}

/// Run a builder `SELECT` and return its first row.
pub(super) async fn select_first(
    executor: &dyn Executor,
    query: &crate::query::SelectQuery<'_>,
) -> Result<Option<Row>, DatabaseError> {
    let mut cx = Render::default();
    let sql = select_sql(query, Some(1), &mut cx);
    Ok(rows(executor, &cx.finish(&sql)?, &select_params(query))
        .await?
        .into_iter()
        .next())
}

/// `INSERT ... RETURNING ROWID INTO`, then the row read back.
pub(super) async fn insert(
    executor: &dyn Executor,
    table_name: &str,
    values: &Values<'_>,
) -> Result<Row, DatabaseError> {
    let mut cx = Render::default();
    let sql = insert_sql(table_name, values, &mut cx);
    let sql = cx.finish(&sql)?;
    let rowids = returning_rowids(executor, &sql, value_params(values)).await?;
    rows_by_rowid(executor, table_name, &rowids)
        .await?
        .into_iter()
        .next()
        .ok_or(DatabaseError::NoRow)
}

fn insert_sql(table_name: &str, values: &Values<'_>, cx: &mut Render) -> String {
    if values.is_empty() {
        return cx.refuse("an INSERT without values");
    }
    let columns = values
        .iter()
        .map(|(name, _)| ident(name))
        .collect::<Vec<_>>()
        .join(", ");
    let values = values
        .iter()
        .map(|(_, value)| value.oracle(cx))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "INSERT INTO {} ({columns}) VALUES ({values}) RETURNING ROWID INTO :switchy_rowid",
        ident(table_name)
    )
}

/// `UPDATE ... RETURNING ROWID INTO`, then the rows read back.
pub(super) async fn update(
    executor: &dyn Executor,
    table_name: &str,
    values: &Values<'_>,
    filters: Option<&Filters>,
    limit: Option<usize>,
) -> Result<Vec<Row>, DatabaseError> {
    let mut cx = Render::default();
    let sql = update_sql(table_name, values, filters, limit, &mut cx);
    let sql = cx.finish(&sql)?;
    let mut params = value_params(values);
    params.extend(filter_params(filters));
    let rowids = returning_rowids(executor, &sql, params).await?;
    rows_by_rowid(executor, table_name, &rowids).await
}

fn update_sql(
    table_name: &str,
    values: &Values<'_>,
    filters: Option<&Filters>,
    limit: Option<usize>,
    cx: &mut Render,
) -> String {
    let set = values
        .iter()
        .map(|(name, value)| format!("{} = ({})", ident(name), value.oracle(cx)))
        .collect::<Vec<_>>()
        .join(", ");
    let mut conditions = conditions(filters, cx);
    if let Some(limit) = limit {
        conditions.push(format!("ROWNUM <= {limit}"));
    }
    format!(
        "UPDATE {} SET {set}{} RETURNING ROWID INTO :switchy_rowid",
        ident(table_name),
        where_clause(&conditions)
    )
}

/// The rows to delete (read first, with their `ROWID`s), then `DELETE` by
/// `ROWID`.
pub(super) async fn delete(
    executor: &dyn Executor,
    table_name: &str,
    filters: Option<&Filters>,
    limit: Option<usize>,
) -> Result<Vec<Row>, DatabaseError> {
    const ROWID_COLUMN: &str = "SWITCHY_ROWID";
    let mut cx = Render::default();
    let mut conditions = conditions(filters, &mut cx);
    if let Some(limit) = limit {
        conditions.push(format!("ROWNUM <= {limit}"));
    }
    let sql = format!(
        "SELECT ROWIDTOCHAR(t.ROWID) AS \"{ROWID_COLUMN}\", t.* FROM {} t{}",
        ident(table_name),
        where_clause(&conditions)
    );
    let mut selected = rows(executor, &cx.finish(&sql)?, &filter_params(filters)).await?;
    let mut rowids = Vec::with_capacity(selected.len());
    for row in &mut selected {
        if let Some(index) = row
            .columns
            .iter()
            .position(|(name, _)| name == ROWID_COLUMN)
            && let (_, DatabaseValue::String(rowid)) = row.columns.remove(index)
        {
            rowids.push(rowid);
        }
    }
    for chunk in rowids.chunks(ROWID_CHUNK) {
        let list = (1..=chunk.len())
            .map(|number| format!("CHARTOROWID(:{number})"))
            .collect::<Vec<_>>()
            .join(", ");
        let params = chunk
            .iter()
            .map(|rowid| DatabaseValue::String(rowid.clone()))
            .collect::<Vec<_>>();
        executor
            .run_sql(
                &format!("DELETE FROM {} WHERE ROWID IN ({list})", ident(table_name)),
                &params,
            )
            .await?;
    }
    Ok(selected)
}

/// `MERGE` of `rows` (each the same columns) on the `unique` columns:
/// matching rows are updated, the rest inserted.
fn merge_sql(
    table_name: &str,
    rows: &[&Values<'_>],
    unique: &[String],
    cx: &mut Render,
) -> Result<String, DatabaseError> {
    let Some(first) = rows.first() else {
        return Err(DatabaseError::InvalidQuery(
            "UPSERT needs at least one row".to_string(),
        ));
    };
    if unique.is_empty() {
        return Err(DatabaseError::InvalidQuery(
            "UPSERT conflict target cannot be empty".to_string(),
        ));
    }
    let columns = first
        .iter()
        .map(|(name, _)| ident(name))
        .collect::<Vec<_>>();
    let source_rows = rows
        .iter()
        .map(|row| {
            format!(
                "SELECT {} FROM dual",
                row.iter()
                    .zip(&columns)
                    .map(|((_, value), column)| format!("{} AS {column}", value.oracle(cx)))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join(" UNION ALL ");
    let on = unique
        .iter()
        .map(|column| format!("target.{column} = source.{column}"))
        .collect::<Vec<_>>()
        .join(" AND ");
    let set = columns
        .iter()
        .filter(|column| !unique.contains(column))
        .map(|column| format!("target.{column} = source.{column}"))
        .collect::<Vec<_>>();
    let matched = if set.is_empty() {
        String::new()
    } else {
        format!(" WHEN MATCHED THEN UPDATE SET {}", set.join(", "))
    };
    Ok(format!(
        "MERGE INTO {} target USING ({source_rows}) source ON ({on}){matched} \
         WHEN NOT MATCHED THEN INSERT ({}) VALUES ({})",
        ident(table_name),
        columns.join(", "),
        columns
            .iter()
            .map(|column| format!("source.{column}"))
            .collect::<Vec<_>>()
            .join(", ")
    ))
}

/// The rows of `table_name` whose `unique` columns hold the values of one
/// of `rows`.
async fn rows_by_key(
    executor: &dyn Executor,
    table_name: &str,
    rows_values: &[&Values<'_>],
    unique: &[String],
) -> Result<Vec<Row>, DatabaseError> {
    let mut cx = Render::default();
    let mut params = Vec::new();
    let mut alternatives = Vec::new();
    for row in rows_values {
        let mut terms = Vec::new();
        for (name, value) in *row {
            if !unique.contains(&ident(name)) {
                continue;
            }
            let rendered = value.oracle(&mut cx);
            params.extend(value.params().unwrap_or_default().into_iter().cloned());
            terms.push(comparison(&ident(name), &rendered, "=", value.is_null()));
        }
        if terms.is_empty() {
            return Err(DatabaseError::InvalidQuery(
                "UPSERT rows must set their conflict target columns".to_string(),
            ));
        }
        alternatives.push(format!("({})", terms.join(" AND ")));
    }
    let sql = format!(
        "SELECT * FROM {} WHERE {}",
        ident(table_name),
        alternatives.join(" OR ")
    );
    rows(executor, &cx.finish(&sql)?, &params).await
}

/// Update the matching rows, or insert one when none match (or `MERGE` on
/// the `unique` columns when given).
pub(super) async fn upsert(
    executor: &dyn Executor,
    table_name: &str,
    values: &Values<'_>,
    filters: Option<&Filters>,
    limit: Option<usize>,
    unique: Option<&[&str]>,
) -> Result<Vec<Row>, DatabaseError> {
    if let Some(unique) = unique {
        if filters.is_some_and(|filters| !filters.is_empty()) {
            return Err(DatabaseError::UnsupportedOperation(
                "UPSERT with both a conflict target and filters on Oracle".to_string(),
            ));
        }
        let unique = unique.iter().map(|name| ident(name)).collect::<Vec<_>>();
        let mut cx = Render::default();
        let sql = merge_sql(table_name, &[values], &unique, &mut cx)?;
        executor
            .run_sql(&cx.finish(&sql)?, &value_params(values))
            .await?;
        return rows_by_key(executor, table_name, &[values], &unique).await;
    }
    let updated = update(executor, table_name, values, filters, limit).await?;
    if updated.is_empty() {
        Ok(vec![insert(executor, table_name, values).await?])
    } else {
        Ok(updated)
    }
}

/// Multi-row `MERGE` on the `unique` columns.
pub(super) async fn upsert_multi(
    executor: &dyn Executor,
    table_name: &str,
    values: &[Vec<(&str, Box<dyn Expression>)>],
    unique: Option<&[Box<dyn Expression>]>,
) -> Result<Vec<Row>, DatabaseError> {
    let Some(first) = values.first() else {
        return Ok(Vec::new());
    };
    if first.is_empty() {
        return Err(DatabaseError::InvalidQuery(
            "multi-row UPSERT rows cannot be empty".to_owned(),
        ));
    }
    if values.iter().skip(1).any(|row| {
        row.len() != first.len()
            || row
                .iter()
                .zip(first)
                .any(|((column, _), (expected, _))| column != expected)
    }) {
        return Err(DatabaseError::InvalidQuery(
            "multi-row UPSERT rows must use the same columns in the same order".to_owned(),
        ));
    }
    let unique = unique
        .ok_or_else(|| {
            DatabaseError::InvalidQuery("multi-row UPSERT requires a conflict target".to_owned())
        })?
        .iter()
        .map(|expression| match expression.expression_type() {
            crate::query::ExpressionType::Identifier(identifier) => Ok(ident(&identifier.value)),
            _ => Err(DatabaseError::InvalidQuery(
                "multi-row UPSERT conflict targets must be identifiers".to_owned(),
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    let rows_ref = values.iter().map(Vec::as_slice).collect::<Vec<_>>();
    let mut cx = Render::default();
    let sql = merge_sql(table_name, &rows_ref, &unique, &mut cx)?;
    let params = values
        .iter()
        .flat_map(|row| value_params(row))
        .collect::<Vec<_>>();
    executor.run_sql(&cx.finish(&sql)?, &params).await?;
    rows_by_key(executor, table_name, &rows_ref, &unique).await
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::query::{FilterableQuery as _, select, where_eq, where_gt};

    #[test]
    fn selects_fetch_first_with_numbered_binds() {
        let query = select("albums")
            .columns(&["id", "title"])
            .filter(Box::new(where_gt("price", 10_i64)))
            .filter(Box::new(where_eq("format", "vinyl")))
            .limit(5);
        let mut cx = Render::default();
        let sql = select_sql(&query, query.limit, &mut cx);
        let sql = cx.finish(&sql).unwrap();
        assert!(sql.ends_with(" FETCH FIRST 5 ROWS ONLY"), "{sql}");
        assert!(
            sql.contains(":1") && sql.contains(":2") && !sql.contains('?'),
            "{sql}"
        );
        assert_eq!(select_params(&query).len(), 2);
    }

    #[test]
    fn changes_return_rowids() {
        let values: Vec<(&str, Box<dyn Expression>)> = vec![
            ("title", Box::new(DatabaseValue::String("Blue".into()))),
            ("price", Box::new(DatabaseValue::Null)),
        ];
        let sql = Render::default()
            .finish(&insert_sql("albums", &values, &mut Render::default()))
            .unwrap();
        assert_eq!(
            sql,
            "INSERT INTO \"albums\" (\"title\", \"price\") VALUES (:1, NULL) RETURNING ROWID INTO :switchy_rowid"
                .replace('"', if cfg!(feature = "raw-sql") { "" } else { "\"" })
        );
        let sql = Render::default()
            .finish(&update_sql(
                "albums",
                &values,
                None,
                Some(1),
                &mut Render::default(),
            ))
            .unwrap();
        assert!(
            sql.contains(" WHERE ROWNUM <= 1 RETURNING ROWID INTO :switchy_rowid"),
            "{sql}"
        );
        let mut cx = Render::default();
        insert_sql("albums", &[], &mut cx);
        assert!(cx.finish("").is_err());
    }

    #[test]
    fn upserts_merge_on_the_unique_columns() {
        let values: Vec<(&str, Box<dyn Expression>)> = vec![
            ("id", Box::new(DatabaseValue::Int64(1))),
            ("title", Box::new(DatabaseValue::String("Blue".into()))),
        ];
        let mut cx = Render::default();
        let sql = merge_sql("albums", &[&values], &[ident("id")], &mut cx).unwrap();
        let sql = cx.finish(&sql).unwrap();
        assert!(sql.starts_with("MERGE INTO "), "{sql}");
        assert!(sql.contains("USING (SELECT :1 AS "), "{sql}");
        assert!(sql.contains("WHEN MATCHED THEN UPDATE SET"), "{sql}");
    }
}
