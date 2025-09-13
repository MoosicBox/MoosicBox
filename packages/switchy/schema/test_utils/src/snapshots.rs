//! Snapshot testing utilities for migration verification using JSON format
//!
//! This module provides utilities for capturing and comparing database schemas
//! and migration results using insta's snapshot testing with JSON serialization.
//! JSON is used for its wide compatibility, active maintenance, and human readability
//! when pretty-printed.

use crate::TestError;
use std::path::PathBuf;
use switchy_database::DatabaseError;
use switchy_schema::MigrationError;

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

    /// Run the snapshot test
    ///
    /// # Errors
    ///
    /// * Returns `SnapshotError` if test execution fails
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
