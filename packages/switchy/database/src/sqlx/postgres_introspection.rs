//! `PostgreSQL` schema introspection for the sqlx backend
//!
//! The catalog SQL and mapping logic are shared with the raw `tokio-postgres`
//! backend in [`crate::postgres_common`]; this module only adapts a
//! [`PgConnection`] to [`PgCatalog`].
//!
//! Introspection queries `pg_catalog` directly, so it is schema-aware: the
//! table-oriented methods (`table_exists`, `get_table_info`, ...) use the
//! connection's default schema (`current_schema()`), while the relation
//! methods accept any schema. Data type mapping never fails; types without a
//! [`DataType`](crate::schema::DataType) variant map to `DataType::Custom`
//! and [`ColumnInfo::native_type`] carries the exact `format_type()` name.

use async_trait::async_trait;
use sqlx::{PgConnection, Row};

use crate::DatabaseError;
use crate::postgres_common::PgCatalog;
use crate::schema::{ColumnInfo, RelationRef, RelationSummary, SchemaInfo, TableInfo};

fn sqlx_error(e: sqlx::Error) -> DatabaseError {
    DatabaseError::PostgresSqlx(crate::sqlx::postgres::SqlxDatabaseError::from(e))
}

#[async_trait]
impl PgCatalog for PgConnection {
    async fn catalog_rows(
        &mut self,
        sql: &str,
        params: &[Option<&str>],
    ) -> Result<Vec<Vec<Option<String>>>, DatabaseError> {
        let mut query = sqlx::query(sqlx::AssertSqlSafe(sql.to_string()));
        for param in params {
            query = query.bind(param.map(ToString::to_string));
        }
        let rows = query.fetch_all(&mut *self).await.map_err(sqlx_error)?;
        rows.iter()
            .map(|row| {
                (0..row.len())
                    .map(|i| row.try_get::<Option<String>, _>(i).map_err(sqlx_error))
                    .collect()
            })
            .collect()
    }
}

/// Check if a table exists in the default schema
pub async fn postgres_sqlx_table_exists(
    conn: &mut PgConnection,
    table_name: &str,
) -> Result<bool, DatabaseError> {
    crate::postgres_common::pg_table_exists(conn, table_name).await
}

/// List all table names in the default schema
pub async fn postgres_sqlx_list_tables(
    conn: &mut PgConnection,
) -> Result<Vec<String>, DatabaseError> {
    crate::postgres_common::pg_list_tables(conn).await
}

/// Get column metadata for a table in the default schema
pub async fn postgres_sqlx_get_table_columns(
    conn: &mut PgConnection,
    table_name: &str,
) -> Result<Vec<ColumnInfo>, DatabaseError> {
    crate::postgres_common::pg_get_table_columns(conn, table_name).await
}

/// Check if a column exists in a table in the default schema
pub async fn postgres_sqlx_column_exists(
    conn: &mut PgConnection,
    table_name: &str,
    column_name: &str,
) -> Result<bool, DatabaseError> {
    crate::postgres_common::pg_column_exists(conn, table_name, column_name).await
}

/// Get full table information in the default schema
pub async fn postgres_sqlx_get_table_info(
    conn: &mut PgConnection,
    table_name: &str,
) -> Result<Option<TableInfo>, DatabaseError> {
    crate::postgres_common::pg_get_table_info(conn, table_name).await
}

/// List all schemas
pub async fn postgres_sqlx_list_schemas(
    conn: &mut PgConnection,
) -> Result<Vec<SchemaInfo>, DatabaseError> {
    crate::postgres_common::pg_list_schemas(conn).await
}

/// List relations in a schema (`None` = default schema)
pub async fn postgres_sqlx_list_relations(
    conn: &mut PgConnection,
    schema: Option<&str>,
) -> Result<Vec<RelationSummary>, DatabaseError> {
    crate::postgres_common::pg_list_relations(conn, schema).await
}

/// Get full information about any relation in any schema
pub async fn postgres_sqlx_get_relation_info(
    conn: &mut PgConnection,
    relation: &RelationRef,
) -> Result<Option<TableInfo>, DatabaseError> {
    crate::postgres_common::pg_get_relation_info(conn, relation).await
}

/// Get the definition of a view or materialized view
pub async fn postgres_sqlx_get_view_definition(
    conn: &mut PgConnection,
    relation: &RelationRef,
) -> Result<Option<String>, DatabaseError> {
    crate::postgres_common::pg_get_view_definition(conn, relation).await
}
