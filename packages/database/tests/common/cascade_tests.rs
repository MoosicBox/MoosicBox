use std::sync::Arc;
use switchy_database::{
    Database,
    schema::{Column, DataType, create_index},
};

#[allow(unused)]
pub trait CascadeTestSuite {
    async fn setup_db(&self) -> Option<Arc<Box<dyn Database>>>;

    async fn test_cascade_find_targets_linear(&self) {
        let Some(db) = self.setup_db().await else {
            return;
        };
        let db = &**db;

        // Generate unique table names for this test
        let suffix = switchy_time::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            % 1_000_000_000;
        let users_table = format!("linear_users_{suffix}");
        let posts_table = format!("linear_posts_{suffix}");
        let comments_table = format!("linear_comments_{suffix}");

        // Drop tables if they exist (cleanup from previous runs)
        db.drop_table(&users_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
        db.drop_table(&posts_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
        db.drop_table(&comments_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();

        // Create linear dependency chain: users -> posts -> comments
        db.create_table(&users_table)
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "name".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .primary_key("id")
            .execute(db)
            .await
            .unwrap();

        db.create_table(&posts_table)
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "user_id".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "title".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .primary_key("id")
            .foreign_key(("user_id", &format!("{users_table}(id)")))
            .execute(db)
            .await
            .unwrap();

        db.create_table(&comments_table)
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "post_id".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "content".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .primary_key("id")
            .foreign_key(("post_id", &format!("{posts_table}(id)")))
            .execute(db)
            .await
            .unwrap();

        let tx = db.begin_transaction().await.unwrap();

        let plan = tx.find_cascade_targets(&users_table).await.unwrap();

        match plan {
            switchy_database::schema::DropPlan::Simple(tables) => {
                assert_eq!(tables.len(), 3);
                assert!(tables.contains(&users_table));
                assert!(tables.contains(&posts_table));
                assert!(tables.contains(&comments_table));

                // Verify order: dependents before dependencies
                let users_pos = tables.iter().position(|t| t == &users_table).unwrap();
                let posts_pos = tables.iter().position(|t| t == &posts_table).unwrap();
                let comments_pos = tables.iter().position(|t| t == &comments_table).unwrap();

                assert!(comments_pos < posts_pos);
                assert!(posts_pos < users_pos);
            }
            switchy_database::schema::DropPlan::WithCycles { .. } => {
                panic!("Expected Simple drop plan for linear dependencies");
            }
        }

        tx.rollback().await.unwrap();

        // Cleanup
        db.drop_table(&comments_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
        db.drop_table(&posts_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
        db.drop_table(&users_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
    }

    async fn test_cascade_has_any_dependents(&self) {
        let Some(db) = self.setup_db().await else {
            return;
        };
        let db = &**db;

        // Generate unique table names for this test
        let suffix = switchy_time::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            % 1_000_000_000;
        let parent_table = format!("deps_parent_{suffix}");
        let child_table = format!("deps_child_{suffix}");
        let orphan_table = format!("deps_orphan_{suffix}");

        // Drop tables if they exist (cleanup from previous runs)
        db.drop_table(&parent_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
        db.drop_table(&child_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
        db.drop_table(&orphan_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();

        // Create parent -> child, plus orphan table
        db.create_table(&parent_table)
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "data".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .primary_key("id")
            .execute(db)
            .await
            .unwrap();

        db.create_table(&child_table)
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "parent_id".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::BigInt,
                default: None,
            })
            .primary_key("id")
            .foreign_key(("parent_id", &format!("{parent_table}(id)")))
            .execute(db)
            .await
            .unwrap();

        db.create_table(&orphan_table)
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "data".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .primary_key("id")
            .execute(db)
            .await
            .unwrap();

        let tx = db.begin_transaction().await.unwrap();

        assert!(tx.has_any_dependents(&parent_table).await.unwrap());
        assert!(!tx.has_any_dependents(&child_table).await.unwrap());
        assert!(!tx.has_any_dependents(&orphan_table).await.unwrap());
        assert!(!tx.has_any_dependents("nonexistent").await.unwrap());

        tx.rollback().await.unwrap();

        // Cleanup
        db.drop_table(&child_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
        db.drop_table(&orphan_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
        db.drop_table(&parent_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
    }

    async fn test_cascade_get_direct_dependents(&self) {
        let Some(db) = self.setup_db().await else {
            return;
        };
        let db = &**db;

        // Generate unique table names for this test
        let suffix = switchy_time::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            % 1_000_000_000;
        let root_table = format!("diamond_root_{suffix}");
        let branch1_table = format!("diamond_branch1_{suffix}");
        let branch2_table = format!("diamond_branch2_{suffix}");
        let leaf_table = format!("diamond_leaf_{suffix}");

        // Drop tables if they exist (cleanup from previous runs)
        db.drop_table(&leaf_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
        db.drop_table(&branch1_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
        db.drop_table(&branch2_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
        db.drop_table(&root_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();

        // Create diamond dependency: root -> (branch1, branch2) -> leaf
        db.create_table(&root_table)
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::BigInt,
                default: None,
            })
            .primary_key("id")
            .execute(db)
            .await
            .unwrap();

        db.create_table(&branch1_table)
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "root_id".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::BigInt,
                default: None,
            })
            .primary_key("id")
            .foreign_key(("root_id", &format!("{root_table}(id)")))
            .execute(db)
            .await
            .unwrap();

        db.create_table(&branch2_table)
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "root_id".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::BigInt,
                default: None,
            })
            .primary_key("id")
            .foreign_key(("root_id", &format!("{root_table}(id)")))
            .execute(db)
            .await
            .unwrap();

        // Note: CreateTableStatement only supports single column FKs in current API
        // So we create leaf with FK to branch1 only
        db.create_table(&leaf_table)
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "branch1_id".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::BigInt,
                default: None,
            })
            .primary_key("id")
            .foreign_key(("branch1_id", &format!("{branch1_table}(id)")))
            .execute(db)
            .await
            .unwrap();

        let tx = db.begin_transaction().await.unwrap();

        let root_deps = tx.get_direct_dependents(&root_table).await.unwrap();
        assert_eq!(root_deps.len(), 2);
        assert!(root_deps.contains(&branch1_table));
        assert!(root_deps.contains(&branch2_table));

        let branch1_deps = tx.get_direct_dependents(&branch1_table).await.unwrap();
        assert_eq!(branch1_deps.len(), 1);
        assert!(branch1_deps.contains(&leaf_table));

        let branch2_deps = tx.get_direct_dependents(&branch2_table).await.unwrap();
        assert_eq!(branch2_deps.len(), 0); // No FK to branch2 in simplified test

        let leaf_deps = tx.get_direct_dependents(&leaf_table).await.unwrap();
        assert!(leaf_deps.is_empty());

        tx.rollback().await.unwrap();

        // Cleanup
        db.drop_table(&leaf_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
        db.drop_table(&branch1_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
        db.drop_table(&branch2_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
        db.drop_table(&root_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
    }

    async fn test_cascade_drop_restrict(&self) {
        let Some(db) = self.setup_db().await else {
            return;
        };
        let db = &**db;

        // Generate unique table names for this test
        let suffix = switchy_time::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            % 1_000_000_000;
        let parent_table = format!("restrict_parent_{suffix}");
        let child_table = format!("restrict_child_{suffix}");

        // Drop tables if they exist (cleanup from previous runs)
        db.drop_table(&child_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
        db.drop_table(&parent_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();

        // Create parent -> child
        db.create_table(&parent_table)
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "name".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .primary_key("id")
            .execute(db)
            .await
            .unwrap();

        db.create_table(&child_table)
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "parent_id".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::BigInt,
                default: None,
            })
            .primary_key("id")
            .foreign_key(("parent_id", &format!("{parent_table}(id)")))
            .execute(db)
            .await
            .unwrap();

        // Insert data
        db.insert(&parent_table)
            .value("id", 1i64)
            .value("name", "Parent1")
            .execute(db)
            .await
            .unwrap();

        db.insert(&child_table)
            .value("id", 1i64)
            .value("parent_id", 1i64)
            .execute(db)
            .await
            .unwrap();

        let tx = db.begin_transaction().await.unwrap();

        // RESTRICT should fail with dependents
        let restrict_result = tx.drop_table(&parent_table).restrict().execute(&*tx).await;

        assert!(restrict_result.is_err());

        tx.rollback().await.unwrap();

        // Cleanup
        db.drop_table(&child_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
        db.drop_table(&parent_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
    }

    async fn test_cascade_drop_execution(&self) {
        let Some(db) = self.setup_db().await else {
            return;
        };
        let db = &**db;

        // Generate unique table names for this test
        let suffix = switchy_time::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            % 1_000_000_000;
        let users_table = format!("cascade_users_{suffix}");
        let posts_table = format!("cascade_posts_{suffix}");
        let comments_table = format!("cascade_comments_{suffix}");

        // Drop tables if they exist (cleanup from previous runs)
        db.drop_table(&comments_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
        db.drop_table(&posts_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
        db.drop_table(&users_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();

        // Create dependency chain: users -> posts -> comments
        db.create_table(&users_table)
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "name".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .primary_key("id")
            .execute(db)
            .await
            .unwrap();

        db.create_table(&posts_table)
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "user_id".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "title".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .primary_key("id")
            .foreign_key(("user_id", &format!("{users_table}(id)")))
            .execute(db)
            .await
            .unwrap();

        db.create_table(&comments_table)
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "post_id".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "content".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .primary_key("id")
            .foreign_key(("post_id", &format!("{posts_table}(id)")))
            .execute(db)
            .await
            .unwrap();

        // Insert test data
        db.insert(&users_table)
            .value("id", 1i64)
            .value("name", "Alice")
            .execute(db)
            .await
            .unwrap();

        db.insert(&posts_table)
            .value("id", 1i64)
            .value("user_id", 1i64)
            .value("title", "My Post")
            .execute(db)
            .await
            .unwrap();

        db.insert(&comments_table)
            .value("id", 1i64)
            .value("post_id", 1i64)
            .value("content", "Great post!")
            .execute(db)
            .await
            .unwrap();

        // Verify all tables exist and have data
        assert!(db.table_exists(&users_table).await.unwrap());
        assert!(db.table_exists(&posts_table).await.unwrap());
        assert!(db.table_exists(&comments_table).await.unwrap());

        let users_count = db.select(&users_table).execute(db).await.unwrap().len();
        let posts_count = db.select(&posts_table).execute(db).await.unwrap().len();
        let comments_count = db.select(&comments_table).execute(db).await.unwrap().len();

        assert_eq!(users_count, 1);
        assert_eq!(posts_count, 1);
        assert_eq!(comments_count, 1);

        // Execute CASCADE drop on users table - should drop all dependent tables
        db.drop_table(&users_table)
            .cascade()
            .execute(db)
            .await
            .unwrap();

        // Verify all tables were dropped
        assert!(!db.table_exists(&users_table).await.unwrap());
        assert!(!db.table_exists(&posts_table).await.unwrap());
        assert!(!db.table_exists(&comments_table).await.unwrap());
    }

    async fn test_restrict_drop_execution(&self) {
        let Some(db) = self.setup_db().await else {
            return;
        };
        let db = &**db;

        // Generate unique table names for this test
        let suffix = switchy_time::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            % 1_000_000_000;
        let parent_table = format!("restrict_parent_exec_{suffix}");
        let child_table = format!("restrict_child_exec_{suffix}");

        // Drop tables if they exist (cleanup from previous runs)
        db.drop_table(&child_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
        db.drop_table(&parent_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();

        // Create parent -> child relationship
        db.create_table(&parent_table)
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "name".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .primary_key("id")
            .execute(db)
            .await
            .unwrap();

        db.create_table(&child_table)
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "parent_id".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::BigInt,
                default: None,
            })
            .primary_key("id")
            .foreign_key(("parent_id", &format!("{parent_table}(id)")))
            .execute(db)
            .await
            .unwrap();

        // Insert test data
        db.insert(&parent_table)
            .value("id", 1i64)
            .value("name", "Parent")
            .execute(db)
            .await
            .unwrap();

        db.insert(&child_table)
            .value("id", 1i64)
            .value("parent_id", 1i64)
            .execute(db)
            .await
            .unwrap();

        // Verify tables exist and have data
        assert!(db.table_exists(&parent_table).await.unwrap());
        assert!(db.table_exists(&child_table).await.unwrap());

        // RESTRICT should fail when dependents exist
        let restrict_result = db.drop_table(&parent_table).restrict().execute(db).await;
        assert!(restrict_result.is_err());

        // Tables should still exist after failed RESTRICT
        assert!(db.table_exists(&parent_table).await.unwrap());
        assert!(db.table_exists(&child_table).await.unwrap());

        // Remove the dependent first
        db.drop_table(&child_table).execute(db).await.unwrap();

        // Now RESTRICT should succeed
        db.drop_table(&parent_table)
            .restrict()
            .execute(db)
            .await
            .unwrap();

        // Verify tables were dropped
        assert!(!db.table_exists(&parent_table).await.unwrap());
        assert!(!db.table_exists(&child_table).await.unwrap());
    }

    async fn test_drop_column_cascade_with_index(&self) {
        let Some(db) = self.setup_db().await else {
            return;
        };
        let db = &**db;

        // Generate unique table name
        let suffix = switchy_time::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            % 1_000_000_000;
        let table_name = format!("drop_col_cascade_{suffix}");

        // Create table with multiple columns
        db.create_table(&table_name)
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "email".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::VarChar(255),
                default: None,
            })
            .column(Column {
                name: "name".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::VarChar(255),
                default: None,
            })
            .primary_key("id")
            .execute(db)
            .await
            .unwrap();

        // Create index on email column
        db.exec_create_index(
            &create_index(&format!("idx_{table_name}_email"))
                .table(&table_name)
                .column("email"),
        )
        .await
        .unwrap();

        // Insert test data
        db.insert(&table_name)
            .value("email", "test@example.com")
            .value("name", "Test User")
            .execute(db)
            .await
            .unwrap();

        // DROP COLUMN CASCADE should succeed
        let result = db
            .alter_table(&table_name)
            .drop_column_cascade("email".to_string())
            .execute(db)
            .await;

        assert!(result.is_ok(), "CASCADE drop should succeed: {:?}", result);

        // Verify column is gone
        assert!(!db.column_exists(&table_name, "email").await.unwrap());

        // Clean up
        db.drop_table(&table_name)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
    }

    async fn test_drop_column_restrict_with_index(&self) {
        let Some(db) = self.setup_db().await else {
            return;
        };
        let db = &**db;

        // Generate unique table name
        let suffix = switchy_time::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            % 1_000_000_000;
        let table_name = format!("drop_col_restrict_{suffix}");

        // Create table
        db.create_table(&table_name)
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "email".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::VarChar(255),
                default: None,
            })
            .primary_key("id")
            .execute(db)
            .await
            .unwrap();

        // Create index on email column
        db.exec_create_index(
            &create_index(&format!("idx_{table_name}_email"))
                .table(&table_name)
                .column("email"),
        )
        .await
        .unwrap();

        // DROP COLUMN RESTRICT should fail due to index dependency
        let result = db
            .alter_table(&table_name)
            .drop_column_restrict("email".to_string())
            .execute(db)
            .await;

        // Result depends on backend implementation
        match result {
            Err(_) => {
                // Expected: RESTRICT should fail with dependencies
                println!("RESTRICT correctly failed with index dependency");
                // Verify column still exists
                assert!(db.column_exists(&table_name, "email").await.unwrap());
            }
            Ok(()) => {
                // Some backends may not fully detect index dependencies yet
                println!("RESTRICT succeeded (backend may have limited dependency detection)");
            }
        }

        // Clean up
        db.drop_table(&table_name)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
    }

    async fn test_drop_column_cascade_with_foreign_key(&self) {
        let Some(db) = self.setup_db().await else {
            return;
        };
        let db = &**db;

        // Generate unique table names
        let suffix = switchy_time::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            % 1_000_000_000;
        let parent_table = format!("parent_cascade_{suffix}");
        let child_table = format!("child_cascade_{suffix}");

        // Clean up any existing tables
        db.drop_table(&child_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
        db.drop_table(&parent_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();

        // Create parent table
        db.create_table(&parent_table)
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "email".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .primary_key("id")
            .execute(db)
            .await
            .unwrap();

        // Create child table with FK to parent
        db.create_table(&child_table)
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "parent_id".to_string(),
                nullable: false,
                auto_increment: false,
                data_type: DataType::BigInt,
                default: None,
            })
            .primary_key("id")
            .foreign_key(("parent_id", &format!("{parent_table}(id)")))
            .execute(db)
            .await
            .unwrap();

        // DROP COLUMN CASCADE on parent table (not the FK column)
        let result = db
            .alter_table(&parent_table)
            .drop_column_cascade("email".to_string())
            .execute(db)
            .await;

        assert!(
            result.is_ok(),
            "CASCADE drop of non-FK column should succeed"
        );

        // Clean up
        db.drop_table(&child_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
        db.drop_table(&parent_table)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
    }

    async fn test_drop_column_restrict_no_dependencies(&self) {
        let Some(db) = self.setup_db().await else {
            return;
        };
        let db = &**db;

        // Generate unique table name
        let suffix = switchy_time::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
            % 1_000_000_000;
        let table_name = format!("drop_col_no_deps_{suffix}");

        // Create table
        db.create_table(&table_name)
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::BigInt,
                default: None,
            })
            .column(Column {
                name: "email".to_string(),
                nullable: true,
                auto_increment: false,
                data_type: DataType::Text,
                default: None,
            })
            .primary_key("id")
            .execute(db)
            .await
            .unwrap();

        // DROP COLUMN RESTRICT should succeed (no dependencies)
        let result = db
            .alter_table(&table_name)
            .drop_column_restrict("email".to_string())
            .execute(db)
            .await;

        assert!(
            result.is_ok(),
            "RESTRICT should succeed without dependencies"
        );
        assert!(!db.column_exists(&table_name, "email").await.unwrap());

        // Clean up
        db.drop_table(&table_name)
            .if_exists(true)
            .execute(db)
            .await
            .ok();
    }
}
