//! Observation-only scalar lookup rendering and schema validation.
use super::{Connection, DatabaseError, RusqliteDatabaseError, identifier, invalid};
use crate::schema::write_guard::{
    GuardEvent, GuardValue, ObservationLookupKey, UniqueScalarLookup, WriteObservation,
};

#[cfg(test)]
mod tests;

pub(super) const fn budget(lookup: &UniqueScalarLookup) -> usize {
    let key = match &lookup.key {
        ObservationLookupKey::NewColumn(key) | ObservationLookupKey::OldColumn(key) => key,
    };
    lookup
        .table
        .len()
        .saturating_add(lookup.key_column.len())
        .saturating_add(lookup.value_column.len())
        .saturating_add(key.len())
        .saturating_add(32)
}

pub(super) fn render(
    lookup: &UniqueScalarLookup,
    event: GuardEvent,
    status: bool,
) -> Result<String, DatabaseError> {
    if !(1..=1_048_576).contains(&lookup.max_bytes) {
        return Err(invalid());
    }
    let key = match &lookup.key {
        ObservationLookupKey::NewColumn(column) if event != GuardEvent::BeforeDelete => {
            format!("NEW.{}", identifier(column)?)
        }
        ObservationLookupKey::OldColumn(column) if event != GuardEvent::BeforeInsert => {
            format!("OLD.{}", identifier(column)?)
        }
        _ => return Err(invalid()),
    };
    let table = identifier(&lookup.table)?;
    let key_column = identifier(&lookup.key_column)?;
    let value_column = identifier(&lookup.value_column)?;
    // At most two matches; never choose a row when cardinality is ambiguous.
    let rows = format!(
        "SELECT {value_column} AS lookup_value FROM {table} WHERE {key_column} COLLATE BINARY = {key} COLLATE BINARY LIMIT 2"
    );
    let state = format!(
        "CASE WHEN COUNT(*) = 0 THEN 0 WHEN COUNT(*) != 1 THEN 2 WHEN MAX(typeof(lookup_value)) = 'null' THEN 3 WHEN MAX(typeof(lookup_value)) != 'text' THEN 5 WHEN MAX(length(CAST(lookup_value AS BLOB))) > {} THEN 4 ELSE 1 END",
        lookup.max_bytes
    );
    let selected = if status {
        state
    } else {
        format!("CASE WHEN ({state}) = 1 THEN MAX(lookup_value) ELSE NULL END")
    };
    let unsupported = if status { "5" } else { "NULL" };
    Ok(format!(
        "CASE WHEN typeof({key}) NOT IN ('null','text','integer') THEN {unsupported} ELSE (SELECT {selected} FROM ({rows})) END"
    ))
}

pub(super) fn verify(
    connection: &Connection,
    observation: &WriteObservation,
) -> Result<(), DatabaseError> {
    for mapping in &observation.columns {
        if let GuardValue::UniqueScalarLookup(lookup)
        | GuardValue::UniqueScalarLookupStatus(lookup) = &mapping.value
        {
            let source = identifier(&observation.table)?;
            let key = match &lookup.key {
                ObservationLookupKey::NewColumn(key) | ObservationLookupKey::OldColumn(key) => key,
            };
            identifier(key)?;
            let mut statement = connection
                .prepare(&format!("PRAGMA table_info({source})"))
                .map_err(RusqliteDatabaseError::Rusqlite)?;
            let columns = statement
                .query_map([], |row| row.get::<_, String>(1))
                .map_err(RusqliteDatabaseError::Rusqlite)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(RusqliteDatabaseError::Rusqlite)?;
            if !columns.contains(key) {
                return Err(invalid());
            }
            verify_selector(connection, lookup)?;
        }
    }
    Ok(())
}
fn verify_selector(
    connection: &Connection,
    lookup: &UniqueScalarLookup,
) -> Result<(), DatabaseError> {
    if !(1..=1_048_576).contains(&lookup.max_bytes) {
        return Err(invalid());
    }
    let table = identifier(&lookup.table)?;
    identifier(&lookup.key_column)?;
    identifier(&lookup.value_column)?;
    let mut statement = connection
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(RusqliteDatabaseError::Rusqlite)?;
    let columns = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(5)?,
            ))
        })
        .map_err(RusqliteDatabaseError::Rusqlite)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(RusqliteDatabaseError::Rusqlite)?;
    if !columns
        .iter()
        .any(|(name, _, _)| name == &lookup.value_column)
        || !columns
            .iter()
            .any(|(name, _, _)| name == &lookup.key_column)
    {
        return Err(invalid());
    }
    let integer_primary = columns.iter().filter(|(_, _, pk)| *pk != 0).count() == 1
        && columns.iter().any(|(name, kind, pk)| {
            name == &lookup.key_column && *pk == 1 && kind.eq_ignore_ascii_case("INTEGER")
        });
    if integer_primary {
        return Ok(());
    }
    let mut statement = connection
        .prepare(&format!("PRAGMA index_list({table})"))
        .map_err(RusqliteDatabaseError::Rusqlite)?;
    let indexes = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(1)?,
                row.get::<_, bool>(2)?,
                row.get::<_, bool>(4)?,
            ))
        })
        .map_err(RusqliteDatabaseError::Rusqlite)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(RusqliteDatabaseError::Rusqlite)?;
    for (name, unique, partial) in indexes {
        if !unique || partial {
            continue;
        }
        let mut statement = connection
            .prepare(&format!("PRAGMA index_xinfo({})", identifier(&name)?))
            .map_err(RusqliteDatabaseError::Rusqlite)?;
        let keys = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, bool>(5)?,
                ))
            })
            .map_err(RusqliteDatabaseError::Rusqlite)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(RusqliteDatabaseError::Rusqlite)?;
        let keys: Vec<_> = keys.into_iter().filter(|(_, _, key)| *key).collect();
        if keys.len() == 1
            && keys[0].0.as_deref() == Some(lookup.key_column.as_str())
            && keys[0].1 == "BINARY"
        {
            return Ok(());
        }
    }
    Err(invalid())
}
