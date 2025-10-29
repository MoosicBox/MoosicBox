//! Multi-producer, multi-consumer channel implementation.
//!
//! This module provides MPMC channels for message passing between tasks.

pub mod flume;

pub use flume::*;
