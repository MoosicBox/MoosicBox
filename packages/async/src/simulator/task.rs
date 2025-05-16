pub use super::runtime::{JoinHandle, spawn};

pub use tokio::task::yield_now;

#[derive(Debug, Clone, thiserror::Error, Default)]
pub struct JoinError;

impl JoinError {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self
    }
}

impl std::fmt::Display for JoinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("JoinError")
    }
}
