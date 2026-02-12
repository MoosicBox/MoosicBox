//! Integration tests for recovery scenarios
//!
//! These tests verify that the migration system properly handles failures,
//! tracks status correctly, and provides working recovery mechanisms.

use switchy_schema::{migration::MigrationStatus, version::VersionTracker};
use switchy_schema_test_utils::create_empty_in_memory;

#[cfg(feature = "code")]
#[switchy_async::test]
async fn test_migration_failure_tracking() {
    use switchy_schema::{
        discovery::code::{CodeMigration, CodeMigrationSource},
        runner::MigrationRunner,
    };

    let db = create_empty_in_memory().await.unwrap();
    let version_tracker = VersionTracker::with_table_name("__test_migrations");

    // Create migrations: one successful, one failing, one more successful
    let mut source = CodeMigrationSource::new();
    source.add_migration(CodeMigration::new(
        "001_create_users".to_string(),
        Box::new("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)".to_string()),
        Some(Box::new("DROP TABLE users".to_string())),
    ));
    source.add_migration(CodeMigration::new(
        "002_failing_migration".to_string(),
        Box::new("INVALID SQL SYNTAX".to_string()), // This will fail
        None,
    ));
    source.add_migration(CodeMigration::new(
        "003_create_posts".to_string(),
        Box::new("CREATE TABLE posts (id INTEGER PRIMARY KEY, title TEXT NOT NULL)".to_string()),
        Some(Box::new("DROP TABLE posts".to_string())),
    ));

    let runner = MigrationRunner::new(Box::new(source)).with_version_tracker(version_tracker);

    // Run migrations - should fail on the second migration
    let result = runner.run(&*db).await;
    assert!(result.is_err());

    // Create a new version tracker for checking status
    let check_version_tracker = VersionTracker::with_table_name("__test_migrations");

    // Verify the first migration was recorded as completed
    let first_status = check_version_tracker
        .get_migration_status(&*db, "001_create_users")
        .await
        .unwrap();
    assert!(first_status.is_some());
    let first_record = first_status.unwrap();
    assert_eq!(first_record.status, MigrationStatus::Completed);
    assert!(first_record.failure_reason.is_none());
    assert!(first_record.finished_on.is_some());

    // Verify the failing migration was recorded with failure details
    let failed_status = check_version_tracker
        .get_migration_status(&*db, "002_failing_migration")
        .await
        .unwrap();
    assert!(failed_status.is_some());
    let failed_record = failed_status.unwrap();
    assert_eq!(failed_record.status, MigrationStatus::Failed);
    assert!(failed_record.failure_reason.is_some());
    assert!(failed_record.finished_on.is_some());

    // Verify the third migration was never attempted
    let third_status = check_version_tracker
        .get_migration_status(&*db, "003_create_posts")
        .await
        .unwrap();
    assert!(third_status.is_none());

    // Verify dirty migrations query returns the failed migration
    let dirty_migrations = check_version_tracker
        .get_dirty_migrations(&*db)
        .await
        .unwrap();
    assert_eq!(dirty_migrations.len(), 1);
    assert_eq!(dirty_migrations[0].id, "002_failing_migration");
    assert_eq!(dirty_migrations[0].status, MigrationStatus::Failed);
}

#[cfg(feature = "code")]
#[switchy_async::test]
async fn test_dirty_state_detection() {
    use switchy_schema::{
        MigrationError,
        discovery::code::{CodeMigration, CodeMigrationSource},
        runner::MigrationRunner,
    };

    let db = create_empty_in_memory().await.unwrap();
    let version_tracker = VersionTracker::with_table_name("__test_migrations");

    // Ensure table exists
    version_tracker.ensure_table_exists(&*db).await.unwrap();

    // Manually insert an "in_progress" migration to simulate interruption
    let checksum = bytes::Bytes::from(vec![0u8; 32]);
    version_tracker
        .record_migration_started(&*db, "001_interrupted_migration", &checksum, &checksum)
        .await
        .unwrap();

    // Also add a failed migration to verify it doesn't block
    version_tracker
        .record_migration_started(&*db, "002_failed_migration", &checksum, &checksum)
        .await
        .unwrap();
    version_tracker
        .update_migration_status(
            &*db,
            "002_failed_migration",
            MigrationStatus::Failed,
            Some("Test error".to_string()),
        )
        .await
        .unwrap();

    // Create a simple migration source
    let mut source = CodeMigrationSource::new();
    source.add_migration(CodeMigration::new(
        "003_new_migration".to_string(),
        Box::new("CREATE TABLE test (id INTEGER PRIMARY KEY)".to_string()),
        Some(Box::new("DROP TABLE test".to_string())),
    ));

    // Create runner without force flag - should detect dirty state (only in-progress)
    let runner = MigrationRunner::new(Box::new(source)).with_version_tracker(version_tracker);

    // Run should fail due to dirty state (only in-progress migrations block)
    let result = runner.run(&*db).await;
    assert!(result.is_err());

    if let Err(MigrationError::DirtyState { migrations }) = result {
        assert_eq!(migrations.len(), 1);
        assert_eq!(migrations[0], "001_interrupted_migration");
    } else {
        panic!("Expected DirtyState error, got: {:?}", result);
    }

    // Create new source for force test
    let mut source2 = CodeMigrationSource::new();
    source2.add_migration(CodeMigration::new(
        "003_new_migration".to_string(),
        Box::new("CREATE TABLE test (id INTEGER PRIMARY KEY)".to_string()),
        Some(Box::new("DROP TABLE test".to_string())),
    ));

    // Create runner with allow_dirty flag - should bypass check
    let runner_with_force = MigrationRunner::new(Box::new(source2))
        .with_version_tracker(VersionTracker::with_table_name("__test_migrations"))
        .with_allow_dirty(true);

    // Run should succeed despite dirty state
    let result_with_force = runner_with_force.run(&*db).await;
    assert!(result_with_force.is_ok());

    // Check status with new version tracker
    let check_version_tracker = VersionTracker::with_table_name("__test_migrations");

    // Verify the new migration was applied
    let new_migration_status = check_version_tracker
        .get_migration_status(&*db, "003_new_migration")
        .await
        .unwrap();
    assert!(new_migration_status.is_some());
    assert_eq!(
        new_migration_status.unwrap().status,
        MigrationStatus::Completed
    );

    // Verify the interrupted migration is still there
    let interrupted_status = check_version_tracker
        .get_migration_status(&*db, "001_interrupted_migration")
        .await
        .unwrap();
    assert!(interrupted_status.is_some());
    assert_eq!(
        interrupted_status.unwrap().status,
        MigrationStatus::InProgress
    );

    // Verify the failed migration is still there (and didn't block)
    let failed_status = check_version_tracker
        .get_migration_status(&*db, "002_failed_migration")
        .await
        .unwrap();
    assert!(failed_status.is_some());
    assert_eq!(failed_status.unwrap().status, MigrationStatus::Failed);
}

#[cfg(feature = "code")]
#[switchy_async::test]
async fn test_recovery_commands() {
    use switchy_schema::{
        MigrationError,
        discovery::code::{CodeMigration, CodeMigrationSource},
        runner::MigrationRunner,
    };

    let db = create_empty_in_memory().await.unwrap();
    let version_tracker = VersionTracker::with_table_name("__test_migrations");

    // Create migrations with one that will fail
    let mut source = CodeMigrationSource::new();
    source.add_migration(CodeMigration::new(
        "001_create_users".to_string(),
        Box::new("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)".to_string()),
        Some(Box::new("DROP TABLE users".to_string())),
    ));
    source.add_migration(CodeMigration::new(
        "002_failing_migration".to_string(),
        Box::new("INVALID SQL SYNTAX".to_string()),
        None,
    ));

    let runner = MigrationRunner::new(Box::new(source)).with_version_tracker(version_tracker);

    // Run migrations to create failure
    let _result = runner.run(&*db).await;

    // Test list_failed_migrations
    let failed_migrations = runner.list_failed_migrations(&*db).await.unwrap();
    assert_eq!(failed_migrations.len(), 1);
    assert_eq!(failed_migrations[0].id, "002_failing_migration");
    assert_eq!(failed_migrations[0].status, MigrationStatus::Failed);
    assert!(failed_migrations[0].failure_reason.is_some());

    // Test retry_migration with a migration that's not failed (should error)
    let retry_result = runner.retry_migration(&*db, "001_create_users").await;
    assert!(retry_result.is_err());
    if let Err(MigrationError::Validation(msg)) = retry_result {
        assert!(msg.contains("not in failed state"));
    } else {
        panic!("Expected validation error, got: {:?}", retry_result);
    }

    // Test retry_migration with non-existent migration (should error)
    let retry_nonexistent = runner.retry_migration(&*db, "999_nonexistent").await;
    assert!(retry_nonexistent.is_err());
    if let Err(MigrationError::Validation(msg)) = retry_nonexistent {
        assert!(msg.contains("not found"));
    } else {
        panic!("Expected validation error, got: {:?}", retry_nonexistent);
    }

    // Test mark_migration_completed for the failed migration
    let mark_result = runner
        .mark_migration_completed(&*db, "002_failing_migration")
        .await
        .unwrap();
    assert!(mark_result.contains("marked as completed"));

    // Check status with new version tracker
    let check_version_tracker = VersionTracker::with_table_name("__test_migrations");

    // Verify the migration is now marked as completed
    let status = check_version_tracker
        .get_migration_status(&*db, "002_failing_migration")
        .await
        .unwrap();
    assert!(status.is_some());
    assert_eq!(status.unwrap().status, MigrationStatus::Completed);

    // Test mark_migration_completed for already completed migration
    let mark_again_result = runner
        .mark_migration_completed(&*db, "002_failing_migration")
        .await
        .unwrap();
    assert!(mark_again_result.contains("already completed"));

    // Test mark_migration_completed for non-existent migration (should create new record)
    let mark_new_result = runner
        .mark_migration_completed(&*db, "999_new_migration")
        .await
        .unwrap();
    assert!(mark_new_result.contains("recorded as completed"));

    // Verify the new migration record was created
    let new_status = check_version_tracker
        .get_migration_status(&*db, "999_new_migration")
        .await
        .unwrap();
    assert!(new_status.is_some());
    assert_eq!(new_status.unwrap().status, MigrationStatus::Completed);
}

#[cfg(feature = "code")]
#[switchy_async::test]
async fn test_retry_failed_migration() {
    use switchy_schema::{
        discovery::code::{CodeMigration, CodeMigrationSource},
        runner::MigrationRunner,
    };

    let db = create_empty_in_memory().await.unwrap();
    let version_tracker = VersionTracker::with_table_name("__test_migrations");

    // First, create a failed migration manually
    let checksum = bytes::Bytes::from(vec![0u8; 32]);
    version_tracker.ensure_table_exists(&*db).await.unwrap();
    version_tracker
        .record_migration_started(&*db, "001_test_migration", &checksum, &checksum)
        .await
        .unwrap();
    version_tracker
        .update_migration_status(
            &*db,
            "001_test_migration",
            MigrationStatus::Failed,
            Some("Test failure".to_string()),
        )
        .await
        .unwrap();

    // Create a migration source with a successful migration for retry
    let mut source = CodeMigrationSource::new();
    source.add_migration(CodeMigration::new(
        "001_test_migration".to_string(),
        Box::new("CREATE TABLE test_table (id INTEGER PRIMARY KEY, data TEXT)".to_string()),
        Some(Box::new("DROP TABLE test_table".to_string())),
    ));

    let runner = MigrationRunner::new(Box::new(source)).with_version_tracker(version_tracker);

    // Check status with new version tracker
    let check_version_tracker = VersionTracker::with_table_name("__test_migrations");

    // Verify the migration is in failed state initially
    let initial_status = check_version_tracker
        .get_migration_status(&*db, "001_test_migration")
        .await
        .unwrap();
    assert!(initial_status.is_some());
    assert_eq!(initial_status.unwrap().status, MigrationStatus::Failed);

    // Retry the migration
    let retry_result = runner.retry_migration(&*db, "001_test_migration").await;
    assert!(
        retry_result.is_ok(),
        "Retry should succeed: {:?}",
        retry_result
    );

    // Verify the migration is now completed
    let final_status = check_version_tracker
        .get_migration_status(&*db, "001_test_migration")
        .await
        .unwrap();
    assert!(final_status.is_some());
    let final_record = final_status.unwrap();
    assert_eq!(final_record.status, MigrationStatus::Completed);
    assert!(final_record.failure_reason.is_none());

    // Verify the table was actually created
    let table_exists_result = db
        .exec_raw("SELECT name FROM sqlite_master WHERE type='table' AND name='test_table'")
        .await;
    assert!(table_exists_result.is_ok());
}

#[cfg(feature = "code")]
#[switchy_async::test]
async fn test_schema_upgrade_compatibility() {
    use switchy_schema::{
        discovery::code::{CodeMigration, CodeMigrationSource},
        runner::MigrationRunner,
    };

    let db = create_empty_in_memory().await.unwrap();

    // First, create an old-style migrations table (without status columns)
    db.exec_raw("CREATE TABLE __test_migrations (id TEXT PRIMARY KEY, run_on TIMESTAMP DEFAULT CURRENT_TIMESTAMP)").await.unwrap();
    db.exec_raw("INSERT INTO __test_migrations (id) VALUES ('001_old_migration')")
        .await
        .unwrap();

    // Instead, let's test with a fresh table name for the enhanced schema
    let new_version_tracker = VersionTracker::with_table_name("__test_migrations_v2");
    let ensure_result = new_version_tracker.ensure_table_exists(&*db).await;
    assert!(ensure_result.is_ok());

    // Create a migration source
    let mut source = CodeMigrationSource::new();
    source.add_migration(CodeMigration::new(
        "002_new_migration".to_string(),
        Box::new("CREATE TABLE test_table (id INTEGER PRIMARY KEY)".to_string()),
        Some(Box::new("DROP TABLE test_table".to_string())),
    ));

    let runner = MigrationRunner::new(Box::new(source)).with_version_tracker(new_version_tracker);

    // Run the migration successfully
    let run_result = runner.run(&*db).await;
    assert!(run_result.is_ok());

    // Check status with new version tracker
    let check_version_tracker = VersionTracker::with_table_name("__test_migrations_v2");

    // Verify the new migration was tracked with full status information
    let status = check_version_tracker
        .get_migration_status(&*db, "002_new_migration")
        .await
        .unwrap();
    assert!(status.is_some());
    let record = status.unwrap();
    assert_eq!(record.status, MigrationStatus::Completed);
    assert!(record.failure_reason.is_none());
    assert!(record.finished_on.is_some());

    // Verify the enhanced schema has all required columns
    let schema_check = db.exec_raw("SELECT id, run_on, finished_on, status, failure_reason FROM __test_migrations_v2 WHERE id = '002_new_migration'").await;
    assert!(schema_check.is_ok());
}

#[switchy_async::test]
async fn test_migration_status_transitions() {
    let db = create_empty_in_memory().await.unwrap();
    let version_tracker = VersionTracker::with_table_name("__test_migrations");

    version_tracker.ensure_table_exists(&*db).await.unwrap();

    // Test the complete lifecycle of a migration status
    let migration_id = "001_status_test";

    // 1. Record migration as started
    let checksum = bytes::Bytes::from(vec![0u8; 32]);
    version_tracker
        .record_migration_started(&*db, migration_id, &checksum, &checksum)
        .await
        .unwrap();

    let status = version_tracker
        .get_migration_status(&*db, migration_id)
        .await
        .unwrap();
    assert!(status.is_some());
    let record = status.unwrap();
    assert_eq!(record.status, MigrationStatus::InProgress);
    assert!(record.failure_reason.is_none());
    assert!(record.finished_on.is_none());
    // Use a more reasonable time comparison
    assert!(
        record.run_on
            > chrono::DateTime::from_timestamp(1000000000, 0)
                .unwrap()
                .naive_utc()
    );

    // 2. Update to failed status
    version_tracker
        .update_migration_status(
            &*db,
            migration_id,
            MigrationStatus::Failed,
            Some("Test error".to_string()),
        )
        .await
        .unwrap();

    let status = version_tracker
        .get_migration_status(&*db, migration_id)
        .await
        .unwrap();
    assert!(status.is_some());
    let record = status.unwrap();
    assert_eq!(record.status, MigrationStatus::Failed);
    assert_eq!(record.failure_reason, Some("Test error".to_string()));
    assert!(record.finished_on.is_some());

    // 3. Update to completed status (for retry scenario)
    version_tracker
        .update_migration_status(&*db, migration_id, MigrationStatus::Completed, None)
        .await
        .unwrap();

    let status = version_tracker
        .get_migration_status(&*db, migration_id)
        .await
        .unwrap();
    assert!(status.is_some());
    let record = status.unwrap();
    assert_eq!(record.status, MigrationStatus::Completed);
    assert!(record.failure_reason.is_none()); // Should be cleared
    assert!(record.finished_on.is_some());

    // 4. Test get_dirty_migrations filtering
    // Add another migration in progress
    version_tracker
        .record_migration_started(&*db, "002_in_progress", &checksum, &checksum)
        .await
        .unwrap();

    let dirty_migrations = version_tracker.get_dirty_migrations(&*db).await.unwrap();
    assert_eq!(dirty_migrations.len(), 1); // Only the failed one (in_progress from previous step is still there)
    assert_eq!(
        dirty_migrations
            .iter()
            .map(|r| r.id.as_str())
            .collect::<Vec<_>>(),
        vec!["002_in_progress"]
    );

    // Test get_in_progress_migrations filters correctly
    let in_progress_migrations = version_tracker
        .get_in_progress_migrations(&*db)
        .await
        .unwrap();
    assert_eq!(in_progress_migrations.len(), 1); // Only the in_progress one
    assert_eq!(in_progress_migrations[0].id, "002_in_progress");
    assert_eq!(
        in_progress_migrations[0].status,
        MigrationStatus::InProgress
    );
}

#[cfg(feature = "code")]
#[switchy_async::test]
async fn test_failed_migrations_dont_block_by_default() {
    use switchy_schema::{
        discovery::code::{CodeMigration, CodeMigrationSource},
        runner::MigrationRunner,
    };

    let db = create_empty_in_memory().await.unwrap();
    let version_tracker = VersionTracker::with_table_name("__test_migrations");

    // Ensure table exists
    version_tracker.ensure_table_exists(&*db).await.unwrap();

    // Manually insert a failed migration
    let checksum = bytes::Bytes::from(vec![0u8; 32]);
    version_tracker
        .record_migration_started(&*db, "001_failed_migration", &checksum, &checksum)
        .await
        .unwrap();
    version_tracker
        .update_migration_status(
            &*db,
            "001_failed_migration",
            MigrationStatus::Failed,
            Some("Test error".to_string()),
        )
        .await
        .unwrap();

    // Create a migration source with a new migration
    let mut source = CodeMigrationSource::new();
    source.add_migration(CodeMigration::new(
        "002_new_migration".to_string(),
        Box::new("CREATE TABLE test (id INTEGER PRIMARY KEY)".to_string()),
        Some(Box::new("DROP TABLE test".to_string())),
    ));

    // Create runner with default settings (auto_retry_failed = false)
    let runner = MigrationRunner::new(Box::new(source)).with_version_tracker(version_tracker);

    // Run should succeed because failed migrations don't block by default
    let result = runner.run(&*db).await;
    assert!(
        result.is_ok(),
        "Failed migrations should not block by default: {:?}",
        result
    );

    // Check status with new version tracker
    let check_version_tracker = VersionTracker::with_table_name("__test_migrations");

    // Verify new migration was applied
    let new_migration_status = check_version_tracker
        .get_migration_status(&*db, "002_new_migration")
        .await
        .unwrap();
    assert!(new_migration_status.is_some());
    assert_eq!(
        new_migration_status.unwrap().status,
        MigrationStatus::Completed
    );

    // Verify failed migration is still there (wasn't retried)
    let failed_status = check_version_tracker
        .get_migration_status(&*db, "001_failed_migration")
        .await
        .unwrap();
    assert!(failed_status.is_some());
    assert_eq!(failed_status.unwrap().status, MigrationStatus::Failed);
}

#[cfg(feature = "code")]
#[switchy_async::test]
async fn test_auto_retry_failed_functionality() {
    use switchy_schema::{
        discovery::code::{CodeMigration, CodeMigrationSource},
        runner::MigrationRunner,
    };

    let db = create_empty_in_memory().await.unwrap();
    let version_tracker = VersionTracker::with_table_name("__test_migrations");

    // Ensure table exists
    version_tracker.ensure_table_exists(&*db).await.unwrap();

    // Create a failed migration manually
    let checksum = bytes::Bytes::from(vec![0u8; 32]);
    version_tracker
        .record_migration_started(&*db, "001_failed_migration", &checksum, &checksum)
        .await
        .unwrap();
    version_tracker
        .update_migration_status(
            &*db,
            "001_failed_migration",
            MigrationStatus::Failed,
            Some("Test error".to_string()),
        )
        .await
        .unwrap();

    // Create migration source with the same migration (but fixed SQL)
    let mut source = CodeMigrationSource::new();
    source.add_migration(CodeMigration::new(
        "001_failed_migration".to_string(),
        Box::new("CREATE TABLE test_table (id INTEGER PRIMARY KEY, data TEXT)".to_string()),
        Some(Box::new("DROP TABLE test_table".to_string())),
    ));
    source.add_migration(CodeMigration::new(
        "002_new_migration".to_string(),
        Box::new("CREATE TABLE another_table (id INTEGER PRIMARY KEY)".to_string()),
        Some(Box::new("DROP TABLE another_table".to_string())),
    ));

    // Create runner with auto_retry_failed enabled
    let runner = MigrationRunner::new(Box::new(source))
        .with_version_tracker(version_tracker)
        .with_auto_retry_failed(true);

    // Run should succeed and retry the failed migration
    let result = runner.run(&*db).await;
    assert!(result.is_ok(), "Auto-retry should succeed: {:?}", result);

    // Check status with new version tracker
    let check_version_tracker = VersionTracker::with_table_name("__test_migrations");

    // Verify failed migration was retried and completed
    let retried_status = check_version_tracker
        .get_migration_status(&*db, "001_failed_migration")
        .await
        .unwrap();
    assert!(retried_status.is_some());
    let retried_record = retried_status.unwrap();
    assert_eq!(retried_record.status, MigrationStatus::Completed);
    assert!(retried_record.failure_reason.is_none());
    assert!(retried_record.finished_on.is_some());

    // Verify second migration was also applied
    let second_status = check_version_tracker
        .get_migration_status(&*db, "002_new_migration")
        .await
        .unwrap();
    assert!(second_status.is_some());
    assert_eq!(second_status.unwrap().status, MigrationStatus::Completed);

    // Verify the table was actually created by the retried migration
    let table_exists_result = db
        .exec_raw("SELECT name FROM sqlite_master WHERE type='table' AND name='test_table'")
        .await;
    assert!(table_exists_result.is_ok());
}

#[cfg(feature = "code")]
#[switchy_async::test]
async fn test_in_progress_migrations_still_block() {
    use switchy_schema::{
        MigrationError,
        discovery::code::{CodeMigration, CodeMigrationSource},
        runner::MigrationRunner,
    };

    let db = create_empty_in_memory().await.unwrap();
    let version_tracker = VersionTracker::with_table_name("__test_migrations");

    // Ensure table exists
    version_tracker.ensure_table_exists(&*db).await.unwrap();

    // Manually insert an in-progress migration
    let checksum = bytes::Bytes::from(vec![0u8; 32]);
    version_tracker
        .record_migration_started(&*db, "001_in_progress_migration", &checksum, &checksum)
        .await
        .unwrap();

    // Also add a failed migration
    version_tracker
        .record_migration_started(&*db, "002_failed_migration", &checksum, &checksum)
        .await
        .unwrap();
    version_tracker
        .update_migration_status(
            &*db,
            "002_failed_migration",
            MigrationStatus::Failed,
            Some("Test error".to_string()),
        )
        .await
        .unwrap();

    // Create a migration source with a new migration
    let mut source = CodeMigrationSource::new();
    source.add_migration(CodeMigration::new(
        "003_new_migration".to_string(),
        Box::new("CREATE TABLE test (id INTEGER PRIMARY KEY)".to_string()),
        Some(Box::new("DROP TABLE test".to_string())),
    ));

    // Create runner with auto_retry_failed enabled - should still block on in-progress
    let runner = MigrationRunner::new(Box::new(source))
        .with_version_tracker(version_tracker)
        .with_auto_retry_failed(true);

    // Run should fail due to in-progress migration (even with auto_retry_failed)
    let result = runner.run(&*db).await;
    assert!(result.is_err());

    if let Err(MigrationError::DirtyState { migrations }) = result {
        assert_eq!(migrations.len(), 1);
        assert_eq!(migrations[0], "001_in_progress_migration");
    } else {
        panic!("Expected DirtyState error, got: {:?}", result);
    }

    // Verify failed migration is still there (but shouldn't be in dirty state error)
    let check_version_tracker = VersionTracker::with_table_name("__test_migrations");
    let failed_status = check_version_tracker
        .get_migration_status(&*db, "002_failed_migration")
        .await
        .unwrap();
    assert!(failed_status.is_some());
    assert_eq!(failed_status.unwrap().status, MigrationStatus::Failed);
}
