//! Database dependency management utilities for CASCADE and RESTRICT operations
//!
//! This module provides shared infrastructure for discovering and managing foreign key
//! dependencies between database tables. It supports both forward and backward dependency
//! tracking with cycle detection for safe CASCADE operations.

use crate::{DatabaseError, DatabaseTransaction};
use std::collections::{BTreeMap, BTreeSet};

/// Represents foreign key dependencies between tables
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    /// Map from table name to set of tables that depend on it
    pub dependents: BTreeMap<String, BTreeSet<String>>,
    /// Map from table name to set of tables it depends on
    pub dependencies: BTreeMap<String, BTreeSet<String>>,
}

impl DependencyGraph {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            dependents: BTreeMap::new(),
            dependencies: BTreeMap::new(),
        }
    }

    pub fn add_dependency(&mut self, dependent: String, depends_on: String) {
        self.dependents
            .entry(depends_on.clone())
            .or_default()
            .insert(dependent.clone());
        self.dependencies
            .entry(dependent)
            .or_default()
            .insert(depends_on);
    }

    #[must_use]
    pub fn get_dependents(&self, table: &str) -> Option<&BTreeSet<String>> {
        self.dependents.get(table)
    }

    #[must_use]
    pub fn get_dependencies(&self, table: &str) -> Option<&BTreeSet<String>> {
        self.dependencies.get(table)
    }

    #[must_use]
    pub fn has_dependents(&self, table: &str) -> bool {
        self.dependents
            .get(table)
            .is_some_and(|deps| !deps.is_empty())
    }

    /// Performs topological sort with optional subset filtering
    ///
    /// * If `subset` is `Some`, only sort those tables and their dependencies
    /// * If `subset` is `None`, sort all tables in the graph
    /// * Returns tables in dependency order (roots first, leaves last)
    ///
    /// # Errors
    ///
    /// * Returns `CycleError` if circular dependencies are detected
    pub fn topological_sort(
        &self,
        subset: Option<&BTreeSet<String>>,
    ) -> Result<Vec<String>, CycleError> {
        let working_set = subset.map_or_else(
            || {
                let mut all_tables = BTreeSet::new();
                all_tables.extend(self.dependencies.keys().cloned());
                all_tables.extend(self.dependents.keys().cloned());
                all_tables
            },
            std::clone::Clone::clone,
        );

        let mut visited = BTreeSet::new();
        let mut visiting = BTreeSet::new();
        let mut result = Vec::new();

        for table in &working_set {
            if !visited.contains(table) {
                self.topological_visit(
                    table,
                    &working_set,
                    &mut visited,
                    &mut visiting,
                    &mut result,
                )?;
            }
        }

        Ok(result)
    }

    fn topological_visit(
        &self,
        table: &str,
        working_set: &BTreeSet<String>,
        visited: &mut BTreeSet<String>,
        visiting: &mut BTreeSet<String>,
        result: &mut Vec<String>,
    ) -> Result<(), CycleError> {
        if visiting.contains(table) {
            // Found a cycle
            let cycle_tables: Vec<String> = visiting.iter().cloned().collect();
            return Err(CycleError {
                tables: cycle_tables,
                message: format!("Circular dependency involving table '{table}'"),
            });
        }

        if visited.contains(table) {
            return Ok(());
        }

        visiting.insert(table.to_string());

        // Visit dependencies first (tables this table depends on)
        if let Some(deps) = self.dependencies.get(table) {
            for dep in deps {
                if working_set.contains(dep) {
                    self.topological_visit(dep, working_set, visited, visiting, result)?;
                }
            }
        }

        visiting.remove(table);
        visited.insert(table.to_string());
        result.push(table.to_string());

        Ok(())
    }

    /// Resolves the order for dropping a set of tables, handling cycles
    ///
    /// # Errors
    ///
    /// * Returns `DatabaseError` if internal operations fail (currently never fails)
    pub fn resolve_drop_order(
        &self,
        tables_to_drop: BTreeSet<String>,
    ) -> Result<DropPlan, DatabaseError> {
        match self.topological_sort(Some(&tables_to_drop)) {
            Ok(sorted) => Ok(DropPlan::Simple(sorted)),
            Err(CycleError {
                tables: _cycle_tables,
                message: _,
            }) => {
                // All tables in the set need to be dropped even with cycles
                Ok(DropPlan::WithCycles {
                    tables: tables_to_drop.into_iter().collect(),
                    requires_fk_disable: true,
                })
            }
        }
    }

    /// Collects all tables that depend on the given table (recursively)
    #[must_use]
    pub fn collect_all_dependents(&self, table: &str) -> BTreeSet<String> {
        let mut collected = BTreeSet::new();
        self.collect_dependents_recursive(table, &mut collected);
        collected
    }

    fn collect_dependents_recursive(&self, table: &str, collected: &mut BTreeSet<String>) {
        // Add the table itself
        collected.insert(table.to_string());

        // Recursively add all dependent tables
        if let Some(dependents) = self.get_dependents(table) {
            for dependent in dependents {
                if !collected.contains(dependent) {
                    self.collect_dependents_recursive(dependent, collected);
                }
            }
        }
    }

    /// Checks if a table exists in the dependency graph
    #[must_use]
    pub fn table_exists(&self, table: &str) -> bool {
        self.dependencies.contains_key(table) || self.dependents.contains_key(table)
    }
}

impl Default for DependencyGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Error when circular dependencies are detected
#[derive(Debug)]
pub struct CycleError {
    /// Tables involved in the circular dependency
    pub tables: Vec<String>,
    /// Human-readable description of the cycle
    pub message: String,
}

impl std::fmt::Display for CycleError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Circular dependency detected: {} (tables: {:?})",
            self.message, self.tables
        )
    }
}

impl std::error::Error for CycleError {}

/// Represents a plan for dropping tables with dependency handling
#[derive(Debug, Clone)]
pub enum DropPlan {
    /// Simple drop order with no cycles
    Simple(Vec<String>),
    /// Tables with circular dependencies requiring FK constraint disable
    WithCycles {
        tables: Vec<String>,
        requires_fk_disable: bool,
    },
}

/// Discover all foreign key dependencies for `SQLite`
///
/// Uses the generic introspection API to discover foreign key relationships between tables.
/// This implementation leverages the `list_tables()` method and `get_table_info()` to build
/// a complete dependency graph without requiring backend-specific PRAGMA commands.
///
/// # Errors
///
/// * Returns `DatabaseError` if database queries fail
pub async fn discover_dependencies_sqlite(
    tx: &dyn DatabaseTransaction,
) -> Result<DependencyGraph, DatabaseError> {
    let mut graph = DependencyGraph::new();

    // Use new list_tables() method
    let tables = tx.list_tables().await?;

    // Use existing get_table_info() for foreign keys
    for table_name in tables {
        if let Some(table_info) = tx.get_table_info(&table_name).await? {
            // Iterate over foreign_keys BTreeMap, ignoring the constraint name
            for (_fk_name, fk) in table_info.foreign_keys {
                graph.add_dependency(table_name.clone(), fk.referenced_table);
            }
        }
    }

    Ok(graph)
}

/// Get tables that depend on the given table
///
/// # Errors
///
/// * Returns `DatabaseError` if dependency discovery fails
pub async fn get_table_dependencies_sqlite(
    tx: &dyn DatabaseTransaction,
    table_name: &str,
) -> Result<BTreeSet<String>, DatabaseError> {
    let graph = discover_dependencies_sqlite(tx).await?;
    Ok(graph
        .get_dependents(table_name)
        .cloned()
        .unwrap_or_default())
}

// Helper function for recursive cascade traversal with cycle detection
fn visit_cascade_recursive(
    current: &str,
    all_deps: &BTreeMap<String, BTreeSet<String>>,
    visited: &mut BTreeSet<String>,
    visiting: &mut BTreeSet<String>,
    to_drop: &mut Vec<String>,
    has_cycle: &mut bool,
) {
    if visiting.contains(current) {
        *has_cycle = true;
        return; // Cycle detected
    }
    if !visited.insert(current.to_string()) {
        return; // Already processed
    }

    visiting.insert(current.to_string());

    // Visit all dependents first
    if let Some(dependents) = all_deps.get(current) {
        for dependent in dependents {
            visit_cascade_recursive(dependent, all_deps, visited, visiting, to_drop, has_cycle);
        }
    }

    visiting.remove(current);
    to_drop.push(current.to_string());
}

/// Find all tables that would be affected by CASCADE deletion of the specified table
///
/// Returns a `DropPlan` which handles both simple and circular dependencies.
/// For simple cases: `DropPlan::Simple(Vec<String>)` with dependents first.
/// For cycles: `DropPlan::WithCycles` indicating FK constraints must be disabled.
///
/// # Performance
///
/// Time: O(d * f) where d = dependent tables, f = foreign keys per table
/// Space: O(d) for visited set and results
/// Note: Optimized for targeted discovery instead of analyzing all tables
///
/// # Errors
///
/// * Returns `DatabaseError` if dependency discovery fails
pub async fn find_cascade_targets(
    tx: &dyn DatabaseTransaction,
    table_name: &str,
) -> Result<DropPlan, DatabaseError> {
    let mut to_drop = Vec::new();
    let mut visited = BTreeSet::new();
    let mut has_cycle = false;

    // First, build a map of all dependencies we need to consider
    let mut all_deps = BTreeMap::new();
    let mut to_check = vec![table_name.to_string()];
    let mut discovered = BTreeSet::new();

    // Discover all tables involved in the cascade
    while let Some(current) = to_check.pop() {
        if !discovered.insert(current.clone()) {
            continue;
        }

        let dependents = get_direct_dependents(tx, &current).await?;
        all_deps.insert(current.clone(), dependents.clone());

        for dep in dependents {
            to_check.push(dep);
        }
    }

    // Now traverse using proper cycle detection
    let mut visiting = BTreeSet::new();
    visit_cascade_recursive(
        table_name,
        &all_deps,
        &mut visited,
        &mut visiting,
        &mut to_drop,
        &mut has_cycle,
    );

    // The recursive function already puts them in the right order:
    // - Visits dependents first, then the current table
    // - So dependents are added to to_drop before their dependencies
    // No need to reverse!

    if has_cycle {
        Ok(DropPlan::WithCycles {
            tables: to_drop,
            requires_fk_disable: true,
        })
    } else {
        Ok(DropPlan::Simple(to_drop))
    }
}

/// Check if a table has any dependents (for RESTRICT validation)
/// Returns immediately upon finding first dependent for efficiency
///
/// # Performance
///
/// Best case: O(1) - stops at first dependent found
/// Worst case: O(n) - only when table has no dependents
///
/// # Errors
///
/// * Returns `DatabaseError` if introspection fails
pub async fn has_any_dependents(
    tx: &dyn DatabaseTransaction,
    table_name: &str,
) -> Result<bool, DatabaseError> {
    let all_tables = tx.list_tables().await?;

    for table in all_tables {
        if table == table_name {
            continue;
        }

        if let Some(info) = tx.get_table_info(&table).await? {
            for fk in info.foreign_keys.values() {
                if fk.referenced_table == table_name {
                    return Ok(true); // EARLY TERMINATION - key optimization
                }
            }
        }
    }

    Ok(false)
}

/// Get direct dependents of a table (one level only, no recursion)
///
/// # Errors
///
/// * Returns `DatabaseError` if table introspection fails
pub async fn get_direct_dependents(
    tx: &dyn DatabaseTransaction,
    table_name: &str,
) -> Result<BTreeSet<String>, DatabaseError> {
    let mut dependents = BTreeSet::new();

    // We must use list_tables() as it's the only way to discover tables
    // with existing Database methods, but we optimize by only calling
    // get_table_info() on each table once and only as needed
    let all_tables = tx.list_tables().await?;

    for table in all_tables {
        if table == table_name {
            continue; // Skip self-references
        }

        // Get info for this specific table (not all upfront)
        if let Some(info) = tx.get_table_info(&table).await? {
            // Check if this table references our target
            for fk in info.foreign_keys.values() {
                if fk.referenced_table == table_name {
                    dependents.insert(table.clone());
                    break; // Found dependency, move to next table
                }
            }
        }
    }

    Ok(dependents)
}

/// Recursively find all tables that depend on the specified table
/// More efficient than building full graph when only one table's dependents are needed
///
/// # Errors
///
/// * Returns `DatabaseError` if dependency discovery fails
pub async fn get_all_dependents_recursive(
    tx: &dyn DatabaseTransaction,
    table_name: &str,
) -> Result<BTreeSet<String>, DatabaseError> {
    let mut all_dependents = BTreeSet::new();
    let mut to_check = vec![table_name.to_string()];
    let mut visited = BTreeSet::new();

    while let Some(current_table) = to_check.pop() {
        if !visited.insert(current_table.clone()) {
            continue; // Already processed
        }

        // Reuse get_direct_dependents for consistency
        let direct_deps = get_direct_dependents(tx, &current_table).await?;

        for dep in direct_deps {
            if all_dependents.insert(dep.clone()) {
                to_check.push(dep); // Queue for recursive checking
            }
        }
    }

    Ok(all_dependents)
}

/// For `SQLite` `PRAGMA`: Table names cannot be parameterized
/// Basic validation for PRAGMA syntax - NOT comprehensive security
///
/// # Errors
///
/// * Returns `DatabaseError::InvalidQuery` if table name contains unsafe characters
pub fn validate_table_name_for_pragma(name: &str) -> Result<(), DatabaseError> {
    // Only allow safe characters for PRAGMA usage
    if name.chars().all(|c| c.is_alphanumeric() || c == '_') && !name.is_empty() {
        Ok(())
    } else {
        Err(DatabaseError::InvalidQuery(format!(
            "Table name contains unsafe characters for PRAGMA: {name}"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    // Mock Transaction for testing the new async functions
    #[cfg(feature = "simulator")]
    mod async_tests {
        use super::*;
        use crate::Database;
        use crate::simulator::SimulationDatabase;

        async fn create_test_database_with_dependencies() -> SimulationDatabase {
            let db = SimulationDatabase::new().unwrap();

            // Create tables with foreign key dependencies
            // users (root)
            db.exec_raw(
                "CREATE TABLE users (
                    id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL
                )",
            )
            .await
            .unwrap();

            // posts (depends on users)
            db.exec_raw(
                "CREATE TABLE posts (
                    id INTEGER PRIMARY KEY,
                    title TEXT NOT NULL,
                    user_id INTEGER,
                    FOREIGN KEY (user_id) REFERENCES users(id)
                )",
            )
            .await
            .unwrap();

            // comments (depends on posts)
            db.exec_raw(
                "CREATE TABLE comments (
                    id INTEGER PRIMARY KEY,
                    content TEXT NOT NULL,
                    post_id INTEGER,
                    FOREIGN KEY (post_id) REFERENCES posts(id)
                )",
            )
            .await
            .unwrap();

            // tags (independent table)
            db.exec_raw(
                "CREATE TABLE tags (
                    id INTEGER PRIMARY KEY,
                    name TEXT NOT NULL
                )",
            )
            .await
            .unwrap();

            // post_tags (depends on both posts and tags)
            db.exec_raw(
                "CREATE TABLE post_tags (
                    post_id INTEGER,
                    tag_id INTEGER,
                    PRIMARY KEY (post_id, tag_id),
                    FOREIGN KEY (post_id) REFERENCES posts(id),
                    FOREIGN KEY (tag_id) REFERENCES tags(id)
                )",
            )
            .await
            .unwrap();

            db
        }

        async fn create_test_database_with_cycles() -> SimulationDatabase {
            let db = SimulationDatabase::new().unwrap();

            // Create a three-table cycle: A -> B -> C -> A
            // This avoids SQLite's limitations with direct circular references

            db.exec_raw(
                "CREATE TABLE cycle_a (
                    id INTEGER PRIMARY KEY,
                    b_ref INTEGER,
                    FOREIGN KEY (b_ref) REFERENCES cycle_b(id)
                )",
            )
            .await
            .unwrap();

            db.exec_raw(
                "CREATE TABLE cycle_b (
                    id INTEGER PRIMARY KEY,
                    c_ref INTEGER,
                    FOREIGN KEY (c_ref) REFERENCES cycle_c(id)
                )",
            )
            .await
            .unwrap();

            db.exec_raw(
                "CREATE TABLE cycle_c (
                    id INTEGER PRIMARY KEY,
                    a_ref INTEGER,
                    FOREIGN KEY (a_ref) REFERENCES cycle_a(id)
                )",
            )
            .await
            .unwrap();

            db
        }

        #[switchy_async::test]
        async fn test_get_direct_dependents_basic() {
            let db = create_test_database_with_dependencies().await;
            let tx = db.begin_transaction().await.unwrap();

            // Test that users has posts as a dependent
            let dependents = get_direct_dependents(&*tx, "users").await.unwrap();
            assert_eq!(dependents.len(), 1);
            assert!(dependents.contains("posts"));

            // Test that posts has comments and post_tags as dependents
            let dependents = get_direct_dependents(&*tx, "posts").await.unwrap();
            assert_eq!(dependents.len(), 2);
            assert!(dependents.contains("comments"));
            assert!(dependents.contains("post_tags"));

            // Test that tags has post_tags as a dependent
            let dependents = get_direct_dependents(&*tx, "tags").await.unwrap();
            assert_eq!(dependents.len(), 1);
            assert!(dependents.contains("post_tags"));

            // Test that comments has no dependents
            let dependents = get_direct_dependents(&*tx, "comments").await.unwrap();
            assert!(dependents.is_empty());

            tx.commit().await.unwrap();
        }

        #[switchy_async::test]
        async fn test_get_direct_dependents_non_existent_table() {
            let db = create_test_database_with_dependencies().await;
            let tx = db.begin_transaction().await.unwrap();

            // Test non-existent table
            let dependents = get_direct_dependents(&*tx, "non_existent").await.unwrap();
            assert!(dependents.is_empty());

            tx.commit().await.unwrap();
        }

        #[switchy_async::test]
        async fn test_has_any_dependents_early_termination() {
            let db = create_test_database_with_dependencies().await;
            let tx = db.begin_transaction().await.unwrap();

            // Test tables with dependents (should return true)
            assert!(has_any_dependents(&*tx, "users").await.unwrap());
            assert!(has_any_dependents(&*tx, "posts").await.unwrap());
            assert!(has_any_dependents(&*tx, "tags").await.unwrap());

            // Test table without dependents
            assert!(!has_any_dependents(&*tx, "comments").await.unwrap());
            assert!(!has_any_dependents(&*tx, "post_tags").await.unwrap());

            // Test non-existent table
            assert!(!has_any_dependents(&*tx, "non_existent").await.unwrap());

            tx.commit().await.unwrap();
        }

        #[switchy_async::test]
        async fn test_get_all_dependents_recursive() {
            let db = create_test_database_with_dependencies().await;
            let tx = db.begin_transaction().await.unwrap();

            // Test users (root) - should find all its recursive dependents
            let all_dependents = get_all_dependents_recursive(&*tx, "users").await.unwrap();
            assert_eq!(all_dependents.len(), 3); // posts, comments, post_tags
            assert!(all_dependents.contains("posts"));
            assert!(all_dependents.contains("comments"));
            assert!(all_dependents.contains("post_tags"));

            // Test posts - should find its dependents
            let all_dependents = get_all_dependents_recursive(&*tx, "posts").await.unwrap();
            assert_eq!(all_dependents.len(), 2); // comments, post_tags
            assert!(all_dependents.contains("comments"));
            assert!(all_dependents.contains("post_tags"));

            // Test tags - should find only post_tags
            let all_dependents = get_all_dependents_recursive(&*tx, "tags").await.unwrap();
            assert_eq!(all_dependents.len(), 1);
            assert!(all_dependents.contains("post_tags"));

            // Test leaf table - should have no dependents
            let all_dependents = get_all_dependents_recursive(&*tx, "comments")
                .await
                .unwrap();
            assert!(all_dependents.is_empty());

            tx.commit().await.unwrap();
        }

        #[switchy_async::test]
        async fn test_find_cascade_targets_simple_case() {
            let db = create_test_database_with_dependencies().await;
            let tx = db.begin_transaction().await.unwrap();

            // Test CASCADE targets for users (should include all dependents)
            let drop_plan = find_cascade_targets(&*tx, "users").await.unwrap();
            match drop_plan {
                DropPlan::Simple(tables) => {
                    assert_eq!(tables.len(), 4); // users + 3 dependents
                    // Should include all tables in proper order (dependents first)
                    assert!(tables.contains(&"users".to_string()));
                    assert!(tables.contains(&"posts".to_string()));
                    assert!(tables.contains(&"comments".to_string()));
                    assert!(tables.contains(&"post_tags".to_string()));

                    // Verify order: dependents should come before dependencies
                    let users_pos = tables.iter().position(|t| t == "users").unwrap();
                    let posts_pos = tables.iter().position(|t| t == "posts").unwrap();
                    assert!(posts_pos < users_pos, "posts should come before users");
                }
                DropPlan::WithCycles { .. } => {
                    panic!("Expected Simple drop plan, got WithCycles");
                }
            }

            // Test CASCADE targets for posts
            let drop_plan = find_cascade_targets(&*tx, "posts").await.unwrap();
            match drop_plan {
                DropPlan::Simple(tables) => {
                    assert_eq!(tables.len(), 3); // posts + 2 dependents
                    assert!(tables.contains(&"posts".to_string()));
                    assert!(tables.contains(&"comments".to_string()));
                    assert!(tables.contains(&"post_tags".to_string()));
                }
                DropPlan::WithCycles { .. } => {
                    panic!("Expected Simple drop plan, got WithCycles");
                }
            }

            // Test leaf table (should only include itself)
            let drop_plan = find_cascade_targets(&*tx, "comments").await.unwrap();
            match drop_plan {
                DropPlan::Simple(tables) => {
                    assert_eq!(tables.len(), 1);
                    assert_eq!(tables[0], "comments");
                }
                DropPlan::WithCycles { .. } => {
                    panic!("Expected Simple drop plan, got WithCycles");
                }
            }

            tx.commit().await.unwrap();
        }

        #[switchy_async::test]
        async fn test_find_cascade_targets_with_cycles() {
            let db = create_test_database_with_cycles().await;
            let tx = db.begin_transaction().await.unwrap();

            // Test CASCADE targets with circular dependencies (A -> B -> C -> A)
            let drop_plan = find_cascade_targets(&*tx, "cycle_a").await.unwrap();
            match drop_plan {
                DropPlan::WithCycles {
                    tables,
                    requires_fk_disable,
                } => {
                    assert!(requires_fk_disable);
                    assert_eq!(tables.len(), 3);
                    assert!(tables.contains(&"cycle_a".to_string()));
                    assert!(tables.contains(&"cycle_b".to_string()));
                    assert!(tables.contains(&"cycle_c".to_string()));
                }
                DropPlan::Simple(_) => {
                    panic!("Expected WithCycles drop plan, got Simple");
                }
            }

            // Test from cycle_b as well
            let drop_plan = find_cascade_targets(&*tx, "cycle_b").await.unwrap();
            match drop_plan {
                DropPlan::WithCycles {
                    tables,
                    requires_fk_disable,
                } => {
                    assert!(requires_fk_disable);
                    assert_eq!(tables.len(), 3);
                    assert!(tables.contains(&"cycle_a".to_string()));
                    assert!(tables.contains(&"cycle_b".to_string()));
                    assert!(tables.contains(&"cycle_c".to_string()));
                }
                DropPlan::Simple(_) => {
                    panic!("Expected WithCycles drop plan, got Simple");
                }
            }

            tx.commit().await.unwrap();
        }

        #[switchy_async::test]
        async fn test_edge_case_self_references() {
            let db = SimulationDatabase::new().unwrap();

            // Create table with self-reference
            db.exec_raw(
                "CREATE TABLE self_ref (
                    id INTEGER PRIMARY KEY,
                    parent_id INTEGER,
                    name TEXT,
                    FOREIGN KEY (parent_id) REFERENCES self_ref(id)
                )",
            )
            .await
            .unwrap();

            let tx = db.begin_transaction().await.unwrap();

            // Test get_direct_dependents with self-reference
            let dependents = get_direct_dependents(&*tx, "self_ref").await.unwrap();

            // SQLite should detect the self-reference if foreign keys are properly introspected
            if dependents.contains("self_ref") {
                assert_eq!(dependents.len(), 1);

                // Test has_any_dependents (should return true due to self-reference)
                assert!(has_any_dependents(&*tx, "self_ref").await.unwrap());

                // Test find_cascade_targets (should detect cycle due to self-reference)
                let drop_plan = find_cascade_targets(&*tx, "self_ref").await.unwrap();
                match drop_plan {
                    DropPlan::WithCycles {
                        tables,
                        requires_fk_disable,
                    } => {
                        assert!(requires_fk_disable);
                        assert_eq!(tables.len(), 1);
                        assert!(tables.contains(&"self_ref".to_string()));
                    }
                    DropPlan::Simple(tables) => {
                        // If cycle detection didn't work, at least verify the table is included
                        assert_eq!(tables.len(), 1);
                        assert!(tables.contains(&"self_ref".to_string()));
                    }
                }
            } else {
                // If SQLite doesn't report self-reference, that's also valid behavior
                // Just test that the table doesn't crash the algorithm
                assert!(dependents.is_empty());
                assert!(!has_any_dependents(&*tx, "self_ref").await.unwrap());

                let drop_plan = find_cascade_targets(&*tx, "self_ref").await.unwrap();
                match drop_plan {
                    DropPlan::Simple(tables) => {
                        assert_eq!(tables.len(), 1);
                        assert!(tables.contains(&"self_ref".to_string()));
                    }
                    DropPlan::WithCycles { .. } => {
                        // This is also acceptable if detected
                    }
                }
            }

            tx.commit().await.unwrap();
        }

        #[switchy_async::test]
        async fn test_transaction_rollback_with_failed_operations() {
            let db = create_test_database_with_dependencies().await;

            // Begin transaction
            let tx = db.begin_transaction().await.unwrap();

            // Perform some dependency discovery operations
            let dependents = get_direct_dependents(&*tx, "users").await.unwrap();
            assert!(!dependents.is_empty());

            // Test that operations work within transaction context
            let has_deps = has_any_dependents(&*tx, "posts").await.unwrap();
            assert!(has_deps);

            // Rollback transaction
            tx.rollback().await.unwrap();

            // Create a new transaction and verify database state is unchanged
            let tx2 = db.begin_transaction().await.unwrap();
            let dependents_after = get_direct_dependents(&*tx2, "users").await.unwrap();
            assert_eq!(dependents, dependents_after); // Should be the same

            tx2.commit().await.unwrap();
        }

        #[switchy_async::test]
        async fn test_complex_dependency_chains() {
            let db = SimulationDatabase::new().unwrap();

            // Create complex dependency chain: A -> B -> C -> D
            db.exec_raw("CREATE TABLE table_d (id INTEGER PRIMARY KEY, name TEXT)")
                .await
                .unwrap();

            db.exec_raw(
                "CREATE TABLE table_c (
                    id INTEGER PRIMARY KEY, 
                    d_id INTEGER,
                    FOREIGN KEY (d_id) REFERENCES table_d(id)
                )",
            )
            .await
            .unwrap();

            db.exec_raw(
                "CREATE TABLE table_b (
                    id INTEGER PRIMARY KEY, 
                    c_id INTEGER,
                    FOREIGN KEY (c_id) REFERENCES table_c(id)
                )",
            )
            .await
            .unwrap();

            db.exec_raw(
                "CREATE TABLE table_a (
                    id INTEGER PRIMARY KEY, 
                    b_id INTEGER,
                    FOREIGN KEY (b_id) REFERENCES table_b(id)
                )",
            )
            .await
            .unwrap();

            let tx = db.begin_transaction().await.unwrap();

            // Test cascade from root (table_d)
            let drop_plan = find_cascade_targets(&*tx, "table_d").await.unwrap();
            match drop_plan {
                DropPlan::Simple(tables) => {
                    assert_eq!(tables.len(), 4);
                    // Verify proper ordering: dependents first
                    let d_pos = tables.iter().position(|t| t == "table_d").unwrap();
                    let c_pos = tables.iter().position(|t| t == "table_c").unwrap();
                    let b_pos = tables.iter().position(|t| t == "table_b").unwrap();
                    let a_pos = tables.iter().position(|t| t == "table_a").unwrap();

                    assert!(a_pos < b_pos);
                    assert!(b_pos < c_pos);
                    assert!(c_pos < d_pos);
                }
                DropPlan::WithCycles { .. } => {
                    panic!("Expected Simple drop plan, got WithCycles");
                }
            }

            // Test recursive dependents
            let all_deps = get_all_dependents_recursive(&*tx, "table_d").await.unwrap();
            assert_eq!(all_deps.len(), 3); // c, b, a
            assert!(all_deps.contains("table_c"));
            assert!(all_deps.contains("table_b"));
            assert!(all_deps.contains("table_a"));

            tx.commit().await.unwrap();
        }
    }

    #[test]
    fn test_new_graph_is_empty() {
        let graph = DependencyGraph::new();
        assert!(graph.dependencies.is_empty());
    }

    #[test]
    fn test_add_single_dependency() {
        let mut graph = DependencyGraph::new();
        graph.add_dependency("posts".to_string(), "users".to_string());

        // Posts depends on users
        assert!(graph.dependencies.contains_key("posts"));
        assert!(graph.dependencies["posts"].contains("users"));

        // Users has dependents (posts depends on it)
        assert!(graph.dependents.contains_key("users"));
        assert!(graph.dependents["users"].contains("posts"));

        // Users has no dependencies (no entry created since it doesn't depend on anything)
        assert!(!graph.dependencies.contains_key("users"));
    }

    #[test]
    fn test_add_multiple_dependencies() {
        let mut graph = DependencyGraph::new();
        graph.add_dependency("posts".to_string(), "users".to_string());
        graph.add_dependency("comments".to_string(), "posts".to_string());
        graph.add_dependency("comments".to_string(), "users".to_string());

        assert_eq!(graph.dependencies["comments"].len(), 2);
        assert!(graph.dependencies["comments"].contains("posts"));
        assert!(graph.dependencies["comments"].contains("users"));
    }

    #[test]
    fn test_topological_sort_linear_chain() {
        let mut graph = DependencyGraph::new();
        graph.add_dependency("posts".to_string(), "users".to_string());
        graph.add_dependency("comments".to_string(), "posts".to_string());

        // Test sorting all tables - returns roots first, leaves last
        let result = graph.topological_sort(None).unwrap();
        assert_eq!(result, vec!["users", "posts", "comments"]);

        // Test sorting with subset that includes only one table
        let subset = BTreeSet::from(["comments".to_string()]);
        let result = graph.topological_sort(Some(&subset)).unwrap();
        assert_eq!(result, vec!["comments"]);
    }

    #[test]
    fn test_topological_sort_diamond_dependency() {
        let mut graph = DependencyGraph::new();
        // Diamond: D depends on B and C, both depend on A
        graph.add_dependency("D".to_string(), "B".to_string());
        graph.add_dependency("D".to_string(), "C".to_string());
        graph.add_dependency("B".to_string(), "A".to_string());
        graph.add_dependency("C".to_string(), "A".to_string());

        // Test sorting all tables - roots first (A), leaves last (D)
        let result = graph.topological_sort(None).unwrap();
        assert_eq!(result.len(), 4);
        assert_eq!(result[0], "A"); // Root comes first
        assert_eq!(result[3], "D"); // Leaf comes last
        // B and C can be in either order at positions 1 and 2
        assert!(result.contains(&"B".to_string()));
        assert!(result.contains(&"C".to_string()));
    }

    #[test]
    fn test_topological_sort_detects_simple_cycle() {
        let mut graph = DependencyGraph::new();
        graph.add_dependency("A".to_string(), "B".to_string());
        graph.add_dependency("B".to_string(), "A".to_string());

        let result = graph.topological_sort(None);
        assert!(matches!(result, Err(CycleError { .. })));

        if let Err(CycleError { tables, .. }) = result {
            assert!(tables.contains(&"A".to_string()));
            assert!(tables.contains(&"B".to_string()));
        }
    }

    #[test]
    fn test_topological_sort_detects_complex_cycle() {
        let mut graph = DependencyGraph::new();
        graph.add_dependency("A".to_string(), "B".to_string());
        graph.add_dependency("B".to_string(), "C".to_string());
        graph.add_dependency("C".to_string(), "D".to_string());
        graph.add_dependency("D".to_string(), "B".to_string()); // Creates cycle B->C->D->B

        let result = graph.topological_sort(None);
        assert!(matches!(result, Err(CycleError { .. })));
    }

    #[test]
    fn test_topological_sort_independent_table() {
        let mut graph = DependencyGraph::new();

        // Add a table with no dependencies
        graph.add_dependency("independent".to_string(), String::new());
        graph.dependencies.get_mut("independent").unwrap().clear(); // Remove the empty dependency

        let subset = BTreeSet::from(["independent".to_string()]);
        let result = graph.topological_sort(Some(&subset)).unwrap();
        assert_eq!(result, vec!["independent"]);
    }

    #[test]
    fn test_get_dependencies_direct() {
        let mut graph = DependencyGraph::new();
        graph.add_dependency("posts".to_string(), "users".to_string());

        let deps = graph.get_dependencies("posts").unwrap();
        assert_eq!(*deps, BTreeSet::from(["users".to_string()]));
    }

    #[test]
    fn test_get_dependencies_transitive() {
        let mut graph = DependencyGraph::new();
        graph.add_dependency("comments".to_string(), "posts".to_string());
        graph.add_dependency("posts".to_string(), "users".to_string());

        // Direct dependencies only
        let deps = graph.get_dependencies("comments").unwrap();
        assert_eq!(*deps, BTreeSet::from(["posts".to_string()]));

        // For transitive dependencies, use topological sort on all tables (roots first)
        let sorted = graph.topological_sort(None).unwrap();
        assert_eq!(sorted, vec!["users", "posts", "comments"]);
    }

    #[test]
    fn test_get_dependencies_empty() {
        let graph = DependencyGraph::new();
        assert!(graph.get_dependencies("nonexistent").is_none());

        let mut graph = DependencyGraph::new();
        graph.add_dependency("posts".to_string(), "users".to_string());

        // Users has no dependencies
        let deps = graph.get_dependencies("users");
        assert!(deps.is_none() || deps.unwrap().is_empty());
    }

    #[test]
    fn test_get_dependents() {
        let mut graph = DependencyGraph::new();
        graph.add_dependency("posts".to_string(), "users".to_string());
        graph.add_dependency("comments".to_string(), "users".to_string());

        let dependents = graph.get_dependents("users").unwrap();
        assert_eq!(dependents.len(), 2);
        assert!(dependents.contains("posts"));
        assert!(dependents.contains("comments"));

        // Table with no dependents
        assert!(
            graph.get_dependents("posts").is_none()
                || graph.get_dependents("posts").unwrap().is_empty()
        );
    }

    #[test]
    fn test_has_dependents() {
        let mut graph = DependencyGraph::new();
        graph.add_dependency("posts".to_string(), "users".to_string());

        assert!(graph.has_dependents("users"));
        assert!(!graph.has_dependents("posts"));
        assert!(!graph.has_dependents("nonexistent"));
    }

    #[test_log::test]
    fn test_collect_all_dependents_simple_chain() {
        let mut graph = DependencyGraph::new();
        // Chain: users <- posts <- comments
        graph.add_dependency("posts".to_string(), "users".to_string());
        graph.add_dependency("comments".to_string(), "posts".to_string());

        // Starting from users, should collect posts and comments
        let all_deps = graph.collect_all_dependents("users");
        assert_eq!(
            all_deps,
            BTreeSet::from([
                "users".to_string(),
                "posts".to_string(),
                "comments".to_string()
            ])
        );
    }

    #[test_log::test]
    fn test_collect_all_dependents_diamond_pattern() {
        let mut graph = DependencyGraph::new();
        // Diamond: users <- posts <- post_tags, users <- comments <- post_tags
        graph.add_dependency("posts".to_string(), "users".to_string());
        graph.add_dependency("comments".to_string(), "users".to_string());
        graph.add_dependency("post_tags".to_string(), "posts".to_string());
        graph.add_dependency("post_tags".to_string(), "comments".to_string());

        // Starting from users, should collect all tables
        let all_deps = graph.collect_all_dependents("users");
        assert_eq!(
            all_deps,
            BTreeSet::from([
                "users".to_string(),
                "posts".to_string(),
                "comments".to_string(),
                "post_tags".to_string()
            ])
        );
    }

    #[test_log::test]
    fn test_collect_all_dependents_leaf_table() {
        let mut graph = DependencyGraph::new();
        graph.add_dependency("posts".to_string(), "users".to_string());
        graph.add_dependency("comments".to_string(), "posts".to_string());

        // Starting from leaf table (comments) - should only include itself
        let all_deps = graph.collect_all_dependents("comments");
        assert_eq!(all_deps, BTreeSet::from(["comments".to_string()]));
    }

    #[test_log::test]
    fn test_collect_all_dependents_nonexistent_table() {
        let graph = DependencyGraph::new();

        // Nonexistent table - should still include itself
        let all_deps = graph.collect_all_dependents("nonexistent");
        assert_eq!(all_deps, BTreeSet::from(["nonexistent".to_string()]));
    }

    #[test_log::test]
    fn test_table_exists_in_graph() {
        let mut graph = DependencyGraph::new();
        graph.add_dependency("posts".to_string(), "users".to_string());
        graph.add_dependency("comments".to_string(), "posts".to_string());

        // Table with dependents
        assert!(graph.table_exists("users"));

        // Table with both dependencies and dependents
        assert!(graph.table_exists("posts"));

        // Table with only dependencies
        assert!(graph.table_exists("comments"));

        // Nonexistent table
        assert!(!graph.table_exists("nonexistent"));
    }

    #[test_log::test]
    fn test_table_exists_only_in_dependents() {
        let mut graph = DependencyGraph::new();
        graph.add_dependency("posts".to_string(), "users".to_string());

        // users only appears in dependents, not in dependencies
        assert!(graph.table_exists("users"));
    }

    #[test_log::test]
    fn test_resolve_drop_order_simple() {
        let mut graph = DependencyGraph::new();
        graph.add_dependency("posts".to_string(), "users".to_string());
        graph.add_dependency("comments".to_string(), "posts".to_string());

        let tables_to_drop = BTreeSet::from([
            "users".to_string(),
            "posts".to_string(),
            "comments".to_string(),
        ]);

        let result = graph.resolve_drop_order(tables_to_drop).unwrap();
        match result {
            DropPlan::Simple(order) => {
                assert_eq!(order.len(), 3);
                // Verify proper ordering: users should come first (root), comments last
                assert_eq!(order[0], "users");
                assert_eq!(order[2], "comments");
            }
            DropPlan::WithCycles { .. } => panic!("Expected Simple plan, got WithCycles"),
        }
    }

    #[test_log::test]
    fn test_resolve_drop_order_with_cycle() {
        let mut graph = DependencyGraph::new();
        // Create a cycle: A -> B -> A
        graph.add_dependency("A".to_string(), "B".to_string());
        graph.add_dependency("B".to_string(), "A".to_string());

        let tables_to_drop = BTreeSet::from(["A".to_string(), "B".to_string()]);

        let result = graph.resolve_drop_order(tables_to_drop).unwrap();
        match result {
            DropPlan::WithCycles {
                tables,
                requires_fk_disable,
            } => {
                assert!(requires_fk_disable);
                assert_eq!(tables.len(), 2);
                assert!(tables.contains(&"A".to_string()));
                assert!(tables.contains(&"B".to_string()));
            }
            DropPlan::Simple(_) => panic!("Expected WithCycles plan, got Simple"),
        }
    }

    #[test_log::test]
    fn test_validate_table_name_for_pragma_valid_names() {
        assert!(validate_table_name_for_pragma("users").is_ok());
        assert!(validate_table_name_for_pragma("my_table").is_ok());
        assert!(validate_table_name_for_pragma("Table123").is_ok());
        assert!(validate_table_name_for_pragma("_private").is_ok());
        assert!(validate_table_name_for_pragma("a").is_ok());
    }

    #[test_log::test]
    fn test_validate_table_name_for_pragma_empty_rejected() {
        let result = validate_table_name_for_pragma("");
        assert!(result.is_err());
        match result {
            Err(crate::DatabaseError::InvalidQuery(msg)) => {
                assert!(msg.contains("unsafe characters"));
            }
            _ => panic!("Expected InvalidQuery error"),
        }
    }

    #[test_log::test]
    fn test_validate_table_name_for_pragma_special_chars_rejected() {
        // SQL injection attempts
        assert!(validate_table_name_for_pragma("users; DROP TABLE users").is_err());
        assert!(validate_table_name_for_pragma("users--").is_err());
        assert!(validate_table_name_for_pragma("users'").is_err());
        assert!(validate_table_name_for_pragma("users\"").is_err());

        // Other invalid characters
        assert!(validate_table_name_for_pragma("table-name").is_err());
        assert!(validate_table_name_for_pragma("table.name").is_err());
        assert!(validate_table_name_for_pragma("table name").is_err());
        assert!(validate_table_name_for_pragma("table*").is_err());
    }

    #[test]
    fn test_column_dependencies_struct() {
        let deps = ColumnDependencies {
            indexes: vec!["idx_email".to_string(), "idx_composite".to_string()],
            foreign_keys: vec!["fk_user_email".to_string()],
        };

        assert_eq!(deps.indexes.len(), 2);
        assert_eq!(deps.foreign_keys.len(), 1);
        assert!(deps.indexes.contains(&"idx_email".to_string()));
        assert!(deps.foreign_keys.contains(&"fk_user_email".to_string()));
    }

    #[cfg(feature = "simulator")]
    #[switchy_async::test]
    async fn test_get_column_dependencies_empty_table() {
        use crate::Database;
        use crate::simulator::SimulationDatabase;

        let db = SimulationDatabase::new().unwrap();

        // Create a simple table without indexes or foreign keys
        db.exec_raw(
            "CREATE TABLE simple_table (
                id INTEGER PRIMARY KEY,
                name TEXT,
                email TEXT
            )",
        )
        .await
        .unwrap();

        let tx = db.begin_transaction().await.unwrap();

        // Test column that exists but has no dependencies
        let deps = super::get_column_dependencies(&*tx, "simple_table", "email")
            .await
            .unwrap();
        assert!(deps.indexes.is_empty());
        assert!(deps.foreign_keys.is_empty());

        // Test nonexistent column
        let deps = super::get_column_dependencies(&*tx, "simple_table", "nonexistent")
            .await
            .unwrap();
        assert!(deps.indexes.is_empty());
        assert!(deps.foreign_keys.is_empty());

        // Test nonexistent table
        let deps = super::get_column_dependencies(&*tx, "nonexistent_table", "email")
            .await
            .unwrap();
        assert!(deps.indexes.is_empty());
        assert!(deps.foreign_keys.is_empty());

        tx.commit().await.unwrap();
    }
}

/// Column-level dependency information
#[derive(Debug, Clone)]
pub struct ColumnDependencies {
    /// Indexes that use this column
    pub indexes: Vec<String>,
    /// Foreign key constraints that reference this column
    pub foreign_keys: Vec<String>,
}

/// Get column dependencies using existing table introspection infrastructure
///
/// # Errors
///
/// * Returns `DatabaseError` if table introspection fails
pub async fn get_column_dependencies(
    tx: &dyn DatabaseTransaction,
    table_name: &str,
    column_name: &str,
) -> Result<ColumnDependencies, DatabaseError> {
    let mut deps = ColumnDependencies {
        indexes: Vec::new(),
        foreign_keys: Vec::new(),
    };

    // Get table info using existing introspection
    if let Some(table_info) = tx.get_table_info(table_name).await? {
        // Find indexes that contain this column
        for (index_name, index_info) in table_info.indexes {
            if index_info.columns.contains(&column_name.to_string()) {
                deps.indexes.push(index_name);
            }
        }

        // Find foreign keys that reference this column
        for (fk_name, fk_info) in table_info.foreign_keys {
            if fk_info.column == column_name {
                deps.foreign_keys.push(fk_name);
            }
        }
    }

    Ok(deps)
}
