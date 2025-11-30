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

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test]
    fn navigation_step_description_go_to() {
        let step = NavigationStep::GoTo {
            url: "/dashboard".to_string(),
        };
        assert_eq!(step.description(), "Navigate to /dashboard");
    }

    #[test_log::test]
    fn navigation_step_description_go_back() {
        let step = NavigationStep::GoBack;
        assert_eq!(step.description(), "Go back");
    }

    #[test_log::test]
    fn navigation_step_description_go_forward() {
        let step = NavigationStep::GoForward;
        assert_eq!(step.description(), "Go forward");
    }

    #[test_log::test]
    fn navigation_step_description_reload() {
        let step = NavigationStep::Reload;
        assert_eq!(step.description(), "Reload page");
    }

    #[test_log::test]
    fn navigation_step_description_set_hash() {
        let step = NavigationStep::SetHash {
            hash: "section-1".to_string(),
        };
        assert_eq!(step.description(), "Set hash to section-1");
    }
}
