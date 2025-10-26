//! Channel utilities.
//!
//! This crate provides abstractions and implementations for sending and receiving messages
//! across channels with support for prioritization.

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "futures-channel")]
pub mod futures_channel;

/// A trait for sending messages of type `T` with error type `E`.
pub trait MoosicBoxSender<T, E> {
    /// # Errors
    ///
    /// * If the send failed
    fn send(&self, msg: T) -> Result<(), E>;
}
