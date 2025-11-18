//! # Borrowed Migrations Example
//!
//! This example demonstrates advanced usage patterns with non-static lifetimes,
//! showing how to create migrations that borrow data and use explicit lifetime management.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

use async_trait::async_trait;
use std::{collections::HashMap, sync::Arc};
use switchy_database::{
    Database,
    schema::{Column, DataType, create_table},
};
use switchy_schema::{
    Result,
    discovery::code::CodeMigration,
    migration::{Migration, MigrationSource},
};

/// Configuration structure that migrations can borrow from
#[derive(Debug)]
struct DatabaseConfig {
    table_prefix: String,
    default_charset: String,
    tables: HashMap<String, TableConfig>,
}

#[derive(Debug)]
struct TableConfig {
    name: String,
    columns: Vec<ColumnConfig>,
    primary_key: String,
}

#[derive(Debug)]
struct ColumnConfig {
    name: String,
    data_type: DataType,
    nullable: bool,
}

impl DatabaseConfig {
    #[must_use]
    #[allow(clippy::missing_const_for_fn)] // HashMap::new() is not const
    fn new() -> Self {
        let mut tables = HashMap::new();

        tables.insert(
            "users".to_string(),
            TableConfig {
                name: "users".to_string(),
                columns: vec![
                    ColumnConfig {
                        name: "id".to_string(),
                        data_type: DataType::Int,
                        nullable: false,
                    },
                    ColumnConfig {
                        name: "username".to_string(),
                        data_type: DataType::VarChar(50),
                        nullable: false,
                    },
                    ColumnConfig {
                        name: "email".to_string(),
                        data_type: DataType::VarChar(255),
                        nullable: false,
                    },
                    ColumnConfig {
                        name: "created_at".to_string(),
                        data_type: DataType::DateTime,
                        nullable: false,
                    },
                ],
                primary_key: "id".to_string(),
            },
        );

        tables.insert(
            "posts".to_string(),
            TableConfig {
                name: "posts".to_string(),
                columns: vec![
                    ColumnConfig {
                        name: "id".to_string(),
                        data_type: DataType::Int,
                        nullable: false,
                    },
                    ColumnConfig {
                        name: "user_id".to_string(),
                        data_type: DataType::Int,
                        nullable: false,
                    },
                    ColumnConfig {
                        name: "title".to_string(),
                        data_type: DataType::VarChar(200),
                        nullable: false,
                    },
                    ColumnConfig {
                        name: "content".to_string(),
                        data_type: DataType::Text,
                        nullable: true,
                    },
                ],
                primary_key: "id".to_string(),
            },
        );

        Self {
            table_prefix: "app_".to_string(),
            default_charset: "utf8mb4".to_string(),
            tables,
        }
    }
}

/// Migration that borrows from configuration
struct ConfigBasedMigration<'a> {
    id: String,
    config: &'a DatabaseConfig,
    table_name: &'a str,
}

impl<'a> ConfigBasedMigration<'a> {
    #[must_use]
    const fn new(id: String, config: &'a DatabaseConfig, table_name: &'a str) -> Self {
        Self {
            id,
            config,
            table_name,
        }
    }
}

#[async_trait]
impl<'a> Migration<'a> for ConfigBasedMigration<'a> {
    fn id(&self) -> &str {
        &self.id
    }

    async fn up(&self, db: &dyn Database) -> Result<()> {
        if let Some(table_config) = self.config.tables.get(self.table_name) {
            let full_table_name = format!("{}{}", self.config.table_prefix, table_config.name);

            let mut create_stmt = create_table(&full_table_name).if_not_exists(true);

            for col_config in &table_config.columns {
                create_stmt = create_stmt.column(Column {
                    name: col_config.name.clone(),
                    nullable: col_config.nullable,
                    auto_increment: col_config.name == table_config.primary_key,
                    data_type: col_config.data_type.clone(),
                    default: None,
                });
            }

            create_stmt = create_stmt.primary_key(&table_config.primary_key);

            create_stmt.execute(db).await?;
        }
        Ok(())
    }

    async fn down(&self, db: &dyn Database) -> Result<()> {
        if let Some(table_config) = self.config.tables.get(self.table_name) {
            let full_table_name = format!("{}{}", self.config.table_prefix, table_config.name);
            let drop_sql = format!("DROP TABLE IF EXISTS {full_table_name}");
            db.exec_raw(&drop_sql).await?;
        }
        Ok(())
    }

    fn description(&self) -> Option<&str> {
        Some("Configuration-based table creation")
    }
}

/// Migration source that borrows from configuration
struct ConfigBasedMigrationSource<'a> {
    config: &'a DatabaseConfig,
}

impl<'a> ConfigBasedMigrationSource<'a> {
    #[must_use]
    const fn new(config: &'a DatabaseConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl<'a> MigrationSource<'a> for ConfigBasedMigrationSource<'a> {
    async fn migrations(&self) -> Result<Vec<Arc<dyn Migration<'a> + 'a>>> {
        let mut migrations: Vec<Arc<dyn Migration<'a> + 'a>> = Vec::new();

        // Create migrations for each table in the configuration
        for table_name in self.config.tables.keys() {
            let migration = ConfigBasedMigration::new(
                format!("create_{table_name}"),
                self.config,
                table_name,
            );
            migrations.push(Arc::new(migration));
        }

        // Sort by migration ID for deterministic ordering
        migrations.sort_by(|a, b| a.id().cmp(b.id()));

        Ok(migrations)
    }
}

/// Function that creates a migration borrowing from external data
fn create_table_migration<'a>(
    table_name: &'a str,
    columns: &'a [(&'a str, DataType)],
    primary_key: &'a str,
) -> CodeMigration<'a> {
    let mut create_stmt = create_table(table_name).if_not_exists(true);

    for (col_name, data_type) in columns {
        create_stmt = create_stmt.column(Column {
            name: (*col_name).to_string(),
            nullable: false,
            auto_increment: *col_name == primary_key,
            data_type: data_type.clone(),
            default: None,
        });
    }

    create_stmt = create_stmt.primary_key(primary_key);

    CodeMigration::new(
        format!("create_{table_name}"),
        Box::new(create_stmt),
        None,
    )
}

/// Function that creates multiple related migrations
fn create_blog_schema_migrations<'a>() -> Vec<CodeMigration<'a>> {
    vec![
        create_table_migration(
            "authors",
            &[
                ("id", DataType::Int),
                ("name", DataType::VarChar(100)),
                ("email", DataType::VarChar(255)),
            ],
            "id",
        ),
        create_table_migration(
            "categories",
            &[
                ("id", DataType::Int),
                ("name", DataType::VarChar(50)),
                ("slug", DataType::VarChar(50)),
            ],
            "id",
        ),
        create_table_migration(
            "articles",
            &[
                ("id", DataType::Int),
                ("author_id", DataType::Int),
                ("category_id", DataType::Int),
                ("title", DataType::VarChar(200)),
                ("content", DataType::Text),
            ],
            "id",
        ),
    ]
}

#[tokio::main]
async fn main() -> Result<()> {
    println!("Borrowed Migrations Example");
    println!("===========================");

    // Example 1: Configuration-based migrations
    println!("\n1. Configuration-Based Migrations:");
    let config = DatabaseConfig::new();
    let config_source = ConfigBasedMigrationSource::new(&config);
    let config_migrations = config_source.migrations().await?;

    println!("  Configuration:");
    println!("    - Table prefix: {}", config.table_prefix);
    println!("    - Default charset: {}", config.default_charset);
    println!("    - Tables defined: {}", config.tables.len());

    println!("  Generated migrations:");
    for migration in &config_migrations {
        println!(
            "    - {}: {}",
            migration.id(),
            migration.description().unwrap_or("No description")
        );
    }

    // Example 2: Function-generated migrations with borrowed data
    println!("\n2. Function-Generated Migrations:");
    let table_name = "products";
    let columns = [
        ("id", DataType::Int),
        ("name", DataType::VarChar(100)),
        ("price", DataType::Decimal(10, 2)),
        ("description", DataType::Text),
    ];
    let primary_key = "id";

    let product_migration = create_table_migration(table_name, &columns, primary_key);
    println!("  - Generated migration: {}", product_migration.id());
    println!("  - Borrows table name: '{table_name}'");
    println!("  - Borrows column definitions and primary key");

    // Example 3: Schema generation from borrowed data
    println!("\n3. Blog Schema Migrations:");
    let blog_migrations = create_blog_schema_migrations();

    println!(
        "  Generated {} migrations for blog schema:",
        blog_migrations.len()
    );
    for migration in &blog_migrations {
        println!("    - {}", migration.id());
    }

    // Example 4: Temporary migration source
    println!("\n4. Temporary Migration Source:");
    {
        let temp_config = DatabaseConfig::new();
        let temp_source = ConfigBasedMigrationSource::new(&temp_config);
        let temp_migrations = temp_source.migrations().await?;

        println!(
            "  - Created temporary source with {} migrations",
            temp_migrations.len()
        );
        println!("  - Source borrows from config with lifetime 'a");
        println!("  - Migrations are valid only within this scope");
    }
    println!("  - Temporary source and migrations are now dropped");

    println!("\n✅ All borrowed migration examples completed successfully!");
    println!("💡 These migrations demonstrate:");
    println!("   - Borrowing data with explicit lifetimes");
    println!("   - Configuration-driven migration generation");
    println!("   - Temporary migration sources");
    println!("   - Function-based migration creation");

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[switchy_async::test]
    async fn test_config_based_migration() {
        let config = DatabaseConfig::new();
        let migration = ConfigBasedMigration::new("test_migration".to_string(), &config, "users");

        assert_eq!(migration.id(), "test_migration");
        assert_eq!(
            migration.description(),
            Some("Configuration-based table creation")
        );
    }

    #[switchy_async::test]
    async fn test_config_based_migration_source() {
        let config = DatabaseConfig::new();
        let source = ConfigBasedMigrationSource::new(&config);
        let migrations = source.migrations().await.unwrap();

        assert_eq!(migrations.len(), 2); // users and posts tables

        // Check that migrations are sorted by ID
        let ids: Vec<&str> = migrations.iter().map(|m| m.id()).collect();
        let mut sorted_ids = ids.clone();
        sorted_ids.sort();
        assert_eq!(ids, sorted_ids);
    }

    #[test]
    fn test_create_table_migration() {
        let columns = [("id", DataType::Int), ("name", DataType::Text)];

        let migration = create_table_migration("test_table", &columns, "id");
        assert_eq!(migration.id(), "create_test_table");
    }

    #[test]
    fn test_blog_schema_migrations() {
        let migrations = create_blog_schema_migrations();
        assert_eq!(migrations.len(), 3);

        let ids: Vec<&str> = migrations.iter().map(|m| m.id()).collect();
        assert!(ids.contains(&"create_authors"));
        assert!(ids.contains(&"create_categories"));
        assert!(ids.contains(&"create_articles"));
    }
}
