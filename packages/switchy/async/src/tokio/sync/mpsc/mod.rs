//! Multi-producer, single-consumer channel implementation.
//!
//! This module provides MPSC channels for message passing between tasks.

pub mod flume;
pub mod tokio;

pub use tokio::*;

/// Capacity-limited async channels, including reservations and explicit closure.
pub mod bounded {
    pub use crate::bounded::*;
}
