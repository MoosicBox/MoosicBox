//! T-SQL for the query builder, and the statements that run it on an
//! [`Executor`].
//!
//! Identifiers are quoted with `"` (connections run with
//! `QUOTED_IDENTIFIER ON`); values become `@P1`, `@P2`, ... parameters bound
//! in order, except the null-like values
//! [`Expression::params`](crate::query::Expression::params) leaves out,
//! which render as `NULL`, and "now", which renders as `SYSUTCDATETIME()`.
//! Row limits are `TOP (n)`, and changed rows come back through `OUTPUT
//! INSERTED.*` / `OUTPUT DELETED.*` (which SQL Server refuses on tables with
//! enabled triggers). Upserts on a unique key are `MERGE ... WITH
//! (HOLDLOCK)`.
//!
//! The `SQLite`-specific expressions (JSON type predicates, storage classes
//! and casts, zero blobs, bounded text, schema predicates), table aliases
//! and index hints are refused with [`DatabaseError::UnsupportedOperation`].

use std::fmt::Write as _;

use crate::query::{BooleanExpression, Expression};
use crate::{DatabaseError, DatabaseValue, Row};

use super::value::now_sql;
use super::{Executor, Mode};

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
                    "{what} on SQL Server"
                )))
            },
        )
    }
}

/// Number the `?` placeholders (outside literals) `@P1`, `@P2`, ...
fn number_placeholders(sql: &str) -> String {
    let Ok(tokens) = super::tsql::tokenize(sql) else {
        return sql.to_string();
    };
    let mut out = String::with_capacity(sql.len());
    let mut copied = 0;
    let mut number = 0;
    for token in tokens {
        if token.kind == super::tsql::Kind::Symbol && token.text == "?" {
            number += 1;
            out.push_str(&sql[copied..token.start]);
            let _ = write!(out, "@P{number}");
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

/// T-SQL rendering of query builder expressions.
trait ToTsql {
    /// Render (with `?` placeholders, numbered by [`Render::finish`]).
    fn tsql(&self, cx: &mut Render) -> String;
}

impl<T: Expression + ?Sized> ToTsql for T {
    fn tsql(&self, cx: &mut Render) -> String {
        to_sql(self, cx)
    }
}

/// Render `expression` (with `?` placeholders, numbered by [`Render::finish`]).
#[allow(clippy::too_many_lines)]
fn to_sql<E: Expression + ?Sized>(expression: &E, cx: &mut Render) -> String {
    use crate::query::{ExpressionType, SortDirection};
    let list = |values: &[Box<dyn Expression>], cx: &mut Render, separator: &str| {
        values
            .iter()
            .map(|value| value.tsql(cx))
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
            format!("({} + {})", value.left.tsql(cx), value.right.tsql(cx))
        }
        ExpressionType::GtExpression(value) => {
            format!("({} > {})", value.left.tsql(cx), value.right.tsql(cx))
        }
        ExpressionType::EqExpression(value) => {
            let (left, right) = (value.left.tsql(cx), value.right.tsql(cx));
            if value.right.is_null() {
                format!("({left} IS NULL)")
            } else {
                format!("({left} = {right})")
            }
        }
        ExpressionType::NotEqExpression(value) => {
            let (left, right) = (value.left.tsql(cx), value.right.tsql(cx));
            if value.right.is_null() {
                format!("({left} IS NOT NULL)")
            } else {
                format!("({left} <> {right})")
            }
        }
        ExpressionType::Eq(value) => {
            let (left, right) = (value.left.tsql(cx), value.right.tsql(cx));
            if value.right.is_null() {
                format!("({left} IS NULL)")
            } else {
                format!("({left} = {right})")
            }
        }
        ExpressionType::NotEq(value) => {
            let (left, right) = (value.left.tsql(cx), value.right.tsql(cx));
            if value.right.is_null() {
                format!("({left} IS NOT NULL)")
            } else {
                format!("({left} <> {right})")
            }
        }
        ExpressionType::Gt(value) => {
            assert!(!value.right.is_null(), "Invalid > comparison with NULL");
            format!("({} > {})", value.left.tsql(cx), value.right.tsql(cx))
        }
        ExpressionType::Gte(value) => {
            assert!(!value.right.is_null(), "Invalid >= comparison with NULL");
            format!("({} >= {})", value.left.tsql(cx), value.right.tsql(cx))
        }
        ExpressionType::Lt(value) => {
            assert!(!value.right.is_null(), "Invalid < comparison with NULL");
            format!("({} < {})", value.left.tsql(cx), value.right.tsql(cx))
        }
        ExpressionType::Lte(value) => {
            assert!(!value.right.is_null(), "Invalid <= comparison with NULL");
            format!("({} <= {})", value.left.tsql(cx), value.right.tsql(cx))
        }
        ExpressionType::In(value) => {
            format!("{} IN ({})", value.left.tsql(cx), value.values.tsql(cx))
        }
        ExpressionType::NotIn(value) => {
            format!("{} NOT IN ({})", value.left.tsql(cx), value.values.tsql(cx))
        }
        ExpressionType::Or(value) => {
            let conditions = value
                .conditions
                .iter()
                .map(|condition| condition.tsql(cx))
                .collect::<Vec<_>>();
            format!("({})", conditions.join(" OR "))
        }
        ExpressionType::And(value) => {
            let conditions = value
                .conditions
                .iter()
                .map(|condition| condition.tsql(cx))
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
            value.expression.tsql(cx),
            match value.direction {
                SortDirection::Asc => "ASC",
                SortDirection::Desc => "DESC",
            }
        ),
        ExpressionType::NotLike(value) => format!(
            "({} NOT LIKE {})",
            value.left.tsql(cx),
            value.right.tsql(cx)
        ),
        ExpressionType::InList(value) => list(&value.values, cx, ","),
        ExpressionType::Coalesce(value) => format!("COALESCE({})", list(&value.values, cx, ",")),
        #[cfg(feature = "raw-sql")]
        ExpressionType::Literal(value) => value.value.clone(),
        ExpressionType::Max(value) => format!("MAX({})", value.expression.tsql(cx)),
        ExpressionType::Min(value) => format!("MIN({})", value.expression.tsql(cx)),
        ExpressionType::Count(value) => format!(
            "COUNT_BIG({})",
            value
                .expression
                .as_ref()
                .map_or_else(|| "*".to_owned(), |expression| expression.tsql(cx))
        ),
        // UTF-8 bytes of the text.
        ExpressionType::ByteLength(value) => format!(
            "DATALENGTH(CAST(CAST({} AS nvarchar(max)) COLLATE Latin1_General_100_CI_AS_SC_UTF8 AS varchar(max)))",
            value.expression.tsql(cx)
        ),
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
                let expression = projection.expression.tsql(cx);
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
    if let Some(limit) = limit {
        let _ = write!(sql, "TOP ({limit}) ");
    }
    let _ = write!(sql, "{columns} FROM {}", ident(query.table_name));
    for join in query.joins.iter().flatten() {
        let _ = write!(sql, " {}", join.tsql(cx));
    }
    let filters = query
        .filters
        .iter()
        .flatten()
        .map(|filter| filter.tsql(cx))
        .collect::<Vec<_>>();
    if !filters.is_empty() {
        let _ = write!(sql, " WHERE {}", filters.join(" AND "));
    }
    let groups = query
        .groups
        .iter()
        .map(|group| group.tsql(cx))
        .collect::<Vec<_>>();
    if !groups.is_empty() {
        let _ = write!(sql, " GROUP BY {}", groups.join(", "));
    }
    let sorts = query
        .sorts
        .iter()
        .flatten()
        .map(|sort| sort.tsql(cx))
        .collect::<Vec<_>>();
    if !sorts.is_empty() {
        let _ = write!(sql, " ORDER BY {}", sorts.join(", "));
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

/// ` WHERE ...` for `filters` (empty without any).
fn where_clause(filters: Option<&Filters>, cx: &mut Render) -> String {
    let filters = filters
        .into_iter()
        .flatten()
        .map(|filter| filter.tsql(cx))
        .collect::<Vec<_>>();
    if filters.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", filters.join(" AND "))
    }
}

/// Run `sql` and return its rows.
async fn rows(
    executor: &dyn Executor,
    sql: &str,
    params: &[DatabaseValue],
) -> Result<Vec<Row>, DatabaseError> {
    Ok(executor.run(sql, params, Mode::Rows).await?.into_rows())
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

/// `INSERT ... OUTPUT INSERTED.*`.
pub(super) async fn insert(
    executor: &dyn Executor,
    table_name: &str,
    values: &Values<'_>,
) -> Result<Row, DatabaseError> {
    let mut cx = Render::default();
    let sql = insert_sql(table_name, values, &mut cx);
    rows(executor, &cx.finish(&sql)?, &value_params(values))
        .await?
        .into_iter()
        .next()
        .ok_or(DatabaseError::NoRow)
}

fn insert_sql(table_name: &str, values: &Values<'_>, cx: &mut Render) -> String {
    let table = ident(table_name);
    if values.is_empty() {
        return format!("INSERT INTO {table} OUTPUT INSERTED.* DEFAULT VALUES");
    }
    let columns = values
        .iter()
        .map(|(name, _)| ident(name))
        .collect::<Vec<_>>()
        .join(", ");
    let values = values
        .iter()
        .map(|(_, value)| value.tsql(cx))
        .collect::<Vec<_>>()
        .join(", ");
    format!("INSERT INTO {table} ({columns}) OUTPUT INSERTED.* VALUES ({values})")
}

/// `UPDATE [TOP (n)] ... OUTPUT INSERTED.*`.
pub(super) async fn update(
    executor: &dyn Executor,
    table_name: &str,
    values: &Values<'_>,
    filters: Option<&Filters>,
    limit: Option<usize>,
) -> Result<Vec<Row>, DatabaseError> {
    let mut cx = Render::default();
    let sql = update_sql(table_name, values, filters, limit, &mut cx);
    let mut params = value_params(values);
    params.extend(filter_params(filters));
    rows(executor, &cx.finish(&sql)?, &params).await
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
        .map(|(name, value)| format!("{} = ({})", ident(name), value.tsql(cx)))
        .collect::<Vec<_>>()
        .join(", ");
    let top = limit.map_or_else(String::new, |limit| format!(" TOP ({limit})"));
    format!(
        "UPDATE{top} {} SET {set} OUTPUT INSERTED.*{}",
        ident(table_name),
        where_clause(filters, cx)
    )
}

/// `DELETE [TOP (n)] ... OUTPUT DELETED.*`.
pub(super) async fn delete(
    executor: &dyn Executor,
    table_name: &str,
    filters: Option<&Filters>,
    limit: Option<usize>,
) -> Result<Vec<Row>, DatabaseError> {
    let mut cx = Render::default();
    let top = limit.map_or_else(String::new, |limit| format!(" TOP ({limit})"));
    let sql = format!(
        "DELETE{top} FROM {} OUTPUT DELETED.*{}",
        ident(table_name),
        where_clause(filters, &mut cx)
    );
    rows(executor, &cx.finish(&sql)?, &filter_params(filters)).await
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
                "({})",
                row.iter()
                    .map(|(_, value)| value.tsql(cx))
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
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
        "MERGE INTO {} WITH (HOLDLOCK) AS target USING (VALUES {source_rows}) AS source ({}) \
         ON {on}{matched} WHEN NOT MATCHED THEN INSERT ({}) VALUES ({}) OUTPUT INSERTED.*;",
        ident(table_name),
        columns.join(", "),
        columns.join(", "),
        columns
            .iter()
            .map(|column| format!("source.{column}"))
            .collect::<Vec<_>>()
            .join(", ")
    ))
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
                "UPSERT with both a conflict target and filters on SQL Server".to_string(),
            ));
        }
        let unique = unique.iter().map(|name| ident(name)).collect::<Vec<_>>();
        let mut cx = Render::default();
        let sql = merge_sql(table_name, &[values], &unique, &mut cx)?;
        return rows(executor, &cx.finish(&sql)?, &value_params(values)).await;
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
    rows(executor, &cx.finish(&sql)?, &params).await
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::query::{FilterableQuery as _, select, where_eq, where_gt};

    #[test]
    fn selects_use_top_and_numbered_parameters() {
        let query = select("albums")
            .columns(&["id", "title"])
            .filter(Box::new(where_gt("price", 10_i64)))
            .filter(Box::new(where_eq("format", "vinyl")))
            .limit(5);
        let mut cx = Render::default();
        let sql = select_sql(&query, query.limit, &mut cx);
        let sql = cx.finish(&sql).unwrap();
        assert!(sql.starts_with("SELECT TOP (5) "), "{sql}");
        assert!(
            sql.contains("@P1") && sql.contains("@P2") && !sql.contains('?'),
            "{sql}"
        );
        assert_eq!(select_params(&query).len(), 2);
    }

    #[test]
    fn changes_return_rows_with_output() {
        let cx = Render::default();
        let values: Vec<(&str, Box<dyn Expression>)> = vec![
            ("title", Box::new(DatabaseValue::String("Blue".into()))),
            ("price", Box::new(DatabaseValue::Null)),
        ];
        let sql = cx
            .finish(&insert_sql("albums", &values, &mut Render::default()))
            .unwrap();
        assert_eq!(
            sql,
            "INSERT INTO \"albums\" (\"title\", \"price\") OUTPUT INSERTED.* VALUES (@P1, NULL)"
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
        assert!(sql.starts_with("UPDATE TOP (1) "), "{sql}");
        assert!(sql.contains("OUTPUT INSERTED.*"), "{sql}");
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
        assert!(sql.contains("USING (VALUES (@P1, @P2)) AS source"), "{sql}");
        assert!(sql.contains("WHEN MATCHED THEN UPDATE SET"), "{sql}");
        assert!(sql.ends_with("OUTPUT INSERTED.*;"), "{sql}");
    }
}
