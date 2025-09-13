//! Snapshot testing utilities for migration verification using JSON format
//!
//! This module provides utilities for capturing and comparing database schemas
//! and migration results using insta's snapshot testing with JSON serialization.
//! JSON is used for its wide compatibility, active maintenance, and human readability
//! when pretty-printed.

use crate::TestError;
use std::path::PathBuf;
use switchy_database::{Database, DatabaseError};
use switchy_schema::MigrationError;

#[cfg(feature = "snapshots")]
use serde::{Deserialize, Serialize};

/// Error type for snapshot testing operations
#[derive(Debug, thiserror::Error)]
pub enum SnapshotError {
    /// Database operation failed
    #[error("Database error: {0}")]
    Database(#[from] DatabaseError),

    /// Migration operation failed
    #[error("Migration error: {0}")]
    Migration(#[from] MigrationError),

    /// IO operation failed
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Snapshot validation failed
    #[error("Snapshot validation failed: {0}")]
    Validation(String),

    /// Test utilities error
    #[error("Test error: {0}")]
    Test(#[from] TestError),

    /// JSON serialization/deserialization error
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

/// Result type for snapshot operations
pub type Result<T> = std::result::Result<T, SnapshotError>;

/// Snapshot structure for migration state verification
/// Note: This structure will grow in later phases.
/// Breaking changes to snapshot structure are acceptable during development.
/// Regenerate snapshots with `cargo insta review` when structure changes.
#[cfg(feature = "snapshots")]
#[derive(Debug, Serialize, Deserialize)]
struct MigrationSnapshot {
    test_name: String,
    migration_sequence: Vec<String>,
}

/// Placeholder for snapshot testing functionality
/// Full implementation will come in Phase 11.4.2+
pub struct SnapshotTester {
    // Implementation to follow in subsequent phases
}

/// Migration snapshot test struct for verifying database schema changes
pub struct MigrationSnapshotTest {
    test_name: String,
    migrations_dir: PathBuf,
    assert_schema: bool,
    assert_sequence: bool,
}

impl MigrationSnapshotTest {
    /// Create a new migration snapshot test
    #[must_use]
    pub fn new(test_name: &str) -> Self {
        Self {
            test_name: test_name.to_string(),
            // Points to dedicated snapshot test migrations
            migrations_dir: PathBuf::from("./test-resources/snapshot-migrations/minimal"),
            assert_schema: true,
            assert_sequence: true,
        }
    }

    #[must_use]
    pub fn migrations_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.migrations_dir = path.into();
        self
    }

    #[must_use]
    pub const fn assert_schema(mut self, enabled: bool) -> Self {
        self.assert_schema = enabled;
        self
    }

    #[must_use]
    pub const fn assert_sequence(mut self, enabled: bool) -> Self {
        self.assert_sequence = enabled;
        self
    }

    /// Optionally integrate with existing test builder for complex scenarios
    #[must_use]
    pub fn with_test_builder(self, _builder: crate::MigrationTestBuilder<'_>) -> Self {
        // Will be implemented in later phases
        self
    }

    /// Create a test database using existing utilities
    ///
    /// # Errors
    ///
    /// * Returns `SnapshotError` if database creation fails
    #[cfg(feature = "snapshots")]
    async fn create_test_database(&self) -> Result<Box<dyn Database>> {
        // Use existing test_utils helper (SQLite in-memory)
        // This database persists for the entire test lifecycle
        let db = crate::create_empty_in_memory()
            .await
            .map_err(TestError::from)?;
        Ok(db)
    }

    /// Run the snapshot test
    ///
    /// # Errors
    ///
    /// * Returns `SnapshotError` if test execution fails
    #[cfg(feature = "snapshots")]
    pub async fn run(self) -> Result<()> {
        // Create SQLite database - persists for entire test
        let db = self.create_test_database().await?;

        // Verify database works
        db.exec_raw("SELECT 1").await?;

        // Create snapshot with database info
        let snapshot = MigrationSnapshot {
            test_name: self.test_name.clone(),
            migration_sequence: vec![], // No migrations yet
        };

        insta::assert_json_snapshot!(self.test_name, snapshot);
        Ok(())
    }

    /// Run the snapshot test (non-snapshots version)
    ///
    /// # Errors
    ///
    /// * Returns `SnapshotError` if test execution fails
    #[cfg(not(feature = "snapshots"))]
    pub fn run(self) -> Result<()> {
        // Still minimal but uses configuration
        println!("Test: {}", self.test_name);
        println!("Migrations: {}", self.migrations_dir.display());
        println!(
            "Schema: {}, Sequence: {}",
            self.assert_schema, self.assert_sequence
        );

        Ok(())
    }
}
