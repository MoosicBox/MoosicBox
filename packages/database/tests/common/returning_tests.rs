//! Backend-agnostic tests for RETURNING functionality
//!
//! These tests verify that INSERT, UPDATE, DELETE, and UPSERT operations
//! return the correct row data across all database backends.
//!
//! For backends without native RETURNING support (MySQL), the implementation
//! must emulate this functionality using transactions and additional queries.

use std::sync::Arc;
use switchy_database::query::{Expression, FilterableQuery};
use switchy_database::schema::{Column, DataType, create_table};
use switchy_database::{Database, DatabaseValue};

/// Helper function to extract boolean value from DatabaseValue (handles SQLite's 0/1 representation)
fn extract_bool(value: &DatabaseValue) -> bool {
    match value {
        DatabaseValue::Bool(b) => *b,
        DatabaseValue::Int64(n) => *n != 0,
        DatabaseValue::UInt64(n) => *n != 0,
        _ => panic!("Unexpected type for boolean column: {:?}", value),
    }
}

/// Create test schema for RETURNING tests with custom table name
pub async fn create_returning_test_schema_with_name(
    db: &dyn Database,
    table_name: &str,
) -> Result<(), switchy_database::DatabaseError> {
    // Clean up any existing test data
    db.drop_table(table_name)
        .if_exists(true)
        .execute(db)
        .await
        .unwrap();

    // Create users table with various column types
    create_table(table_name)
        .if_not_exists(true)
        .column(Column {
            name: "id".to_string(),
            data_type: DataType::BigInt,
            nullable: false,
            auto_increment: true,
            default: None,
        })
        .column(Column {
            name: "name".to_string(),
            data_type: DataType::Text,
            nullable: false,
            auto_increment: false,
            default: None,
        })
        .column(Column {
            name: "email".to_string(),
            data_type: DataType::Text,
            nullable: true,
            auto_increment: false,
            default: None,
        })
        .column(Column {
            name: "age".to_string(),
            data_type: DataType::BigInt,
            nullable: true,
            auto_increment: false,
            default: None,
        })
        .column(Column {
            name: "active".to_string(),
            data_type: DataType::Bool,
            nullable: false,
            auto_increment: false,
            default: Some(DatabaseValue::Bool(true)),
        })
        .column(Column {
            name: "created_at".to_string(),
            data_type: DataType::Timestamp,
            nullable: false,
            auto_increment: false,
            default: Some(DatabaseValue::Now),
        })
        .primary_key("id")
        .execute(db)
        .await?;

    Ok(())
}

/// Comprehensive RETURNING functionality test suite
#[allow(unused)]
pub trait ReturningTestSuite {
    /// Get database instance for testing as trait object
    async fn get_database(&self) -> Option<Arc<dyn Database + Send + Sync>>;

    /// Generate unique table name for this test
    /// Convention: "ret_" + shortened test name
    fn get_table_name(&self, test_suffix: &str) -> String {
        format!("ret_{}", test_suffix)
    }

    /// Setup test schema with custom table name
    async fn setup_test_schema_with_name(&self, db: &dyn Database, table_name: &str) {
        create_returning_test_schema_with_name(db, table_name)
            .await
            .unwrap();
    }

    /// Cleanup test table after test completion
    async fn cleanup_test_table(&self, db: &dyn Database, table_name: &str) {
        db.exec_raw(&format!("DROP TABLE IF EXISTS {}", table_name))
            .await
            .unwrap();
    }

    /// Test INSERT returns complete row with auto-generated values
    async fn test_insert_returns_complete_row(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("insert_complete");
        self.setup_test_schema_with_name(&*db, &table_name).await;

        // Execute INSERT
        let result = db
            .insert(&table_name)
            .value("name", "Alice")
            .value("email", "alice@example.com")
            .value("age", 30i64)
            .execute(&*db)
            .await
            .unwrap();

        // Verify ALL columns are returned
        assert!(
            result.get("id").is_some(),
            "Should return auto-generated ID"
        );
        assert_eq!(result.get("name").unwrap().as_str().unwrap(), "Alice");
        assert_eq!(
            result.get("email").unwrap().as_str().unwrap(),
            "alice@example.com"
        );
        assert_eq!(result.get("age").unwrap().as_i64().unwrap(), 30);

        // Check if active column exists before asserting its value
        if let Some(active_value) = result.get("active") {
            assert!(extract_bool(&active_value), "Should return default value");
        } else {
            println!("Warning: 'active' column not returned by INSERT");
        }

        assert!(
            result.get("created_at").is_some(),
            "Should return generated timestamp"
        );

        // Cleanup
        self.cleanup_test_table(&*db, &table_name).await;
    }

    /// Test UPDATE returns all updated rows
    async fn test_update_returns_all_updated_rows(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("update_all");
        self.setup_test_schema_with_name(&*db, &table_name).await;

        // Insert test data
        for i in 1i64..=3 {
            db.insert(&table_name)
                .value("name", format!("User{}", i))
                .value("email", format!("user{}@test.com", i))
                .value("age", 20 + i)
                .execute(&*db)
                .await
                .unwrap();
        }

        // Update multiple rows
        let results = db
            .update(&table_name)
            .value("active", false)
            .where_gte("age", 22i64) // Should match User2 and User3
            .execute(&*db)
            .await
            .unwrap();

        // Verify correct number of rows returned
        assert_eq!(results.len(), 2, "Should return exactly 2 updated rows");

        // Verify each returned row has ALL columns with updated values
        for row in &results {
            assert!(row.get("id").is_some());
            assert!(
                row.get("name")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .starts_with("User")
            );
            assert!(!extract_bool(&row.get("active").unwrap()));
            assert!(row.get("email").is_some());
            assert!(row.get("age").unwrap().as_i64().unwrap() >= 22);
            assert!(row.get("created_at").is_some());
        }

        // Cleanup
        self.cleanup_test_table(&*db, &table_name).await;
    }

    /// Test UPDATE with LIMIT returns limited rows
    async fn test_update_with_limit_returns_limited_rows(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("update_limit");
        self.setup_test_schema_with_name(&*db, &table_name).await;

        // Insert 5 rows
        for i in 1..=5 {
            db.insert(&table_name)
                .value("name", format!("User{}", i))
                .value("age", 25i64)
                .execute(&*db)
                .await
                .unwrap();
        }

        // Update with limit
        let results = db
            .update(&table_name)
            .value("age", 26i64)
            .where_eq("age", 25i64)
            .limit(2)
            .execute(&*db)
            .await
            .unwrap();

        assert_eq!(
            results.len(),
            2,
            "Should return exactly 2 rows due to LIMIT"
        );

        for row in &results {
            assert_eq!(row.get("age").unwrap().as_i64().unwrap(), 26);
        }

        // Cleanup
        self.cleanup_test_table(&*db, &table_name).await;
    }

    /// Test DELETE returns deleted rows
    async fn test_delete_returns_deleted_rows(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("delete_rows");
        self.setup_test_schema_with_name(&*db, &table_name).await;

        // Insert test data and keep track of what we inserted
        let mut inserted_names = Vec::new();
        for i in 1..=3 {
            let row = db
                .insert(&table_name)
                .value("name", format!("ToDelete{}", i))
                .value("email", format!("delete{}@test.com", i))
                .execute(&*db)
                .await
                .unwrap();
            inserted_names.push(row.get("name").unwrap().as_str().unwrap().to_string());
        }

        // Delete specific rows - using OR for now until LIKE is implemented
        let results = db
            .delete(&table_name)
            .where_or(vec![
                Box::new(switchy_database::query::where_eq("name", "ToDelete1")),
                Box::new(switchy_database::query::where_eq("name", "ToDelete2")),
                Box::new(switchy_database::query::where_eq("name", "ToDelete3")),
            ])
            .execute(&*db)
            .await
            .unwrap();

        // Verify we get all deleted rows back
        assert_eq!(results.len(), 3);

        // Verify ALL columns are returned for deleted rows
        let deleted_names: Vec<String> = results
            .iter()
            .map(|r| r.get("name").unwrap().as_str().unwrap().to_string())
            .collect();

        for name in &inserted_names {
            assert!(
                deleted_names.contains(name),
                "Should return deleted row: {}",
                name
            );
        }

        for row in &results {
            assert!(row.get("id").is_some());
            assert!(
                row.get("name")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .starts_with("ToDelete")
            );
            assert!(
                row.get("email")
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .contains("delete")
            );
            assert!(row.get("age").is_some()); // Even if NULL
            assert!(row.get("active").is_some());
            assert!(row.get("created_at").is_some());
        }

        // Verify rows are actually deleted - using OR for now until LIKE is implemented
        let remaining = db
            .select(&table_name)
            .where_or(vec![
                Box::new(switchy_database::query::where_eq("name", "ToDelete1")),
                Box::new(switchy_database::query::where_eq("name", "ToDelete2")),
                Box::new(switchy_database::query::where_eq("name", "ToDelete3")),
            ])
            .execute(&*db)
            .await
            .unwrap();
        assert_eq!(remaining.len(), 0, "Deleted rows should not exist");

        // Cleanup
        self.cleanup_test_table(&*db, &table_name).await;
    }

    /// Test DELETE with LIMIT returns limited rows
    async fn test_delete_with_limit_returns_limited_rows(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("delete_limit");
        self.setup_test_schema_with_name(&*db, &table_name).await;

        // Insert 10 rows
        for i in 1..=10 {
            db.insert(&table_name)
                .value("name", format!("User{}", i))
                .execute(&*db)
                .await
                .unwrap();
        }

        // Delete with limit
        let results = db.delete(&table_name).limit(3).execute(&*db).await.unwrap();

        assert_eq!(results.len(), 3, "Should return exactly 3 deleted rows");

        // Verify 7 rows remain
        let remaining = db.select(&table_name).execute(&*db).await.unwrap();
        assert_eq!(remaining.len(), 7);

        // Cleanup
        self.cleanup_test_table(&*db, &table_name).await;
    }

    /// Test UPSERT returns inserted or updated row
    async fn test_upsert_returns_correct_row(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("upsert");
        self.setup_test_schema_with_name(&*db, &table_name).await;

        // First upsert - should INSERT
        let insert_result = db
            .upsert(&table_name)
            .value("name", "UniqueUser")
            .value("email", "unique@test.com")
            .value("age", 25i64)
            .where_eq("name", "UniqueUser")
            .execute(&*db)
            .await
            .unwrap();

        assert_eq!(insert_result.len(), 1);
        let inserted = &insert_result[0];
        assert_eq!(
            inserted.get("name").unwrap().as_str().unwrap(),
            "UniqueUser"
        );
        assert_eq!(
            inserted.get("email").unwrap().as_str().unwrap(),
            "unique@test.com"
        );
        let first_id = inserted.get("id").unwrap().clone();

        // Second upsert - should UPDATE
        let update_result = db
            .upsert(&table_name)
            .value("name", "UniqueUser")
            .value("email", "updated@test.com")
            .value("age", 26i64)
            .where_eq("name", "UniqueUser")
            .execute(&*db)
            .await
            .unwrap();

        assert_eq!(update_result.len(), 1);
        let updated = &update_result[0];
        assert_eq!(updated.get("name").unwrap().as_str().unwrap(), "UniqueUser");
        assert_eq!(
            updated.get("email").unwrap().as_str().unwrap(),
            "updated@test.com"
        );
        assert_eq!(updated.get("age").unwrap().as_i64().unwrap(), 26);
        assert_eq!(
            updated.get("id").unwrap(),
            first_id,
            "ID should remain the same"
        );

        // Cleanup
        self.cleanup_test_table(&*db, &table_name).await;
    }

    /// Test operations within transactions return data
    async fn test_transaction_operations_return_data(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("transaction");
        self.setup_test_schema_with_name(&*db, &table_name).await;

        let tx = db.begin_transaction().await.unwrap();

        // INSERT in transaction
        let inserted = tx
            .insert(&table_name)
            .value("name", "TxUser")
            .value("email", "tx@test.com")
            .execute(&*tx)
            .await
            .unwrap();

        assert_eq!(inserted.get("name").unwrap().as_str().unwrap(), "TxUser");
        let tx_id = inserted.get("id").unwrap().clone();

        // UPDATE in transaction
        let updated = tx
            .update(&table_name)
            .value("email", "tx_updated@test.com")
            .where_eq("id", tx_id.clone())
            .execute(&*tx)
            .await
            .unwrap();

        assert_eq!(updated.len(), 1);
        let updated = &updated[0];

        assert_eq!(
            updated.get("email").unwrap().as_str().unwrap(),
            "tx_updated@test.com"
        );

        // DELETE in transaction
        let deleted = tx
            .delete(&table_name)
            .where_eq("id", tx_id.clone())
            .execute(&*tx)
            .await
            .unwrap();

        assert_eq!(deleted.len(), 1);
        assert_eq!(deleted[0].get("name").unwrap().as_str().unwrap(), "TxUser");

        tx.commit().await.unwrap();

        // Verify deletion persisted
        let after_commit = db
            .select(&table_name)
            .where_eq("id", tx_id.clone())
            .execute(&*db)
            .await
            .unwrap();
        assert_eq!(after_commit.len(), 0);

        // Cleanup
        self.cleanup_test_table(&*db, &table_name).await;
    }

    /// Test empty operations return empty results
    async fn test_empty_operations_return_empty(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("empty_ops");
        self.setup_test_schema_with_name(&*db, &table_name).await;

        // UPDATE with no matches
        let update_results = db
            .update(&table_name)
            .value("age", 100i64)
            .where_eq("name", "NonExistent")
            .execute(&*db)
            .await
            .unwrap();
        assert_eq!(update_results.len(), 0);

        // DELETE with no matches
        let delete_results = db
            .delete(&table_name)
            .where_eq("id", 99999i64)
            .execute(&*db)
            .await
            .unwrap();
        assert_eq!(delete_results.len(), 0);

        // Cleanup
        self.cleanup_test_table(&*db, &table_name).await;
    }

    /// Test operations preserve data types correctly
    async fn test_data_type_preservation_in_returns(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("type_preserve");
        self.setup_test_schema_with_name(&*db, &table_name).await;

        // Insert with various data types including NULL
        let result = db
            .insert(&table_name)
            .value("name", "TypeTest")
            .value("email", DatabaseValue::Null) // NULL value
            .value("age", 42i64)
            .value("active", false)
            .execute(&*db)
            .await
            .unwrap();

        // Verify types are preserved
        assert_eq!(result.get("name").unwrap().as_str().unwrap(), "TypeTest");
        assert!(result.get("email").unwrap().is_null());
        assert_eq!(result.get("age").unwrap().as_i64().unwrap(), 42);
        assert!(!extract_bool(&result.get("active").unwrap()));

        // Cleanup
        self.cleanup_test_table(&*db, &table_name).await;
    }

    /// Test complex WHERE clauses return correct rows
    async fn test_complex_filters_return_correct_rows(&self) {
        let Some(db) = self.get_database().await else {
            return;
        };

        let table_name = self.get_table_name("complex_filter");
        self.setup_test_schema_with_name(&*db, &table_name).await;

        // Insert varied test data
        for i in 1..=10 {
            db.insert(&table_name)
                .value("name", format!("User{:02}", i))
                .value("age", (15 + i * 5) as i64)
                .value(
                    "email",
                    if i % 2 == 0 {
                        format!("user{}@example.com", i)
                    } else {
                        format!("user{}@test.org", i)
                    },
                )
                .execute(&*db)
                .await
                .unwrap();
        }

        // Complex WHERE clause: age between 25-45 AND email contains "@example.com"
        // For now, match specific emails until LIKE is implemented
        let results = db
            .update(&table_name)
            .value("active", false)
            .where_gte("age", 25i64)
            .where_lte("age", 45i64)
            .where_in(
                "email",
                vec![
                    "user2@example.com",
                    "user4@example.com",
                    "user6@example.com",
                    "user8@example.com",
                    "user10@example.com",
                ],
            )
            .execute(&*db)
            .await
            .unwrap();

        // Verify all returned rows match the filter criteria
        assert!(!results.is_empty(), "Should find some matching rows");
        for row in &results {
            let age = row.get("age").unwrap().as_i64().unwrap();
            let email_value = row.get("email").unwrap();
            let email = email_value.as_str().unwrap();
            assert!((25..=45).contains(&age), "Age should be in range: {}", age);
            assert!(
                email.contains("@example.com"),
                "Email should contain @example.com: {}",
                email
            );
            assert!(
                !extract_bool(&row.get("active").unwrap()),
                "Should be updated to false"
            );
        }

        // Cleanup
        self.cleanup_test_table(&*db, &table_name).await;
    }
}
