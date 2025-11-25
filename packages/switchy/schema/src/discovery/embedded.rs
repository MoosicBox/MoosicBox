//! # Embedded Migrations
//!
//! This module provides support for embedded migrations using the `include_dir!` macro.
//! Embedded migrations are compiled directly into your binary, making deployment simpler
//! and ensuring migrations are always available at runtime.
//!
//! ## Features
//!
//! * **Compile-time inclusion**: Migrations are embedded during build
//! * **No filesystem dependencies**: Works without file system access
//! * **Deterministic ordering**: Migrations sorted alphabetically by directory name
//! * **Optional up/down SQL**: Both up.sql and down.sql are optional
//! * **Empty file handling**: Empty SQL files are treated as no-ops
//!
//! ## Directory Structure
//!
//! Embedded migrations expect a specific directory structure:
//!
//! ```text
//! migrations/
//! ├── 001_create_users/
//! │   ├── up.sql      # Optional migration SQL
//! │   └── down.sql    # Optional rollback SQL
//! ├── 002_add_posts/
//! │   └── up.sql      # down.sql is optional
//! └── 003_indexes/
//!     ├── up.sql
//!     └── down.sql
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use switchy_schema::runner::MigrationRunner;
//! use include_dir::{Dir, include_dir};
//!
//! // Include the migrations directory at compile time
//! static MIGRATIONS: Dir<'static> = include_dir!("migrations");
//!
//! # async fn example(db: &dyn switchy_database::Database) -> switchy_schema::Result<()> {
//! // Create and run embedded migrations (recommended approach)
//! let runner = MigrationRunner::new_embedded(&MIGRATIONS);
//! runner.run(db).await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Migration Content
//!
//! * **up.sql**: Forward migration SQL executed when applying the migration
//! * **down.sql**: Reverse migration SQL executed when rolling back the migration
//! * Both files are optional and can be empty
//! * Empty or missing files result in no-op operations
//!
//! ## Error Handling
//!
//! The embedded migration system handles several edge cases gracefully:
//!
//! * Missing SQL files are treated as no-ops
//! * Empty SQL files are treated as no-ops
//! * Invalid UTF-8 content is converted using lossy conversion
//! * Directory entries without SQL files are skipped

use crate::{Result, migration::Migration, migration::MigrationSource};
use std::collections::BTreeMap;
use std::sync::Arc;

use async_trait::async_trait;
use bytes::Bytes;
use include_dir::{Dir, DirEntry};
use sha2::{Digest, Sha256};

/// A single embedded migration with optional up and down SQL content
///
/// Embedded migrations are loaded from directories included at compile-time
/// using the `include_dir!` macro. Each migration consists of:
///
/// * **ID**: The directory name (used for ordering)
/// * **Up content**: Optional SQL for applying the migration
/// * **Down content**: Optional SQL for rolling back the migration
///
/// Both up and down content are optional. Missing or empty SQL files
/// are treated as no-op operations.
pub struct EmbeddedMigration {
    id: String,
    up_content: Option<Bytes>,
    down_content: Option<Bytes>,
}

impl EmbeddedMigration {
    /// Create a new embedded migration with the given ID and content
    #[must_use]
    pub const fn new(id: String, up_content: Option<Bytes>, down_content: Option<Bytes>) -> Self {
        Self {
            id,
            up_content,
            down_content,
        }
    }
}

#[async_trait]
impl Migration<'static> for EmbeddedMigration {
    fn id(&self) -> &str {
        &self.id
    }

    async fn up(&self, db: &dyn switchy_database::Database) -> Result<()> {
        match &self.up_content {
            Some(sql) if !sql.is_empty() => {
                let sql_str = String::from_utf8_lossy(sql);
                db.exec_raw(&sql_str).await?;
            }
            _ => {
                // Empty or missing up.sql is a no-op
            }
        }
        Ok(())
    }

    async fn down(&self, db: &dyn switchy_database::Database) -> Result<()> {
        match &self.down_content {
            Some(sql) if !sql.is_empty() => {
                let sql_str = String::from_utf8_lossy(sql);
                db.exec_raw(&sql_str).await?;
            }
            _ => {
                // Empty or missing down.sql is a no-op
            }
        }
        Ok(())
    }

    async fn up_checksum(&self) -> Result<bytes::Bytes> {
        let mut hasher = Sha256::new();
        match &self.up_content {
            Some(content) => hasher.update(content),
            None => hasher.update(b""), // Hash empty bytes for None
        }
        Ok(bytes::Bytes::from(hasher.finalize().to_vec()))
    }

    async fn down_checksum(&self) -> Result<bytes::Bytes> {
        let mut hasher = Sha256::new();
        match &self.down_content {
            Some(content) => hasher.update(content),
            None => hasher.update(b""), // Hash empty bytes for None
        }
        Ok(bytes::Bytes::from(hasher.finalize().to_vec()))
    }
}

/// Migration source for embedded migrations using `include_dir`
///
/// This source loads migrations from a directory structure that was embedded
/// at compile-time using the `include_dir!` macro. It provides the most reliable
/// way to distribute migrations with your application binary.
///
/// ## Features
///
/// * **Deterministic loading**: Migrations are sorted alphabetically by directory name
/// * **No runtime dependencies**: All content is embedded at compile time
/// * **Graceful handling**: Missing or empty SQL files are treated as no-ops
/// * **Memory efficient**: Uses `Bytes` for zero-copy string operations
///
/// ## Example
///
/// ```rust,ignore
/// use switchy_schema::{
///     runner::MigrationRunner,
///     discovery::embedded::EmbeddedMigrationSource
/// };
/// use include_dir::{Dir, include_dir};
///
/// static MIGRATIONS: Dir<'static> = include_dir!("migrations");
///
/// // Create source directly (advanced usage)
/// let source = EmbeddedMigrationSource::new(&MIGRATIONS);
///
/// // Or use the convenience constructor (recommended)
/// let runner = MigrationRunner::new_embedded(&MIGRATIONS);
/// ```
///
/// The migration source will automatically scan the embedded directory structure
/// and create migration objects for each subdirectory containing SQL files.
pub struct EmbeddedMigrationSource {
    migrations_dir: &'static Dir<'static>,
}

impl EmbeddedMigrationSource {
    /// Create a new embedded migration source from an included directory
    #[must_use]
    pub const fn new(migrations_dir: &'static Dir<'static>) -> Self {
        Self { migrations_dir }
    }

    /// Extract migrations from the embedded directory structure
    fn extract_migrations(&self) -> BTreeMap<String, EmbeddedMigration> {
        let mut migrations = BTreeMap::new();

        for entry in self.migrations_dir.entries() {
            if let DirEntry::Dir(migration_dir) = entry {
                let migration_id = migration_dir
                    .path()
                    .file_name()
                    .and_then(|name| name.to_str())
                    .map(String::from);

                if let Some(id) = migration_id {
                    // Look for up.sql and down.sql files within this migration directory
                    let mut up_content = None;
                    let mut down_content = None;

                    for file in migration_dir.files() {
                        if let Some(file_name) = file.path().file_name().and_then(|n| n.to_str()) {
                            match file_name {
                                "up.sql" => up_content = Some(Bytes::from(file.contents())),
                                "down.sql" => down_content = Some(Bytes::from(file.contents())),
                                _ => {} // Ignore other files
                            }
                        }
                    }

                    let migration = EmbeddedMigration::new(id.clone(), up_content, down_content);
                    migrations.insert(id, migration);
                }
            }
        }

        migrations
    }
}

#[async_trait]
impl MigrationSource<'static> for EmbeddedMigrationSource {
    async fn migrations(&self) -> Result<Vec<Arc<dyn Migration<'static> + 'static>>> {
        let migration_map = self.extract_migrations();

        let migrations: Vec<Arc<dyn Migration<'static> + 'static>> = migration_map
            .into_values()
            .map(|m| Arc::new(m) as Arc<dyn Migration<'static> + 'static>)
            .collect();

        Ok(migrations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use include_dir::include_dir;

    static TEST_MIGRATIONS: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/test_migrations");

    #[switchy_async::test]
    async fn test_extract_migrations() {
        let source = EmbeddedMigrationSource::new(&TEST_MIGRATIONS);
        let migration_map = source.extract_migrations();

        // Should find 4 migrations
        assert_eq!(migration_map.len(), 4);

        // Check migration IDs are extracted correctly
        assert!(migration_map.contains_key("2023-10-13-195407_create_users"));
        assert!(migration_map.contains_key("2023-10-14-120000_add_timestamps"));
        assert!(migration_map.contains_key("2023-10-15-090000_empty_migration"));
        assert!(migration_map.contains_key("2023-10-16-150000_up_only"));
    }

    #[switchy_async::test]
    async fn test_migration_content_loading() {
        let source = EmbeddedMigrationSource::new(&TEST_MIGRATIONS);
        let migration_map = source.extract_migrations();

        // Test migration with both up and down
        let create_users = &migration_map["2023-10-13-195407_create_users"];
        assert!(create_users.up_content.is_some());
        assert!(create_users.down_content.is_some());

        let up_sql = String::from_utf8_lossy(create_users.up_content.as_ref().unwrap());
        assert!(up_sql.contains("CREATE TABLE users"));

        let down_sql = String::from_utf8_lossy(create_users.down_content.as_ref().unwrap());
        assert!(down_sql.contains("DROP TABLE users"));

        // Test migration with only up.sql
        let up_only = &migration_map["2023-10-16-150000_up_only"];
        assert!(up_only.up_content.is_some());
        assert!(up_only.down_content.is_none());

        // Test empty migration
        let empty = &migration_map["2023-10-15-090000_empty_migration"];
        assert!(empty.up_content.is_some());
        assert!(empty.up_content.as_ref().unwrap().is_empty());
    }

    #[switchy_async::test]
    async fn test_migrations_source_trait() {
        let source = EmbeddedMigrationSource::new(&TEST_MIGRATIONS);
        let migrations = source.migrations().await.unwrap();

        // Should return 4 migrations
        assert_eq!(migrations.len(), 4);

        // Migrations should be sorted by ID (BTreeMap guarantees this)
        let ids: Vec<&str> = migrations.iter().map(|m| m.id()).collect();
        assert_eq!(ids[0], "2023-10-13-195407_create_users");
        assert_eq!(ids[1], "2023-10-14-120000_add_timestamps");
        assert_eq!(ids[2], "2023-10-15-090000_empty_migration");
        assert_eq!(ids[3], "2023-10-16-150000_up_only");
    }

    #[test]
    fn test_embedded_migration_creation() {
        let up_content = Some(Bytes::from("CREATE TABLE test;"));
        let down_content = Some(Bytes::from("DROP TABLE test;"));

        let migration = EmbeddedMigration::new(
            "test_migration".to_string(),
            up_content.clone(),
            down_content.clone(),
        );

        assert_eq!(migration.id(), "test_migration");
        assert_eq!(migration.up_content, up_content);
        assert_eq!(migration.down_content, down_content);
    }

    #[switchy_async::test]
    async fn test_embedded_migration_checksums() {
        let migration = EmbeddedMigration::new(
            "test_migration".to_string(),
            Some(Bytes::from("CREATE TABLE test (id INTEGER PRIMARY KEY)")),
            Some(Bytes::from("DROP TABLE test")),
        );

        let up_checksum = migration.up_checksum().await.unwrap();
        let down_checksum = migration.down_checksum().await.unwrap();

        // Verify checksums are 32 bytes (SHA256)
        assert_eq!(up_checksum.len(), 32);
        assert_eq!(down_checksum.len(), 32);

        // Verify they're not all zeros
        assert_ne!(up_checksum, bytes::Bytes::from(vec![0u8; 32]));
        assert_ne!(down_checksum, bytes::Bytes::from(vec![0u8; 32]));

        // Verify they're different (different content should produce different hashes)
        assert_ne!(up_checksum, down_checksum);

        // Test with None content
        let migration_none = EmbeddedMigration::new("test_migration_none".to_string(), None, None);

        let up_checksum_none = migration_none.up_checksum().await.unwrap();
        let down_checksum_none = migration_none.down_checksum().await.unwrap();

        // Should be equal since both hash empty content
        assert_eq!(up_checksum_none, down_checksum_none);
        assert_eq!(up_checksum_none.len(), 32);
    }
}
