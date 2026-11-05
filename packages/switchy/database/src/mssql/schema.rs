//! Schema statements (`CREATE`/`DROP TABLE`, indexes, `ALTER TABLE`) for
//! the SQL Server backend, and the statements that run them on an
//! [`Executor`].
//!
//! Auto-increment columns are `IDENTITY(1, 1)`. `IF NOT EXISTS` is written
//! as an `IF OBJECT_ID(...) IS NULL` guard (SQL Server has no `CREATE ...
//! IF NOT EXISTS`). Defaults are default constraints, so changing or
//! dropping a column first drops the default constraint the catalog names
//! for it; renaming a column runs `sp_rename` with the names bound as
//! parameters.

use std::fmt::Write as _;

use crate::schema::{AlterOperation, DataType};
use crate::{DatabaseError, DatabaseValue};

use super::{Executor, Mode};

fn identifier(name: &str) -> String {
    crate::query::render_identifier(name, '"')
}

/// `name` in brackets, for names read back from the catalog.
fn bracketed(name: &str) -> String {
    format!("[{}]", name.replace(']', "]]"))
}

/// `text` as an `N'...'` literal.
fn literal(text: &str) -> String {
    format!("N'{}'", text.replace('\'', "''"))
}

/// The SQL Server type a [`DataType`] is stored as.
fn type_sql(data_type: &DataType) -> Result<String, DatabaseError> {
    Ok(match data_type {
        DataType::VarChar(size) => format!("NVARCHAR({size})"),
        DataType::Char(size) => format!("NCHAR({size})"),
        DataType::Text | DataType::Json | DataType::Jsonb => "NVARCHAR(MAX)".to_string(),
        // tinyint is unsigned (0-255) in SQL Server; DataType::TinyInt is signed.
        DataType::TinyInt | DataType::SmallInt => "SMALLINT".to_string(),
        DataType::Int | DataType::Serial => "INT".to_string(),
        DataType::BigInt | DataType::BigSerial => "BIGINT".to_string(),
        DataType::Real => "REAL".to_string(),
        DataType::Double => "FLOAT".to_string(),
        DataType::Decimal(precision, scale) => format!("DECIMAL({precision}, {scale})"),
        DataType::Money => "MONEY".to_string(),
        DataType::Bool => "BIT".to_string(),
        DataType::Date => "DATE".to_string(),
        DataType::Time => "TIME".to_string(),
        // `timestamp` is SQL Server's row version type, not a date.
        DataType::DateTime | DataType::Timestamp => "DATETIME2".to_string(),
        DataType::Blob | DataType::Binary(None) => "VARBINARY(MAX)".to_string(),
        DataType::Binary(Some(size)) => format!("VARBINARY({size})"),
        DataType::Uuid => "UNIQUEIDENTIFIER".to_string(),
        DataType::Xml => "XML".to_string(),
        DataType::Inet => "VARCHAR(45)".to_string(),
        DataType::MacAddr => "VARCHAR(17)".to_string(),
        DataType::Array(_) => {
            return Err(DatabaseError::UnsupportedDataType(
                "arrays (SQL Server has no array type)".to_string(),
            ));
        }
        DataType::Custom(name) => name.clone(),
    })
}

/// A column default as a T-SQL literal or expression.
fn default_sql(value: &DatabaseValue) -> String {
    use DatabaseValue as V;
    if crate::query::Expression::is_null(value) {
        return "NULL".to_string();
    }
    if let Some(now) = super::value::now_sql(value) {
        return now;
    }
    match value {
        V::String(text) | V::StringOpt(Some(text)) => literal(text),
        V::Bool(value) | V::BoolOpt(Some(value)) => if *value { "1" } else { "0" }.to_string(),
        V::Int8(value) | V::Int8Opt(Some(value)) => value.to_string(),
        V::Int16(value) | V::Int16Opt(Some(value)) => value.to_string(),
        V::Int32(value) | V::Int32Opt(Some(value)) => value.to_string(),
        V::Int64(value) | V::Int64Opt(Some(value)) => value.to_string(),
        V::UInt8(value) | V::UInt8Opt(Some(value)) => value.to_string(),
        V::UInt16(value) | V::UInt16Opt(Some(value)) => value.to_string(),
        V::UInt32(value) | V::UInt32Opt(Some(value)) => value.to_string(),
        V::UInt64(value) | V::UInt64Opt(Some(value)) => value.to_string(),
        V::Real32(value) | V::Real32Opt(Some(value)) => value.to_string(),
        V::Real64(value) | V::Real64Opt(Some(value)) => value.to_string(),
        #[cfg(feature = "decimal")]
        V::Decimal(value) | V::DecimalOpt(Some(value)) => value.to_string(),
        #[cfg(feature = "uuid")]
        V::Uuid(value) | V::UuidOpt(Some(value)) => literal(&value.to_string()),
        V::DateTime(value) => literal(&value.format("%Y-%m-%dT%H:%M:%S%.f").to_string()),
        V::Date(value) | V::DateOpt(Some(value)) => literal(&value.to_string()),
        V::Time(value) | V::TimeOpt(Some(value)) => literal(&value.to_string()),
        V::DateTimeTz(value) | V::DateTimeTzOpt(Some(value)) => literal(&value.to_rfc3339()),
        V::Bytes(bytes) | V::BytesOpt(Some(bytes)) => {
            let mut hex = String::from("0x");
            for byte in bytes {
                let _ = write!(hex, "{byte:02X}");
            }
            hex
        }
        #[cfg(feature = "json")]
        V::Json(value) | V::JsonOpt(Some(value)) => literal(&value.to_string()),
        V::Other { text, .. } => literal(text),
        _ => "NULL".to_string(),
    }
}

/// A column's definition: name, type, identity, nullability, default.
fn column_sql(
    name: &str,
    data_type: &DataType,
    nullable: bool,
    identity: bool,
    default: Option<&DatabaseValue>,
) -> Result<String, DatabaseError> {
    let mut sql = format!("{} {}", identifier(name), type_sql(data_type)?);
    if identity || matches!(data_type, DataType::Serial | DataType::BigSerial) {
        sql.push_str(" IDENTITY(1, 1)");
    }
    sql.push_str(if nullable { " NULL" } else { " NOT NULL" });
    if let Some(default) = default {
        write!(sql, " DEFAULT {}", default_sql(default)).ok();
    }
    Ok(sql)
}

/// `CREATE TABLE` for `statement` (guarded by `IF OBJECT_ID(...) IS NULL`
/// for `if_not_exists`).
///
/// # Errors
///
/// * [`DatabaseError::InvalidSchema`] for a table without columns or a
///   constraint that can't be rendered
/// * [`DatabaseError::UnsupportedDataType`] for array columns
pub(super) fn create_table_sql(
    statement: &crate::schema::CreateTableStatement<'_>,
) -> Result<String, DatabaseError> {
    if statement.columns.is_empty() {
        return Err(DatabaseError::InvalidSchema(format!(
            "table '{}' has no columns",
            statement.table_name
        )));
    }
    let mut parts = statement
        .columns
        .iter()
        .map(|column| {
            column_sql(
                &column.name,
                &column.data_type,
                column.nullable,
                column.auto_increment,
                column.default.as_ref(),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    if let Some(primary_key) = statement.primary_key {
        parts.push(format!("PRIMARY KEY ({})", identifier(primary_key)));
    }
    for constraint in &statement.constraints {
        parts.push(constraint.render('"')?);
    }
    for (source, target) in &statement.foreign_keys {
        parts.push(format!(
            "FOREIGN KEY ({}) REFERENCES {}",
            identifier(source),
            identifier(target)
        ));
    }
    let create = format!(
        "CREATE TABLE {} ({})",
        identifier(statement.table_name),
        parts.join(", ")
    );
    Ok(if statement.if_not_exists {
        format!(
            "IF OBJECT_ID({}, N'U') IS NULL {create}",
            literal(statement.table_name)
        )
    } else {
        create
    })
}

/// The first column of each row, as text.
fn names(rows: Vec<Vec<DatabaseValue>>) -> Vec<String> {
    rows.into_iter()
        .filter_map(|row| match row.into_iter().next() {
            Some(DatabaseValue::String(name)) => Some(name),
            _ => None,
        })
        .collect()
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
            "SELECT DISTINCT OBJECT_SCHEMA_NAME(fk.parent_object_id) + N'.' + \
             OBJECT_NAME(fk.parent_object_id) AS name FROM sys.foreign_keys fk \
             WHERE fk.referenced_object_id = OBJECT_ID(@P1) \
             AND fk.parent_object_id <> fk.referenced_object_id",
            &[DatabaseValue::String(table.to_string())],
            Mode::Rows,
        )
        .await?;
    Ok(names(result.rows).into_iter().collect())
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
                    // Catalog names are `schema.table`; the first is as given.
                    let target = table.split_once('.').map_or_else(
                        || identifier(&table),
                        |(schema, name)| format!("{}.{}", bracketed(schema), bracketed(name)),
                    );
                    executor
                        .run(&format!("DROP TABLE {if_exists}{target}"), &[], Mode::Batch)
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
            Mode::Batch,
        )
        .await?;
    Ok(())
}

/// `CREATE INDEX` for `statement` (a filtered index for a predicate).
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
        "CREATE {}INDEX {} ON {} ({})",
        if statement.unique { "UNIQUE " } else { "" },
        identifier(statement.index_name),
        identifier(statement.table_name),
        columns.join(", ")
    );
    if let Some(predicate) = &statement.predicate {
        write!(sql, " WHERE {}", predicate.render('"')?).ok();
    }
    Ok(if statement.if_not_exists {
        format!(
            "IF NOT EXISTS (SELECT 1 FROM sys.indexes WHERE name = {} AND object_id = \
             OBJECT_ID({})) {sql}",
            literal(statement.index_name),
            literal(statement.table_name)
        )
    } else {
        sql
    })
}

/// `DROP INDEX name ON table` for `statement`.
#[must_use]
pub(super) fn drop_index_sql(statement: &crate::schema::DropIndexStatement<'_>) -> String {
    format!(
        "DROP INDEX {}{} ON {}",
        if statement.if_exists {
            "IF EXISTS "
        } else {
            ""
        },
        identifier(statement.index_name),
        identifier(statement.table_name)
    )
}

/// The name of the default constraint on `table`.`column`, if it has one.
async fn default_constraint(
    executor: &dyn Executor,
    table: &str,
    column: &str,
) -> Result<Option<String>, DatabaseError> {
    let result = executor
        .run(
            "SELECT d.name FROM sys.default_constraints d JOIN sys.columns c \
             ON c.object_id = d.parent_object_id AND c.column_id = d.parent_column_id \
             WHERE d.parent_object_id = OBJECT_ID(@P1) AND c.name = @P2",
            &[
                DatabaseValue::String(table.to_string()),
                DatabaseValue::String(column.to_string()),
            ],
            Mode::Rows,
        )
        .await?;
    Ok(names(result.rows).into_iter().next())
}

/// Drop the default constraint of `table`.`column`, if it has one.
async fn drop_default(
    executor: &dyn Executor,
    table: &str,
    column: &str,
) -> Result<(), DatabaseError> {
    if let Some(name) = default_constraint(executor, table, column).await? {
        executor
            .run(
                &format!(
                    "ALTER TABLE {} DROP CONSTRAINT {}",
                    identifier(table),
                    bracketed(&name)
                ),
                &[],
                Mode::Batch,
            )
            .await?;
    }
    Ok(())
}

/// Whether `table`.`column` allows nulls now.
async fn is_nullable(
    executor: &dyn Executor,
    table: &str,
    column: &str,
) -> Result<bool, DatabaseError> {
    let result = executor
        .run(
            "SELECT CAST(is_nullable AS int) FROM sys.columns \
             WHERE object_id = OBJECT_ID(@P1) AND name = @P2",
            &[
                DatabaseValue::String(table.to_string()),
                DatabaseValue::String(column.to_string()),
            ],
            Mode::Rows,
        )
        .await?;
    match result.rows.first().and_then(|row| row.first()) {
        Some(DatabaseValue::Int32(value)) => Ok(*value != 0),
        _ => Err(DatabaseError::InvalidSchema(format!(
            "column '{column}' of '{table}' doesn't exist"
        ))),
    }
}

/// Run `ALTER TABLE`, one statement (or a few) per operation.
///
/// # Errors
///
/// * [`DatabaseError::UnsupportedDataType`] for array columns
/// * [`DatabaseError::InvalidSchema`] if a `CHECK` can't be rendered or a
///   modified column doesn't exist
/// * If a statement fails
pub(super) async fn alter_table(
    executor: &dyn Executor,
    statement: &crate::schema::AlterTableStatement<'_>,
) -> Result<(), DatabaseError> {
    let table = identifier(statement.table_name);
    for operation in &statement.operations {
        match operation {
            AlterOperation::AddColumn {
                name,
                data_type,
                nullable,
                default,
            } => {
                let mut sql = format!(
                    "ALTER TABLE {table} ADD {}",
                    column_sql(name, data_type, *nullable, false, default.as_ref())?
                );
                if let Some(check) = statement.column_checks.get(name) {
                    write!(sql, " CHECK ({})", check.render('"')?).ok();
                }
                executor.run(&sql, &[], Mode::Batch).await?;
            }
            AlterOperation::DropColumn { name, .. } => {
                drop_default(executor, statement.table_name, name).await?;
                executor
                    .run(
                        &format!("ALTER TABLE {table} DROP COLUMN {}", identifier(name)),
                        &[],
                        Mode::Batch,
                    )
                    .await?;
            }
            AlterOperation::RenameColumn { old_name, new_name } => {
                executor
                    .run(
                        "EXEC sp_rename @P1, @P2, 'COLUMN'",
                        &[
                            DatabaseValue::String(format!(
                                "{}.{}",
                                statement.table_name,
                                bracketed(old_name)
                            )),
                            DatabaseValue::String(new_name.clone()),
                        ],
                        Mode::Batch,
                    )
                    .await?;
            }
            AlterOperation::ModifyColumn {
                name,
                new_data_type,
                new_nullable,
                new_default,
            } => {
                let nullable = match new_nullable {
                    Some(nullable) => *nullable,
                    None => is_nullable(executor, statement.table_name, name).await?,
                };
                if new_default.is_some() {
                    drop_default(executor, statement.table_name, name).await?;
                }
                executor
                    .run(
                        &format!(
                            "ALTER TABLE {table} ALTER COLUMN {} {}{}",
                            identifier(name),
                            type_sql(new_data_type)?,
                            if nullable { " NULL" } else { " NOT NULL" }
                        ),
                        &[],
                        Mode::Batch,
                    )
                    .await?;
                if let Some(default) = new_default {
                    executor
                        .run(
                            &format!(
                                "ALTER TABLE {table} ADD DEFAULT {} FOR {}",
                                default_sql(default),
                                identifier(name)
                            ),
                            &[],
                            Mode::Batch,
                        )
                        .await?;
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::Column;

    #[test]
    fn create_table_uses_identity_and_unicode_text() {
        let statement = crate::schema::create_table("albums")
            .if_not_exists(true)
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
            .column(Column {
                name: "added".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::DateTime,
                default: Some(DatabaseValue::Now),
            })
            .primary_key("id");
        let sql = create_table_sql(&statement).unwrap();
        assert!(
            sql.starts_with("IF OBJECT_ID(N'albums', N'U') IS NULL CREATE TABLE "),
            "{sql}"
        );
        assert!(sql.contains("BIGINT IDENTITY(1, 1) NOT NULL"), "{sql}");
        assert!(sql.contains("NVARCHAR(200) NULL DEFAULT N'it''s'"), "{sql}");
        assert!(
            sql.contains("DATETIME2 NOT NULL DEFAULT SYSUTCDATETIME()"),
            "{sql}"
        );
        assert!(sql.contains("PRIMARY KEY ("), "{sql}");
    }

    #[test]
    fn arrays_are_refused_and_indexes_are_guarded() {
        assert!(matches!(
            type_sql(&DataType::Array(Box::new(DataType::Int))),
            Err(DatabaseError::UnsupportedDataType(_))
        ));
        let statement = crate::schema::create_index("albums_title")
            .table("albums")
            .column("title")
            .if_not_exists(true);
        let sql = create_index_sql(&statement).unwrap();
        assert!(
            sql.starts_with("IF NOT EXISTS (SELECT 1 FROM sys.indexes"),
            "{sql}"
        );
        assert_eq!(bracketed("a]b"), "[a]]b]");
    }
}
