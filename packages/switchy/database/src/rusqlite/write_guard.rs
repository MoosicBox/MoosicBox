use super::{Connection, RusqliteDatabaseError};
use crate::{
    DatabaseError,
    schema::write_guard::{GuardEvent, GuardPredicate, GuardScalar, GuardValue, WriteGuard},
};
use rusqlite::OptionalExtension as _;
use std::collections::BTreeSet;
use std::fmt::Write as _;

fn invalid() -> DatabaseError {
    DatabaseError::InvalidSchema("invalid typed write guard".into())
}
fn identifier(value: &str) -> Result<String, DatabaseError> {
    if value.is_empty() || value.contains('\0') {
        return Err(invalid());
    }
    Ok(format!("\"{}\"", value.replace('"', "\"\"")))
}
fn scalar(value: &GuardScalar) -> Result<String, DatabaseError> {
    Ok(match value {
        GuardScalar::Null => "NULL".into(),
        GuardScalar::Integer(value) => value.to_string(),
        GuardScalar::Bool(value) => i32::from(*value).to_string(),
        GuardScalar::Text(value) => {
            if value.contains('\0') {
                return Err(invalid());
            }
            format!("'{}'", value.replace('\'', "''"))
        }
    })
}
fn value(
    value: &GuardValue,
    event: GuardEvent,
    scope: &BTreeSet<String>,
) -> Result<String, DatabaseError> {
    Ok(match value {
        GuardValue::NewColumn(column) => format!("NEW.{}", identifier(column)?),
        GuardValue::OldColumn(column) if event == GuardEvent::BeforeUpdate => {
            format!("OLD.{}", identifier(column)?)
        }
        GuardValue::Column { relation, column } if scope.contains(relation) => {
            format!("{}.{}", identifier(relation)?, identifier(column)?)
        }
        GuardValue::Constant(constant) => scalar(constant)?,
        _ => return Err(invalid()),
    })
}
fn predicate(
    p: &GuardPredicate,
    event: GuardEvent,
    scope: &BTreeSet<String>,
    depth: usize,
) -> Result<String, DatabaseError> {
    if depth > 64 {
        return Err(invalid());
    }
    let val = |v| value(v, event, scope);
    Ok(match p {
        GuardPredicate::Equal(a, b) => format!("({} = {})", val(a)?, val(b)?),
        GuardPredicate::NotEqual(a, b) => format!("({} <> {})", val(a)?, val(b)?),
        GuardPredicate::IsNull(v) => format!("({} IS NULL)", val(v)?),
        GuardPredicate::IsNotNull(v) => format!("({} IS NOT NULL)", val(v)?),
        GuardPredicate::InValues(v, values) => {
            if values.is_empty() {
                return Err(invalid());
            }
            format!(
                "({} IN ({}))",
                val(v)?,
                values
                    .iter()
                    .map(scalar)
                    .collect::<Result<Vec<_>, _>>()?
                    .join(",")
            )
        }
        GuardPredicate::All(items) | GuardPredicate::Any(items) => {
            if items.is_empty() {
                return Err(invalid());
            }
            let op = if matches!(p, GuardPredicate::All(_)) {
                " AND "
            } else {
                " OR "
            };
            format!(
                "({})",
                items
                    .iter()
                    .map(|p| predicate(p, event, scope, depth + 1))
                    .collect::<Result<Vec<_>, _>>()?
                    .join(op)
            )
        }
        GuardPredicate::Exists(relation) => {
            let mut scope = scope.clone();
            bind(&mut scope, &relation.alias)?;
            let mut from = format!(
                "{} AS {}",
                identifier(&relation.table)?,
                identifier(&relation.alias)?
            );
            for join in &relation.joins {
                bind(&mut scope, &join.alias)?;
                if !matches!(join.left, GuardValue::Column { .. })
                    || !matches!(join.right, GuardValue::Column { .. })
                {
                    return Err(invalid());
                }
                write!(
                    from,
                    " INNER JOIN {} AS {} ON {} = {}",
                    identifier(&join.table)?,
                    identifier(&join.alias)?,
                    value(&join.left, event, &scope)?,
                    value(&join.right, event, &scope)?
                )
                .map_err(|_| invalid())?;
            }
            format!(
                "EXISTS (SELECT 1 FROM {from} WHERE {})",
                predicate(&relation.predicate, event, &scope, depth + 1)?
            )
        }
    })
}
fn bind(scope: &mut BTreeSet<String>, alias: &str) -> Result<(), DatabaseError> {
    if alias.eq_ignore_ascii_case("new")
        || alias.eq_ignore_ascii_case("old")
        || scope.iter().any(|v| v.eq_ignore_ascii_case(alias))
    {
        return Err(invalid());
    }
    identifier(alias)?;
    scope.insert(alias.into());
    Ok(())
}
fn validate_budget(guard: &WriteGuard) -> Result<(), DatabaseError> {
    let mut remaining = 4096_usize;
    let mut bytes = guard.name.len() + guard.table.len() + guard.violation.len();
    let mut pending = vec![(&guard.reject_when, 0_usize)];
    while let Some((predicate, depth)) = pending.pop() {
        if remaining == 0 || depth > 64 || bytes > 262_144 {
            return Err(invalid());
        }
        remaining -= 1;
        match predicate {
            GuardPredicate::All(items) | GuardPredicate::Any(items) => {
                if items.len() > remaining {
                    return Err(invalid());
                }
                pending.extend(items.iter().map(|item| (item, depth + 1)));
            }
            GuardPredicate::Exists(relation) => {
                if relation.joins.len() > 32 {
                    return Err(invalid());
                }
                bytes += relation.table.len() + relation.alias.len();
                for join in &relation.joins {
                    bytes += join.table.len()
                        + join.alias.len()
                        + value_bytes(&join.left)
                        + value_bytes(&join.right);
                }
                pending.push((&relation.predicate, depth + 1));
            }
            GuardPredicate::Equal(a, b) | GuardPredicate::NotEqual(a, b) => {
                bytes += value_bytes(a) + value_bytes(b);
            }
            GuardPredicate::IsNull(v) | GuardPredicate::IsNotNull(v) => bytes += value_bytes(v),
            GuardPredicate::InValues(v, values) => {
                if values.len() > remaining {
                    return Err(invalid());
                }
                remaining -= values.len();
                bytes += value_bytes(v)
                    + values
                        .iter()
                        .map(|v| match v {
                            GuardScalar::Text(v) => v.len(),
                            _ => 24,
                        })
                        .sum::<usize>();
            }
        }
    }
    if bytes > 262_144 {
        return Err(invalid());
    }
    Ok(())
}
const fn value_bytes(value: &GuardValue) -> usize {
    match value {
        GuardValue::NewColumn(v)
        | GuardValue::OldColumn(v)
        | GuardValue::Constant(GuardScalar::Text(v)) => v.len(),
        GuardValue::Column { relation, column } => relation.len() + column.len(),
        GuardValue::Constant(_) => 24,
    }
}

fn definition(guard: &WriteGuard) -> Result<String, DatabaseError> {
    validate_budget(guard)?;
    if guard.violation.is_empty()
        || !guard
            .violation
            .bytes()
            .all(|v| v.is_ascii_alphanumeric() || v == b'_')
    {
        return Err(invalid());
    }
    let event = match guard.event {
        GuardEvent::BeforeInsert => "INSERT",
        GuardEvent::BeforeUpdate => "UPDATE",
    };
    Ok(format!(
        "CREATE TRIGGER {} BEFORE {event} ON {} WHEN {} BEGIN SELECT RAISE(ABORT, 'switchy_guard_v1:{}'); END",
        identifier(&guard.name)?,
        identifier(&guard.table)?,
        predicate(&guard.reject_when, guard.event, &BTreeSet::new(), 0)?,
        guard.violation
    ))
}
/// Verify exact guard definitions in bounded batches on one existing connection.
///
/// No validation result is cached: committed or transactional DDL is observed anew.
/// # Errors
/// Rejects invalid definitions, conflicting duplicate names and database failures.
pub fn verify_write_guards_on_connection(
    connection: &Connection,
    guards: &[WriteGuard],
) -> Result<bool, DatabaseError> {
    let mut expected = std::collections::BTreeMap::new();
    for guard in guards {
        let sql = definition(guard)?;
        if expected
            .insert(guard.name.as_str(), sql.clone())
            .is_some_and(|previous| previous != sql)
        {
            return Err(invalid());
        }
    }
    let entries = expected.into_iter().collect::<Vec<_>>();
    for batch in entries.chunks(128) {
        let placeholders = std::iter::repeat_n("?", batch.len())
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            "SELECT name,sql FROM sqlite_schema WHERE type='trigger' AND name IN ({placeholders})"
        );
        let mut statement = connection
            .prepare_cached(&sql)
            .map_err(RusqliteDatabaseError::Rusqlite)?;
        let rows = statement
            .query_map(
                rusqlite::params_from_iter(batch.iter().map(|(name, _)| *name)),
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .map_err(RusqliteDatabaseError::Rusqlite)?;
        let actual = rows
            .collect::<Result<std::collections::BTreeMap<_, _>, _>>()
            .map_err(RusqliteDatabaseError::Rusqlite)?;
        if actual.len() != batch.len()
            || batch
                .iter()
                .any(|(name, definition)| actual.get(*name) != Some(definition))
        {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Verify an exact versioned guard definition without modifying the connection.
/// # Errors
/// Returns an error for invalid guard definitions or database failures.
pub fn verify_write_guard_on_connection(
    connection: &Connection,
    guard: &WriteGuard,
) -> Result<bool, DatabaseError> {
    let expected = definition(guard)?;
    let stored: Option<String> = connection
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE type='trigger' AND name=?1",
            [&guard.name],
            |row| row.get(0),
        )
        .optional()
        .map_err(RusqliteDatabaseError::Rusqlite)?;
    Ok(stored.as_deref() == Some(expected.as_str()))
}
/// Install a persistent rejection guard on the caller's active transaction.
/// # Errors
/// Rejects autocommit, invalid definitions, conflicting guard names and database errors.
pub fn install_write_guard_on_connection(
    connection: &Connection,
    guard: &WriteGuard,
) -> Result<(), DatabaseError> {
    if connection.is_autocommit() {
        return Err(DatabaseError::InvalidSchema(
            "guard installation requires an owned transaction".into(),
        ));
    }
    let sql = definition(guard)?;
    let stored: Option<String> = connection
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE type='trigger' AND name=?1",
            [&guard.name],
            |row| row.get(0),
        )
        .optional()
        .map_err(RusqliteDatabaseError::Rusqlite)?;
    if let Some(stored) = stored {
        return if stored == sql {
            Ok(())
        } else {
            Err(DatabaseError::InvalidSchema(
                "conflicting write guard definition".into(),
            ))
        };
    }
    connection
        .execute(&sql, [])
        .map_err(RusqliteDatabaseError::Rusqlite)?;
    Ok(())
}
