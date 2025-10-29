//! Multi-producer, multi-consumer channel implementation for simulator runtime.
//!
//! This module provides MPMC channels with deterministic execution for testing.

pub mod flume;

pub use flume::*;
