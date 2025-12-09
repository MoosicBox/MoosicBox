//! # Code-Based Migrations
//!
//! This module provides programmatic migration support using the [`Executable`] trait,
//! allowing migrations to be defined using both raw SQL and type-safe query builders.
//!
//! ## Static Example (Owned Data)
//!
//! Most code migrations own their data and use the `'static` lifetime:
//!
//! ```rust
//! use switchy_schema::discovery::code::{CodeMigration, CodeMigrationSource};
//! use switchy_database::Executable;
//!
//! // Create a migration with owned SQL strings
//! let migration = CodeMigration::new(
//!     "001_create_users".to_string(),
//!     Box::new("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT NOT NULL)".to_string()),
//!     Some(Box::new("DROP TABLE users".to_string())),
//! );
//!
//! // Add to a migration source
//! let mut source = CodeMigrationSource::new();
//! source.add_migration(migration);
//! ```
//!
//! ## Query Builder Example
//!
//! Using type-safe query builders from the database package:
//!
//! ```rust
//! use switchy_schema::discovery::code::CodeMigration;
//! use switchy_database::schema::{create_table, Column, DataType};
//!
//! // Create a migration using query builders
//! let create_table_stmt = create_table("users")
//!     .if_not_exists(true)
//!     .column(Column {
//!         name: "id".to_string(),
//!         nullable: false,
//!         auto_increment: true,
//!         data_type: DataType::Int,
//!         default: None,
//!     })
//!     .column(Column {
//!         name: "name".to_string(),
//!         nullable: false,
//!         auto_increment: false,
//!         data_type: DataType::Text,
//!         default: None,
//!     })
//!     .primary_key("id");
//!
//! let migration = CodeMigration::new(
//!     "001_create_users_typed".to_string(),
//!     Box::new(create_table_stmt),
//!     None,
//! );
//! ```
//!
//! ## Non-Static Example (Borrowed Query Builder)
//!
//! Advanced usage with borrowed data and explicit lifetimes:
//!
//! ```rust
//! use switchy_schema::discovery::code::CodeMigration;
//! use switchy_database::schema::{create_table, Column, DataType};
//!
//! fn create_table_migration<'a>(table_name: &'a str, id_column: &'a str) -> CodeMigration<'a> {
//!     let stmt = create_table(table_name)
//!         .column(Column {
//!             name: id_column.to_string(),
//!             nullable: false,
//!             auto_increment: true,
//!             data_type: DataType::Int,
//!             default: None,
//!         })
//!         .primary_key(id_column);
//!
//!     CodeMigration::new(
//!         format!("create_{}", table_name),
//!         Box::new(stmt),
//!         None,
//!     )
//! }
//!
//! // Usage with borrowed data
//! let migration = create_table_migration("products", "product_id");
//! ```
//!
//! ## Migration Source Usage
//!
//! Collecting multiple code migrations:
//!
//! ```rust
//! use switchy_schema::discovery::code::{CodeMigration, CodeMigrationSource};
//! use switchy_schema::migration::MigrationSource;
//!
//! # async fn example() -> switchy_schema::Result<()> {
//! let mut source = CodeMigrationSource::new();
//!
//! // Add multiple migrations
//! source.add_migration(CodeMigration::new(
//!     "001_users".to_string(),
//!     Box::new("CREATE TABLE users (id INTEGER PRIMARY KEY)".to_string()),
//!     Some(Box::new("DROP TABLE users".to_string())),
//! ));
//!
//! source.add_migration(CodeMigration::new(
//!     "002_posts".to_string(),
//!     Box::new("CREATE TABLE posts (id INTEGER PRIMARY KEY, user_id INTEGER)".to_string()),
//!     Some(Box::new("DROP TABLE posts".to_string())),
//! ));
//!
//! // Get all migrations (sorted by ID)
//! let migrations = source.migrations().await?;
//! # Ok(())
//! # }
//! ```
//!
//! ## Auto-Reversible Migrations (requires `auto-reverse` feature)
//!
//! For operations that support automatic reversal, use `ReversibleCodeMigration`:
//!
//! ```rust
//! # #[cfg(feature = "auto-reverse")]
//! use switchy_schema::discovery::code::ReversibleCodeMigration;
//! use switchy_database::schema::create_table;
//! # use switchy_database::schema::{Column, DataType};
//!
//! # #[cfg(feature = "auto-reverse")]
//! let create = create_table("users")
//!     .column(Column {
//!         name: "id".to_string(),
//!         data_type: DataType::Int,
//!         nullable: false,
//!         auto_increment: true,
//!         default: None,
//!     });
//!
//! # #[cfg(feature = "auto-reverse")]
//! // Automatically generates both UP and DOWN migrations
//! let migration = ReversibleCodeMigration::new(
//!     "001_create_users",
//!     create,
//! );
//! ```
//!
//! ## Type Safety
//!
//! The type system prevents using non-reversible operations:
//!
//! ```compile_fail
//! # #[cfg(feature = "auto-reverse")]
//! use switchy_schema::discovery::code::ReversibleCodeMigration;
//! use switchy_database::schema::drop_table;
//!
//! let drop = drop_table("users");
//!
//! // This will NOT compile - DropTableStatement doesn't implement AutoReversible
//! let migration = ReversibleCodeMigration::new("bad", drop);
//! ```

use std::sync::Arc;

use async_trait::async_trait;

use crate::{
    Result, checksum_database::ChecksumDatabase, migration::Migration, migration::MigrationSource,
};
use switchy_database::Executable;

/// Migration implementation for code-based migrations using `Executable`
///
/// This struct represents a single migration defined in Rust code rather than SQL files.
/// It uses the [`Executable`] trait from `switchy_database` to support both raw SQL strings
/// and type-safe query builders.
///
/// # Type Parameters
///
/// * `'a` - Lifetime of the executable content. Use `'static` for owned data (most common)
///   or a shorter lifetime when borrowing data.
///
/// # Examples
///
/// ## Basic SQL Migration
///
/// ```rust
/// use switchy_schema::discovery::code::CodeMigration;
///
/// let migration = CodeMigration::new(
///     "001_create_users".to_string(),
///     Box::new("CREATE TABLE users (id INTEGER PRIMARY KEY)".to_string()),
///     Some(Box::new("DROP TABLE users".to_string())),
/// );
/// ```
///
/// ## Migration Without Rollback
///
/// ```rust
/// use switchy_schema::discovery::code::CodeMigration;
///
/// let migration = CodeMigration::new(
///     "002_data_migration".to_string(),
///     Box::new("INSERT INTO users (id) SELECT id FROM legacy_users".to_string()),
///     None, // No rollback possible for data migration
/// );
/// ```
pub struct CodeMigration<'a> {
    id: String,
    up_sql: Box<dyn Executable + 'a>,
    down_sql: Option<Box<dyn Executable + 'a>>,
}

impl<'a> CodeMigration<'a> {
    /// Create a new code migration
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier for this migration
    /// * `up_sql` - Forward migration SQL or query builder
    /// * `down_sql` - Optional rollback SQL or query builder
    #[must_use]
    pub fn new(
        id: String,
        up_sql: Box<dyn Executable + 'a>,
        down_sql: Option<Box<dyn Executable + 'a>>,
    ) -> Self {
        Self {
            id,
            up_sql,
            down_sql,
        }
    }
}

#[async_trait]
impl<'a> Migration<'a> for CodeMigration<'a> {
    fn id(&self) -> &str {
        &self.id
    }

    async fn up(&self, db: &dyn switchy_database::Database) -> Result<()> {
        self.up_sql.execute(db).await?;
        Ok(())
    }

    async fn down(&self, db: &dyn switchy_database::Database) -> Result<()> {
        if let Some(down_sql) = &self.down_sql {
            down_sql.execute(db).await?;
        }
        Ok(())
    }

    async fn up_checksum(&self) -> Result<bytes::Bytes> {
        // For code migrations, we use ChecksumDatabase to capture the actual SQL operations
        // that are executed when the Executable runs, giving us real content-based checksums
        let checksum_db = ChecksumDatabase::new();
        self.up_sql.execute(&checksum_db).await.map_err(|e| {
            crate::MigrationError::Execution(format!(
                "Failed to execute migration for checksum: {e}"
            ))
        })?;
        Ok(checksum_db.finalize().await)
    }

    async fn down_checksum(&self) -> Result<bytes::Bytes> {
        if let Some(down_sql) = &self.down_sql {
            // For code migrations, we use ChecksumDatabase to capture the actual SQL operations
            let checksum_db = ChecksumDatabase::new();
            down_sql.execute(&checksum_db).await.map_err(|e| {
                crate::MigrationError::Execution(format!(
                    "Failed to execute down migration for checksum: {e}"
                ))
            })?;
            Ok(checksum_db.finalize().await)
        } else {
            // Hash empty bytes for None - consistent with other migration types
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(b"");
            Ok(bytes::Bytes::from(hasher.finalize().to_vec()))
        }
    }
}

/// A code migration that automatically generates its Down migration from a reversible Up operation
#[cfg(feature = "auto-reverse")]
pub struct ReversibleCodeMigration<'a, T: switchy_database::schema::AutoReversible + 'a> {
    id: String,
    up_operation: T,
    _phantom: std::marker::PhantomData<&'a ()>,
}

#[cfg(feature = "auto-reverse")]
impl<'a, T: switchy_database::schema::AutoReversible + 'a> ReversibleCodeMigration<'a, T> {
    /// Create a new reversible code migration
    ///
    /// The down migration will be automatically generated by reversing the up operation.
    /// This only works with operations that implement `AutoReversible`.
    ///
    /// # Arguments
    ///
    /// * `id` - Unique identifier for this migration
    /// * `up_operation` - Forward operation that implements `AutoReversible`
    #[must_use]
    pub fn new(id: impl Into<String>, up_operation: T) -> Self {
        Self {
            id: id.into(),
            up_operation,
            _phantom: std::marker::PhantomData,
        }
    }
}

#[cfg(feature = "auto-reverse")]
impl<'a, T> From<ReversibleCodeMigration<'a, T>> for CodeMigration<'a>
where
    T: switchy_database::schema::AutoReversible + 'a,
    T::Reversed: 'a,
{
    fn from(rev: ReversibleCodeMigration<'a, T>) -> Self {
        let id = rev.id.clone();
        let down = rev.up_operation.reverse();
        CodeMigration::new(id, Box::new(rev.up_operation), Some(Box::new(down)))
    }
}

/// Migration source for code-based migrations with registry
///
/// This struct serves as a container for programmatically-defined migrations.
/// Migrations are added using [`add_migration`](Self::add_migration) and retrieved
/// via the [`MigrationSource`] trait implementation.
///
/// # Features
///
/// * **Programmatic definition**: Define migrations in Rust code
/// * **Automatic ordering**: Migrations are sorted by ID when retrieved
/// * **Type-safe builders**: Can use query builders instead of raw SQL
/// * **Flexible lifetime**: Supports both owned and borrowed data
///
/// # Examples
///
/// ```rust
/// use switchy_schema::discovery::code::{CodeMigration, CodeMigrationSource};
///
/// let mut source = CodeMigrationSource::new();
///
/// source.add_migration(CodeMigration::new(
///     "001_create_users".to_string(),
///     Box::new("CREATE TABLE users (id INTEGER PRIMARY KEY)".to_string()),
///     Some(Box::new("DROP TABLE users".to_string())),
/// ));
///
/// source.add_migration(CodeMigration::new(
///     "002_add_email".to_string(),
///     Box::new("ALTER TABLE users ADD COLUMN email TEXT".to_string()),
///     None,
/// ));
/// ```
pub struct CodeMigrationSource<'a> {
    migrations: Vec<Arc<dyn Migration<'a> + 'a>>,
}

impl<'a> CodeMigrationSource<'a> {
    /// Create a new empty code migration source
    #[must_use]
    pub const fn new() -> Self {
        Self {
            migrations: Vec::new(),
        }
    }

    /// Add a migration to this source
    ///
    /// Migrations will be sorted by ID when retrieved via `migrations()`.
    pub fn add_migration(&mut self, migration: CodeMigration<'a>) {
        self.migrations.push(Arc::new(migration));
    }
}

impl Default for CodeMigrationSource<'_> {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<'a> MigrationSource<'a> for CodeMigrationSource<'a> {
    async fn migrations(&self) -> Result<Vec<Arc<dyn Migration<'a> + 'a>>> {
        // Sort migrations by ID for deterministic ordering
        let mut sorted_migrations = self.migrations.clone();
        sorted_migrations.sort_by(|a, b| a.id().cmp(b.id()));
        Ok(sorted_migrations)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[switchy_async::test]
    async fn test_code_migration_creation() {
        let up_sql = Box::new("CREATE TABLE test (id INTEGER PRIMARY KEY);".to_string());
        let down_sql = Some(Box::new("DROP TABLE test;".to_string()) as Box<dyn Executable>);

        let migration = CodeMigration::new("001_create_test".to_string(), up_sql, down_sql);

        assert_eq!(migration.id(), "001_create_test");
    }

    #[switchy_async::test]
    async fn test_code_migration_source() {
        let mut source = CodeMigrationSource::new();

        let migration = CodeMigration::new(
            "001_test".to_string(),
            Box::new("SELECT 1;".to_string()),
            None,
        );

        source.add_migration(migration);

        // Test that migrations() returns the added migration
        let migrations = source.migrations().await.unwrap();
        assert_eq!(migrations.len(), 1);
        assert_eq!(migrations[0].id(), "001_test");
    }

    #[switchy_async::test]
    async fn test_code_migration_with_query_builder() {
        // Test using the database query builders
        use switchy_database::schema::{Column, DataType, create_table};

        let create_table_stmt = create_table("users")
            .if_not_exists(true)
            .column(Column {
                name: "id".to_string(),
                nullable: false,
                auto_increment: true,
                data_type: DataType::Int,
                default: None,
            })
            .primary_key("id");

        let migration = CodeMigration::new(
            "001_create_users".to_string(),
            Box::new(create_table_stmt),
            None,
        );

        assert_eq!(migration.id(), "001_create_users");
    }

    #[switchy_async::test]
    async fn test_code_migration_source_ordering() {
        let mut source = CodeMigrationSource::new();

        // Add migrations in non-alphabetical order
        source.add_migration(CodeMigration::new(
            "003_third".to_string(),
            Box::new("SELECT 3;".to_string()),
            None,
        ));

        source.add_migration(CodeMigration::new(
            "001_first".to_string(),
            Box::new("SELECT 1;".to_string()),
            None,
        ));

        source.add_migration(CodeMigration::new(
            "002_second".to_string(),
            Box::new("SELECT 2;".to_string()),
            None,
        ));

        // Test that migrations are returned in sorted order
        let migrations = source.migrations().await.unwrap();
        assert_eq!(migrations.len(), 3);
        assert_eq!(migrations[0].id(), "001_first");
        assert_eq!(migrations[1].id(), "002_second");
        assert_eq!(migrations[2].id(), "003_third");
    }

    #[switchy_async::test]
    async fn test_code_migration_source_list() {
        let mut source = CodeMigrationSource::new();

        // Add migrations with descriptions
        source.add_migration(CodeMigration::new(
            "001_users".to_string(),
            Box::new("CREATE TABLE users (id INTEGER);".to_string()),
            None,
        ));

        source.add_migration(CodeMigration::new(
            "002_posts".to_string(),
            Box::new("CREATE TABLE posts (id INTEGER);".to_string()),
            None,
        ));

        // Test list() method
        let list = source.list().await.unwrap();
        assert_eq!(list.len(), 2);

        // Should be sorted by ID
        assert_eq!(list[0].id, "001_users");
        assert_eq!(list[1].id, "002_posts");

        // Applied status should default to false
        assert!(!list[0].applied);
        assert!(!list[1].applied);

        // Description should be None for code migrations (no description implemented)
        assert_eq!(list[0].description, None);
        assert_eq!(list[1].description, None);
    }

    #[switchy_async::test]
    async fn test_code_migration_checksums() {
        use sha2::{Digest, Sha256};

        // Test with String migration (should use ChecksumDatabase)
        let migration = CodeMigration::new(
            "test_migration".to_string(),
            Box::new("CREATE TABLE test (id INTEGER PRIMARY KEY)".to_string()),
            Some(Box::new("DROP TABLE test".to_string())),
        );

        let up_checksum = migration.up_checksum().await.unwrap();
        let down_checksum = migration.down_checksum().await.unwrap();

        // Verify checksums are 32 bytes (SHA256)
        assert_eq!(up_checksum.len(), 32);
        assert_eq!(down_checksum.len(), 32);

        // Verify they're not all zeros (should be real checksums from ChecksumDatabase)
        assert_ne!(up_checksum, bytes::Bytes::from(vec![0u8; 32]));
        assert_ne!(down_checksum, bytes::Bytes::from(vec![0u8; 32]));

        // Verify they're different (different SQL should produce different hashes)
        assert_ne!(up_checksum, down_checksum);

        // Test with None down migration
        let migration_no_down = CodeMigration::new(
            "test_migration_no_down".to_string(),
            Box::new("CREATE TABLE test (id INTEGER PRIMARY KEY)".to_string()),
            None,
        );

        let up_checksum_no_down = migration_no_down.up_checksum().await.unwrap();
        let down_checksum_no_down = migration_no_down.down_checksum().await.unwrap();

        // Up checksum should still be real
        assert_ne!(up_checksum_no_down, bytes::Bytes::from(vec![0u8; 32]));

        // Down checksum should be hash of empty bytes (consistent with other migration types)
        let mut hasher = Sha256::new();
        hasher.update(b"");
        let expected_empty_hash = bytes::Bytes::from(hasher.finalize().to_vec());
        assert_eq!(down_checksum_no_down, expected_empty_hash);
    }

    #[switchy_async::test]
    async fn test_code_operation_changes_produce_different_checksums() {
        // Migration with INSERT operation
        let migration1 = CodeMigration::new(
            "test1".to_string(),
            Box::new("INSERT INTO users (name) VALUES ('Alice')".to_string()),
            None,
        );

        // Migration with different INSERT operation
        let migration2 = CodeMigration::new(
            "test2".to_string(),
            Box::new("INSERT INTO users (name) VALUES ('Bob')".to_string()),
            None,
        );

        // Migration with different operation type
        let migration3 = CodeMigration::new(
            "test3".to_string(),
            Box::new("CREATE TABLE users (id INTEGER, name TEXT)".to_string()),
            None,
        );

        let checksum1 = migration1.up_checksum().await.unwrap();
        let checksum2 = migration2.up_checksum().await.unwrap();
        let checksum3 = migration3.up_checksum().await.unwrap();

        // All checksums should be valid 32-byte hashes
        assert_eq!(checksum1.len(), 32);
        assert_eq!(checksum2.len(), 32);
        assert_eq!(checksum3.len(), 32);

        // All checksums should be different (different operations produce different hashes)
        assert_ne!(
            checksum1, checksum2,
            "Different SQL strings should produce different checksums"
        );
        assert_ne!(
            checksum1, checksum3,
            "Different operation types should produce different checksums"
        );
        assert_ne!(
            checksum2, checksum3,
            "Different operation types should produce different checksums"
        );

        // Test with same operation - should produce identical checksums
        let migration1_duplicate = CodeMigration::new(
            "test1_dup".to_string(),
            Box::new("INSERT INTO users (name) VALUES ('Alice')".to_string()),
            None,
        );

        let checksum1_duplicate = migration1_duplicate.up_checksum().await.unwrap();
        assert_eq!(
            checksum1, checksum1_duplicate,
            "Same operation should produce identical checksums"
        );
    }

    #[cfg(feature = "auto-reverse")]
    mod auto_reverse_tests {
        use super::*;
        use switchy_database::schema::{Column, DataType, create_table};

        #[test]
        fn test_reversible_migration_conversion() {
            let create = create_table("posts").column(Column {
                name: "id".to_string(),
                data_type: DataType::Int,
                nullable: false,
                auto_increment: true,
                default: None,
            });

            let reversible = ReversibleCodeMigration::new("001_create_posts", create);
            let migration: CodeMigration = reversible.into();

            assert_eq!(migration.id(), "001_create_posts");
            // Down migration should be Some(DropTableStatement)
        }

        #[test]
        fn test_type_safety_non_reversible() {
            // This should NOT compile (uncomment to verify):
            // let drop = drop_table("users");
            // let reversible = ReversibleCodeMigration::new("bad", drop);
            // Compile error: AutoReversible not implemented for DropTableStatement
        }
    }
}
