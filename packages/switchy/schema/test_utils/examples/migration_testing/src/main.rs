#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Migration Testing Example
//!
//! This example demonstrates how to use `switchy_schema_test_utils` to test database
//! migrations with various testing patterns.

use async_trait::async_trait;
use std::{collections::BTreeMap, sync::Arc};
use switchy_database::{Database, DatabaseError, Executable};
use switchy_schema::migration::Migration;
use switchy_schema_test_utils::{
    MigrationTestBuilder, TestError, create_empty_in_memory, mutations::MutationBuilder,
    verify_migrations_full_cycle, verify_migrations_with_mutations, verify_migrations_with_state,
};

/// Example migration that creates a users table
struct CreateUsersTable;

#[async_trait]
impl Migration<'static> for CreateUsersTable {
    fn id(&self) -> &'static str {
        "001_create_users"
    }

    async fn up(&self, db: &dyn Database) -> switchy_schema::Result<()> {
        println!("  [UP] Creating users table...");
        db.exec_raw(
            "CREATE TABLE users (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                email TEXT
            )",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, db: &dyn Database) -> switchy_schema::Result<()> {
        println!("  [DOWN] Dropping users table...");
        db.exec_raw("DROP TABLE users").await?;
        Ok(())
    }
}

/// Example migration that creates a posts table
struct CreatePostsTable;

#[async_trait]
impl Migration<'static> for CreatePostsTable {
    fn id(&self) -> &'static str {
        "002_create_posts"
    }

    async fn up(&self, db: &dyn Database) -> switchy_schema::Result<()> {
        println!("  [UP] Creating posts table...");
        db.exec_raw(
            "CREATE TABLE posts (
                id INTEGER PRIMARY KEY,
                user_id INTEGER NOT NULL,
                title TEXT NOT NULL,
                content TEXT,
                FOREIGN KEY (user_id) REFERENCES users(id)
            )",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, db: &dyn Database) -> switchy_schema::Result<()> {
        println!("  [DOWN] Dropping posts table...");
        db.exec_raw("DROP TABLE posts").await?;
        Ok(())
    }
}

/// Example migration that adds an index on posts
struct AddPostsUserIndex;

#[async_trait]
impl Migration<'static> for AddPostsUserIndex {
    fn id(&self) -> &'static str {
        "003_add_posts_user_index"
    }

    async fn up(&self, db: &dyn Database) -> switchy_schema::Result<()> {
        println!("  [UP] Adding index on posts.user_id...");
        db.exec_raw("CREATE INDEX idx_posts_user_id ON posts(user_id)")
            .await?;
        Ok(())
    }

    async fn down(&self, db: &dyn Database) -> switchy_schema::Result<()> {
        println!("  [DOWN] Dropping index on posts.user_id...");
        db.exec_raw("DROP INDEX idx_posts_user_id").await?;
        Ok(())
    }
}

/// Helper function to create test migrations vector
fn create_test_migrations() -> Vec<Arc<dyn Migration<'static> + 'static>> {
    vec![
        Arc::new(CreateUsersTable) as Arc<dyn Migration<'static> + 'static>,
        Arc::new(CreatePostsTable),
        Arc::new(AddPostsUserIndex),
    ]
}

/// Example 1: Test migrations with full forward/backward cycle
async fn example_full_cycle() -> Result<(), TestError> {
    println!("\n=== Example 1: Full Cycle Testing ===");
    println!("Testing migrations forward then backward...\n");

    let db = create_empty_in_memory().await?;
    let migrations = create_test_migrations();

    verify_migrations_full_cycle(db.as_ref(), migrations).await?;

    println!("\n✓ Full cycle test passed!");
    Ok(())
}

/// Example 2: Test migrations with pre-seeded state
async fn example_with_state() -> Result<(), TestError> {
    println!("\n=== Example 2: Testing with Pre-seeded State ===");
    println!("Setting up initial database state before migrations...\n");

    let db = create_empty_in_memory().await?;
    let migrations = create_test_migrations();

    verify_migrations_with_state(db.as_ref(), migrations, |db| {
        Box::pin(async move {
            println!("  [SETUP] Creating config table with initial data...");
            db.exec_raw("CREATE TABLE config (key TEXT PRIMARY KEY, value TEXT)")
                .await?;
            db.exec_raw("INSERT INTO config (key, value) VALUES ('version', '1.0.0')")
                .await?;
            Ok::<(), DatabaseError>(())
        })
    })
    .await?;

    println!("\n✓ Pre-seeded state test passed!");
    Ok(())
}

/// Example 3: Test migrations with data mutations between steps
async fn example_with_mutations() -> Result<(), TestError> {
    println!("\n=== Example 3: Testing with Data Mutations ===");
    println!("Inserting data between migration steps...\n");

    let db = create_empty_in_memory().await?;
    let migrations = create_test_migrations();

    // Using BTreeMap for mutations
    let mut mutation_map = BTreeMap::new();
    mutation_map.insert(
        "001_create_users".to_string(),
        Arc::new(
            "INSERT INTO users (name, email) VALUES ('Alice', 'alice@example.com')".to_string(),
        ) as Arc<dyn Executable>,
    );
    mutation_map.insert(
        "002_create_posts".to_string(),
        Arc::new(
            "INSERT INTO posts (user_id, title, content) VALUES (1, 'First Post', 'Hello World!')"
                .to_string(),
        ) as Arc<dyn Executable>,
    );

    verify_migrations_with_mutations(db.as_ref(), migrations, mutation_map).await?;

    println!("\n✓ Data mutations test passed!");
    Ok(())
}

/// Example 4: Using `MutationBuilder` for cleaner syntax
async fn example_with_mutation_builder() -> Result<(), TestError> {
    println!("\n=== Example 4: Using MutationBuilder ===");
    println!("Building mutations with fluent API...\n");

    let db = create_empty_in_memory().await?;
    let migrations = create_test_migrations();

    // Using MutationBuilder for cleaner syntax
    let mutations = MutationBuilder::new()
        .add_mutation(
            "001_create_users",
            "INSERT INTO users (name, email) VALUES ('Bob', 'bob@example.com')",
        )
        .add_mutation(
            "001_create_users",
            "INSERT INTO users (name, email) VALUES ('Charlie', 'charlie@example.com')",
        )
        .add_mutation(
            "002_create_posts",
            "INSERT INTO posts (user_id, title, content) VALUES (1, 'Bobs Post', 'Content here')",
        )
        .build();

    verify_migrations_with_mutations(db.as_ref(), migrations, mutations).await?;

    println!("\n✓ MutationBuilder test passed!");
    Ok(())
}

/// Example 5: Using `MigrationTestBuilder` for advanced scenarios
async fn example_with_test_builder() -> Result<(), TestError> {
    println!("\n=== Example 5: Using MigrationTestBuilder ===");
    println!("Testing with before/after breakpoints...\n");

    let db = create_empty_in_memory().await?;
    let migrations = create_test_migrations();

    MigrationTestBuilder::new(migrations)
        .with_table_name("__test_migrations")
        .with_data_after("001_create_users", |db| {
            Box::pin(async move {
                println!("  [AFTER 001] Inserting test user data...");
                db.exec_raw("INSERT INTO users (name, email) VALUES ('Dave', 'dave@example.com')")
                    .await
            })
        })
        .with_data_before("003_add_posts_user_index", |db| {
            Box::pin(async move {
                println!("  [BEFORE 003] Inserting post data before index...");
                db.exec_raw(
                    "INSERT INTO posts (user_id, title, content) VALUES (1, 'Test Post', 'Content')",
                )
                .await
            })
        })
        .run(db.as_ref())
        .await?;

    println!("\n✓ MigrationTestBuilder test passed!");
    Ok(())
}

/// Example 6: Testing a single migration
async fn example_single_migration() -> Result<(), TestError> {
    println!("\n=== Example 6: Testing Single Migration ===");
    println!("Testing just one migration in isolation...\n");

    let db = create_empty_in_memory().await?;
    let single_migration =
        vec![Arc::new(CreateUsersTable) as Arc<dyn Migration<'static> + 'static>];

    verify_migrations_full_cycle(db.as_ref(), single_migration).await?;

    println!("\n✓ Single migration test passed!");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), TestError> {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║  Migration Testing Example - switchy_schema_test_utils      ║");
    println!("╚══════════════════════════════════════════════════════════════╝");

    // Run all examples
    example_full_cycle().await?;
    example_with_state().await?;
    example_with_mutations().await?;
    example_with_mutation_builder().await?;
    example_with_test_builder().await?;
    example_single_migration().await?;

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║  All migration tests passed successfully! ✓                 ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    Ok(())
}
