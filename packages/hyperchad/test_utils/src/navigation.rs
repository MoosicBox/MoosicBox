//! Browser navigation utilities for test workflows.
//!
//! This module provides types for controlling browser navigation during tests,
//! including URL navigation, history operations, and hash manipulation.

use serde::{Deserialize, Serialize};

/// A navigation action in the browser.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NavigationStep {
    /// Navigate to a URL.
    GoTo {
        /// URL to navigate to.
        url: String,
    },
    /// Navigate back in history.
    GoBack,
    /// Navigate forward in history.
    GoForward,
    /// Reload the current page.
    Reload,
    /// Set the URL hash fragment.
    SetHash {
        /// Hash fragment to set (without the # symbol).
        hash: String,
    },
}

impl NavigationStep {
    /// Returns a human-readable description of this navigation step.
    #[must_use]
    pub fn description(&self) -> String {
        match self {
            Self::GoTo { url } => format!("Navigate to {url}"),
            Self::GoBack => "Go back".to_string(),
            Self::GoForward => "Go forward".to_string(),
            Self::Reload => "Reload page".to_string(),
            Self::SetHash { hash } => format!("Set hash to {hash}"),
        }
    }
}
