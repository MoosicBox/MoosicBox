use async_trait::async_trait;

use crate::{
    Database, DatabaseError, Row,
    query::{
        DeleteStatement, InsertStatement, SelectQuery, UpdateStatement, UpsertMultiStatement,
        UpsertStatement,
    },
    rusqlite::RusqliteDatabase,
};

#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct SimulationDatabase {
    inner: RusqliteDatabase,
}

impl SimulationDatabase {
    /// # Errors
    ///
    /// * If the database connection fails to open in memory
    pub fn new() -> Result<Self, DatabaseError> {
        let library = ::rusqlite::Connection::open_in_memory()
            .map_err(|e| DatabaseError::Rusqlite(e.into()))?;
        library
            .busy_timeout(std::time::Duration::from_millis(10))
            .map_err(|e| DatabaseError::Rusqlite(e.into()))?;
        let library = std::sync::Arc::new(tokio::sync::Mutex::new(library));
        Ok(Self {
            inner: RusqliteDatabase::new(library),
        })
    }
}

#[async_trait]
impl Database for SimulationDatabase {
    async fn query(&self, query: &SelectQuery<'_>) -> Result<Vec<Row>, DatabaseError> {
        self.inner.query(query).await
    }

    async fn query_first(&self, query: &SelectQuery<'_>) -> Result<Option<Row>, DatabaseError> {
        self.inner.query_first(query).await
    }

    async fn exec_update(
        &self,
        statement: &UpdateStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        self.inner.exec_update(statement).await
    }

    async fn exec_update_first(
        &self,
        statement: &UpdateStatement<'_>,
    ) -> Result<Option<Row>, DatabaseError> {
        self.inner.exec_update_first(statement).await
    }

    async fn exec_insert(&self, statement: &InsertStatement<'_>) -> Result<Row, DatabaseError> {
        self.inner.exec_insert(statement).await
    }

    async fn exec_upsert(
        &self,
        statement: &UpsertStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        self.inner.exec_upsert(statement).await
    }

    async fn exec_upsert_first(
        &self,
        statement: &UpsertStatement<'_>,
    ) -> Result<Row, DatabaseError> {
        self.inner.exec_upsert_first(statement).await
    }

    async fn exec_upsert_multi(
        &self,
        statement: &UpsertMultiStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        self.inner.exec_upsert_multi(statement).await
    }

    async fn exec_delete(
        &self,
        statement: &DeleteStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        self.inner.exec_delete(statement).await
    }

    async fn exec_delete_first(
        &self,
        statement: &DeleteStatement<'_>,
    ) -> Result<Option<Row>, DatabaseError> {
        self.inner.exec_delete_first(statement).await
    }

    async fn exec_raw(&self, statement: &str) -> Result<(), DatabaseError> {
        self.inner.exec_raw(statement).await
    }
}
