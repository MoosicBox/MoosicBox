//! `PostgreSQL` schema introspection for the raw `tokio-postgres` backend
//!
//! The catalog SQL and mapping logic are shared with the sqlx backend in
//! [`crate::postgres_common`]; this module only adapts a
//! [`tokio_postgres::Client`] to [`PgCatalog`].
//!
//! Introspection queries `pg_catalog` directly, so it is schema-aware: the
//! table-oriented methods (`table_exists`, `get_table_info`, ...) use the
//! connection's default schema (`current_schema()`), while the relation
//! methods accept any schema. Data type mapping never fails; types without a
//! [`DataType`](crate::schema::DataType) variant map to `DataType::Custom`
//! and [`ColumnInfo::native_type`] carries the exact `format_type()` name.

use async_trait::async_trait;
use tokio_postgres::Client;

use crate::DatabaseError;
use crate::postgres_common::PgCatalog;
use crate::schema::{
    ColumnInfo, DatabaseInfo, RelationRef, RelationSummary, SchemaInfo, TableInfo,
};

/// [`PgCatalog`] adapter over a borrowed `tokio-postgres` client.
pub struct TokioCatalog<'a>(pub &'a Client);

#[async_trait]
impl PgCatalog for TokioCatalog<'_> {
    async fn catalog_rows(
        &mut self,
        sql: &str,
        params: &[Option<&str>],
    ) -> Result<Vec<Vec<Option<String>>>, DatabaseError> {
        let params: Vec<&(dyn tokio_postgres::types::ToSql + Sync)> = params
            .iter()
            .map(|p| p as &(dyn tokio_postgres::types::ToSql + Sync))
            .collect();
        let rows = self.0.query(sql, &params).await.map_err(|e| {
            DatabaseError::Postgres(crate::postgres::postgres::PostgresDatabaseError::from(e))
        })?;
        rows.iter()
            .map(|row| {
                (0..row.len())
                    .map(|i| {
                        row.try_get::<_, Option<String>>(i).map_err(|e| {
                            DatabaseError::Postgres(
                                crate::postgres::postgres::PostgresDatabaseError::from(e),
                            )
                        })
                    })
                    .collect()
            })
            .collect()
    }
}

/// Check if a table exists in the default schema
pub async fn postgres_table_exists(
    client: &Client,
    table_name: &str,
) -> Result<bool, DatabaseError> {
    crate::postgres_common::pg_table_exists(&mut TokioCatalog(client), table_name).await
}

/// List all table names in the default schema
pub async fn postgres_list_tables(client: &Client) -> Result<Vec<String>, DatabaseError> {
    crate::postgres_common::pg_list_tables(&mut TokioCatalog(client)).await
}

/// Get column metadata for a table in the default schema
pub async fn postgres_get_table_columns(
    client: &Client,
    table_name: &str,
) -> Result<Vec<ColumnInfo>, DatabaseError> {
    crate::postgres_common::pg_get_table_columns(&mut TokioCatalog(client), table_name).await
}

/// Check if a column exists in a table in the default schema
pub async fn postgres_column_exists(
    client: &Client,
    table_name: &str,
    column_name: &str,
) -> Result<bool, DatabaseError> {
    crate::postgres_common::pg_column_exists(&mut TokioCatalog(client), table_name, column_name)
        .await
}

/// Get full table information in the default schema
pub async fn postgres_get_table_info(
    client: &Client,
    table_name: &str,
) -> Result<Option<TableInfo>, DatabaseError> {
    crate::postgres_common::pg_get_table_info(&mut TokioCatalog(client), table_name).await
}

/// List all schemas
pub async fn postgres_list_schemas(client: &Client) -> Result<Vec<SchemaInfo>, DatabaseError> {
    crate::postgres_common::pg_list_schemas(&mut TokioCatalog(client)).await
}

/// List the connectable, non-template databases of the server
pub async fn postgres_list_databases(client: &Client) -> Result<Vec<DatabaseInfo>, DatabaseError> {
    crate::postgres_common::pg_list_databases(&mut TokioCatalog(client)).await
}

/// List relations in a schema (`None` = default schema)
pub async fn postgres_list_relations(
    client: &Client,
    schema: Option<&str>,
) -> Result<Vec<RelationSummary>, DatabaseError> {
    crate::postgres_common::pg_list_relations(&mut TokioCatalog(client), schema).await
}

/// Get full information about any relation in any schema
pub async fn postgres_get_relation_info(
    client: &Client,
    relation: &RelationRef,
) -> Result<Option<TableInfo>, DatabaseError> {
    crate::postgres_common::pg_get_relation_info(&mut TokioCatalog(client), relation).await
}

/// Get the definition of a view or materialized view
pub async fn postgres_get_view_definition(
    client: &Client,
    relation: &RelationRef,
) -> Result<Option<String>, DatabaseError> {
    crate::postgres_common::pg_get_view_definition(&mut TokioCatalog(client), relation).await
}
