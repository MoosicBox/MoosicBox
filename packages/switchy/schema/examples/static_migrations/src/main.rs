//! # Static Migrations Example
//!
//! This example demonstrates the most common usage patterns with static lifetimes,
//! covering all three discovery methods: embedded, directory, and code-based migrations.

use std::sync::Arc;

use async_trait::async_trait;
use switchy_database::{
    Database,
    schema::{Column, DataType, create_table},
};
use switchy_schema::{
    Result,
    discovery::code::{CodeMigration, CodeMigrationSource},
    migration::{Migration, MigrationSource},
};

/// Example of a custom migration with static lifetime
struct CustomMigration {
    id: String,
    up_sql: String,
    down_sql: Option<String>,
}

impl CustomMigration {
    /// Creates a new custom migration.
    ///
    /// # Parameters
    ///
    /// * `id` - Unique identifier for the migration
    /// * `up_sql` - SQL to execute when applying the migration
    /// * `down_sql` - Optional SQL to execute when rolling back the migration
    #[must_use]
    fn new(
        id: impl Into<String>,
        up_sql: impl Into<String>,
        down_sql: Option<impl Into<String>>,
    ) -> Self {
        Self {
            id: id.into(),
            up_sql: up_sql.into(),
            down_sql: down_sql.map(Into::into),
        }
    }
}

#[async_trait]
impl Migration<'static> for CustomMigration {
    /// Returns the unique identifier for this migration.
    fn id(&self) -> &str {
        &self.id
    }

    /// Applies the migration.
    ///
    /// # Errors
    ///
    /// * Returns an error if the SQL execution fails
    async fn up(&self, db: &dyn Database) -> Result<()> {
        if !self.up_sql.trim().is_empty() {
            db.exec_raw(&self.up_sql).await?;
        }
        Ok(())
    }

    /// Rolls back the migration.
    ///
    /// # Errors
    ///
    /// * Returns an error if the SQL execution fails
    async fn down(&self, db: &dyn Database) -> Result<()> {
        if let Some(down_sql) = &self.down_sql
            && !down_sql.trim().is_empty()
        {
            db.exec_raw(down_sql).await?;
        }
        Ok(())
    }

    /// Returns an optional description of the migration.
    fn description(&self) -> Option<&str> {
        Some("Custom migration example")
    }
}

/// Example migration source that owns its migrations
struct CustomMigrationSource {
    migrations: Vec<CustomMigration>,
}

impl CustomMigrationSource {
    /// Creates a new migration source with example migrations.
    #[must_use]
    fn new() -> Self {
        Self {
            migrations: vec![
                CustomMigration::new(
                    "001_create_users",
                    "CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL, email TEXT UNIQUE)",
                    Some("DROP TABLE users"),
                ),
                CustomMigration::new(
                    "002_create_posts",
                    "CREATE TABLE posts (id INTEGER PRIMARY KEY, user_id INTEGER, title TEXT, content TEXT, FOREIGN KEY(user_id) REFERENCES users(id))",
                    Some("DROP TABLE posts"),
                ),
                CustomMigration::new(
                    "003_add_indexes",
                    "CREATE INDEX idx_posts_user_id ON posts(user_id); CREATE INDEX idx_users_email ON users(email)",
                    Some("DROP INDEX idx_posts_user_id; DROP INDEX idx_users_email"),
                ),
            ],
        }
    }
}

#[async_trait]
impl MigrationSource<'static> for CustomMigrationSource {
    /// Returns the list of available migrations.
    ///
    /// # Errors
    ///
    /// * This implementation never returns an error, but the trait requires Result return type
    async fn migrations(&self) -> Result<Vec<Arc<dyn Migration<'static> + 'static>>> {
        let migrations: Vec<Arc<dyn Migration<'static> + 'static>> = self
            .migrations
            .iter()
            .map(|m| {
                Arc::new(CustomMigration::new(
                    m.id.clone(),
                    m.up_sql.clone(),
                    m.down_sql.clone(),
                )) as Arc<dyn Migration<'static> + 'static>
            })
            .collect();
        Ok(migrations)
    }
}

/// Demonstrates static lifetime migrations with various discovery methods.
///
/// # Errors
///
/// * Returns an error if migration discovery or execution fails
#[switchy_async::main]
async fn main() -> Result<()> {
    println!("Static Migrations Example");
    println!("=========================");

    // Example 1: Custom Migration Source
    println!("\n1. Custom Migration Source:");
    let custom_source = CustomMigrationSource::new();
    let custom_migrations = custom_source.migrations().await?;

    for migration in &custom_migrations {
        println!("  - Migration: {}", migration.id());
        if let Some(desc) = migration.description() {
            println!("    Description: {}", desc);
        }
    }

    // Example 2: Directory Migration Source
    println!("\n2. Directory Migration Source:");
    // Note: This would work if the directory exists
    // let directory_source = DirectoryMigrationSource::from_path(PathBuf::from("./migrations"));
    // let directory_migrations = directory_source.migrations().await?;
    println!("  - Would load migrations from ./migrations directory");
    println!("  - Each subdirectory becomes a migration");
    println!("  - up.sql and down.sql files define the migration logic");

    // Example 3: Code Migration Source with Raw SQL
    println!("\n3. Code Migration Source (Raw SQL):");
    let mut code_source = CodeMigrationSource::new();

    code_source.add_migration(CodeMigration::new(
        "001_create_categories".to_string(),
        Box::new(
            "CREATE TABLE categories (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE)"
                .to_string(),
        ),
        Some(Box::new("DROP TABLE categories".to_string())),
    ));

    code_source.add_migration(CodeMigration::new(
        "002_create_tags".to_string(),
        Box::new(
            "CREATE TABLE tags (id INTEGER PRIMARY KEY, name TEXT NOT NULL, color TEXT)"
                .to_string(),
        ),
        Some(Box::new("DROP TABLE tags".to_string())),
    ));

    let code_migrations = code_source.migrations().await?;
    for migration in &code_migrations {
        println!("  - Code Migration: {}", migration.id());
    }

    // Example 4: Code Migration Source with Query Builders
    println!("\n4. Code Migration Source (Query Builders):");
    let mut builder_source = CodeMigrationSource::new();

    // Create a table using the query builder
    let create_products_table = create_table("products")
        .if_not_exists(true)
        .column(Column {
            name: "id".to_string(),
            nullable: false,
            auto_increment: true,
            data_type: DataType::Int,
            default: None,
        })
        .column(Column {
            name: "name".to_string(),
            nullable: false,
            auto_increment: false,
            data_type: DataType::Text,
            default: None,
        })
        .column(Column {
            name: "price".to_string(),
            nullable: false,
            auto_increment: false,
            data_type: DataType::Real,
            default: None,
        })
        .column(Column {
            name: "category_id".to_string(),
            nullable: true,
            auto_increment: false,
            data_type: DataType::Int,
            default: None,
        })
        .primary_key("id");

    builder_source.add_migration(CodeMigration::new(
        "001_create_products".to_string(),
        Box::new(create_products_table),
        None, // No down migration for this example
    ));

    let builder_migrations = builder_source.migrations().await?;
    for migration in &builder_migrations {
        println!("  - Builder Migration: {}", migration.id());
    }

    println!("\n✅ All static migration examples completed successfully!");
    println!("💡 These migrations all use 'static lifetimes and own their data.");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[switchy_async::test]
    async fn test_custom_migration() {
        let migration = CustomMigration::new(
            "test_migration",
            "CREATE TABLE test (id INTEGER)",
            Some("DROP TABLE test"),
        );

        assert_eq!(migration.id(), "test_migration");
        assert_eq!(migration.description(), Some("Custom migration example"));
    }

    #[switchy_async::test]
    async fn test_custom_migration_source() {
        let source = CustomMigrationSource::new();
        let migrations = source.migrations().await.unwrap();

        assert_eq!(migrations.len(), 3);
        assert_eq!(migrations[0].id(), "001_create_users");
        assert_eq!(migrations[1].id(), "002_create_posts");
        assert_eq!(migrations[2].id(), "003_add_indexes");
    }

    #[switchy_async::test]
    async fn test_code_migration_source() {
        let mut source = CodeMigrationSource::new();

        source.add_migration(CodeMigration::new(
            "001_test".to_string(),
            Box::new("CREATE TABLE test (id INTEGER)".to_string()),
            None,
        ));

        let migrations = source.migrations().await.unwrap();
        assert_eq!(migrations.len(), 1);
    }
}
