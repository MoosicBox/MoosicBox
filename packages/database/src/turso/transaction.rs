#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]

use std::{
    pin::Pin,
    sync::atomic::{AtomicBool, Ordering},
};

use async_trait::async_trait;

use crate::{DatabaseError, DatabaseValue, Row};

use super::{
    TursoDatabaseError, from_turso_row, to_turso_params, turso_transform_query_for_params,
};

pub struct TursoTransaction {
    connection: Pin<Box<turso::Connection>>,
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
            connection: Box::pin(connection),
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

    async fn savepoint(&self, _name: &str) -> Result<Box<dyn crate::Savepoint>, DatabaseError> {
        unimplemented!("Savepoints not yet implemented for Turso backend")
    }

    #[cfg(feature = "cascade")]
    async fn find_cascade_targets(
        &self,
        _table_name: &str,
    ) -> Result<crate::schema::DropPlan, DatabaseError> {
        unimplemented!("Cascade not yet implemented for Turso backend")
    }

    #[cfg(feature = "cascade")]
    async fn has_any_dependents(&self, _table_name: &str) -> Result<bool, DatabaseError> {
        unimplemented!("Cascade not yet implemented for Turso backend")
    }

    #[cfg(feature = "cascade")]
    async fn get_direct_dependents(
        &self,
        _table_name: &str,
    ) -> Result<std::collections::BTreeSet<String>, DatabaseError> {
        unimplemented!("Cascade not yet implemented for Turso backend")
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
        _query: &crate::query::SelectQuery<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        unimplemented!(
            "Query builder not yet implemented for Turso backend - use query_raw_params instead"
        )
    }

    async fn query_first(
        &self,
        _query: &crate::query::SelectQuery<'_>,
    ) -> Result<Option<Row>, DatabaseError> {
        unimplemented!(
            "Query builder not yet implemented for Turso backend - use query_raw_params instead"
        )
    }

    async fn exec_update(
        &self,
        _statement: &crate::query::UpdateStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        unimplemented!(
            "Query builder not yet implemented for Turso backend - use exec_raw_params instead"
        )
    }

    async fn exec_update_first(
        &self,
        _statement: &crate::query::UpdateStatement<'_>,
    ) -> Result<Option<Row>, DatabaseError> {
        unimplemented!(
            "Query builder not yet implemented for Turso backend - use exec_raw_params instead"
        )
    }

    async fn exec_insert(
        &self,
        _statement: &crate::query::InsertStatement<'_>,
    ) -> Result<Row, DatabaseError> {
        unimplemented!(
            "Query builder not yet implemented for Turso backend - use exec_raw_params instead"
        )
    }

    async fn exec_upsert(
        &self,
        _statement: &crate::query::UpsertStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        unimplemented!(
            "Query builder not yet implemented for Turso backend - use exec_raw_params instead"
        )
    }

    async fn exec_upsert_first(
        &self,
        _statement: &crate::query::UpsertStatement<'_>,
    ) -> Result<Row, DatabaseError> {
        unimplemented!(
            "Query builder not yet implemented for Turso backend - use exec_raw_params instead"
        )
    }

    async fn exec_upsert_multi(
        &self,
        _statement: &crate::query::UpsertMultiStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        unimplemented!(
            "Query builder not yet implemented for Turso backend - use exec_raw_params instead"
        )
    }

    async fn exec_delete(
        &self,
        _statement: &crate::query::DeleteStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        unimplemented!(
            "Query builder not yet implemented for Turso backend - use exec_raw_params instead"
        )
    }

    async fn exec_delete_first(
        &self,
        _statement: &crate::query::DeleteStatement<'_>,
    ) -> Result<Option<Row>, DatabaseError> {
        unimplemented!(
            "Query builder not yet implemented for Turso backend - use exec_raw_params instead"
        )
    }

    #[cfg(feature = "schema")]
    async fn exec_create_table(
        &self,
        _statement: &crate::schema::CreateTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        unimplemented!(
            "Schema operations not yet implemented for Turso backend - use exec_raw instead"
        )
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_table(
        &self,
        _statement: &crate::schema::DropTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        unimplemented!(
            "Schema operations not yet implemented for Turso backend - use exec_raw instead"
        )
    }

    #[cfg(feature = "schema")]
    async fn exec_create_index(
        &self,
        _statement: &crate::schema::CreateIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        unimplemented!(
            "Schema operations not yet implemented for Turso backend - use exec_raw instead"
        )
    }

    #[cfg(feature = "schema")]
    async fn exec_drop_index(
        &self,
        _statement: &crate::schema::DropIndexStatement<'_>,
    ) -> Result<(), DatabaseError> {
        unimplemented!(
            "Schema operations not yet implemented for Turso backend - use exec_raw instead"
        )
    }

    #[cfg(feature = "schema")]
    async fn exec_alter_table(
        &self,
        _statement: &crate::schema::AlterTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        unimplemented!(
            "Schema operations not yet implemented for Turso backend - use exec_raw instead"
        )
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
                let sql_upper = sql.to_uppercase();
                if sql_upper.contains("FOREIGN KEY") {
                    let parts: Vec<&str> = sql.split("FOREIGN KEY").collect();
                    for part in parts.iter().skip(1) {
                        if let Some(col_start) = part.find('(') {
                            if let Some(col_end) = part[col_start..].find(')') {
                                let column = part[col_start + 1..col_start + col_end]
                                    .trim()
                                    .trim_matches('`')
                                    .trim_matches('"')
                                    .to_string();

                                if let Some(ref_start) = part.find("REFERENCES") {
                                    let ref_part = &part[ref_start + 10..];
                                    if let Some(ref_table_end) = ref_part.find('(') {
                                        let referenced_table =
                                            ref_part[..ref_table_end].trim().to_string();

                                        if let Some(ref_col_end) =
                                            ref_part[ref_table_end..].find(')')
                                        {
                                            let referenced_column = ref_part
                                                [ref_table_end + 1..ref_table_end + ref_col_end]
                                                .trim()
                                                .trim_matches('`')
                                                .trim_matches('"')
                                                .to_string();

                                            let on_update = if ref_part
                                                .to_uppercase()
                                                .contains("ON UPDATE CASCADE")
                                            {
                                                Some("CASCADE".to_string())
                                            } else if ref_part
                                                .to_uppercase()
                                                .contains("ON UPDATE SET NULL")
                                            {
                                                Some("SET NULL".to_string())
                                            } else if ref_part
                                                .to_uppercase()
                                                .contains("ON UPDATE RESTRICT")
                                            {
                                                Some("RESTRICT".to_string())
                                            } else {
                                                None
                                            };

                                            let on_delete = if ref_part
                                                .to_uppercase()
                                                .contains("ON DELETE CASCADE")
                                            {
                                                Some("CASCADE".to_string())
                                            } else if ref_part
                                                .to_uppercase()
                                                .contains("ON DELETE SET NULL")
                                            {
                                                Some("SET NULL".to_string())
                                            } else if ref_part
                                                .to_uppercase()
                                                .contains("ON DELETE RESTRICT")
                                            {
                                                Some("RESTRICT".to_string())
                                            } else {
                                                None
                                            };

                                            let fk_name = format!(
                                                "{table}_{column}_{referenced_table}_{referenced_column}"
                                            );

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
                                }
                            }
                        }
                    }
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
