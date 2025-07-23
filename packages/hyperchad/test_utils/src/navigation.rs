use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NavigationStep {
    GoTo { url: String },
    GoBack,
    GoForward,
    Reload,
    SetHash { hash: String },
}

impl NavigationStep {
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
