#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use std::sync::atomic::{AtomicBool, Ordering};

use async_trait::async_trait;

use crate::{DatabaseError, DatabaseValue, Row};

use super::{
    TursoDatabaseError, from_turso_row, to_turso_params, turso_transform_query_for_params,
};

#[cfg(feature = "schema")]
use std::sync::LazyLock;

#[cfg(feature = "schema")]
static FK_PATTERN: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(
        r#"(?i)FOREIGN\s+KEY\s*\(([^)]+)\)\s*REFERENCES\s+((?:[^\s(,\[\]"'`]+|"(?:[^"]|"")*"|`(?:[^`]|``)*`|\[(?:[^\]])*\]|'(?:[^']|'')*'))\s*\(([^)]+)\)([^,)]*)"#
    ).expect("FK regex pattern should compile")
});

#[cfg(feature = "schema")]
static ON_UPDATE_PATTERN: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?i)ON\s+UPDATE\s+(CASCADE|SET\s+NULL|SET\s+DEFAULT|RESTRICT|NO\s+ACTION)")
        .expect("ON UPDATE regex pattern should compile")
});

#[cfg(feature = "schema")]
static ON_DELETE_PATTERN: LazyLock<regex::Regex> = LazyLock::new(|| {
    regex::Regex::new(r"(?i)ON\s+DELETE\s+(CASCADE|SET\s+NULL|SET\s+DEFAULT|RESTRICT|NO\s+ACTION)")
        .expect("ON DELETE regex pattern should compile")
});

/// Turso database transaction with commit/rollback capabilities
pub struct TursoTransaction {
    connection: turso::Connection,
    committed: AtomicBool,
    rolled_back: AtomicBool,
}

impl std::fmt::Debug for TursoTransaction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TursoTransaction")
            .field("connection", &"<turso::Connection>")
            .field("committed", &self.committed.load(Ordering::SeqCst))
            .field("rolled_back", &self.rolled_back.load(Ordering::SeqCst))
            .finish()
    }
}

impl TursoTransaction {
    /// Create a new Turso transaction
    ///
    /// # Errors
    ///
    /// * Returns error if transaction cannot be started
    pub async fn new(connection: turso::Connection) -> Result<Self, TursoDatabaseError> {
        connection
            .execute("BEGIN DEFERRED", ())
            .await
            .map_err(|e| {
                TursoDatabaseError::Transaction(format!("Failed to begin transaction: {e}"))
            })?;

        Ok(Self {
            connection,
            committed: AtomicBool::new(false),
            rolled_back: AtomicBool::new(false),
        })
    }
}

#[async_trait]
impl crate::DatabaseTransaction for TursoTransaction {
    async fn commit(self: Box<Self>) -> Result<(), DatabaseError> {
        if self.committed.load(Ordering::SeqCst) {
            return Err(DatabaseError::TransactionCommitted);
        }

        if self.rolled_back.load(Ordering::SeqCst) {
            return Err(DatabaseError::TransactionRolledBack);
        }

        self.connection
            .execute("COMMIT", ())
            .await
            .map_err(|e| DatabaseError::Turso(e.into()))?;

        self.committed.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn rollback(self: Box<Self>) -> Result<(), DatabaseError> {
        if self.committed.load(Ordering::SeqCst) {
            return Err(DatabaseError::TransactionCommitted);
        }

        if self.rolled_back.load(Ordering::SeqCst) {
            return Err(DatabaseError::TransactionRolledBack);
        }

        self.connection
            .execute("ROLLBACK", ())
            .await
            .map_err(|e| DatabaseError::Turso(e.into()))?;

        self.rolled_back.store(true, Ordering::SeqCst);
        Ok(())
    }

    async fn savepoint(&self, name: &str) -> Result<Box<dyn crate::Savepoint>, DatabaseError> {
        crate::validate_savepoint_name(name)?;

        unimplemented!(
            "Turso v0.2.2 does not support SAVEPOINT syntax yet. This feature will be available in future Turso versions. Consider using multiple transactions or upgrade to a newer Turso version when available."
        );
    }

    #[cfg(feature = "cascade")]
    async fn find_cascade_targets(
        &self,
        table_name: &str,
    ) -> Result<crate::schema::DropPlan, DatabaseError> {
        let drop_order = { super::find_cascade_dependents(&self.connection, table_name).await? };

        Ok(crate::schema::DropPlan::Simple(drop_order))
    }

    #[cfg(feature = "cascade")]
    async fn has_any_dependents(&self, table_name: &str) -> Result<bool, DatabaseError> {
        super::has_dependents(&self.connection, table_name).await
    }

    #[cfg(feature = "cascade")]
    async fn get_direct_dependents(
        &self,
        table_name: &str,
    ) -> Result<std::collections::BTreeSet<String>, DatabaseError> {
        let mut direct_dependents = std::collections::BTreeSet::new();

        // Get all tables
        let mut stmt = self
            .connection
            .prepare(
                "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
            )
            .await
            .map_err(|e| DatabaseError::Turso(e.into()))?;

        let mut rows = stmt
            .query(())
            .await
            .map_err(|e| DatabaseError::Turso(e.into()))?;

        let mut table_names = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| DatabaseError::Turso(e.into()))?
        {
            let name_value = row
                .get_value(0)
                .map_err(|e| DatabaseError::Turso(e.into()))?;

            if let turso::Value::Text(name) = name_value {
                table_names.push(name);
            }
        }

        // Check each table for direct foreign keys referencing table_name
        for check_table in table_names {
            if check_table == table_name {
                continue;
            }

            crate::schema::dependencies::validate_table_name_for_pragma(&check_table)?;

            // Get CREATE TABLE SQL to parse foreign keys
            let sql = {
                let mut sql_stmt = self
                    .connection
                    .prepare("SELECT sql FROM sqlite_master WHERE type='table' AND name=?")
                    .await
                    .map_err(|e| DatabaseError::Turso(e.into()))?;

                let mut sql_rows = sql_stmt
                    .query((turso::Value::Text(check_table.clone()),))
                    .await
                    .map_err(|e| DatabaseError::Turso(e.into()))?;

                if let Some(sql_row) = sql_rows
                    .next()
                    .await
                    .map_err(|e| DatabaseError::Turso(e.into()))?
                {
                    let sql_value = sql_row
                        .get_value(0)
                        .map_err(|e| DatabaseError::Turso(e.into()))?;

                    match sql_value {
                        turso::Value::Text(s) => Some(s),
                        _ => None,
                    }
                } else {
                    None
                }
            };

            if let Some(create_sql) = sql {
                // Parse foreign keys from CREATE TABLE SQL using Phase 4 regex
                for cap in super::FK_PATTERN.captures_iter(&create_sql) {
                    let referenced_table = super::strip_identifier_quotes(&cap[2]);

                    if referenced_table == table_name {
                        direct_dependents.insert(check_table.clone());
                        break;
                    }
                }
            }
        }

        Ok(direct_dependents)
    }
}

#[async_trait]
impl crate::Database for TursoTransaction {
    async fn query_raw(&self, query: &str) -> Result<Vec<Row>, DatabaseError> {
        let mut stmt = self
            .connection
            .prepare(query)
            .await
            .map_err(|e| DatabaseError::Turso(e.into()))?;

        let column_info = stmt.columns();
        let column_names: Vec<String> = column_info.iter().map(|c| c.name().to_string()).collect();

        let mut rows = stmt
            .query(())
            .await
            .map_err(|e| DatabaseError::Turso(e.into()))?;

        let mut results = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| DatabaseError::Turso(e.into()))?
        {
            results.push(from_turso_row(&column_names, &row).map_err(DatabaseError::Turso)?);
        }

        Ok(results)
    }

    async fn query_raw_params(
        &self,
        query: &str,
        params: &[DatabaseValue],
    ) -> Result<Vec<Row>, DatabaseError> {
        let (transformed_query, filtered_params) = turso_transform_query_for_params(query, params)?;

        let mut stmt = self
            .connection
            .prepare(&transformed_query)
            .await
            .map_err(|e| DatabaseError::Turso(e.into()))?;

        let column_info = stmt.columns();
        let column_names: Vec<String> = column_info.iter().map(|c| c.name().to_string()).collect();

        let turso_params = to_turso_params(&filtered_params).map_err(DatabaseError::Turso)?;

        let mut rows = stmt
            .query(turso_params)
            .await
            .map_err(|e| DatabaseError::Turso(e.into()))?;

        let mut results = Vec::new();
        while let Some(row) = rows
            .next()
            .await
            .map_err(|e| DatabaseError::Turso(e.into()))?
        {
            results.push(from_turso_row(&column_names, &row).map_err(DatabaseError::Turso)?);
        }

        Ok(results)
    }

    async fn exec_raw(&self, statement: &str) -> Result<(), DatabaseError> {
        self.connection
            .execute(statement, ())
            .await
            .map_err(|e| DatabaseError::Turso(e.into()))?;

        Ok(())
    }

    async fn exec_raw_params(
        &self,
        query: &str,
        params: &[DatabaseValue],
    ) -> Result<u64, DatabaseError> {
        let (transformed_query, filtered_params) = turso_transform_query_for_params(query, params)?;

        let turso_params = to_turso_params(&filtered_params).map_err(DatabaseError::Turso)?;

        let mut stmt = self
            .connection
            .prepare(&transformed_query)
            .await
            .map_err(|e| DatabaseError::Turso(e.into()))?;

        let affected_rows = stmt
            .execute(turso_params)
            .await
            .map_err(|e| DatabaseError::Turso(e.into()))?;

        Ok(affected_rows)
    }

    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn crate::DatabaseTransaction>, DatabaseError> {
        Err(DatabaseError::AlreadyInTransaction)
    }

    async fn query(
        &self,
        query: &crate::query::SelectQuery<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        Ok(super::select(
            &self.connection,
            query.table_name,
            query.distinct,
            query.columns,
            query.filters.as_deref(),
            query.joins.as_deref(),
            query.sorts.as_deref(),
            query.limit,
        )
        .await
        .map_err(DatabaseError::Turso)?)
    }

    async fn query_first(
        &self,
        query: &crate::query::SelectQuery<'_>,
    ) -> Result<Option<Row>, DatabaseError> {
        Ok(super::find_row(
            &self.connection,
            query.table_name,
            query.distinct,
            query.columns,
            query.filters.as_deref(),
            query.joins.as_deref(),
            query.sorts.as_deref(),
        )
        .await
        .map_err(DatabaseError::Turso)?)
    }

    async fn exec_update(
        &self,
        statement: &crate::query::UpdateStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        Ok(super::update_and_get_rows(
            &self.connection,
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            None,
        )
        .await
        .map_err(DatabaseError::Turso)?)
    }

    async fn exec_update_first(
        &self,
        statement: &crate::query::UpdateStatement<'_>,
    ) -> Result<Option<Row>, DatabaseError> {
        Ok(super::update_and_get_row(
            &self.connection,
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            Some(1),
        )
        .await
        .map_err(DatabaseError::Turso)?)
    }

    async fn exec_insert(
        &self,
        statement: &crate::query::InsertStatement<'_>,
    ) -> Result<Row, DatabaseError> {
        Ok(
            super::insert_and_get_row(&self.connection, statement.table_name, &statement.values)
                .await
                .map_err(DatabaseError::Turso)?,
        )
    }

    async fn exec_upsert(
        &self,
        statement: &crate::query::UpsertStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        Ok(super::upsert(
            &self.connection,
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            statement.limit,
        )
        .await
        .map_err(DatabaseError::Turso)?)
    }

    async fn exec_upsert_first(
        &self,
        statement: &crate::query::UpsertStatement<'_>,
    ) -> Result<Row, DatabaseError> {
        Ok(super::upsert_and_get_row(
            &self.connection,
            statement.table_name,
            &statement.values,
            statement.filters.as_deref(),
            statement.limit,
        )
        .await
        .map_err(DatabaseError::Turso)?)
    }

    async fn exec_upsert_multi(
        &self,
        statement: &crate::query::UpsertMultiStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        let mut all_results = Vec::new();
        for values in &statement.values {
            let results = {
                super::upsert(&self.connection, statement.table_name, values, None, None)
                    .await
                    .map_err(DatabaseError::Turso)?
            };
            all_results.extend(results);
        }

        Ok(all_results)
    }

    async fn exec_delete(
        &self,
        statement: &crate::query::DeleteStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        Ok(super::delete(
            &self.connection,
            statement.table_name,
            statement.filters.as_deref(),
            None,
        )
        .await
        .map_err(DatabaseError::Turso)?)
    }

    async fn exec_delete_first(
        &self,
        statement: &crate::query::DeleteStatement<'_>,
    ) -> Result<Option<Row>, DatabaseError> {
        let rows = {
            super::delete(
                &self.connection,
                statement.table_name,
                statement.filters.as_deref(),
                Some(1),
            )
            .await
            .map_err(DatabaseError::Turso)?
        };
        Ok(rows.into_iter().next())
    }

    #[cfg(feature = "schema")]
    async fn exec_create_table(
        &self,
        statement: &crate::schema::CreateTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        super::exec_create_table(&self.connection, statement).await
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_table(
        &self,
        statement: &crate::schema::DropTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        super::exec_drop_table(&self.connection, statement).await
    }

    #[cfg(feature = "schema")]
    async fn exec_create_index(
        &self,
        statement: &crate::schema::CreateIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        super::exec_create_index(&self.connection, statement).await
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_index(
        &self,
        statement: &crate::schema::DropIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        super::exec_drop_index(&self.connection, statement).await
    }

    #[cfg(feature = "schema")]
    async fn exec_alter_table(
        &self,
        statement: &crate::schema::AlterTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        super::exec_alter_table(&self.connection, statement).await
    }

    #[cfg(feature = "schema")]
    async fn table_exists(&self, table: &str) -> Result<bool, DatabaseError> {
        let query = "SELECT name FROM sqlite_master WHERE type='table' AND name=?";
        let rows = self
            .query_raw_params(query, &[DatabaseValue::String(table.to_string())])
            .await?;
        Ok(!rows.is_empty())
    }

    #[cfg(feature = "schema")]
    async fn list_tables(&self) -> Result<Vec<String>, DatabaseError> {
        let query =
            "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'";
        let rows = self.query_raw(query).await?;

        Ok(rows
            .into_iter()
            .filter_map(|row| row.get("name"))
            .filter_map(|v| match v {
                DatabaseValue::String(s) => Some(s),
                _ => None,
            })
            .collect())
    }

    #[cfg(feature = "schema")]
    #[allow(
        clippy::too_many_lines,
        clippy::single_match_else,
        clippy::option_if_let_else,
        clippy::collapsible_if
    )]
    async fn get_table_info(
        &self,
        table: &str,
    ) -> Result<Option<crate::schema::TableInfo>, DatabaseError> {
        if !self.table_exists(table).await? {
            return Ok(None);
        }

        let columns = self.get_table_columns(table).await?;

        let columns_map = columns
            .into_iter()
            .map(|col| (col.name.clone(), col))
            .collect();

        let indexes = {
            let index_query =
                "SELECT name, sql FROM sqlite_master WHERE type='index' AND tbl_name=?";
            let index_rows = self
                .query_raw_params(index_query, &[DatabaseValue::String(table.to_string())])
                .await?;

            let mut indexes = std::collections::BTreeMap::new();

            for index_row in index_rows {
                let index_name = match index_row.get("name") {
                    Some(DatabaseValue::String(s)) => s.clone(),
                    _ => continue,
                };

                let sql = match index_row.get("sql") {
                    Some(DatabaseValue::String(s)) => s.clone(),
                    _ => {
                        let is_primary = index_name.starts_with("sqlite_autoindex_");
                        indexes.insert(
                            index_name.clone(),
                            crate::schema::IndexInfo {
                                name: index_name,
                                unique: false,
                                columns: Vec::new(),
                                is_primary,
                            },
                        );
                        continue;
                    }
                };

                let is_unique = sql.to_uppercase().contains("UNIQUE");
                let is_primary = index_name.starts_with("sqlite_autoindex_");

                let index_columns = {
                    if let Some(start) = sql.find('(') {
                        if let Some(end) = sql.rfind(')') {
                            let cols_str = &sql[start + 1..end];
                            cols_str
                                .split(',')
                                .map(|s| s.trim().trim_matches('`').trim_matches('"').to_string())
                                .collect()
                        } else {
                            Vec::new()
                        }
                    } else {
                        Vec::new()
                    }
                };

                indexes.insert(
                    index_name.clone(),
                    crate::schema::IndexInfo {
                        name: index_name,
                        unique: is_unique,
                        columns: index_columns,
                        is_primary,
                    },
                );
            }

            indexes
        };

        let foreign_keys = {
            let create_sql_query = "SELECT sql FROM sqlite_master WHERE type='table' AND name=?";
            let create_sql_rows = self
                .query_raw_params(
                    create_sql_query,
                    &[DatabaseValue::String(table.to_string())],
                )
                .await?;

            let create_sql = create_sql_rows
                .into_iter()
                .find_map(|row| match row.get("sql") {
                    Some(DatabaseValue::String(s)) => Some(s),
                    _ => None,
                });

            let mut foreign_keys = std::collections::BTreeMap::new();

            if let Some(sql) = create_sql {
                for cap in FK_PATTERN.captures_iter(&sql) {
                    let column = strip_identifier_quotes(&cap[1]);
                    let referenced_table = strip_identifier_quotes(&cap[2]);
                    let referenced_column = strip_identifier_quotes(&cap[3]);

                    let fk_actions = &cap[4];

                    let on_update = ON_UPDATE_PATTERN.captures(fk_actions).and_then(|c| {
                        let action = c[1].to_uppercase();
                        if action == "NO ACTION" {
                            None
                        } else {
                            Some(action)
                        }
                    });

                    let on_delete = ON_DELETE_PATTERN.captures(fk_actions).and_then(|c| {
                        let action = c[1].to_uppercase();
                        if action == "NO ACTION" {
                            None
                        } else {
                            Some(action)
                        }
                    });

                    let fk_name =
                        format!("{table}_{column}_{referenced_table}_{referenced_column}");

                    foreign_keys.insert(
                        fk_name.clone(),
                        crate::schema::ForeignKeyInfo {
                            name: fk_name,
                            column,
                            referenced_table,
                            referenced_column,
                            on_update,
                            on_delete,
                        },
                    );
                }
            }

            foreign_keys
        };

        Ok(Some(crate::schema::TableInfo {
            name: table.to_string(),
            columns: columns_map,
            indexes,
            foreign_keys,
        }))
    }

    #[cfg(feature = "schema")]
    async fn get_table_columns(
        &self,
        table: &str,
    ) -> Result<Vec<crate::schema::ColumnInfo>, DatabaseError> {
        let query = format!("PRAGMA table_info({table})");
        let rows = self.query_raw(&query).await?;

        let create_sql_query = "SELECT sql FROM sqlite_master WHERE type='table' AND name=?";
        let create_sql_rows = self
            .query_raw_params(
                create_sql_query,
                &[DatabaseValue::String(table.to_string())],
            )
            .await?;

        let create_sql = create_sql_rows
            .into_iter()
            .find_map(|row| match row.get("sql") {
                Some(DatabaseValue::String(s)) => Some(s),
                _ => None,
            });

        let mut columns = Vec::new();

        for row in rows {
            let ordinal = match row.get("cid") {
                Some(DatabaseValue::Int64(i)) => {
                    u32::try_from(i).unwrap_or_else(|_| u32::try_from(columns.len()).unwrap_or(0))
                }
                _ => u32::try_from(columns.len()).unwrap_or(0),
            };

            let name = match row.get("name") {
                Some(DatabaseValue::String(s)) => s.clone(),
                _ => continue,
            };

            let type_str = match row.get("type") {
                Some(DatabaseValue::String(s)) => s.clone(),
                _ => String::from("TEXT"),
            };

            let not_null = match row.get("notnull") {
                Some(DatabaseValue::Int64(i)) => i != 0,
                _ => false,
            };

            let is_pk = match row.get("pk") {
                Some(DatabaseValue::Int64(i)) => i != 0,
                _ => false,
            };

            let default_value = match row.get("dflt_value") {
                Some(DatabaseValue::String(s)) => Some(s.clone()),
                _ => None,
            };

            let data_type = super::sqlite_type_to_data_type(&type_str);
            let default_val = super::parse_default_value(default_value.as_deref());

            let auto_increment = if is_pk {
                super::check_autoincrement_in_sql(create_sql.as_deref(), &name)
            } else {
                false
            };

            columns.push(crate::schema::ColumnInfo {
                name,
                data_type,
                nullable: !not_null,
                is_primary_key: is_pk,
                auto_increment,
                default_value: default_val,
                ordinal_position: ordinal + 1,
            });
        }

        Ok(columns)
    }

    #[cfg(feature = "schema")]
    async fn column_exists(&self, table: &str, column: &str) -> Result<bool, DatabaseError> {
        let columns = self.get_table_columns(table).await?;
        Ok(columns.iter().any(|col| col.name == column))
    }
}

/// Strips quotes from `SQLite` identifiers and unescapes internal quotes.
///
/// Handles all 4 `SQLite` identifier quoting styles:
/// - Double quotes: `"name"` or `"my ""quoted"" name"` → `my "quoted" name`
/// - Backticks: `` `name` `` or `` `my ``tick`` name` `` → `my `tick` name`
/// - Single quotes: `'name'` or `'my ''quoted'' name'` → `my 'quoted' name`
/// - Square brackets: `[name]` → `name` (no escaping needed)
/// - Unquoted: `name` → `name` (returned as-is)
#[cfg(feature = "schema")]
fn strip_identifier_quotes(identifier: &str) -> String {
    let identifier = identifier.trim();

    if identifier.len() < 2 {
        return identifier.to_string();
    }

    if identifier.starts_with('"') && identifier.ends_with('"') {
        identifier[1..identifier.len() - 1].replace("\"\"", "\"")
    } else if identifier.starts_with('`') && identifier.ends_with('`') {
        identifier[1..identifier.len() - 1].replace("``", "`")
    } else if identifier.starts_with('[') && identifier.ends_with(']') {
        identifier[1..identifier.len() - 1].to_string()
    } else if identifier.starts_with('\'') && identifier.ends_with('\'') {
        identifier[1..identifier.len() - 1].replace("''", "'")
    } else {
        identifier.to_string()
    }
}
