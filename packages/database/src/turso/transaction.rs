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
    async fn table_exists(&self, _table: &str) -> Result<bool, DatabaseError> {
        unimplemented!("Schema introspection not yet implemented for Turso backend")
    }

    #[cfg(feature = "schema")]
    async fn list_tables(&self) -> Result<Vec<String>, DatabaseError> {
        unimplemented!("Schema introspection not yet implemented for Turso backend")
    }

    #[cfg(feature = "schema")]
    async fn get_table_info(
        &self,
        _table: &str,
    ) -> Result<Option<crate::schema::TableInfo>, DatabaseError> {
        unimplemented!("Schema introspection not yet implemented for Turso backend")
    }

    #[cfg(feature = "schema")]
    async fn get_table_columns(
        &self,
        _table: &str,
    ) -> Result<Vec<crate::schema::ColumnInfo>, DatabaseError> {
        unimplemented!("Schema introspection not yet implemented for Turso backend")
    }

    #[cfg(feature = "schema")]
    async fn column_exists(&self, _table: &str, _column: &str) -> Result<bool, DatabaseError> {
        unimplemented!("Schema introspection not yet implemented for Turso backend")
    }
}
