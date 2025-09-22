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

/// Plan for dropping tables
#[derive(Debug)]
pub enum DropPlan {
    /// Simple ordered drop (no cycles)
    Simple(Vec<String>),
    /// Requires disabling foreign keys due to cycles
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
