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
    ///
    /// # Panics
    ///
    /// * If time goes backwards
    pub fn new() -> Result<Self, DatabaseError> {
        use std::sync::atomic::AtomicU64;

        static ID: AtomicU64 = AtomicU64::new(0);

        let id = ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let db_url = format!("file:sqlx_memdb_{id}_{timestamp}:?mode=memory&cache=shared&uri=true");

        let mut connections = Vec::new();
        for _ in 0..5 {
            let conn = ::rusqlite::Connection::open(&db_url)
                .map_err(|e| DatabaseError::Rusqlite(e.into()))?;
            conn.busy_timeout(std::time::Duration::from_millis(10))
                .map_err(|e| DatabaseError::Rusqlite(e.into()))?;
            connections.push(std::sync::Arc::new(tokio::sync::Mutex::new(conn)));
        }

        Ok(Self {
            inner: RusqliteDatabase::new(connections),
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

    #[cfg(feature = "schema")]
    async fn exec_create_table(
        &self,
        statement: &crate::schema::CreateTableStatement<'_>,
    ) -> Result<(), DatabaseError> {
        self.inner.exec_create_table(statement).await
    }

    async fn begin_transaction(
        &self,
    ) -> Result<Box<dyn crate::DatabaseTransaction>, DatabaseError> {
        self.inner.begin_transaction().await
    }
}
