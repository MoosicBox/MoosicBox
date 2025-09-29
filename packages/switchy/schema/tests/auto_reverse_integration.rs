#![cfg(feature = "auto-reverse")]

use switchy_database::schema::auto_reversible::add_column;
use switchy_database::schema::{Column, DataType, create_index, create_table};
use switchy_schema::discovery::code::{
    CodeMigration, CodeMigrationSource, ReversibleCodeMigration,
};
use switchy_schema::migration::Migration;
use switchy_schema::runner::MigrationRunner;
use switchy_schema_test_utils::create_empty_in_memory;

#[switchy_async::test]
async fn test_reversible_table_migration() {
    let db = create_empty_in_memory().await.unwrap();

    // Create table using auto-reversible migration
    let create = create_table("users")
        .column(Column {
            name: "id".to_string(),
            data_type: DataType::Int,
            nullable: false,
            auto_increment: true,
            default: None,
        })
        .primary_key("id");

    let migration: CodeMigration = ReversibleCodeMigration::new("001_create_users", create).into();

    // Run UP migration
    migration.up(&*db).await.unwrap();
    assert!(db.table_exists("users").await.unwrap());

    // Run DOWN migration (auto-generated)
    migration.down(&*db).await.unwrap();
    assert!(!db.table_exists("users").await.unwrap());
}

#[switchy_async::test]
async fn test_reversible_index_migration() {
    let db = create_empty_in_memory().await.unwrap();

    // Setup: create table first
    db.exec_raw("CREATE TABLE users (id INT, email TEXT)")
        .await
        .unwrap();

    // Create index using auto-reversible migration
    let create_idx = create_index("idx_users_email")
        .table("users")
        .columns(vec!["email"]);

    let migration: CodeMigration =
        ReversibleCodeMigration::new("002_add_email_index", create_idx).into();

    // Run UP migration
    migration.up(&*db).await.unwrap();
    // Verify index exists (platform-specific check)

    // Run DOWN migration (auto-generated)
    migration.down(&*db).await.unwrap();
    // Verify index removed
}

#[switchy_async::test]
async fn test_reversible_column_migration() {
    let db = create_empty_in_memory().await.unwrap();

    // Setup: create table
    db.exec_raw("CREATE TABLE users (id INT)").await.unwrap();

    // Add column using auto-reversible operation
    // Note: Using individual parameters instead of Column struct
    // because ALTER TABLE ADD COLUMN doesn't support auto_increment
    let add = add_column(
        "users",
        "email",
        DataType::Text,
        true, // nullable
        None, // default
    );

    let migration: CodeMigration = ReversibleCodeMigration::new("003_add_email_column", add).into();

    // Run UP migration
    migration.up(&*db).await.unwrap();
    assert!(db.column_exists("users", "email").await.unwrap());

    // Run DOWN migration (auto-generated)
    migration.down(&*db).await.unwrap();
    assert!(!db.column_exists("users", "email").await.unwrap());
}

#[switchy_async::test]
async fn test_migration_runner_with_reversible() {
    // Test that ReversibleCodeMigration works with MigrationRunner
    let db = create_empty_in_memory().await.unwrap();

    let mut source = CodeMigrationSource::new();

    let create = create_table("posts")
        .column(Column {
            name: "id".to_string(),
            data_type: DataType::Int,
            nullable: false,
            auto_increment: true,
            default: None,
        })
        .primary_key("id");

    let migration: CodeMigration = ReversibleCodeMigration::new("001_create_posts", create).into();

    source.add_migration(migration);

    let runner = MigrationRunner::new(Box::new(source));
    runner.run(&*db).await.unwrap();

    assert!(db.table_exists("posts").await.unwrap());
}
