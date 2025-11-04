//! Client simulation components for testing the `MoosicBox` server.
//!
//! This module contains client-side simulation components including:
//! * [`fault_injector`] - Injects random faults to test system resilience
//! * [`health_checker`] - Monitors server health status periodically

pub mod fault_injector;
pub mod health_checker;
