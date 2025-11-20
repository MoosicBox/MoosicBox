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
    use pretty_assertions::assert_eq;

    #[test]
    fn test_navigation_step_goto_description() {
        let step = NavigationStep::GoTo {
            url: "https://example.com/page".to_string(),
        };
        assert_eq!(step.description(), "Navigate to https://example.com/page");
    }

    #[test]
    fn test_navigation_step_go_back_description() {
        let step = NavigationStep::GoBack;
        assert_eq!(step.description(), "Go back");
    }

    #[test]
    fn test_navigation_step_go_forward_description() {
        let step = NavigationStep::GoForward;
        assert_eq!(step.description(), "Go forward");
    }

    #[test]
    fn test_navigation_step_reload_description() {
        let step = NavigationStep::Reload;
        assert_eq!(step.description(), "Reload page");
    }

    #[test]
    fn test_navigation_step_set_hash_description() {
        let step = NavigationStep::SetHash {
            hash: "section-1".to_string(),
        };
        assert_eq!(step.description(), "Set hash to section-1");
    }

    #[test]
    fn test_navigation_step_goto_serialization() {
        let step = NavigationStep::GoTo {
            url: "/test-page".to_string(),
        };
        let json = serde_json::to_string(&step).unwrap();
        let deserialized: NavigationStep = serde_json::from_str(&json).unwrap();
        match deserialized {
            NavigationStep::GoTo { url } => assert_eq!(url, "/test-page"),
            _ => panic!("Expected GoTo variant"),
        }
    }

    #[test]
    fn test_navigation_step_go_back_serialization() {
        let step = NavigationStep::GoBack;
        let json = serde_json::to_string(&step).unwrap();
        let deserialized: NavigationStep = serde_json::from_str(&json).unwrap();
        match deserialized {
            NavigationStep::GoBack => {}
            _ => panic!("Expected GoBack variant"),
        }
    }

    #[test]
    fn test_navigation_step_go_forward_serialization() {
        let step = NavigationStep::GoForward;
        let json = serde_json::to_string(&step).unwrap();
        let deserialized: NavigationStep = serde_json::from_str(&json).unwrap();
        match deserialized {
            NavigationStep::GoForward => {}
            _ => panic!("Expected GoForward variant"),
        }
    }

    #[test]
    fn test_navigation_step_reload_serialization() {
        let step = NavigationStep::Reload;
        let json = serde_json::to_string(&step).unwrap();
        let deserialized: NavigationStep = serde_json::from_str(&json).unwrap();
        match deserialized {
            NavigationStep::Reload => {}
            _ => panic!("Expected Reload variant"),
        }
    }

    #[test]
    fn test_navigation_step_set_hash_serialization() {
        let step = NavigationStep::SetHash {
            hash: "top".to_string(),
        };
        let json = serde_json::to_string(&step).unwrap();
        let deserialized: NavigationStep = serde_json::from_str(&json).unwrap();
        match deserialized {
            NavigationStep::SetHash { hash } => assert_eq!(hash, "top"),
            _ => panic!("Expected SetHash variant"),
        }
    }
}
