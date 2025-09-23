#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

//! Mutation handling for advanced migration testing
//!
//! This module provides the `MutationProvider` trait and implementations for testing
//! migrations with data changes between migration steps. This allows verification
//! that migrations handle intermediate state changes correctly.

use std::{collections::BTreeMap, sync::Arc};

use async_trait::async_trait;
use switchy_database::Executable;

/// Trait for providing mutations to be executed between specific migrations
///
/// Mutations are data changes that occur between migration steps, allowing tests
/// to verify that migrations handle intermediate state changes correctly.
///
/// # Examples
///
/// ```rust
/// use std::{collections::BTreeMap, sync::Arc};
/// use switchy_schema_test_utils::mutations::MutationProvider;
/// use switchy_database::Executable;
///
/// # async fn example() {
/// let mut mutations = BTreeMap::new();
/// mutations.insert(
///     "001_create_users".to_string(),
///     Arc::new("INSERT INTO users (name) VALUES ('test')".to_string()) as Arc<dyn Executable>
/// );
///
/// // This mutation will be executed after migration "001_create_users"
/// let mutation = mutations.get_mutation("001_create_users").await;
/// # }
/// ```
#[async_trait]
pub trait MutationProvider: Send + Sync {
    /// Get a mutation to execute after the specified migration ID
    ///
    /// # Arguments
    ///
    /// * `after_migration_id` - The migration ID after which to execute the mutation
    ///
    /// # Returns
    ///
    /// * `Some(mutation)` if a mutation should be executed after this migration
    /// * `None` if no mutation should be executed
    ///
    /// # Errors
    ///
    /// This method does not return errors directly, but the returned `Executable`
    /// may fail when executed against the database.
    async fn get_mutation(&self, after_migration_id: &str) -> Option<Arc<dyn Executable>>;
}

/// Implementation of `MutationProvider` for `BTreeMap<String, Arc<dyn Executable>>`
///
/// Maps migration IDs to mutations using deterministic ordering.
///
/// # Examples
///
/// ```rust
/// use std::{collections::BTreeMap, sync::Arc};
/// use switchy_schema_test_utils::mutations::MutationProvider;
/// use switchy_database::Executable;
///
/// # async fn example() {
/// let mut mutations = BTreeMap::new();
/// mutations.insert(
///     "001_create_users".to_string(),
///     Arc::new("INSERT INTO users (name) VALUES ('test')".to_string()) as Arc<dyn Executable>
/// );
/// mutations.insert(
///     "002_create_posts".to_string(),
///     Arc::new("INSERT INTO posts (title) VALUES ('test post')".to_string()) as Arc<dyn Executable>
/// );
///
/// // Use with verify_migrations_with_mutations
/// // verify_migrations_with_mutations(db, migrations, mutations).await?;
/// # }
/// ```
#[async_trait]
impl MutationProvider for BTreeMap<String, Arc<dyn Executable>> {
    async fn get_mutation(&self, after_migration_id: &str) -> Option<Arc<dyn Executable>> {
        self.get(after_migration_id).cloned() // Cheap Arc cloning!
    }
}

/// Implementation of `MutationProvider` for `Vec<(String, Arc<dyn Executable>)>`
///
/// Provides mutations as an ordered list of (`migration_id`, mutation) pairs.
/// Uses deterministic ordering based on the vector order.
///
/// # Examples
///
/// ```rust
/// use std::sync::Arc;
/// use switchy_schema_test_utils::mutations::MutationProvider;
/// use switchy_database::Executable;
///
/// # async fn example() {
/// let mutations = vec![
///     ("001_create_users".to_string(), Arc::new("INSERT INTO users (name) VALUES ('test')".to_string()) as Arc<dyn Executable>),
///     ("002_create_posts".to_string(), Arc::new("INSERT INTO posts (title) VALUES ('test post')".to_string()) as Arc<dyn Executable>),
/// ];
///
/// // Use with verify_migrations_with_mutations
/// // verify_migrations_with_mutations(db, migrations, mutations).await?;
/// # }
/// ```
#[async_trait]
impl MutationProvider for Vec<(String, Arc<dyn Executable>)> {
    async fn get_mutation(&self, after_migration_id: &str) -> Option<Arc<dyn Executable>> {
        self.iter()
            .find(|(id, _)| id == after_migration_id)
            .map(|(_, executable)| Arc::clone(executable)) // Cheap Arc cloning!
    }
}

/// Builder pattern for constructing mutation sequences
///
/// Provides a fluent interface for building complex mutation sequences
/// with proper ordering and type safety.
///
/// # Examples
///
/// ```rust
/// use switchy_schema_test_utils::mutations::{MutationBuilder, MutationProvider};
/// use switchy_database::Executable;
///
/// # async fn example() {
/// let mutations = MutationBuilder::new()
///     .add_mutation("001_create_users", "INSERT INTO users (name) VALUES ('test')")
///     .add_mutation("002_create_posts", "INSERT INTO posts (title) VALUES ('test post')")
///     .build();
///
/// // Use with verify_migrations_with_mutations
/// // verify_migrations_with_mutations(db, migrations, mutations).await?;
/// # }
/// ```
pub struct MutationBuilder {
    mutations: Vec<(String, Arc<dyn Executable>)>,
}

impl MutationBuilder {
    /// Create a new mutation builder
    #[must_use]
    pub fn new() -> Self {
        Self {
            mutations: Vec::new(),
        }
    }

    /// Add a mutation to be executed after the specified migration
    ///
    /// # Arguments
    ///
    /// * `after_migration_id` - The migration ID after which to execute the mutation
    /// * `mutation` - The mutation to execute (anything that implements `Executable`)
    #[must_use]
    pub fn add_mutation<E>(mut self, after_migration_id: &str, mutation: E) -> Self
    where
        E: Executable + 'static,
    {
        self.mutations
            .push((after_migration_id.to_string(), Arc::new(mutation)));
        self
    }

    /// Build the final mutation provider
    #[must_use]
    pub fn build(self) -> Vec<(String, Arc<dyn Executable>)> {
        self.mutations
    }
}

impl Default for MutationBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{collections::BTreeMap, sync::Arc};

    #[switchy_async::test]
    async fn test_mutation_builder() {
        let mutations = MutationBuilder::new()
            .add_mutation("001_test", "SELECT 1")
            .add_mutation("002_test", "SELECT 2")
            .build();

        assert_eq!(mutations.len(), 2);
        assert_eq!(mutations[0].0, "001_test");
        assert_eq!(mutations[1].0, "002_test");
    }

    #[switchy_async::test]
    async fn test_mutation_builder_default() {
        let mutations = MutationBuilder::default().build();
        assert_eq!(mutations.len(), 0);
    }

    #[switchy_async::test]
    async fn test_btreemap_mutation_provider() {
        let mut mutations = BTreeMap::new();
        mutations.insert(
            "001_test".to_string(),
            Arc::new("SELECT 1".to_string()) as Arc<dyn Executable>,
        );
        mutations.insert(
            "002_test".to_string(),
            Arc::new("SELECT 2".to_string()) as Arc<dyn Executable>,
        );

        // Test existing mutation
        let mutation = mutations.get_mutation("001_test").await;
        assert!(mutation.is_some());

        // Test non-existing mutation
        let no_mutation = mutations.get_mutation("999_nonexistent").await;
        assert!(no_mutation.is_none());
    }

    #[switchy_async::test]
    async fn test_vec_mutation_provider() {
        let mutations = vec![
            (
                "001_test".to_string(),
                Arc::new("SELECT 1".to_string()) as Arc<dyn Executable>,
            ),
            (
                "002_test".to_string(),
                Arc::new("SELECT 2".to_string()) as Arc<dyn Executable>,
            ),
        ];

        // Test existing mutation
        let first_mutation = mutations.get_mutation("001_test").await;
        assert!(first_mutation.is_some());

        // Test non-existing mutation
        let no_mutation = mutations.get_mutation("999_nonexistent").await;
        assert!(no_mutation.is_none());

        // Test second mutation
        let second_mutation = mutations.get_mutation("002_test").await;
        assert!(second_mutation.is_some());
    }

    #[switchy_async::test]
    async fn test_mutation_builder_as_provider() {
        let mutations = MutationBuilder::new()
            .add_mutation("001_test", "SELECT 1")
            .add_mutation("002_test", "SELECT 2")
            .build();

        // Test that builder result works as MutationProvider
        let mutation = mutations.get_mutation("001_test").await;
        assert!(mutation.is_some());

        let no_mutation = mutations.get_mutation("999_nonexistent").await;
        assert!(no_mutation.is_none());
    }

    #[switchy_async::test]
    async fn test_mutation_provider_ordering() {
        // Test that BTreeMap maintains deterministic ordering
        let mut mutations = BTreeMap::new();
        mutations.insert(
            "003_third".to_string(),
            Arc::new("SELECT 3".to_string()) as Arc<dyn Executable>,
        );
        mutations.insert(
            "001_first".to_string(),
            Arc::new("SELECT 1".to_string()) as Arc<dyn Executable>,
        );
        mutations.insert(
            "002_second".to_string(),
            Arc::new("SELECT 2".to_string()) as Arc<dyn Executable>,
        );

        // BTreeMap should maintain sorted order
        let keys: Vec<_> = mutations.keys().collect();
        assert_eq!(keys, vec!["001_first", "002_second", "003_third"]);
    }

    #[switchy_async::test]
    async fn test_mutation_provider_multiple_calls() {
        let mut mutations = BTreeMap::new();
        mutations.insert(
            "001_test".to_string(),
            Arc::new("SELECT 1".to_string()) as Arc<dyn Executable>,
        );

        // Test that we can call get_mutation multiple times (Arc cloning)
        let first_call = mutations.get_mutation("001_test").await;
        let second_call = mutations.get_mutation("001_test").await;

        assert!(first_call.is_some());
        assert!(second_call.is_some());
        // Both should be valid Arc references to the same underlying data
    }
}
