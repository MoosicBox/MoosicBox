use async_trait::async_trait;

use crate::{
    Database, DatabaseError, Row,
    query::{
        DeleteStatement, InsertStatement, SelectQuery, UpdateStatement, UpsertMultiStatement,
        UpsertStatement,
    },
};

#[allow(clippy::module_name_repetitions)]
#[derive(Debug)]
pub struct SimulationDatabase {}

impl Default for SimulationDatabase {
    fn default() -> Self {
        Self::new()
    }
}

impl SimulationDatabase {
    #[must_use]
    pub const fn new() -> Self {
        Self {}
    }
}

#[async_trait]
impl Database for SimulationDatabase {
    async fn query(&self, _query: &SelectQuery<'_>) -> Result<Vec<Row>, DatabaseError> {
        Ok(vec![])
    }

    async fn query_first(&self, _query: &SelectQuery<'_>) -> Result<Option<Row>, DatabaseError> {
        Ok(None)
    }

    async fn exec_update(
        &self,
        _statement: &UpdateStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        Ok(vec![])
    }

    async fn exec_update_first(
        &self,
        _statement: &UpdateStatement<'_>,
    ) -> Result<Option<Row>, DatabaseError> {
        Ok(None)
    }

    async fn exec_insert(&self, _statement: &InsertStatement<'_>) -> Result<Row, DatabaseError> {
        Ok(Row { columns: vec![] })
    }

    async fn exec_upsert(
        &self,
        _statement: &UpsertStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        Ok(vec![])
    }

    async fn exec_upsert_first(
        &self,
        _statement: &UpsertStatement<'_>,
    ) -> Result<Row, DatabaseError> {
        Ok(Row { columns: vec![] })
    }

    async fn exec_upsert_multi(
        &self,
        _statement: &UpsertMultiStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        Ok(vec![])
    }

    async fn exec_delete(
        &self,
        _statement: &DeleteStatement<'_>,
    ) -> Result<Vec<Row>, DatabaseError> {
        Ok(vec![])
    }

    async fn exec_delete_first(
        &self,
        _statement: &DeleteStatement<'_>,
    ) -> Result<Option<Row>, DatabaseError> {
        Ok(None)
    }
}
