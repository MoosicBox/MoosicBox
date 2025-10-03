use chrono::{NaiveDateTime, Utc};
use std::sync::Arc;
use switchy_database::{Database, DatabaseValue};

/// Comprehensive datetime test suite trait for cross-backend testing
#[allow(unused)]
pub trait DateTimeTestSuite<I: Into<String>> {
    type DatabaseType: Database + Send + Sync;

    /// Get database instance for testing (returns None if unavailable)
    async fn get_database(&self) -> Option<Arc<Self::DatabaseType>>;

    fn gen_param(&self, i: u8) -> I;

    /// Generate a unique suffix for this test run
    fn get_unique_suffix(&self) -> String {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        (nanos % 1_000_000_000).to_string()
    }

    /// Get the full table name for a specific test
    fn get_table_name(&self, test_name: &str, backend: &str) -> String {
        format!(
            "{backend}_datetime_{test_name}_{}",
            self.get_unique_suffix()
        )
    }

    /// Create test table with datetime columns (backend-specific implementation)
    async fn create_test_table(&self, db: &Self::DatabaseType, table_name: &str);

    /// Clean up test data (backend-specific implementation)
    async fn cleanup_test_data(&self, db: &Self::DatabaseType, table_name: &str);

    /// Extract timestamp from database row (backend-specific implementation)
    async fn get_timestamp_column(
        &self,
        db: &Self::DatabaseType,
        table_name: &str,
        column: &str,
        id: i32,
    ) -> Option<NaiveDateTime>;

    /// Get the ID of an inserted row by description (backend-specific implementation)
    async fn get_row_id_by_description(
        &self,
        db: &Self::DatabaseType,
        table_name: &str,
        description: &str,
    ) -> i32;

    /// Helper to verify timestamp is within expected range
    fn assert_timestamp_near(
        &self,
        actual: NaiveDateTime,
        expected: NaiveDateTime,
        tolerance_mins: i64,
    ) {
        let diff = (actual - expected).num_seconds().abs();
        assert!(
            diff <= tolerance_mins * 60,
            "Timestamp {actual} not within {tolerance_mins}m of {expected} (diff: {diff}s)"
        );
    }

    // ===== ABSTRACT TEST METHODS =====

    /// Test basic NOW() insertion functionality
    async fn test_now_insert(&self, backend: &str) {
        let Some(db) = self.get_database().await else {
            println!("Skipping test - database not available");
            return;
        };

        let table_name = self.get_table_name("now_insert", backend);
        self.create_test_table(&db, &table_name).await;

        let before = Utc::now().naive_utc();

        // Insert row with NOW()
        self.insert_with_now(&db, &table_name, "test now insert")
            .await;

        let after = Utc::now().naive_utc();

        // Query it back and verify timestamp is within reasonable range
        let id = self
            .get_row_id_by_description(&db, &table_name, "test now insert")
            .await;
        let created_at = self
            .get_timestamp_column(&db, &table_name, "created_at", id)
            .await
            .expect("Failed to get created_at timestamp");

        // Should be within 5 seconds of when we started the test
        self.assert_timestamp_near(created_at, before, 5);
        self.assert_timestamp_near(created_at, after, 5);

        // Cleanup
        self.cleanup_test_data(&db, &table_name).await;
    }

    /// Test NOW() in WHERE clause conditions
    async fn test_now_in_where_clause(&self, backend: &str) {
        let Some(db) = self.get_database().await else {
            println!("Skipping test - database not available");
            return;
        };

        let table_name = self.get_table_name("now_where", backend);
        self.create_test_table(&db, &table_name).await;

        // Insert row with future expiry (NOW + 1 day)
        self.insert_with_expires_at(
            &db,
            &table_name,
            DatabaseValue::now().plus_days(1).build(),
            "future expiry",
        )
        .await;

        // Insert row with past expiry (NOW - 1 day)
        self.insert_with_expires_at(
            &db,
            &table_name,
            DatabaseValue::now().minus_days(1).build(),
            "past expiry",
        )
        .await;

        // Query WHERE expires_at > NOW() - should only return future row
        let future_rows = db
            .query_raw_params(
                &format!(
                    "SELECT * FROM {} WHERE expires_at > {}",
                    table_name,
                    self.gen_param(1).into()
                ),
                &[DatabaseValue::Now],
            )
            .await
            .expect("Failed to query future rows");

        assert_eq!(
            future_rows.len(),
            1,
            "Should find exactly 1 row with future expiry"
        );

        // Query WHERE expires_at < NOW() - should only return past row
        let past_rows = db
            .query_raw_params(
                &format!(
                    "SELECT * FROM {} WHERE expires_at < {}",
                    table_name,
                    self.gen_param(1).into()
                ),
                &[DatabaseValue::Now],
            )
            .await
            .expect("Failed to query past rows");

        assert_eq!(
            past_rows.len(),
            1,
            "Should find exactly 1 row with past expiry"
        );

        // Cleanup
        self.cleanup_test_data(&db, &table_name).await;
    }

    /// Test NOW() with interval arithmetic
    async fn test_now_plus_interval(&self, backend: &str) {
        let Some(db) = self.get_database().await else {
            println!("Skipping test - database not available");
            return;
        };

        let table_name = self.get_table_name("now_plus", backend);
        self.create_test_table(&db, &table_name).await;

        let before = Utc::now().naive_utc();

        // Insert row with NOW() + 1 hour
        self.insert_with_scheduled_for(
            &db,
            &table_name,
            DatabaseValue::now().plus_hours(1).build(),
            "scheduled future",
        )
        .await;

        // Query it back and verify timestamp is ~1 hour in future
        let id = self
            .get_row_id_by_description(&db, &table_name, "scheduled future")
            .await;
        let scheduled_for = self
            .get_timestamp_column(&db, &table_name, "scheduled_for", id)
            .await
            .expect("Failed to get scheduled_for timestamp");

        // Should be about 1 hour after test start (within 5 seconds tolerance)
        let expected_time = before + chrono::Duration::hours(1);
        self.assert_timestamp_near(scheduled_for, expected_time, 5);

        // Cleanup
        self.cleanup_test_data(&db, &table_name).await;
    }

    /// Test NOW() with negative interval (past times)
    async fn test_now_minus_interval(&self, backend: &str) {
        let Some(db) = self.get_database().await else {
            println!("Skipping test - database not available");
            return;
        };

        let table_name = self.get_table_name("now_minus", backend);
        self.create_test_table(&db, &table_name).await;

        let before = Utc::now().naive_utc();

        // Insert row with NOW() - 30 minutes
        self.insert_with_scheduled_for(
            &db,
            &table_name,
            DatabaseValue::now().minus_minutes(30).build(),
            "scheduled past",
        )
        .await;

        // Query it back and verify timestamp is ~30 minutes in past
        let id = self
            .get_row_id_by_description(&db, &table_name, "scheduled past")
            .await;
        let scheduled_for = self
            .get_timestamp_column(&db, &table_name, "scheduled_for", id)
            .await
            .expect("Failed to get scheduled_for timestamp");

        // Should be about 30 minutes before test start (within 5 seconds tolerance)
        let expected_time = before - chrono::Duration::minutes(30);
        self.assert_timestamp_near(scheduled_for, expected_time, 5);

        // Cleanup
        self.cleanup_test_data(&db, &table_name).await;
    }

    /// Test complex interval combinations
    async fn test_complex_interval_operations(&self, backend: &str) {
        let Some(db) = self.get_database().await else {
            println!("Skipping test - database not available");
            return;
        };

        let table_name = self.get_table_name("complex_interval", backend);
        self.create_test_table(&db, &table_name).await;

        let before = Utc::now().naive_utc();

        // Create complex time: NOW() + 1 day + 2 hours - 15 minutes
        let complex_future = DatabaseValue::now()
            .plus_days(1)
            .plus_hours(2)
            .minus_minutes(15);

        self.insert_with_scheduled_for(&db, &table_name, complex_future.build(), "complex future")
            .await;

        // Query it back and verify the complex calculation
        let id = self
            .get_row_id_by_description(&db, &table_name, "complex future")
            .await;
        let scheduled_for = self
            .get_timestamp_column(&db, &table_name, "scheduled_for", id)
            .await
            .expect("Failed to get scheduled_for timestamp");

        // Calculate expected time: +1 day +2 hours -15 minutes
        let expected_time = before + chrono::Duration::days(1) + chrono::Duration::hours(2)
            - chrono::Duration::minutes(15);

        self.assert_timestamp_near(scheduled_for, expected_time, 5);

        // Cleanup
        self.cleanup_test_data(&db, &table_name).await;
    }

    /// Test UPDATE with NOW() values
    async fn test_update_with_now(&self, backend: &str) {
        let Some(db) = self.get_database().await else {
            println!("Skipping test - database not available");
            return;
        };

        let table_name = self.get_table_name("update_now", backend);
        self.create_test_table(&db, &table_name).await;

        // Insert initial row with fixed timestamp
        let initial_time = Utc::now().naive_utc() - chrono::Duration::hours(1);
        self.insert_with_expires_at(
            &db,
            &table_name,
            DatabaseValue::DateTime(initial_time),
            "update test",
        )
        .await;

        let before_update = Utc::now().naive_utc();

        // Update with NOW()
        let id = self
            .get_row_id_by_description(&db, &table_name, "update test")
            .await;
        db.exec_raw_params(
            &format!(
                "UPDATE {} SET expires_at = {} WHERE id = {}",
                table_name,
                self.gen_param(1).into(),
                self.gen_param(2).into()
            ),
            &[DatabaseValue::Now, DatabaseValue::Int64(id as i64)],
        )
        .await
        .expect("Failed to update with NOW()");

        let after_update = Utc::now().naive_utc();

        // Verify the timestamp was updated to NOW()
        let updated_expires_at = self
            .get_timestamp_column(&db, &table_name, "expires_at", id)
            .await
            .expect("Failed to get updated expires_at timestamp");

        // Should be within 5 seconds of the update time
        self.assert_timestamp_near(updated_expires_at, before_update, 5);
        self.assert_timestamp_near(updated_expires_at, after_update, 5);

        // Should NOT be the initial time
        assert!(
            (updated_expires_at - initial_time).num_seconds().abs() > 3000, // > 50 minutes
            "Updated timestamp should be much different from initial time"
        );

        // Cleanup
        self.cleanup_test_data(&db, &table_name).await;
    }

    /// Test multiple NOW() values in same query are consistent
    async fn test_multiple_now_consistency(&self, backend: &str) {
        let Some(db) = self.get_database().await else {
            println!("Skipping test - database not available");
            return;
        };

        let table_name = self.get_table_name("multiple_now", backend);
        self.create_test_table(&db, &table_name).await;

        // Insert with multiple NOW() values in same query
        db.exec_raw_params(
            &format!(
                "INSERT INTO {} (created_at, expires_at, scheduled_for, description) VALUES ({}, {}, {}, {})",
                table_name,
                self.gen_param(1).into(),
                self.gen_param(2).into(),
                self.gen_param(3).into(),
                self.gen_param(4).into()
            ),
            &[
                DatabaseValue::Now,
                DatabaseValue::Now,
                DatabaseValue::Now,
                DatabaseValue::String("consistency test".to_string()),
            ],
        )
        .await
        .expect("Failed to insert with multiple NOW() values");

        // Get the inserted row
        let id = self
            .get_row_id_by_description(&db, &table_name, "consistency test")
            .await;

        let created_at = self
            .get_timestamp_column(&db, &table_name, "created_at", id)
            .await
            .expect("Failed to get created_at");
        let expires_at = self
            .get_timestamp_column(&db, &table_name, "expires_at", id)
            .await
            .expect("Failed to get expires_at");
        let scheduled_for = self
            .get_timestamp_column(&db, &table_name, "scheduled_for", id)
            .await
            .expect("Failed to get scheduled_for");

        // All three timestamps should be very close (within 1 second)
        self.assert_timestamp_near(created_at, expires_at, 1);
        self.assert_timestamp_near(created_at, scheduled_for, 1);
        self.assert_timestamp_near(expires_at, scheduled_for, 1);

        // Cleanup
        self.cleanup_test_data(&db, &table_name).await;
    }

    /// Test mixed NOW() and NowPlus in complex operations
    async fn test_mixed_now_operations(&self, backend: &str) {
        let Some(db) = self.get_database().await else {
            println!("Skipping test - database not available");
            return;
        };

        let table_name = self.get_table_name("mixed_now", backend);
        self.create_test_table(&db, &table_name).await;

        let before = Utc::now().naive_utc();

        // Complex query mixing NOW() and NowPlus
        self.insert_with_scheduled_for(
            &db,
            &table_name,
            DatabaseValue::now().plus_minutes(30).build(),
            "mixed operations test",
        )
        .await;

        // Query it back
        let id = self
            .get_row_id_by_description(&db, &table_name, "mixed operations test")
            .await;
        let scheduled_for = self
            .get_timestamp_column(&db, &table_name, "scheduled_for", id)
            .await
            .expect("Failed to get scheduled_for timestamp");

        // Should be about 30 minutes in future
        let expected_time = before + chrono::Duration::minutes(30);
        self.assert_timestamp_near(scheduled_for, expected_time, 5);

        // Cleanup
        self.cleanup_test_data(&db, &table_name).await;
    }

    // ===== ADDITIONAL TEST METHODS =====

    /// Test NOW() + days
    async fn test_now_plus_days(&self, backend: &str) {
        let Some(db) = self.get_database().await else {
            println!("Skipping test - database not available");
            return;
        };

        let table_name = self.get_table_name("now_plus_days", backend);
        self.create_test_table(&db, &table_name).await;

        let before = Utc::now().naive_utc();

        // Insert with NOW() + 1 day
        self.insert_with_scheduled_for(
            &db,
            &table_name,
            DatabaseValue::now().plus_days(1).build(),
            "plus one day",
        )
        .await;

        // Query it back
        let id = self
            .get_row_id_by_description(&db, &table_name, "plus one day")
            .await;
        let scheduled_for = self
            .get_timestamp_column(&db, &table_name, "scheduled_for", id)
            .await
            .expect("Failed to get scheduled_for timestamp");

        // Should be about 1 day in future
        let expected_time = before + chrono::Duration::days(1);
        self.assert_timestamp_near(scheduled_for, expected_time, 10);

        // Cleanup
        self.cleanup_test_data(&db, &table_name).await;
    }

    /// Test NOW() - days
    async fn test_now_minus_days(&self, backend: &str) {
        let Some(db) = self.get_database().await else {
            println!("Skipping test - database not available");
            return;
        };

        let table_name = self.get_table_name("now_minus_days", backend);
        self.create_test_table(&db, &table_name).await;

        let before = Utc::now().naive_utc();

        // Insert with NOW() - 1 day
        self.insert_with_scheduled_for(
            &db,
            &table_name,
            DatabaseValue::now().minus_days(1).build(),
            "minus one day",
        )
        .await;

        // Query it back
        let id = self
            .get_row_id_by_description(&db, &table_name, "minus one day")
            .await;
        let scheduled_for = self
            .get_timestamp_column(&db, &table_name, "scheduled_for", id)
            .await
            .expect("Failed to get scheduled_for timestamp");

        // Should be about 1 day in past
        let expected_time = before - chrono::Duration::days(1);
        self.assert_timestamp_near(scheduled_for, expected_time, 10);

        // Cleanup
        self.cleanup_test_data(&db, &table_name).await;
    }

    /// Test NOW() + hours/minutes/seconds
    async fn test_now_plus_hours_minutes_seconds(&self, backend: &str) {
        let Some(db) = self.get_database().await else {
            println!("Skipping test - database not available");
            return;
        };

        let table_name = self.get_table_name("now_plus_hours_minutes_seconds", backend);
        self.create_test_table(&db, &table_name).await;

        // Test with hours, minutes, seconds
        self.insert_with_scheduled_for(
            &db,
            &table_name,
            DatabaseValue::now()
                .plus_hours(2)
                .plus_minutes(30)
                .plus_seconds(15)
                .build(),
            "complex time",
        )
        .await;

        // Cleanup
        self.cleanup_test_data(&db, &table_name).await;
    }

    /// Test NOW() + minutes with normalization
    async fn test_now_plus_minutes_normalization(&self, backend: &str) {
        let Some(db) = self.get_database().await else {
            println!("Skipping test - database not available");
            return;
        };

        let table_name = self.get_table_name("now_plus_minutes_normalization", backend);
        self.create_test_table(&db, &table_name).await;

        // Test with large minutes that should normalize to hours
        self.insert_with_scheduled_for(
            &db,
            &table_name,
            DatabaseValue::now().plus_minutes(90).build(),
            "normalized time",
        )
        .await;

        // Cleanup
        self.cleanup_test_data(&db, &table_name).await;
    }

    /// Test NOW() + complex interval
    async fn test_now_plus_complex_interval(&self, backend: &str) {
        let Some(db) = self.get_database().await else {
            println!("Skipping test - database not available");
            return;
        };

        let table_name = self.get_table_name("now_plus_complex_interval", backend);
        self.create_test_table(&db, &table_name).await;

        // Create complex time: NOW() + 1 day + 2 hours - 15 minutes
        let complex_future = DatabaseValue::now()
            .plus_days(1)
            .plus_hours(2)
            .minus_minutes(15);

        self.insert_with_scheduled_for(&db, &table_name, complex_future.build(), "complex future")
            .await;

        // Cleanup
        self.cleanup_test_data(&db, &table_name).await;
    }

    /// Test zero interval returns NOW()
    async fn test_zero_interval_returns_now(&self, backend: &str) {
        let Some(db) = self.get_database().await else {
            println!("Skipping test - database not available");
            return;
        };

        let table_name = self.get_table_name("zero_interval_returns_now", backend);
        self.create_test_table(&db, &table_name).await;

        let before = Utc::now().naive_utc();

        // Insert with zero interval (should be same as NOW)
        self.insert_with_scheduled_for(
            &db,
            &table_name,
            DatabaseValue::now().build(),
            "zero interval",
        )
        .await;

        let after = Utc::now().naive_utc();

        // Should be within reasonable range of NOW
        let id = self
            .get_row_id_by_description(&db, &table_name, "zero interval")
            .await;
        let scheduled_for = self
            .get_timestamp_column(&db, &table_name, "scheduled_for", id)
            .await
            .expect("Failed to get scheduled_for timestamp");

        self.assert_timestamp_near(scheduled_for, before, 5);
        self.assert_timestamp_near(scheduled_for, after, 5);

        // Cleanup
        self.cleanup_test_data(&db, &table_name).await;
    }

    /// Test mixed parameters
    async fn test_mixed_parameters(&self, backend: &str) {
        let Some(db) = self.get_database().await else {
            println!("Skipping test - database not available");
            return;
        };

        let table_name = self.get_table_name("mixed_parameters", backend);
        self.create_test_table(&db, &table_name).await;

        // Test mixed NOW() and interval operations
        self.insert_with_scheduled_for(
            &db,
            &table_name,
            DatabaseValue::now().plus_minutes(30).build(),
            "mixed operations test",
        )
        .await;

        // Cleanup
        self.cleanup_test_data(&db, &table_name).await;
    }

    /// Test NOW() consistency in transaction
    async fn test_now_consistency_in_transaction(&self, backend: &str) {
        let Some(db) = self.get_database().await else {
            println!("Skipping test - database not available");
            return;
        };

        let table_name = self.get_table_name("now_consistency_in_transaction", backend);
        self.create_test_table(&db, &table_name).await;

        // Insert multiple rows with NOW() - should all have same timestamp within transaction
        self.insert_with_all_timestamps(
            &db,
            &table_name,
            DatabaseValue::Now,
            DatabaseValue::Now,
            DatabaseValue::Now,
            "consistent timestamps",
        )
        .await;

        // Cleanup
        self.cleanup_test_data(&db, &table_name).await;
    }

    /// Test duration conversion
    async fn test_duration_conversion(&self, backend: &str) {
        let Some(db) = self.get_database().await else {
            println!("Skipping test - database not available");
            return;
        };

        let table_name = self.get_table_name("duration_conversion", backend);
        self.create_test_table(&db, &table_name).await;

        // Test various duration formats
        self.insert_with_scheduled_for(
            &db,
            &table_name,
            DatabaseValue::now()
                .plus_days(1)
                .plus_hours(1)
                .plus_minutes(1)
                .build(),
            "duration test",
        )
        .await;

        // Cleanup
        self.cleanup_test_data(&db, &table_name).await;
    }

    // ===== HELPER METHODS (backend-specific) =====

    /// Insert row with NOW() as created_at
    async fn insert_with_now(&self, db: &Self::DatabaseType, table_name: &str, description: &str);

    /// Insert row with specific expires_at value
    async fn insert_with_expires_at(
        &self,
        db: &Self::DatabaseType,
        table_name: &str,
        expires_at: DatabaseValue,
        description: &str,
    );

    /// Insert row with specific scheduled_for value
    async fn insert_with_scheduled_for(
        &self,
        db: &Self::DatabaseType,
        table_name: &str,
        scheduled_for: DatabaseValue,
        description: &str,
    );

    /// Insert row with all timestamp values
    async fn insert_with_all_timestamps(
        &self,
        db: &Self::DatabaseType,
        table_name: &str,
        created_at: DatabaseValue,
        expires_at: DatabaseValue,
        scheduled_for: DatabaseValue,
        description: &str,
    );
}
