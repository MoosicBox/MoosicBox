//! Multi-producer, single-consumer channel implementation.
//!
//! This module provides MPSC channels for message passing between tasks.

pub mod flume;
pub mod tokio;

pub use tokio::*;
