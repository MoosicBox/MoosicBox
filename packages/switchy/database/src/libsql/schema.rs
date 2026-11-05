//! Schema statements (`CREATE`/`DROP TABLE`, indexes, `ALTER TABLE`) for
//! the `libSQL` backend, rendered in the `SQLite` dialect and run on an
//! [`Executor`].
//!
//! `ALTER TABLE` supports what `SQLite` does natively: adding, dropping and
//! renaming columns. Changing a column's type, nullability or default needs
//! the table to be rebuilt, which this backend refuses
//! ([`DatabaseError::UnsupportedOperation`]) rather than doing over several
//! round trips.

use std::fmt::Write as _;

use crate::schema::{AlterOperation, DataType};
use crate::{DatabaseError, DatabaseValue, sqlite_common::format_sqlite_interval};

use super::Executor;

fn identifier(name: &str) -> String {
    crate::query::render_identifier(name, '"')
}

/// Always-quoted identifier, for names read back from the catalog.
#[cfg(feature = "cascade")]
fn quoted(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// The `SQLite` type a [`DataType`] is stored as.
fn type_sql(data_type: &DataType) -> String {
    match data_type {
        DataType::VarChar(size) => format!("VARCHAR({size})"),
        DataType::Char(size) => format!("CHAR({size})"),
        DataType::Text
        | DataType::Date
        | DataType::Time
        | DataType::DateTime
        | DataType::Timestamp
        | DataType::Json
        | DataType::Jsonb
        | DataType::Uuid
        | DataType::Xml
        | DataType::Array(..)
        | DataType::Inet
        | DataType::MacAddr
        | DataType::Decimal(..) => "TEXT".to_string(),
        DataType::Bool
        | DataType::TinyInt
        | DataType::SmallInt
        | DataType::Int
        | DataType::BigInt
        | DataType::Serial
        | DataType::BigSerial => "INTEGER".to_string(),
        DataType::Real | DataType::Double | DataType::Money => "REAL".to_string(),
        DataType::Blob | DataType::Binary(_) => "BLOB".to_string(),
        DataType::Custom(name) => name.clone(),
    }
}

/// A column default as an `SQLite` literal or expression.
fn default_sql(value: &DatabaseValue) -> String {
    if crate::query::Expression::is_null(value) {
        return "NULL".to_string();
    }
    let quote = |text: &str| format!("'{}'", text.replace('\'', "''"));
    match value {
        DatabaseValue::String(text) | DatabaseValue::StringOpt(Some(text)) => quote(text),
        DatabaseValue::Bool(value) | DatabaseValue::BoolOpt(Some(value)) => {
            if *value { "1" } else { "0" }.to_string()
        }
        DatabaseValue::Int8(value) | DatabaseValue::Int8Opt(Some(value)) => value.to_string(),
        DatabaseValue::Int16(value) | DatabaseValue::Int16Opt(Some(value)) => value.to_string(),
        DatabaseValue::Int32(value) | DatabaseValue::Int32Opt(Some(value)) => value.to_string(),
        DatabaseValue::Int64(value) | DatabaseValue::Int64Opt(Some(value)) => value.to_string(),
        DatabaseValue::UInt8(value) | DatabaseValue::UInt8Opt(Some(value)) => value.to_string(),
        DatabaseValue::UInt16(value) | DatabaseValue::UInt16Opt(Some(value)) => value.to_string(),
        DatabaseValue::UInt32(value) | DatabaseValue::UInt32Opt(Some(value)) => value.to_string(),
        DatabaseValue::UInt64(value) | DatabaseValue::UInt64Opt(Some(value)) => value.to_string(),
        DatabaseValue::Real32(value) | DatabaseValue::Real32Opt(Some(value)) => value.to_string(),
        DatabaseValue::Real64(value) | DatabaseValue::Real64Opt(Some(value)) => value.to_string(),
        #[cfg(feature = "decimal")]
        DatabaseValue::Decimal(value) | DatabaseValue::DecimalOpt(Some(value)) => {
            quote(&value.to_string())
        }
        #[cfg(feature = "uuid")]
        DatabaseValue::Uuid(value) | DatabaseValue::UuidOpt(Some(value)) => {
            quote(&value.to_string())
        }
        DatabaseValue::Now => "(strftime('%Y-%m-%dT%H:%M:%f', 'now'))".to_string(),
        DatabaseValue::NowPlus(interval) => format!(
            "(strftime('%Y-%m-%dT%H:%M:%f', datetime('now', {})))",
            format_sqlite_interval(interval)
                .iter()
                .map(|modifier| quote(modifier))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        DatabaseValue::DateTime(value) => quote(&value.and_utc().to_rfc3339()),
        other => crate::sqlite_common::sqlite_literal_for_value(other)
            .unwrap_or_else(|| "NULL".to_string()),
    }
}

/// `CREATE TABLE` for `statement`.
///
/// # Errors
///
/// * [`DatabaseError::InvalidSchema`] for a table without columns, an
///   auto-increment column that is not the primary key, or a constraint that
///   can't be rendered
pub(super) fn create_table_sql(
    statement: &crate::schema::CreateTableStatement<'_>,
) -> Result<String, DatabaseError> {
    if statement.columns.is_empty() {
        return Err(DatabaseError::InvalidSchema(format!(
            "table '{}' has no columns",
            statement.table_name
        )));
    }
    let mut sql = String::from("CREATE TABLE ");
    if statement.if_not_exists {
        sql.push_str("IF NOT EXISTS ");
    }
    sql.push_str(&identifier(statement.table_name));
    sql.push_str(" (");
    let mut pk_inline = false;
    for (index, column) in statement.columns.iter().enumerate() {
        let is_pk = statement.primary_key == Some(column.name.as_str());
        if column.auto_increment && !is_pk {
            return Err(DatabaseError::InvalidSchema(format!(
                "Column '{}' must be the primary key to enable auto increment",
                column.name
            )));
        }
        if index > 0 {
            sql.push_str(", ");
        }
        sql.push_str(&identifier(&column.name));
        sql.push(' ');
        sql.push_str(&type_sql(&column.data_type));
        if !column.nullable {
            sql.push_str(" NOT NULL");
        }
        if column.auto_increment {
            sql.push_str(" PRIMARY KEY AUTOINCREMENT");
            pk_inline = true;
        }
        if let Some(default) = &column.default {
            write!(sql, " DEFAULT {}", default_sql(default)).ok();
        }
    }
    if let Some(primary_key) = statement.primary_key
        && !pk_inline
    {
        write!(sql, ", PRIMARY KEY ({})", identifier(primary_key)).ok();
    }
    for constraint in &statement.constraints {
        sql.push_str(", ");
        sql.push_str(&constraint.render('"')?);
    }
    for (source, target) in &statement.foreign_keys {
        write!(
            sql,
            ", FOREIGN KEY ({}) REFERENCES {}",
            identifier(source),
            identifier(target)
        )
        .ok();
    }
    sql.push(')');
    Ok(sql)
}

/// Tables with a foreign key referencing `table` (one level).
///
/// # Errors
///
/// * If the catalog query fails
#[cfg(feature = "cascade")]
pub(super) async fn direct_dependents(
    executor: &dyn Executor,
    table: &str,
) -> Result<std::collections::BTreeSet<String>, DatabaseError> {
    let result = executor
        .run(
            "SELECT DISTINCT m.name AS name FROM sqlite_master m, pragma_foreign_key_list(m.name) f \
             WHERE m.type = 'table' AND f.\"table\" = ? COLLATE NOCASE AND m.name != ? COLLATE NOCASE",
            &[
                DatabaseValue::String(table.to_string()),
                DatabaseValue::String(table.to_string()),
            ],
            true,
        )
        .await?;
    Ok(result
        .rows
        .into_iter()
        .filter_map(|row| match row.into_iter().next() {
            Some(DatabaseValue::String(name)) => Some(name),
            _ => None,
        })
        .collect())
}

/// `table` and every table depending on it, dependents first.
///
/// # Errors
///
/// * If a catalog query fails
#[cfg(feature = "cascade")]
pub(super) async fn drop_order(
    executor: &dyn Executor,
    table: &str,
) -> Result<Vec<String>, DatabaseError> {
    // Depth-first, so each table comes after everything that references it.
    let mut order = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut stack = vec![(table.to_string(), false)];
    while let Some((name, expanded)) = stack.pop() {
        if expanded {
            order.push(name);
            continue;
        }
        if !seen.insert(name.to_lowercase()) {
            continue;
        }
        stack.push((name.clone(), true));
        for dependent in direct_dependents(executor, &name).await? {
            if !seen.contains(&dependent.to_lowercase()) {
                stack.push((dependent, false));
            }
        }
    }
    Ok(order)
}

/// Run `DROP TABLE`, dropping dependents first for `CASCADE` and refusing
/// for `RESTRICT` when other tables depend on it.
///
/// # Errors
///
/// * [`DatabaseError::InvalidQuery`] for `RESTRICT` with dependents
/// * If a statement fails
pub(super) async fn drop_table(
    executor: &dyn Executor,
    statement: &crate::schema::DropTableStatement<'_>,
) -> Result<(), DatabaseError> {
    let if_exists = if statement.if_exists {
        "IF EXISTS "
    } else {
        ""
    };
    #[cfg(feature = "cascade")]
    {
        use crate::schema::DropBehavior;
        match statement.behavior {
            DropBehavior::Cascade => {
                for table in drop_order(executor, statement.table_name).await? {
                    executor
                        .run(
                            &format!("DROP TABLE {if_exists}{}", quoted(&table)),
                            &[],
                            false,
                        )
                        .await?;
                }
                return Ok(());
            }
            DropBehavior::Restrict => {
                if !direct_dependents(executor, statement.table_name)
                    .await?
                    .is_empty()
                {
                    return Err(DatabaseError::InvalidQuery(format!(
                        "Cannot drop table '{}' because other tables depend on it",
                        statement.table_name
                    )));
                }
            }
            DropBehavior::Default => {}
        }
    }
    executor
        .run(
            &format!("DROP TABLE {if_exists}{}", identifier(statement.table_name)),
            &[],
            false,
        )
        .await?;
    Ok(())
}

/// `CREATE INDEX` for `statement`.
///
/// # Errors
///
/// * [`DatabaseError::InvalidSchema`] for an index without columns or a
///   predicate that can't be rendered
pub(super) fn create_index_sql(
    statement: &crate::schema::CreateIndexStatement<'_>,
) -> Result<String, DatabaseError> {
    let mut columns = statement
        .columns
        .iter()
        .map(|column| identifier(column))
        .collect::<Vec<_>>();
    columns.extend(statement.ordered_columns.iter().map(|(column, direction)| {
        format!(
            "{} {}",
            identifier(column),
            match direction {
                crate::query::SortDirection::Asc => "ASC",
                crate::query::SortDirection::Desc => "DESC",
            }
        )
    }));
    if columns.is_empty() {
        return Err(DatabaseError::InvalidSchema(format!(
            "index '{}' has no columns",
            statement.index_name
        )));
    }
    let mut sql = format!(
        "CREATE {}INDEX {}{} ON {} ({})",
        if statement.unique { "UNIQUE " } else { "" },
        if statement.if_not_exists {
            "IF NOT EXISTS "
        } else {
            ""
        },
        identifier(statement.index_name),
        identifier(statement.table_name),
        columns.join(", ")
    );
    if let Some(predicate) = &statement.predicate {
        write!(sql, " WHERE {}", predicate.render('"')?).ok();
    }
    Ok(sql)
}

/// `DROP INDEX` for `statement`.
#[must_use]
pub(super) fn drop_index_sql(statement: &crate::schema::DropIndexStatement<'_>) -> String {
    format!(
        "DROP INDEX {}{}",
        if statement.if_exists {
            "IF EXISTS "
        } else {
            ""
        },
        identifier(statement.index_name)
    )
}

/// The statements `ALTER TABLE` takes, one per operation.
///
/// # Errors
///
/// * [`DatabaseError::UnsupportedOperation`] for `ModifyColumn` (it needs the
///   table rebuilt) and for a `CHECK` on an added column
/// * [`DatabaseError::InvalidSchema`] if a `CHECK` can't be rendered
pub(super) fn alter_table_sql(
    statement: &crate::schema::AlterTableStatement<'_>,
) -> Result<Vec<String>, DatabaseError> {
    let table = identifier(statement.table_name);
    statement
        .operations
        .iter()
        .map(|operation| match operation {
            AlterOperation::AddColumn {
                name,
                data_type,
                nullable,
                default,
            } => {
                let mut sql = format!(
                    "ALTER TABLE {table} ADD COLUMN {} {}",
                    identifier(name),
                    type_sql(data_type)
                );
                if !nullable {
                    sql.push_str(" NOT NULL");
                }
                if let Some(default) = default {
                    write!(sql, " DEFAULT {}", default_sql(default)).ok();
                }
                if let Some(check) = statement.column_checks.get(name) {
                    write!(sql, " CHECK ({})", check.render('"')?).ok();
                }
                Ok(sql)
            }
            AlterOperation::DropColumn { name, .. } => Ok(format!(
                "ALTER TABLE {table} DROP COLUMN {}",
                identifier(name)
            )),
            AlterOperation::RenameColumn { old_name, new_name } => Ok(format!(
                "ALTER TABLE {table} RENAME COLUMN {} TO {}",
                identifier(old_name),
                identifier(new_name)
            )),
            AlterOperation::ModifyColumn { name, .. } => {
                Err(DatabaseError::UnsupportedOperation(format!(
                    "changing column '{name}' needs the table rebuilt, which the libSQL backend \
                     doesn't do; recreate the table instead"
                )))
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;

    use super::*;
    use crate::schema::Column;

    #[test]
    fn create_table_quotes_defaults_and_keys() {
        let statement = crate::schema::create_table("albums")
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "title".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::VarChar(200),
                default: Some(DatabaseValue::String("it's".to_string())),
            })
            .primary_key("id");
        let sql = create_table_sql(&statement).unwrap();
        assert!(sql.contains("PRIMARY KEY AUTOINCREMENT"), "{sql}");
        assert!(sql.contains("DEFAULT 'it''s'"), "{sql}");
        assert!(!sql.contains(", PRIMARY KEY ("), "{sql}");
    }

    #[test]
    fn alter_table_refuses_rebuilds() {
        let statement = crate::schema::alter_table("albums")
            .add_column("year".to_string(), DataType::Int, true, None)
            .rename_column("title".to_string(), "name".to_string());
        assert_eq!(alter_table_sql(&statement).unwrap().len(), 2);
        let statement = crate::schema::alter_table("albums").modify_column(
            "year".to_string(),
            DataType::Text,
            None,
            None,
        );
        assert!(matches!(
            alter_table_sql(&statement),
            Err(DatabaseError::UnsupportedOperation(_))
        ));
    }
}
