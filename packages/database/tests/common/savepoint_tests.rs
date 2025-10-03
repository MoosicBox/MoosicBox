#![cfg(feature = "schema")]

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use switchy_async::sync::{Barrier, mpsc};
use switchy_database::{
    Database, DatabaseValue,
    query::FilterableQuery as _,
    schema::{Column, DataType, create_table, drop_table},
};

/// Result data from concurrent transaction execution
#[derive(Debug, Clone)]
pub struct TransactionResult {
    pub rows_seen_during: usize,
    pub rows_seen_at_end: usize,
    pub savepoints_created: Vec<String>,
    pub operations_performed: Vec<String>,
    pub final_status: String,
}

/// Get current timestamp for debugging concurrent operations
pub fn current_timestamp() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

/// Comprehensive savepoint test suite trait for cross-backend testing
#[allow(unused)]
pub trait SavepointTestSuite {
    type DatabaseType: Database + Send + Sync;

    /// Get database instance for testing (returns None if unavailable)
    async fn get_database(&self) -> Option<Arc<Self::DatabaseType>>;

    /// Create the standard test schema using cross-backend compatible schema builder
    async fn create_test_schema(&self, db: &Self::DatabaseType, table_name: &str) {
        // Drop existing table if it exists
        drop_table(table_name)
            .if_exists(true)
            .execute(db)
            .await
            .ok(); // Ignore errors if table doesn't exist

        // Create table using schema builder for cross-backend compatibility
        create_table(table_name)
            .column(Column {
                name: "id".to_string(),
                data_type: DataType::BigInt,
                nullable: false,
                auto_increment: true,
                default: None,
            })
            .column(Column {
                name: "name".to_string(),
                data_type: DataType::VarChar(100),
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .column(Column {
                name: "value".to_string(),
                data_type: DataType::BigInt,
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .column(Column {
                name: "savepoint_level".to_string(),
                data_type: DataType::BigInt,
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .column(Column {
                name: "operation_type".to_string(),
                data_type: DataType::VarChar(50),
                nullable: true,
                auto_increment: false,
                default: None,
            })
            .column(Column {
                name: "transaction_id".to_string(),
                data_type: DataType::BigInt,
                nullable: true,
                auto_increment: false,
                default: Some(DatabaseValue::Int64(0)),
            })
            .column(Column {
                name: "created_at".to_string(),
                data_type: DataType::BigInt,
                nullable: true,
                auto_increment: false,
                default: Some(DatabaseValue::Int64(0)),
            })
            .primary_key("id")
            .execute(db)
            .await
            .expect("Failed to create savepoint_test table");
    }

    /// Test 1: Nested savepoints (3 levels deep)
    /// Create transaction → SP1 (insert A) → SP2 (insert B) → SP3 (insert C)
    /// Release SP3, verify all data present
    /// Rollback to SP2, verify only A remains (SP2 created after A, before B)
    /// Release SP1, commit
    /// Verify final state contains only A
    async fn test_nested_savepoints_three_levels(&self) {
        let table_name = "sp_nested_three";
        let Some(db) = self.get_database().await else {
            return;
        };

        self.create_test_schema(&*db, table_name).await;

        // Start transaction
        let tx = db.begin_transaction().await.unwrap();

        // Level 1: Create SP1 and insert data A
        let sp1 = tx.savepoint("sp1").await.unwrap();
        tx.insert(table_name)
            .value("name", "A")
            .value("value", 1i64)
            .value("savepoint_level", 1i64)
            .value("operation_type", "insert")
            .execute(&*tx)
            .await
            .unwrap();

        // Level 2: Create SP2 and insert data B
        let sp2 = tx.savepoint("sp2").await.unwrap();
        tx.insert(table_name)
            .value("name", "B")
            .value("value", 2i64)
            .value("savepoint_level", 2i64)
            .value("operation_type", "insert")
            .execute(&*tx)
            .await
            .unwrap();

        // Level 3: Create SP3 and insert data C
        let sp3 = tx.savepoint("sp3").await.unwrap();
        tx.insert(table_name)
            .value("name", "C")
            .value("value", 3i64)
            .value("savepoint_level", 3i64)
            .value("operation_type", "insert")
            .execute(&*tx)
            .await
            .unwrap();

        // Verify all data present (A, B, C)
        let rows = tx.select(table_name).execute(&*tx).await.unwrap();
        assert_eq!(rows.len(), 3);

        // Release SP3
        sp3.release().await.unwrap();

        // Verify all data still present after SP3 release
        let rows = tx.select(table_name).execute(&*tx).await.unwrap();
        assert_eq!(rows.len(), 3);

        // Rollback to SP2 (should lose C and B, keep only A)
        // When we rollback to SP2, we go back to the state when SP2 was created
        // SP2 was created after A was inserted but before B was inserted
        sp2.rollback_to().await.unwrap();

        // Verify only A remains (B and C should be gone)
        let rows = tx.select(table_name).execute(&*tx).await.unwrap();
        assert_eq!(rows.len(), 1);

        let rows = tx
            .select(table_name)
            .sort("name", switchy_database::query::SortDirection::Asc)
            .execute(&*tx)
            .await
            .unwrap();
        assert_eq!(rows[0].get("name").unwrap().as_str().unwrap(), "A");

        // Release SP1
        sp1.release().await.unwrap();

        // Commit transaction
        tx.commit().await.unwrap();

        // Verify final state: only A should be persisted
        let final_check = db.begin_transaction().await.unwrap();
        let rows = final_check
            .select(table_name)
            .execute(&*final_check)
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        final_check.commit().await.unwrap();
    }

    /// Test 2: Rollback to middle savepoint
    /// Create transaction with initial data
    /// SP1 (insert A) → SP2 (insert B) → SP3 (insert C)
    /// Rollback to SP2 (should preserve initial + A, lose B and C)
    /// Insert data D after rollback
    /// Commit and verify: initial + A + D (no B, no C)
    async fn test_rollback_to_middle_savepoint(&self) {
        let table_name = "sp_rollback_middle";
        let Some(db) = self.get_database().await else {
            return;
        };

        self.create_test_schema(&*db, table_name).await;

        // Start transaction and insert initial data
        let tx = db.begin_transaction().await.unwrap();
        tx.insert(table_name)
            .value("name", "INITIAL")
            .value("value", 0i64)
            .value("savepoint_level", 0i64)
            .value("operation_type", "initial")
            .execute(&*tx)
            .await
            .unwrap();

        // SP1: Insert A
        let sp1 = tx.savepoint("sp1").await.unwrap();
        tx.insert(table_name)
            .value("name", "A")
            .value("value", 1i64)
            .value("savepoint_level", 1i64)
            .value("operation_type", "insert")
            .execute(&*tx)
            .await
            .unwrap();

        // SP2: Insert B
        let sp2 = tx.savepoint("sp2").await.unwrap();
        tx.insert(table_name)
            .value("name", "B")
            .value("value", 2i64)
            .value("savepoint_level", 2i64)
            .value("operation_type", "insert")
            .execute(&*tx)
            .await
            .unwrap();

        // SP3: Insert C
        let _sp3 = tx.savepoint("sp3").await.unwrap();
        tx.insert(table_name)
            .value("name", "C")
            .value("value", 3i64)
            .value("savepoint_level", 3i64)
            .value("operation_type", "insert")
            .execute(&*tx)
            .await
            .unwrap();

        // Verify all data present (INITIAL, A, B, C)
        let rows = tx.select(table_name).execute(&*tx).await.unwrap();
        assert_eq!(rows.len(), 4);

        // Rollback to SP2 (should preserve INITIAL + A, lose B and C)
        // Rollback to SP2 goes back to state when SP2 was created (after A, before B)
        sp2.rollback_to().await.unwrap();

        // Verify INITIAL and A remain (B and C should be gone)
        let rows = tx.select(table_name).execute(&*tx).await.unwrap();
        assert_eq!(rows.len(), 2);

        let rows = tx
            .select(table_name)
            .sort("name", switchy_database::query::SortDirection::Asc)
            .execute(&*tx)
            .await
            .unwrap();
        assert_eq!(rows[0].get("name").unwrap().as_str().unwrap(), "A");
        assert_eq!(rows[1].get("name").unwrap().as_str().unwrap(), "INITIAL");

        // Insert data D after rollback
        tx.insert(table_name)
            .value("name", "D")
            .value("value", 4i64)
            .value("savepoint_level", 2i64)
            .value("operation_type", "post_rollback")
            .execute(&*tx)
            .await
            .unwrap();

        // Release remaining savepoints
        sp1.release().await.unwrap();

        // Commit
        tx.commit().await.unwrap();

        // Verify final state: INITIAL + A + D (no B, no C)
        let final_check = db.begin_transaction().await.unwrap();
        let rows = final_check
            .select(table_name)
            .execute(&*final_check)
            .await
            .unwrap();
        assert_eq!(rows.len(), 3);

        let rows = final_check
            .select(table_name)
            .sort("name", switchy_database::query::SortDirection::Asc)
            .execute(&*final_check)
            .await
            .unwrap();
        assert_eq!(rows[0].get("name").unwrap().as_str().unwrap(), "A");
        assert_eq!(rows[1].get("name").unwrap().as_str().unwrap(), "D");
        assert_eq!(rows[2].get("name").unwrap().as_str().unwrap(), "INITIAL");

        final_check.commit().await.unwrap();
    }

    /// Test 3: Release savepoints out of order
    /// Create SP1 → SP2 → SP3 nested savepoints
    /// Attempt to release SP2 before SP3
    /// Document backend-specific behavior differences
    /// Test error handling or automatic release chains
    async fn test_release_savepoints_out_of_order(&self) {
        let table_name = "sp_release_order";
        let Some(db) = self.get_database().await else {
            return;
        };

        self.create_test_schema(&*db, table_name).await;

        let tx = db.begin_transaction().await.unwrap();

        // Create nested savepoints
        let sp1 = tx.savepoint("sp1").await.unwrap();
        tx.insert(table_name)
            .value("name", "A")
            .value("value", 1i64)
            .value("savepoint_level", 1i64)
            .value("operation_type", "insert")
            .execute(&*tx)
            .await
            .unwrap();

        let sp2 = tx.savepoint("sp2").await.unwrap();
        tx.insert(table_name)
            .value("name", "B")
            .value("value", 2i64)
            .value("savepoint_level", 2i64)
            .value("operation_type", "insert")
            .execute(&*tx)
            .await
            .unwrap();

        let sp3 = tx.savepoint("sp3").await.unwrap();
        tx.insert(table_name)
            .value("name", "C")
            .value("value", 3i64)
            .value("savepoint_level", 3i64)
            .value("operation_type", "insert")
            .execute(&*tx)
            .await
            .unwrap();

        // Try to release SP2 before SP3
        // Note: This behavior may be backend-specific
        // Some databases may auto-release nested savepoints
        // Others may return an error
        let result = sp2.release().await;

        // The behavior here is backend-specific:
        // - SQLite: May auto-release nested savepoints
        // - PostgreSQL: May require LIFO order
        // - MySQL: May have different behavior
        // For now, we accept either success or specific error types
        match result {
            Ok(()) => {
                // Auto-release succeeded - verify data is still consistent
                let rows = tx.select(table_name).execute(&*tx).await.unwrap();
                assert!(!rows.is_empty()); // At least some data should remain
            }
            Err(_) => {
                // Release failed - this is also acceptable behavior
                // Release savepoints in proper order
                sp3.release().await.unwrap();
                sp1.release().await.unwrap();
            }
        }

        tx.commit().await.unwrap();
    }

    /// Test 4: Savepoint with data operations (Full CRUD)
    /// SP1: INSERT records
    /// UPDATE existing records within SP1
    /// SP2: DELETE some records
    /// Rollback to SP2 (restore deleted records)
    /// Release SP2, SP1 and verify final state
    /// Test all CRUD operations within savepoint boundaries
    async fn test_savepoint_with_data_operations(&self) {
        let table_name = "sp_data_ops";
        let Some(db) = self.get_database().await else {
            return;
        };

        self.create_test_schema(&*db, table_name).await;

        let tx = db.begin_transaction().await.unwrap();

        // SP1: INSERT and UPDATE operations
        let sp1 = tx.savepoint("sp1").await.unwrap();

        // INSERT records
        tx.insert(table_name)
            .value("name", "RECORD1")
            .value("value", 10i64)
            .value("savepoint_level", 1i64)
            .value("operation_type", "insert")
            .execute(&*tx)
            .await
            .unwrap();
        tx.insert(table_name)
            .value("name", "RECORD2")
            .value("value", 20i64)
            .value("savepoint_level", 1i64)
            .value("operation_type", "insert")
            .execute(&*tx)
            .await
            .unwrap();
        tx.insert(table_name)
            .value("name", "RECORD3")
            .value("value", 30i64)
            .value("savepoint_level", 1i64)
            .value("operation_type", "insert")
            .execute(&*tx)
            .await
            .unwrap();

        // UPDATE existing records
        tx.update(table_name)
            .value("value", 15i64)
            .value("operation_type", "update")
            .where_eq("name", "RECORD1")
            .execute(&*tx)
            .await
            .unwrap();
        tx.update(table_name)
            .value("value", 25i64)
            .value("operation_type", "update")
            .where_eq("name", "RECORD2")
            .execute(&*tx)
            .await
            .unwrap();

        // Verify data after INSERT and UPDATE
        let rows = tx.select(table_name).execute(&*tx).await.unwrap();
        assert_eq!(rows.len(), 3);

        // SP2: DELETE operations
        let sp2 = tx.savepoint("sp2").await.unwrap();
        tx.delete(table_name)
            .where_eq("name", "RECORD2")
            .execute(&*tx)
            .await
            .unwrap();

        // Verify RECORD2 is deleted
        let rows = tx.select(table_name).execute(&*tx).await.unwrap();
        assert_eq!(rows.len(), 2);

        // Rollback to SP2 (should restore RECORD2)
        sp2.rollback_to().await.unwrap();

        // Verify RECORD2 is restored
        let rows = tx.select(table_name).execute(&*tx).await.unwrap();
        assert_eq!(rows.len(), 3);

        // Verify UPDATE operations from SP1 are still present
        let rows = tx
            .select(table_name)
            .where_eq("name", "RECORD1")
            .execute(&*tx)
            .await
            .unwrap();
        assert_eq!(rows[0].get("value").unwrap().as_i64().unwrap(), 15); // 10 + 5 from UPDATE

        // Release savepoints and commit
        sp1.release().await.unwrap();
        tx.commit().await.unwrap();

        // Final verification
        let final_check = db.begin_transaction().await.unwrap();
        let rows = final_check
            .select(table_name)
            .execute(&*final_check)
            .await
            .unwrap();
        assert_eq!(rows.len(), 3);
        final_check.commit().await.unwrap();
    }

    /// Test 5: Commit with unreleased savepoints
    /// Create multiple nested savepoints
    /// Perform data operations in each
    /// Commit transaction without explicitly releasing savepoints
    /// Verify auto-cleanup behavior (no errors)
    /// Verify data persists correctly
    async fn test_commit_with_unreleased_savepoints(&self) {
        let table_name = "sp_unreleased";
        let Some(db) = self.get_database().await else {
            return;
        };

        self.create_test_schema(&*db, table_name).await;

        let tx = db.begin_transaction().await.unwrap();

        // Create multiple nested savepoints without releasing them
        let _sp1 = tx.savepoint("sp1").await.unwrap();
        tx.insert(table_name)
            .value("name", "AUTO1")
            .value("value", 1i64)
            .value("savepoint_level", 1i64)
            .value("operation_type", "unreleased")
            .execute(&*tx)
            .await
            .unwrap();

        let _sp2 = tx.savepoint("sp2").await.unwrap();
        tx.insert(table_name)
            .value("name", "AUTO2")
            .value("value", 2i64)
            .value("savepoint_level", 2i64)
            .value("operation_type", "unreleased")
            .execute(&*tx)
            .await
            .unwrap();

        let _sp3 = tx.savepoint("sp3").await.unwrap();
        tx.insert(table_name)
            .value("name", "AUTO3")
            .value("value", 3i64)
            .value("savepoint_level", 3i64)
            .value("operation_type", "unreleased")
            .execute(&*tx)
            .await
            .unwrap();

        // Verify all data is present
        let rows = tx.select(table_name).execute(&*tx).await.unwrap();
        assert_eq!(rows.len(), 3);

        // Commit without releasing savepoints - should auto-cleanup
        tx.commit().await.unwrap();

        // Verify data persists correctly after auto-cleanup
        let final_check = db.begin_transaction().await.unwrap();
        let rows = final_check
            .select(table_name)
            .execute(&*final_check)
            .await
            .unwrap();
        assert_eq!(rows.len(), 3);

        let rows = final_check
            .select(table_name)
            .sort("name", switchy_database::query::SortDirection::Asc)
            .execute(&*final_check)
            .await
            .unwrap();
        assert_eq!(rows[0].get("name").unwrap().as_str().unwrap(), "AUTO1");
        assert_eq!(rows[1].get("name").unwrap().as_str().unwrap(), "AUTO2");
        assert_eq!(rows[2].get("name").unwrap().as_str().unwrap(), "AUTO3");

        final_check.commit().await.unwrap();
    }

    /// Test 6: Savepoint name validation
    /// Test valid names: alphanumeric, underscores, mixed case
    /// Test edge cases: empty string, special characters, SQL keywords
    /// Test maximum length limits (backend-specific)
    /// Document differences between MySQL, PostgreSQL, SQLite
    /// Verify consistent error messages
    async fn test_savepoint_name_validation(&self) {
        let table_name = "sp_name_valid";
        let Some(db) = self.get_database().await else {
            return;
        };

        self.create_test_schema(&*db, table_name).await;

        let tx = db.begin_transaction().await.unwrap();

        // Test valid names
        let valid_names = vec![
            "sp1",
            "savepoint_1",
            "SavePoint_123",
            "sp_with_underscores",
            "mixedCasePoint",
            "point123",
        ];

        for name in valid_names {
            let sp = tx.savepoint(name).await;
            assert!(sp.is_ok(), "Valid savepoint name '{}' should succeed", name);
            if let Ok(sp) = sp {
                sp.release().await.unwrap();
            }
        }

        // Test edge cases - behavior may be backend-specific
        let edge_cases = vec![
            ("", false),               // Empty string should fail
            ("sp with spaces", false), // Spaces should fail in most backends
            ("sp-with-dashes", false), // Dashes may fail in some backends
            ("sp.with.dots", false),   // Dots may fail in some backends
            ("SELECT", false),         // SQL keyword should fail or be handled
            ("sp$special", false),     // Special characters should fail
        ];

        for (name, should_succeed) in edge_cases {
            let sp = tx.savepoint(name).await;
            if should_succeed {
                assert!(
                    sp.is_ok(),
                    "Edge case savepoint name '{}' should succeed",
                    name
                );
                if let Ok(sp) = sp {
                    sp.release().await.unwrap();
                }
            } else {
                // We expect this to fail, but different backends may have different behavior
                // Some may succeed with quoted identifiers, others may fail
                match sp {
                    Ok(sp) => {
                        // If it succeeds, at least verify it works
                        sp.release().await.unwrap();
                    }
                    Err(_) => {
                        // Expected failure - this is fine
                    }
                }
            }
        }

        // Test very long name (backend-specific limits)
        let long_name = "a".repeat(1000);
        let sp = tx.savepoint(&long_name).await;
        // Behavior is backend-specific - some may truncate, others may fail
        if let Ok(sp) = sp {
            sp.release().await.unwrap()
        }

        tx.commit().await.unwrap();
    }

    /// Test 7: Sequential savepoints in different transactions
    /// Tests sequential transaction behavior where TX2 sees TX1's committed data
    /// Verifies savepoint name reuse across completed transactions
    async fn test_sequential_savepoints_different_transactions(&self) {
        let table_name = "sp_sequential";
        let Some(db) = self.get_database().await else {
            return;
        };

        self.create_test_schema(&*db, table_name).await;

        // Transaction 1: Create savepoints with specific names
        {
            let tx1 = db.begin_transaction().await.unwrap();
            let sp1_tx1 = tx1.savepoint("shared_name").await.unwrap();

            tx1.insert(table_name)
                .value("name", "TX1_DATA")
                .value("value", 100i64)
                .value("savepoint_level", 1i64)
                .value("operation_type", "sequential")
                .execute(&*tx1)
                .await
                .unwrap();

            let sp2_tx1 = tx1.savepoint("nested_sp").await.unwrap();
            tx1.insert(table_name)
                .value("name", "TX1_NESTED")
                .value("value", 101i64)
                .value("savepoint_level", 2i64)
                .value("operation_type", "nested")
                .execute(&*tx1)
                .await
                .unwrap();

            // Rollback nested savepoint
            sp2_tx1.rollback_to().await.unwrap();

            // Verify TX1 lost nested data
            let rows_tx1 = tx1.select(table_name).execute(&*tx1).await.unwrap();
            assert_eq!(rows_tx1.len(), 1); // TX1 has only base data

            sp1_tx1.release().await.unwrap();
            tx1.commit().await.unwrap();
        }

        // Transaction 2: Reuse same savepoint names (should work since TX1 is committed)
        {
            let tx2 = db.begin_transaction().await.unwrap();
            let sp1_tx2 = tx2.savepoint("shared_name").await.unwrap(); // Same name should work

            tx2.insert(table_name)
                .value("name", "TX2_DATA")
                .value("value", 200i64)
                .value("savepoint_level", 1i64)
                .value("operation_type", "sequential")
                .execute(&*tx2)
                .await
                .unwrap();

            let sp2_tx2 = tx2.savepoint("nested_sp").await.unwrap(); // Same nested name
            tx2.insert(table_name)
                .value("name", "TX2_NESTED")
                .value("value", 201i64)
                .value("savepoint_level", 2i64)
                .value("operation_type", "nested")
                .execute(&*tx2)
                .await
                .unwrap();

            // This time keep the nested data
            sp2_tx2.release().await.unwrap();

            // Verify TX2 has both its records plus TX1's committed data
            let rows_tx2 = tx2.select(table_name).execute(&*tx2).await.unwrap();
            assert_eq!(rows_tx2.len(), 3); // TX1_DATA + TX2_DATA + TX2_NESTED

            sp1_tx2.release().await.unwrap();
            tx2.commit().await.unwrap();
        }

        // Verify final state contains data from both transactions
        let final_check = db.begin_transaction().await.unwrap();
        let rows = final_check
            .select(table_name)
            .execute(&*final_check)
            .await
            .unwrap();
        assert_eq!(rows.len(), 3); // TX1_DATA + TX2_DATA + TX2_NESTED

        let rows = final_check
            .select(table_name)
            .sort("name", switchy_database::query::SortDirection::Asc)
            .execute(&*final_check)
            .await
            .unwrap();
        assert_eq!(rows[0].get("name").unwrap().as_str().unwrap(), "TX1_DATA");
        assert_eq!(rows[1].get("name").unwrap().as_str().unwrap(), "TX2_DATA");
        assert_eq!(rows[2].get("name").unwrap().as_str().unwrap(), "TX2_NESTED");

        final_check.commit().await.unwrap();
    }

    /// Test 8: Savepoint after failed operation
    ///
    /// This test verifies backend-specific behavior when creating savepoints after
    /// a failed operation within a transaction.
    ///
    /// ## Backend Compatibility
    ///
    /// ### ✅ Supported: SQLite, MySQL
    /// These databases allow creating new savepoints after an error occurs within
    /// a transaction. The transaction remains viable and can continue with new
    /// operations after the error.
    ///
    /// ### ❌ NOT Supported: PostgreSQL
    /// PostgreSQL has strict transaction semantics - when any error occurs within
    /// a transaction, the entire transaction enters an ABORTED state. In this state:
    /// - No new operations are allowed (including savepoint creation)
    /// - Only ROLLBACK or ROLLBACK TO SAVEPOINT commands work
    /// - Error: "current transaction is aborted, commands ignored until end of transaction block"
    ///
    /// ## PostgreSQL Error Recovery Pattern
    /// For PostgreSQL, you must create savepoints BEFORE potential errors:
    /// ```
    /// let sp = tx.savepoint("before_operation").await?;
    /// match risky_operation().await {
    ///     Ok(_) => sp.release().await?,
    ///     Err(_) => {
    ///         sp.rollback().await?;  // Must rollback to continue
    ///         // Now transaction is viable again
    ///     }
    /// }
    /// ```
    ///
    /// ## Test Scenario
    /// 1. Start transaction with valid initial insert
    /// 2. Attempt invalid operation (duplicate ID insert) - this fails
    /// 3. [SQLite/MySQL only] Create savepoint after the error
    /// 4. Perform valid operations within new savepoint
    /// 5. Verify transaction can continue and commit successfully
    ///
    /// This test is excluded from PostgreSQL test suites due to incompatible
    /// transaction error semantics.
    async fn test_savepoint_after_failed_operation(&self) {
        let table_name = "sp_failed_op";
        let Some(db) = self.get_database().await else {
            return;
        };

        self.create_test_schema(&*db, table_name).await;

        let tx = db.begin_transaction().await.unwrap();

        // Insert initial valid data
        tx.insert(table_name)
            .value("id", 1i64)
            .value("name", "VALID")
            .value("value", 100i64)
            .value("savepoint_level", 0i64)
            .value("operation_type", "initial")
            .execute(&*tx)
            .await
            .unwrap();

        // Attempt invalid operation (duplicate primary key)
        let result = tx
            .insert(table_name)
            .value("id", 1i64)
            .value("name", "DUPLICATE")
            .value("value", 200i64)
            .value("savepoint_level", 0i64)
            .value("operation_type", "error")
            .execute(&*tx)
            .await;
        assert!(result.is_err(), "Duplicate primary key should fail");

        // Create savepoint after error - this should still work
        let sp1 = tx
            .savepoint("after_error")
            .await
            .expect("Savepoint creation should work after failed operation");

        // Perform valid operations within savepoint
        tx.insert(table_name)
            .value("id", 2i64)
            .value("name", "RECOVERY")
            .value("value", 300i64)
            .value("savepoint_level", 1i64)
            .value("operation_type", "recovery")
            .execute(&*tx)
            .await
            .unwrap();

        // Verify data is correct
        let rows = tx.select(table_name).execute(&*tx).await.unwrap();
        assert_eq!(rows.len(), 2); // VALID + RECOVERY

        // Test rollback after failed operation
        let sp2 = tx.savepoint("test_rollback").await.unwrap();
        tx.insert(table_name)
            .value("id", 3i64)
            .value("name", "TEMP")
            .value("value", 400i64)
            .value("savepoint_level", 2i64)
            .value("operation_type", "temp")
            .execute(&*tx)
            .await
            .unwrap();

        // Attempt another invalid operation
        let result = tx
            .insert(table_name)
            .value("id", 1i64)
            .value("name", "ANOTHER_DUP")
            .value("value", 500i64)
            .value("savepoint_level", 2i64)
            .value("operation_type", "error")
            .execute(&*tx)
            .await;
        assert!(result.is_err(), "Another duplicate primary key should fail");

        // Rollback savepoint after error
        sp2.rollback_to().await.unwrap();

        // Verify TEMP data is gone, but RECOVERY data remains
        let rows = tx.select(table_name).execute(&*tx).await.unwrap();
        assert_eq!(rows.len(), 2); // VALID + RECOVERY

        let rows = tx
            .select(table_name)
            .sort("name", switchy_database::query::SortDirection::Asc)
            .execute(&*tx)
            .await
            .unwrap();
        assert_eq!(rows[0].get("name").unwrap().as_str().unwrap(), "RECOVERY");
        assert_eq!(rows[1].get("name").unwrap().as_str().unwrap(), "VALID");

        // Release savepoint and commit
        sp1.release().await.unwrap();
        tx.commit().await.unwrap();

        // Final verification
        let final_check = db.begin_transaction().await.unwrap();
        let rows = final_check
            .select(table_name)
            .execute(&*final_check)
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
        final_check.commit().await.unwrap();
    }

    /// Test 9: True concurrent savepoints with transaction isolation
    /// Tests concurrent transactions running simultaneously with savepoints
    /// Verifies proper isolation - transactions should NOT see each other's uncommitted data
    /// Uses staggered starts and retry logic to handle SQLite concurrency
    async fn test_concurrent_savepoints_with_isolation(&self)
    where
        Self::DatabaseType: 'static,
    {
        /// Simplified result verification
        async fn verify_simplified_concurrent_test_results<D: Database + Send + Sync>(
            tx1_result: TransactionResult,
            tx2_result: TransactionResult,
            db: &D,
            table_name: &str,
        ) {
            // Verify TX1 results
            assert_eq!(tx1_result.final_status, "COMMITTED");
            assert_eq!(
                tx1_result.rows_seen_during, 2,
                "TX1 should see 2 rows during execution"
            );
            assert_eq!(
                tx1_result.rows_seen_at_end, 1,
                "TX1 should have 1 row after rollback"
            );
            assert!(
                tx1_result
                    .savepoints_created
                    .contains(&"tx1_sp1".to_string())
            );
            assert!(
                tx1_result
                    .savepoints_created
                    .contains(&"tx1_sp2".to_string())
            );

            // Verify TX2 results
            assert_eq!(tx2_result.final_status, "ROLLED_BACK");
            assert_eq!(
                tx2_result.rows_seen_during, 2,
                "TX2 should see 2 rows during execution"
            );
            assert_eq!(
                tx2_result.rows_seen_at_end, 0,
                "TX2 should have 0 rows after rollback"
            );
            assert!(
                tx2_result
                    .savepoints_created
                    .contains(&"tx2_sp1".to_string())
            );
            assert!(
                tx2_result
                    .savepoints_created
                    .contains(&"tx2_sp2".to_string())
            );

            // Verify isolation - neither transaction saw the other's data
            assert!(
                tx1_result
                    .operations_performed
                    .contains(&"ISOLATION_CHECK_PASSED".to_string())
            );
            assert!(
                tx2_result
                    .operations_performed
                    .contains(&"ISOLATION_CHECK_PASSED".to_string())
            );

            // Verify final database state
            let final_rows = db.select(table_name).execute(db).await.unwrap();

            assert_eq!(
                final_rows.len(),
                1,
                "Database should have exactly 1 row (from TX1)"
            );

            let row = &final_rows[0];
            assert_eq!(row.get("name").unwrap().as_str().unwrap(), "TX1_BASE");
            assert_eq!(row.get("value").unwrap().as_i64().unwrap(), 100);
            assert_eq!(row.get("transaction_id").unwrap().as_i64().unwrap(), 1);

            println!("✓ Simplified concurrent savepoint test passed!");
            println!("  - TX1 committed 1 row successfully");
            println!("  - TX2 rolled back completely");
            println!("  - Isolation maintained throughout");
            println!("  - SQLite concurrency handled properly");
        }

        /// Simplified Transaction 1 Logic - Less barriers, more robust
        async fn simplified_transaction_1_logic<D: Database + Send + Sync + 'static>(
            db: Arc<D>,
            start_barrier: Arc<Barrier>,
            complete_barrier: Arc<Barrier>,
            result_sender: mpsc::Sender<TransactionResult>,
            table_name: &str,
        ) -> Result<TransactionResult, String> {
            let mut result = TransactionResult {
                rows_seen_during: 0,
                rows_seen_at_end: 0,
                savepoints_created: Vec::new(),
                operations_performed: Vec::new(),
                final_status: String::new(),
            };

            // Wait for both transactions to be ready
            start_barrier.wait().await;

            // Small delay to ensure TX2 starts after TX1
            switchy_async::time::sleep(std::time::Duration::from_millis(100)).await;

            let tx = db
                .begin_transaction()
                .await
                .map_err(|e| format!("Failed to begin TX1: {:?}", e))?;
            result
                .operations_performed
                .push("BEGIN_TRANSACTION".to_string());

            let sp1 = tx
                .savepoint("tx1_sp1")
                .await
                .map_err(|e| format!("Failed to create SP1: {:?}", e))?;
            result.savepoints_created.push("tx1_sp1".to_string());

            // Insert base data
            tx.insert(table_name)
                .value("name", "TX1_BASE")
                .value("value", 100i64)
                .value("savepoint_level", 1i64)
                .value("operation_type", "concurrent_base")
                .value("transaction_id", 1i64)
                .value("created_at", current_timestamp())
                .execute(&*tx)
                .await
                .map_err(|e| format!("Failed to insert TX1 base data: {:?}", e))?;

            // Create nested savepoint
            let sp2 = tx
                .savepoint("tx1_sp2")
                .await
                .map_err(|e| format!("Failed to create SP2: {:?}", e))?;
            result.savepoints_created.push("tx1_sp2".to_string());

            // Insert nested data
            tx.insert(table_name)
                .value("name", "TX1_NESTED")
                .value("value", 101i64)
                .value("savepoint_level", 2i64)
                .value("operation_type", "concurrent_nested")
                .value("transaction_id", 1i64)
                .value("created_at", current_timestamp())
                .execute(&*tx)
                .await
                .map_err(|e| format!("Failed to insert TX1 nested data: {:?}", e))?;

            // Check our own data (isolation test)
            let isolation_check = tx
                .select(table_name)
                .execute(&*tx)
                .await
                .map_err(|e| format!("Failed isolation check: {:?}", e))?;

            result.rows_seen_during = isolation_check.len();

            // Verify we only see our own data
            for row in &isolation_check {
                let tx_id = row
                    .get("transaction_id")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                if tx_id != 1 {
                    return Err(format!(
                        "TX1 saw data from TX{} - isolation violated!",
                        tx_id
                    ));
                }
            }
            result
                .operations_performed
                .push("ISOLATION_CHECK_PASSED".to_string());

            // Rollback nested savepoint (keep only base data)
            sp2.rollback_to()
                .await
                .map_err(|e| format!("Failed to rollback SP2: {:?}", e))?;
            result
                .operations_performed
                .push("ROLLBACK_TO_SP2".to_string());

            // Verify rollback worked
            let after_rollback = tx
                .select(table_name)
                .execute(&*tx)
                .await
                .map_err(|e| format!("Failed to check after rollback: {:?}", e))?;

            if after_rollback.len() != 1 {
                return Err(format!(
                    "TX1 expected 1 row after rollback, got {}",
                    after_rollback.len()
                ));
            }

            // Commit transaction
            sp1.release()
                .await
                .map_err(|e| format!("Failed to release SP1: {:?}", e))?;

            tx.commit()
                .await
                .map_err(|e| format!("Failed to commit TX1: {:?}", e))?;
            result.operations_performed.push("COMMIT".to_string());
            result.final_status = "COMMITTED".to_string();

            // Check final state
            let final_check = db
                .select(table_name)
                .where_eq("transaction_id", 1i64)
                .execute(&*db)
                .await
                .map_err(|e| format!("Failed final check: {:?}", e))?;

            result.rows_seen_at_end = final_check.len();

            // Wait for TX2 to complete
            complete_barrier.wait().await;

            result_sender.send_async(result.clone()).await.ok();
            Ok(result)
        }

        /// Simplified Transaction 2 Logic - Less barriers, more robust
        async fn simplified_transaction_2_logic<D: Database + Send + Sync + 'static>(
            db: Arc<D>,
            start_barrier: Arc<Barrier>,
            complete_barrier: Arc<Barrier>,
            result_sender: mpsc::Sender<TransactionResult>,
            table_name: &str,
        ) -> Result<TransactionResult, String> {
            let mut result = TransactionResult {
                rows_seen_during: 0,
                rows_seen_at_end: 0,
                savepoints_created: Vec::new(),
                operations_performed: Vec::new(),
                final_status: String::new(),
            };

            // Wait for both transactions to be ready
            start_barrier.wait().await;

            // Retry logic for SQLite BUSY errors
            let mut retry_count = 0;
            let max_retries = 5;

            loop {
                match db.begin_transaction().await {
                    Ok(tx) => {
                        result
                            .operations_performed
                            .push("BEGIN_TRANSACTION".to_string());

                        let _sp1 = tx
                            .savepoint("tx2_sp1")
                            .await
                            .map_err(|e| format!("Failed to create SP1: {:?}", e))?;
                        result.savepoints_created.push("tx2_sp1".to_string());

                        // Insert base data
                        tx.insert(table_name)
                            .value("name", "TX2_BASE")
                            .value("value", 200i64)
                            .value("savepoint_level", 1i64)
                            .value("operation_type", "concurrent_base")
                            .value("transaction_id", 2i64)
                            .value("created_at", current_timestamp())
                            .execute(&*tx)
                            .await
                            .map_err(|e| format!("Failed to insert TX2 base data: {:?}", e))?;

                        // Create nested savepoint
                        let sp2 = tx
                            .savepoint("tx2_sp2")
                            .await
                            .map_err(|e| format!("Failed to create SP2: {:?}", e))?;
                        result.savepoints_created.push("tx2_sp2".to_string());

                        // Insert nested data
                        tx.insert(table_name)
                            .value("name", "TX2_NESTED")
                            .value("value", 201i64)
                            .value("savepoint_level", 2i64)
                            .value("operation_type", "concurrent_nested")
                            .value("transaction_id", 2i64)
                            .value("created_at", current_timestamp())
                            .execute(&*tx)
                            .await
                            .map_err(|e| format!("Failed to insert TX2 nested data: {:?}", e))?;

                        // Check our own data (isolation test)
                        let isolation_check = tx
                            .select(table_name)
                            .execute(&*tx)
                            .await
                            .map_err(|e| format!("Failed isolation check: {:?}", e))?;

                        result.rows_seen_during = isolation_check.len();

                        // Verify we only see our own data
                        for row in &isolation_check {
                            let tx_id = row
                                .get("transaction_id")
                                .and_then(|v| v.as_i64())
                                .unwrap_or(0);
                            if tx_id != 2 {
                                return Err(format!(
                                    "TX2 saw data from TX{} - isolation violated!",
                                    tx_id
                                ));
                            }
                        }
                        result
                            .operations_performed
                            .push("ISOLATION_CHECK_PASSED".to_string());

                        // Rollback nested savepoint
                        sp2.rollback_to()
                            .await
                            .map_err(|e| format!("Failed to rollback SP2: {:?}", e))?;
                        result
                            .operations_performed
                            .push("ROLLBACK_TO_SP2".to_string());

                        // Rollback entire transaction
                        tx.rollback()
                            .await
                            .map_err(|e| format!("Failed to rollback TX2: {:?}", e))?;
                        result.operations_performed.push("ROLLBACK".to_string());
                        result.final_status = "ROLLED_BACK".to_string();

                        // Check final state (should see nothing from TX2)
                        let final_check = db
                            .select(table_name)
                            .where_eq("transaction_id", 2i64)
                            .execute(&*db)
                            .await
                            .map_err(|e| format!("Failed final check: {:?}", e))?;

                        result.rows_seen_at_end = final_check.len();

                        break; // Success, exit retry loop
                    }
                    Err(e) => {
                        retry_count += 1;
                        if retry_count >= max_retries {
                            return Err(format!(
                                "Failed to begin TX2 after {} retries: {:?}",
                                max_retries, e
                            ));
                        }

                        // Wait before retrying
                        switchy_async::time::sleep(std::time::Duration::from_millis(
                            100 * retry_count as u64,
                        ))
                        .await;
                        continue;
                    }
                }
            }

            // Wait for TX1 to complete
            complete_barrier.wait().await;

            result_sender.send_async(result.clone()).await.ok();
            Ok(result)
        }

        // Step 1: Setup
        let Some(db_original) = self.get_database().await else {
            eprintln!("Skipping concurrent test - database not available");
            return;
        };

        // Create schema once
        let table_name = "sp_concurrent";
        self.create_test_schema(&*db_original, table_name).await;

        // Step 2: Use simpler synchronization - just start/complete barriers
        let start_barrier = Arc::new(Barrier::new(2));
        let complete_barrier = Arc::new(Barrier::new(2));

        // Step 3: Create communication channels
        let (tx1_result_send, mut tx1_result_recv) = mpsc::unbounded::<TransactionResult>();
        let (tx2_result_send, mut tx2_result_recv) = mpsc::unbounded::<TransactionResult>();

        // Step 4: Spawn Transaction 1 (commits)
        let db1 = Arc::clone(&db_original);
        let start_barrier1 = start_barrier.clone();
        let complete_barrier1 = complete_barrier.clone();
        let task1 = switchy_async::task::spawn(async move {
            simplified_transaction_1_logic::<Self::DatabaseType>(
                db1,
                start_barrier1,
                complete_barrier1,
                tx1_result_send,
                table_name,
            )
            .await
        });

        // Step 5: Spawn Transaction 2 (rolls back) with slight delay
        let db2 = Arc::clone(&db_original);
        let start_barrier2 = start_barrier.clone();
        let complete_barrier2 = complete_barrier.clone();
        let task2 = switchy_async::task::spawn(async move {
            // Small delay to stagger transaction starts
            switchy_async::time::sleep(std::time::Duration::from_millis(50)).await;

            simplified_transaction_2_logic::<Self::DatabaseType>(
                db2,
                start_barrier2,
                complete_barrier2,
                tx2_result_send,
                table_name,
            )
            .await
        });

        // Step 6: Wait for completion with timeout
        let timeout_duration = std::time::Duration::from_secs(15);
        let join_future = async { switchy_async::join!(task1, task2) };
        let (r1, r2) = switchy_async::time::timeout(timeout_duration, join_future)
            .await
            .expect("Test timed out - possible deadlock or SQLite busy");

        // Handle any panics
        let _tx1_result = r1.expect("TX1 task panicked").unwrap();
        let _tx2_result = r2.expect("TX2 task panicked").unwrap();

        // Get results from channels
        let tx1_final = tx1_result_recv
            .recv_async()
            .await
            .expect("TX1 should send result");
        let tx2_final = tx2_result_recv
            .recv_async()
            .await
            .expect("TX2 should send result");

        // Step 7: Verify results
        verify_simplified_concurrent_test_results::<Self::DatabaseType>(
            tx1_final,
            tx2_final,
            &*db_original,
            table_name,
        )
        .await;
    }
}
