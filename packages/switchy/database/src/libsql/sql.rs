//! SQL text for the query builder and schema statements, in the `SQLite`
//! dialect `libSQL` speaks, and the statements that run them on an
//! [`Executor`].
//!
//! Expressions render exactly as on the other `SQLite`-family backends:
//! values become `?` placeholders bound in order, except the null-like values
//! [`Expression::params`](crate::query::Expression::params) leaves out, which
//! render as `NULL`.

use crate::query::Expression as _;
use crate::{DatabaseError, DatabaseValue, Row, sqlite_common::format_sqlite_interval};

use super::Executor;

pub(super) trait ToSql {
    fn to_sql(&self) -> String;
}

impl<T: crate::query::Expression + ?Sized> ToSql for T {
    #[allow(clippy::too_many_lines)]
    fn to_sql(&self) -> String {
        use crate::query::{ExpressionType, SortDirection};
        match self.expression_type() {
            #[cfg(feature = "schema")]
            ExpressionType::SchemaPredicate(value) => value.sqlite.clone(),
            ExpressionType::JsonTypeIs(value) => format!(
                "(json_type({}, {}) = '{}')",
                value.expression.to_sql(),
                value.path.sqlite(),
                value.kind.sqlite()
            ),
            ExpressionType::StorageType(value) => format!("typeof({})", value.expression.to_sql()),
            ExpressionType::Add(value) => {
                format!("({} + {})", value.left.to_sql(), value.right.to_sql())
            }
            ExpressionType::ZeroBlob(value) => format!("zeroblob({})", value.length),
            ExpressionType::StorageCast(value) => format!(
                "CAST({} AS {})",
                value.expression.to_sql(),
                match value.target {
                    crate::query::StorageCastTarget::Blob => "BLOB",
                    crate::query::StorageCastTarget::Text => "TEXT",
                }
            ),
            ExpressionType::BoundedText(value) => format!(
                "CASE WHEN typeof({}) = 'text' AND length(CAST({} AS BLOB)) <= {} THEN {} ELSE {} END",
                value.expression.to_sql(),
                value.expression.to_sql(),
                value.max_bytes,
                value.expression.to_sql(),
                value.fallback.to_sql()
            ),
            ExpressionType::GtExpression(value) => {
                format!("({} > {})", value.left.to_sql(), value.right.to_sql())
            }
            ExpressionType::EqExpression(value) => format!(
                "({} {} {})",
                value.left.to_sql(),
                if value.right.is_null() { "IS" } else { "=" },
                value.right.to_sql()
            ),
            ExpressionType::NotEqExpression(value) => format!(
                "({} {} {})",
                value.left.to_sql(),
                if value.right.is_null() {
                    "IS NOT"
                } else {
                    "!="
                },
                value.right.to_sql()
            ),
            ExpressionType::Eq(value) => {
                if value.right.is_null() {
                    format!("({} IS {})", value.left.to_sql(), value.right.to_sql())
                } else {
                    format!("({} = {})", value.left.to_sql(), value.right.to_sql())
                }
            }
            ExpressionType::Gt(value) => {
                assert!(!value.right.is_null(), "Invalid > comparison with NULL");
                format!("({} > {})", value.left.to_sql(), value.right.to_sql())
            }
            ExpressionType::In(value) => {
                format!("{} IN ({})", value.left.to_sql(), value.values.to_sql())
            }
            ExpressionType::NotIn(value) => {
                format!("{} NOT IN ({})", value.left.to_sql(), value.values.to_sql())
            }
            ExpressionType::Lt(value) => {
                assert!(!value.right.is_null(), "Invalid < comparison with NULL");
                format!("({} < {})", value.left.to_sql(), value.right.to_sql())
            }
            ExpressionType::Or(value) => format!(
                "({})",
                value
                    .conditions
                    .iter()
                    .map(|x| x.to_sql())
                    .collect::<Vec<_>>()
                    .join(" OR ")
            ),
            ExpressionType::And(value) => format!(
                "({})",
                value
                    .conditions
                    .iter()
                    .map(|x| x.to_sql())
                    .collect::<Vec<_>>()
                    .join(" AND ")
            ),
            ExpressionType::Gte(value) => {
                assert!(!value.right.is_null(), "Invalid >= comparison with NULL");
                format!("({} >= {})", value.left.to_sql(), value.right.to_sql())
            }
            ExpressionType::Lte(value) => {
                assert!(!value.right.is_null(), "Invalid <= comparison with NULL");
                format!("({} <= {})", value.left.to_sql(), value.right.to_sql())
            }
            ExpressionType::Join(value) => format!(
                "{} JOIN {} ON {}",
                if value.left { "LEFT" } else { "" },
                value.render_table('"'),
                value.on.render('"')
            ),
            ExpressionType::Sort(value) => format!(
                "({}) {}",
                value.expression.to_sql(),
                match value.direction {
                    SortDirection::Asc => "ASC",
                    SortDirection::Desc => "DESC",
                }
            ),
            ExpressionType::NotLike(value) => format!(
                "({} NOT LIKE {})",
                value.left.to_sql(),
                value.right.to_sql()
            ),
            ExpressionType::NotEq(value) => {
                if value.right.is_null() {
                    format!("({} IS NOT {})", value.left.to_sql(), value.right.to_sql())
                } else {
                    format!("({} != {})", value.left.to_sql(), value.right.to_sql())
                }
            }
            ExpressionType::InList(value) => value
                .values
                .iter()
                .map(|value| value.to_sql())
                .collect::<Vec<_>>()
                .join(","),
            ExpressionType::Coalesce(value) => format!(
                "IFNULL({})",
                value
                    .values
                    .iter()
                    .map(|value| value.to_sql())
                    .collect::<Vec<_>>()
                    .join(",")
            ),
            #[cfg(feature = "raw-sql")]
            ExpressionType::Literal(value) => value.value.clone(),
            ExpressionType::Max(value) => format!("MAX({})", value.expression.to_sql()),
            ExpressionType::Count(value) => format!(
                "COUNT({})",
                value
                    .expression
                    .as_ref()
                    .map_or_else(|| "*".to_owned(), |expression| expression.to_sql())
            ),
            ExpressionType::Min(value) => format!("MIN({})", value.expression.to_sql()),
            ExpressionType::ByteLength(value) => {
                format!("length(CAST({} AS BLOB))", value.expression.to_sql())
            }
            ExpressionType::QualifiedColumn(value) => value.render('"'),
            ExpressionType::Identifier(value) => crate::query::render_identifier(&value.value, '"'),
            ExpressionType::SelectQuery(value) => {
                let columns = if value.projections.is_empty() {
                    crate::query::render_columns(value.columns, '"')
                } else {
                    value
                        .projections
                        .iter()
                        .map(|projection| {
                            let expression = projection.expression.to_sql();
                            projection.alias.as_ref().map_or_else(
                                || expression.clone(),
                                |alias| {
                                    let alias = alias.replace('"', "\"\"");
                                    format!("{expression} AS \"{alias}\"")
                                },
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                };
                let joins = value
                    .joins
                    .iter()
                    .flatten()
                    .map(ToSql::to_sql)
                    .collect::<Vec<_>>()
                    .join(" ");
                let filters = value
                    .filters
                    .iter()
                    .flatten()
                    .map(|x| x.to_sql())
                    .collect::<Vec<_>>()
                    .join(" AND ");
                let where_clause = if filters.is_empty() {
                    String::new()
                } else {
                    format!("WHERE {filters}")
                };
                let groups = value
                    .groups
                    .iter()
                    .map(|x| x.to_sql())
                    .collect::<Vec<_>>()
                    .join(", ");
                let group_clause = if groups.is_empty() {
                    String::new()
                } else {
                    format!("GROUP BY {groups}")
                };
                let sorts = value
                    .sorts
                    .iter()
                    .flatten()
                    .map(ToSql::to_sql)
                    .collect::<Vec<_>>()
                    .join(", ");
                let sort_clause = if sorts.is_empty() {
                    String::new()
                } else {
                    format!("ORDER BY {sorts}")
                };
                let limit = value
                    .limit
                    .map_or_else(String::new, |limit| format!("LIMIT {limit}"));
                format!(
                    "SELECT {} {columns} FROM {} {joins} {where_clause} {group_clause} {sort_clause} {limit}",
                    if value.distinct { "DISTINCT" } else { "" },
                    value.render_table('"')
                )
            }
            ExpressionType::DatabaseValue(value) => match value {
                // Render NULL inline exactly for the values `Expression::params`
                // excludes from binding, so placeholders and bound values stay
                // aligned. Other null-like variants are bound as NULL via `?`.
                value if crate::query::Expression::is_null(value) => "NULL".to_string(),
                DatabaseValue::Now => "strftime('%Y-%m-%dT%H:%M:%f', 'now')".to_string(),
                DatabaseValue::NowPlus(interval) => {
                    let modifiers = format_sqlite_interval(interval);
                    let modifier_str = modifiers
                        .iter()
                        .map(|m| format!("'{m}'"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("strftime('%Y-%m-%dT%H:%M:%f', datetime('now', {modifier_str}))")
                }
                _ => "?".to_string(),
            },
        }
    }
}

fn build_where_clause(filters: Option<&[Box<dyn crate::query::BooleanExpression>]>) -> String {
    filters.map_or_else(String::new, |filters| {
        if filters.is_empty() {
            String::new()
        } else {
            format!("WHERE {}", build_where_props(filters).join(" AND "))
        }
    })
}

fn build_where_props(filters: &[Box<dyn crate::query::BooleanExpression>]) -> Vec<String> {
    use std::ops::Deref;
    filters
        .iter()
        .map(|filter| filter.deref().to_sql())
        .collect()
}

fn build_set_clause(values: &[(&str, Box<dyn crate::query::Expression>)]) -> String {
    if values.is_empty() {
        String::new()
    } else {
        format!("SET {}", build_set_props(values).join(", "))
    }
}

fn build_set_props(values: &[(&str, Box<dyn crate::query::Expression>)]) -> Vec<String> {
    use std::ops::Deref;
    values
        .iter()
        .map(|(name, value)| {
            format!(
                "{}=({})",
                crate::query::render_identifier(name, '"'),
                value.deref().to_sql()
            )
        })
        .collect()
}

fn build_values_clause(values: &[(&str, Box<dyn crate::query::Expression>)]) -> String {
    if values.is_empty() {
        "DEFAULT VALUES".to_string()
    } else {
        format!("VALUES({})", build_values_props(values).join(", "))
    }
}

fn build_values_props(values: &[(&str, Box<dyn crate::query::Expression>)]) -> Vec<String> {
    use std::ops::Deref;
    values
        .iter()
        .map(|(_, value)| value.deref().to_sql())
        .collect()
}

fn bexprs_to_values(values: &[Box<dyn crate::query::BooleanExpression>]) -> Vec<DatabaseValue> {
    values
        .iter()
        .flat_map(|value| value.params().unwrap_or_default().into_iter().cloned())
        .collect()
}

fn bexprs_to_values_opt(
    values: Option<&[Box<dyn crate::query::BooleanExpression>]>,
) -> Option<Vec<DatabaseValue>> {
    values.map(bexprs_to_values)
}

type Values<'a> = [(&'a str, Box<dyn crate::query::Expression>)];
type Filters = [Box<dyn crate::query::BooleanExpression>];

fn value_params(values: &Values<'_>) -> Vec<DatabaseValue> {
    values
        .iter()
        .flat_map(|(_, value)| value.params().unwrap_or_default().into_iter().cloned())
        .collect()
}

/// Run `sql` and return its rows.
async fn rows(
    executor: &dyn Executor,
    sql: &str,
    params: &[DatabaseValue],
) -> Result<Vec<Row>, DatabaseError> {
    log::trace!("libsql: {sql} with params: {params:?}");
    Ok(executor.run(sql, params, true).await?.into_rows())
}

/// Run a builder `SELECT`.
pub(super) async fn select(
    executor: &dyn Executor,
    query: &crate::query::SelectQuery<'_>,
) -> Result<Vec<Row>, DatabaseError> {
    if query.table_alias.is_some() {
        return Err(DatabaseError::UnsupportedOperation(
            "Table alias on this execution path".into(),
        ));
    }
    let params = query
        .params()
        .unwrap_or_default()
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    rows(executor, &query.to_sql(), &params).await
}

/// Run a builder `SELECT` and return its first row.
pub(super) async fn select_first(
    executor: &dyn Executor,
    query: &crate::query::SelectQuery<'_>,
) -> Result<Option<Row>, DatabaseError> {
    if query.table_alias.is_some() {
        return Err(DatabaseError::UnsupportedOperation(
            "Table alias on this execution path".into(),
        ));
    }
    let params = query
        .params()
        .unwrap_or_default()
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    let sql = format!(
        "SELECT * FROM ({}) AS switchy_first LIMIT 1",
        query.to_sql()
    );
    Ok(rows(executor, &sql, &params).await?.into_iter().next())
}

/// `INSERT ... RETURNING *`.
pub(super) async fn insert(
    executor: &dyn Executor,
    table_name: &str,
    values: &Values<'_>,
) -> Result<Row, DatabaseError> {
    let table = crate::query::render_identifier(table_name, '"');
    let sql = if values.is_empty() {
        format!("INSERT INTO {table} DEFAULT VALUES RETURNING *")
    } else {
        let columns = values
            .iter()
            .map(|(name, _)| crate::query::render_identifier(name, '"'))
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "INSERT INTO {table} ({columns}) {} RETURNING *",
            build_values_clause(values)
        )
    };
    rows(executor, &sql, &value_params(values))
        .await?
        .into_iter()
        .next()
        .ok_or(DatabaseError::NoRow)
}

/// The `WHERE` clause selecting the affected rows, limited to `limit` rows
/// by their `rowid` when given.
fn target_rows(table: &str, filters: Option<&Filters>, limit: Option<usize>) -> String {
    limit.map_or_else(
        || build_where_clause(filters),
        |limit| {
            format!(
                "WHERE rowid IN (SELECT rowid FROM {table} {} LIMIT {limit})",
                build_where_clause(filters)
            )
        },
    )
}

/// `UPDATE ... RETURNING *`, limited to `limit` rows when given.
pub(super) async fn update(
    executor: &dyn Executor,
    table_name: &str,
    values: &Values<'_>,
    filters: Option<&Filters>,
    limit: Option<usize>,
) -> Result<Vec<Row>, DatabaseError> {
    let table = crate::query::render_identifier(table_name, '"');
    let sql = format!(
        "UPDATE {table} {} {} RETURNING *",
        build_set_clause(values),
        target_rows(&table, filters, limit)
    );
    let mut params = value_params(values);
    params.extend(bexprs_to_values_opt(filters).unwrap_or_default());
    rows(executor, &sql, &params).await
}

/// `DELETE ... RETURNING *`, limited to `limit` rows when given.
pub(super) async fn delete(
    executor: &dyn Executor,
    table_name: &str,
    filters: Option<&Filters>,
    limit: Option<usize>,
) -> Result<Vec<Row>, DatabaseError> {
    let table = crate::query::render_identifier(table_name, '"');
    let sql = format!(
        "DELETE FROM {table} {} RETURNING *",
        target_rows(&table, filters, limit)
    );
    rows(
        executor,
        &sql,
        &bexprs_to_values_opt(filters).unwrap_or_default(),
    )
    .await
}

/// `INSERT ... ON CONFLICT (unique) DO UPDATE ... RETURNING *`.
async fn upsert_on_conflict(
    executor: &dyn Executor,
    table_name: &str,
    values: &Values<'_>,
    unique: &[&str],
    filters: Option<&Filters>,
) -> Result<Vec<Row>, DatabaseError> {
    if unique.is_empty() {
        return Err(DatabaseError::InvalidQuery(
            "UPSERT conflict target cannot be empty".to_string(),
        ));
    }
    let table = crate::query::render_identifier(table_name, '"');
    let unique = unique
        .iter()
        .map(|name| crate::query::render_identifier(name, '"'))
        .collect::<Vec<_>>();
    let columns = values
        .iter()
        .map(|(name, _)| crate::query::render_identifier(name, '"'))
        .collect::<Vec<_>>();
    let set_clause = columns
        .iter()
        .map(|name| format!("{name}=excluded.{name}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "INSERT INTO {table} ({}) {} ON CONFLICT ({}) DO UPDATE SET {set_clause} {} RETURNING *",
        columns.join(", "),
        build_values_clause(values),
        unique.join(", "),
        build_where_clause(filters),
    );
    let mut params = value_params(values);
    params.extend(bexprs_to_values_opt(filters).unwrap_or_default());
    rows(executor, &sql, &params).await
}

/// Update the matching rows, or insert one when none match (or upsert on
/// the `unique` conflict target when given).
pub(super) async fn upsert(
    executor: &dyn Executor,
    table_name: &str,
    values: &Values<'_>,
    filters: Option<&Filters>,
    limit: Option<usize>,
    unique: Option<&[&str]>,
) -> Result<Vec<Row>, DatabaseError> {
    if let Some(unique) = unique {
        return upsert_on_conflict(executor, table_name, values, unique, filters).await;
    }
    let updated = update(executor, table_name, values, filters, limit).await?;
    if updated.is_empty() {
        Ok(vec![insert(executor, table_name, values).await?])
    } else {
        Ok(updated)
    }
}

/// Multi-row `INSERT ... ON CONFLICT DO UPDATE ... RETURNING *`.
pub(super) async fn upsert_multi(
    executor: &dyn Executor,
    table_name: &str,
    values: &[Vec<(&str, Box<dyn crate::query::Expression>)>],
    unique: Option<&[Box<dyn crate::query::Expression>]>,
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
            crate::query::ExpressionType::Identifier(identifier) => {
                Ok(crate::query::render_identifier(&identifier.value, '"'))
            }
            _ => Err(DatabaseError::InvalidQuery(
                "multi-row UPSERT conflict targets must be identifiers".to_owned(),
            )),
        })
        .collect::<Result<Vec<_>, _>>()?;
    if unique.is_empty() {
        return Err(DatabaseError::InvalidQuery(
            "multi-row UPSERT conflict target cannot be empty".to_owned(),
        ));
    }
    let table = crate::query::render_identifier(table_name, '"');
    let columns = first
        .iter()
        .map(|(name, _)| crate::query::render_identifier(name, '"'))
        .collect::<Vec<_>>();
    let values_clause = values
        .iter()
        .map(|row| format!("({})", build_values_props(row).join(", ")))
        .collect::<Vec<_>>()
        .join(", ");
    let set_clause = columns
        .iter()
        .map(|name| format!("{name}=excluded.{name}"))
        .collect::<Vec<_>>()
        .join(", ");
    let sql = format!(
        "INSERT INTO {table} ({}) VALUES {values_clause} ON CONFLICT ({}) DO UPDATE SET {set_clause} RETURNING *",
        columns.join(", "),
        unique.join(", "),
    );
    let params = values
        .iter()
        .flat_map(|row| value_params(row))
        .collect::<Vec<_>>();
    rows(executor, &sql, &params).await
}
