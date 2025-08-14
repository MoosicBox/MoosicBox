use crate::Result;
use switchy_database::{
    Database, DatabaseValue,
    query::FilterableQuery,
    schema::{Column, DataType},
};

pub const DEFAULT_MIGRATIONS_TABLE: &str = "__switchy_migrations";

pub struct VersionTracker {
    table_name: String,
}

impl VersionTracker {
    #[must_use]
    pub fn new() -> Self {
        Self {
            table_name: DEFAULT_MIGRATIONS_TABLE.to_string(),
        }
    }

    #[must_use]
    #[allow(clippy::missing_const_for_fn)]
    pub fn with_table_name(table_name: String) -> Self {
        Self { table_name }
    }

    #[must_use]
    pub fn table_name(&self) -> &str {
        &self.table_name
    }

    /// Ensure the migrations tracking table exists
    ///
    /// # Errors
    ///
    /// * If the table creation fails
    ///
    /// # Limitations
    ///
    /// * Currently only works with the default table name due to `switchy_database` limitations
    pub async fn ensure_table_exists(&self, db: &dyn Database) -> Result<()> {
        // TODO: This is a limitation - switchy_database requires static table names
        // For now, we only support the default table name
        if self.table_name != DEFAULT_MIGRATIONS_TABLE {
            return Err(crate::MigrationError::Execution(
                "Custom migration table names are not yet supported due to switchy_database limitations".to_string()
            ));
        }

        db.create_table(DEFAULT_MIGRATIONS_TABLE)
            .if_not_exists(true)
            .column(Column {
                name: "name".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .column(Column {
                name: "run_on".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::DateTime,
                default: Some(DatabaseValue::Now),
            })
            .execute(db)
            .await?;

        Ok(())
    }

    /// Check if a migration has been applied
    ///
    /// # Errors
    ///
    /// * If the database query fails
    ///
    /// # Limitations
    ///
    /// * Currently only works with the default table name due to `switchy_database` limitations
    pub async fn is_migration_applied(
        &self,
        db: &dyn Database,
        migration_id: &str,
    ) -> Result<bool> {
        // TODO: This is a limitation - switchy_database requires static table names
        // For now, we only support the default table name
        if self.table_name != DEFAULT_MIGRATIONS_TABLE {
            return Err(crate::MigrationError::Execution(
                "Custom migration table names are not yet supported due to switchy_database limitations".to_string()
            ));
        }

        let results = db
            .select(DEFAULT_MIGRATIONS_TABLE)
            .columns(&["name"])
            .where_eq("name", migration_id)
            .execute(db)
            .await?;

        Ok(!results.is_empty())
    }

    /// Record a migration as completed
    ///
    /// # Errors
    ///
    /// * If the database insert fails
    ///
    /// # Limitations
    ///
    /// * Currently only works with the default table name due to `switchy_database` limitations
    pub async fn record_migration(&self, db: &dyn Database, migration_id: &str) -> Result<()> {
        // TODO: This is a limitation - switchy_database requires static table names
        // For now, we only support the default table name
        if self.table_name != DEFAULT_MIGRATIONS_TABLE {
            return Err(crate::MigrationError::Execution(
                "Custom migration table names are not yet supported due to switchy_database limitations".to_string()
            ));
        }

        db.insert(DEFAULT_MIGRATIONS_TABLE)
            .value("name", migration_id)
            .execute(db)
            .await?;

        Ok(())
    }

    /// Get all applied migrations in reverse chronological order (most recent first)
    ///
    /// # Errors
    ///
    /// * If the database query fails
    ///
    /// # Limitations
    ///
    /// * Currently only works with the default table name due to `switchy_database` limitations
    pub async fn get_applied_migrations(&self, db: &dyn Database) -> Result<Vec<String>> {
        // TODO: This is a limitation - switchy_database requires static table names
        // For now, we only support the default table name
        if self.table_name != DEFAULT_MIGRATIONS_TABLE {
            return Err(crate::MigrationError::Execution(
                "Custom migration table names are not yet supported due to switchy_database limitations".to_string()
            ));
        }

        let results = db
            .select(DEFAULT_MIGRATIONS_TABLE)
            .columns(&["name"])
            .sort("run_on", switchy_database::query::SortDirection::Desc)
            .execute(db)
            .await?;

        let migration_ids = results
            .into_iter()
            .filter_map(|row| {
                row.get("name")
                    .and_then(|value| value.as_str().map(std::string::ToString::to_string))
            })
            .collect();

        Ok(migration_ids)
    }

    /// Remove a migration record (used during rollback)
    ///
    /// # Errors
    ///
    /// * If the database delete fails
    ///
    /// # Limitations
    ///
    /// * Currently only works with the default table name due to `switchy_database` limitations
    pub async fn remove_migration(&self, db: &dyn Database, migration_id: &str) -> Result<()> {
        // TODO: This is a limitation - switchy_database requires static table names
        // For now, we only support the default table name
        if self.table_name != DEFAULT_MIGRATIONS_TABLE {
            return Err(crate::MigrationError::Execution(
                "Custom migration table names are not yet supported due to switchy_database limitations".to_string()
            ));
        }

        db.delete(DEFAULT_MIGRATIONS_TABLE)
            .where_eq("name", migration_id)
            .execute(db)
            .await?;

        Ok(())
    }
}

impl Default for VersionTracker {
    fn default() -> Self {
        Self::new()
    }
}
