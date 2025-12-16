//! Node.js lockfile parsing and diff analysis.
//!
//! This module provides parsers for different Node.js lockfile formats:
//! - npm (`package-lock.json`)
//! - pnpm (`pnpm-lock.yaml`)
//! - bun (`bun.lock`)

mod bun;
mod npm;
mod pnpm;

pub use bun::parse_bun_lockfile;
pub use npm::parse_npm_lockfile;
pub use pnpm::parse_pnpm_lockfile;

use crate::workspace::traits::{Lockfile, LockfileDiffParser, LockfileEntry};

use super::NodePackageManager;

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A single entry in a Node.js lockfile.
#[derive(Debug, Clone)]
pub struct NodeLockEntry {
    /// Package name
    pub name: String,
    /// Package version
    pub version: String,
    /// Package dependencies (names only)
    pub dependencies: Vec<String>,
}

impl LockfileEntry for NodeLockEntry {
    fn name(&self) -> &str {
        &self.name
    }

    fn version(&self) -> &str {
        &self.version
    }

    fn dependencies(&self) -> &[String] {
        &self.dependencies
    }
}

/// Parsed Node.js lockfile (works with any package manager).
#[derive(Debug, Clone)]
pub struct NodeLockfile {
    /// All package entries in the lockfile
    pub entries: Vec<NodeLockEntry>,
}

impl NodeLockfile {
    /// Creates a new empty lockfile.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Parses a lockfile based on package manager type.
    ///
    /// # Errors
    ///
    /// Returns an error if the content cannot be parsed.
    pub fn parse(content: &str, manager: NodePackageManager) -> Result<Self, BoxError> {
        match manager {
            NodePackageManager::Npm => parse_npm_lockfile(content),
            NodePackageManager::Pnpm => parse_pnpm_lockfile(content),
            NodePackageManager::Bun => parse_bun_lockfile(content),
        }
    }

    /// Finds an entry by package name.
    #[must_use]
    pub fn find_by_name(&self, name: &str) -> Option<&NodeLockEntry> {
        self.entries.iter().find(|e| e.name == name)
    }
}

impl Default for NodeLockfile {
    fn default() -> Self {
        Self::new()
    }
}

impl Lockfile for NodeLockfile {
    fn entries(&self) -> Vec<Box<dyn LockfileEntry>> {
        self.entries
            .iter()
            .cloned()
            .map(|e| Box::new(e) as Box<dyn LockfileEntry>)
            .collect()
    }

    fn find(&self, name: &str) -> Option<Box<dyn LockfileEntry>> {
        self.find_by_name(name)
            .cloned()
            .map(|e| Box::new(e) as Box<dyn LockfileEntry>)
    }
}

/// Parser for Node.js lockfile diffs.
#[derive(Debug, Clone)]
pub struct NodeLockDiffParser {
    /// Package manager type (affects parsing strategy)
    pub manager: NodePackageManager,
}

impl NodeLockDiffParser {
    /// Creates a new parser for the specified package manager.
    #[must_use]
    pub const fn new(manager: NodePackageManager) -> Self {
        Self { manager }
    }
}

impl Default for NodeLockDiffParser {
    fn default() -> Self {
        Self::new(NodePackageManager::Npm)
    }
}

impl LockfileDiffParser for NodeLockDiffParser {
    fn parse_changes(&self, changes: &[(char, String)]) -> Vec<String> {
        match self.manager {
            NodePackageManager::Npm => npm::parse_npm_lock_changes(changes),
            NodePackageManager::Pnpm => pnpm::parse_pnpm_lock_changes(changes),
            NodePackageManager::Bun => bun::parse_bun_lock_changes(changes),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::traits::LockfileDiffParser;

    #[test]
    fn test_npm_diff_parser_version_upgrade() {
        let parser = NodeLockDiffParser::new(NodePackageManager::Npm);

        let changes = vec![
            (' ', "  \"packages\": {".to_string()),
            (' ', "    \"node_modules/lodash\": {".to_string()),
            ('-', "      \"version\": \"4.17.20\"".to_string()),
            ('+', "      \"version\": \"4.17.21\"".to_string()),
            (' ', "    },".to_string()),
            (' ', "  }".to_string()),
        ];

        let result = parser.parse_changes(&changes);
        assert!(result.contains(&"lodash".to_string()));
    }

    #[test]
    fn test_npm_diff_parser_new_package() {
        let parser = NodeLockDiffParser::new(NodePackageManager::Npm);

        let changes = vec![
            (' ', "  \"packages\": {".to_string()),
            ('+', "    \"node_modules/new-package\": {".to_string()),
            ('+', "      \"version\": \"1.0.0\"".to_string()),
            ('+', "    },".to_string()),
            (' ', "  }".to_string()),
        ];

        let result = parser.parse_changes(&changes);
        assert!(result.contains(&"new-package".to_string()));
    }

    #[test]
    fn test_pnpm_diff_parser_version_change() {
        let parser = NodeLockDiffParser::new(NodePackageManager::Pnpm);

        let changes = vec![
            (' ', "packages:".to_string()),
            ('-', "  /lodash@4.17.20:".to_string()),
            ('+', "  /lodash@4.17.21:".to_string()),
            (' ', "    resolution: {integrity: sha512-abc}".to_string()),
        ];

        let result = parser.parse_changes(&changes);
        assert!(result.contains(&"lodash".to_string()));
    }

    #[test]
    fn test_pnpm_diff_parser_scoped_package() {
        let parser = NodeLockDiffParser::new(NodePackageManager::Pnpm);

        let changes = vec![
            (' ', "packages:".to_string()),
            ('+', "  /@babel/core@7.23.0:".to_string()),
            ('+', "    resolution: {integrity: sha512-abc}".to_string()),
        ];

        let result = parser.parse_changes(&changes);
        assert!(result.contains(&"@babel/core".to_string()));
    }

    #[test]
    fn test_bun_diff_parser_version_upgrade() {
        let parser = NodeLockDiffParser::new(NodePackageManager::Bun);

        let changes = vec![
            (' ', "  \"packages\": {".to_string()),
            ('-', "    \"lodash@4.17.20\": {".to_string()),
            ('-', "      \"version\": \"4.17.20\"".to_string()),
            ('-', "    },".to_string()),
            ('+', "    \"lodash@4.17.21\": {".to_string()),
            ('+', "      \"version\": \"4.17.21\"".to_string()),
            ('+', "    },".to_string()),
        ];

        let result = parser.parse_changes(&changes);
        assert!(result.contains(&"lodash".to_string()));
    }

    #[test]
    fn test_default_parser_is_npm() {
        let parser = NodeLockDiffParser::default();
        assert!(matches!(parser.manager, NodePackageManager::Npm));
    }
}
