use super::{Connection, RusqliteDatabaseError};
use crate::{
    DatabaseError,
    schema::write_guard::{
        GuardEvent, GuardPredicate, GuardScalar, GuardValue, ObservationEvent, WriteGuard,
        WriteObservation,
    },
};
use rusqlite::OptionalExtension as _;
use std::collections::BTreeSet;
use std::fmt::Write as _;

fn observation_definition(observation: &WriteObservation) -> Result<String, DatabaseError> {
    if observation
        .table
        .eq_ignore_ascii_case(&observation.target_table)
        || observation.columns.is_empty()
        || observation.columns.len() > 128
    {
        return Err(invalid());
    }
    let (event, row_event) = match observation.event {
        ObservationEvent::AfterInsert => ("INSERT", GuardEvent::BeforeInsert),
        ObservationEvent::AfterUpdate => ("UPDATE", GuardEvent::BeforeUpdate),
        ObservationEvent::AfterDelete => ("DELETE", GuardEvent::BeforeDelete),
    };
    // Reuse the same finite predicate budget, without changing any guard definition.
    validate_budget(&WriteGuard {
        name: observation.name.clone(),
        table: observation.table.clone(),
        event: row_event,
        reject_when: observation.when.clone(),
        violation: "observation".into(),
    })?;
    let scope = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let mut columns = Vec::new();
    let mut values = Vec::new();
    let mut bytes =
        observation.name.len() + observation.table.len() + observation.target_table.len();
    for mapping in &observation.columns {
        bytes = bytes
            .checked_add(mapping.column.len())
            .and_then(|n| n.checked_add(value_bytes(&mapping.value)))
            .ok_or_else(invalid)?;
        if bytes > 262_144 || !seen.insert(mapping.column.to_ascii_lowercase()) {
            return Err(invalid());
        }
        columns.push(identifier(&mapping.column)?);
        values.push(value(&mapping.value, row_event, &scope)?);
    }
    let sql = format!(
        "CREATE TRIGGER {} AFTER {event} ON {} WHEN {} BEGIN INSERT INTO {} ({}) VALUES ({}); END",
        identifier(&observation.name)?,
        identifier(&observation.table)?,
        predicate(&observation.when, row_event, &scope, 0)?,
        identifier(&observation.target_table)?,
        columns.join(","),
        values.join(",")
    );
    if sql.len() > 524_288 {
        return Err(invalid());
    }
    Ok(sql)
}

/// Install a finite after-transition observation in an owned transaction.
/// # Errors
/// Rejects autocommit, invalid mappings, conflicting schema and database failures.
pub fn install_write_observation_on_connection(
    connection: &Connection,
    observation: &WriteObservation,
) -> Result<(), DatabaseError> {
    if connection.is_autocommit() {
        return Err(DatabaseError::InvalidSchema(
            "observation installation requires an owned transaction".into(),
        ));
    }
    let sql = observation_definition(observation)?;
    let stored: Option<String> = connection
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE type='trigger' AND name=?1",
            [&observation.name],
            |row| row.get(0),
        )
        .optional()
        .map_err(RusqliteDatabaseError::Rusqlite)?;
    if let Some(stored) = stored {
        return if stored == sql {
            Ok(())
        } else {
            Err(DatabaseError::InvalidSchema(
                "conflicting write observation definition".into(),
            ))
        };
    }
    connection
        .execute(&sql, [])
        .map_err(RusqliteDatabaseError::Rusqlite)?;
    Ok(())
}

/// Verify an exact observation without mutation or cached validity.
/// # Errors
/// Rejects invalid definitions and database failures.
pub fn verify_write_observation_on_connection(
    connection: &Connection,
    observation: &WriteObservation,
) -> Result<bool, DatabaseError> {
    let sql = observation_definition(observation)?;
    let stored: Option<String> = connection
        .query_row(
            "SELECT sql FROM sqlite_schema WHERE type='trigger' AND name=?1",
            [&observation.name],
            |row| row.get(0),
        )
        .optional()
        .map_err(RusqliteDatabaseError::Rusqlite)?;
    Ok(stored.as_deref() == Some(sql.as_str()))
}

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
    operand: &GuardValue,
    event: GuardEvent,
    scope: &BTreeSet<String>,
) -> Result<String, DatabaseError> {
    Ok(match operand {
        GuardValue::NewColumn(column) if event != GuardEvent::BeforeDelete => {
            format!("NEW.{}", identifier(column)?)
        }
        GuardValue::OldColumn(column) if event != GuardEvent::BeforeInsert => {
            format!("OLD.{}", identifier(column)?)
        }
        GuardValue::Column { relation, column } if scope.contains(relation) => {
            format!("{}.{}", identifier(relation)?, identifier(column)?)
        }
        GuardValue::BoundedText {
            value: inner,
            max_bytes,
        }
        | GuardValue::BoundedTextStatus {
            value: inner,
            max_bytes,
        } => {
            if !is_leaf(inner) {
                return Err(invalid());
            }
            let source = value(inner, event, scope)?;
            let retained = format!(
                "typeof({source}) = 'text' AND length(CAST({source} AS BLOB)) <= {max_bytes}"
            );
            if matches!(operand, GuardValue::BoundedTextStatus { .. }) {
                format!(
                    "(CASE WHEN {source} IS NULL THEN 0 WHEN {retained} THEN 1 WHEN typeof({source}) = 'text' THEN 2 ELSE 3 END)"
                )
            } else {
                format!("(CASE WHEN {retained} THEN {source} ELSE NULL END)")
            }
        }
        GuardValue::IntegerOnly { value: inner } | GuardValue::IntegerStatus { value: inner } => {
            if !is_leaf(inner) {
                return Err(invalid());
            }
            let source = value(inner, event, scope)?;
            if matches!(operand, GuardValue::IntegerStatus { .. }) {
                format!(
                    "(CASE WHEN {source} IS NULL THEN 0 WHEN typeof({source}) = 'integer' THEN 1 ELSE 2 END)"
                )
            } else {
                format!("(CASE WHEN typeof({source}) = 'integer' THEN {source} ELSE NULL END)")
            }
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
        GuardPredicate::Not(inner) => format!(
            "(NOT COALESCE({}, 0))",
            predicate(inner, event, scope, depth + 1)?
        ),
        GuardPredicate::IntegerSuccessor { previous, next } => {
            let previous = val(previous)?;
            let next = val(next)?;
            format!(
                "(CASE WHEN typeof({previous}) = 'integer' AND typeof({next}) = 'integer' AND {previous} < 9223372036854775807 THEN {next} = {previous} + 1 ELSE 0 END)"
            )
        }
        GuardPredicate::Equal(a, b) => format!("({} = {})", val(a)?, val(b)?),
        GuardPredicate::NotEqual(a, b) => format!("({} <> {})", val(a)?, val(b)?),
        GuardPredicate::IsNull(v) => format!("({} IS NULL)", val(v)?),
        GuardPredicate::ByteLengthAtMost(v, limit) => {
            let source = val(v)?;
            format!(
                "(CASE WHEN typeof({source}) IN ('text','blob') THEN length(CAST({source} AS BLOB)) <= {limit} ELSE 0 END)"
            )
        }
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
        GuardPredicate::Exists(relation) | GuardPredicate::NotExists(relation) => {
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
            let prefix = if matches!(p, GuardPredicate::NotExists(_)) {
                "NOT "
            } else {
                ""
            };
            format!(
                "{prefix}EXISTS (SELECT 1 FROM {from} WHERE {})",
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
            GuardPredicate::Not(inner) => pending.push((inner, depth + 1)),
            GuardPredicate::Exists(relation) | GuardPredicate::NotExists(relation) => {
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
            GuardPredicate::Equal(a, b)
            | GuardPredicate::NotEqual(a, b)
            | GuardPredicate::IntegerSuccessor {
                previous: a,
                next: b,
            } => {
                bytes += value_bytes(a) + value_bytes(b);
            }
            GuardPredicate::IsNull(v)
            | GuardPredicate::IsNotNull(v)
            | GuardPredicate::ByteLengthAtMost(v, _) => bytes += value_bytes(v),
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
const fn is_leaf(value: &GuardValue) -> bool {
    matches!(
        value,
        GuardValue::NewColumn(_)
            | GuardValue::OldColumn(_)
            | GuardValue::Column { .. }
            | GuardValue::Constant(_)
    )
}

fn value_bytes(value: &GuardValue) -> usize {
    match value {
        GuardValue::NewColumn(v)
        | GuardValue::OldColumn(v)
        | GuardValue::Constant(GuardScalar::Text(v)) => v.len(),
        GuardValue::Column { relation, column } => relation.len() + column.len(),
        GuardValue::BoundedText { value, .. }
        | GuardValue::BoundedTextStatus { value, .. }
        | GuardValue::IntegerOnly { value }
        | GuardValue::IntegerStatus { value } => {
            // Nested mappings are invalid; never recursively walk an untrusted value tree.
            if is_leaf(value) {
                24 + value_bytes(value)
            } else {
                262_145
            }
        }
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
        GuardEvent::BeforeDelete => "DELETE",
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
