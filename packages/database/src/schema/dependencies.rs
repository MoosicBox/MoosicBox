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
    let mut visiting = BTreeSet::new(); // For cycle detection
    let mut stack = vec![table_name.to_string()];
    let mut has_cycle = false;

    while let Some(current_table) = stack.pop() {
        if visiting.contains(&current_table) {
            has_cycle = true;
            continue; // Cycle detected
        }
        if !visited.insert(current_table.clone()) {
            continue; // Already processed
        }

        visiting.insert(current_table.clone());

        // Find tables that directly reference current_table
        let dependents = get_direct_dependents(tx, &current_table).await?;

        // Add dependents to drop list (they come before their dependencies)
        for dependent in &dependents {
            if !visited.contains(dependent) {
                stack.push(dependent.clone());
            }
        }

        visiting.remove(&current_table);
        to_drop.push(current_table);
    }

    // Reverse to get proper drop order (dependents first, then dependencies)
    // This ensures that when we drop, dependents are dropped before their dependencies
    to_drop.reverse();

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

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
}
