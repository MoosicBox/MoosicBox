//! Channel utilities.
//!
//! This crate provides abstractions and implementations for sending and receiving messages
//! across channels with support for prioritization.
//!
//! # Example
//!
//! ```rust
//! # #[cfg(feature = "futures-channel")]
//! # {
//! use moosicbox_channel_utils::futures_channel::unbounded;
//! use moosicbox_channel_utils::MoosicBoxSender;
//!
//! # async fn example() {
//! // Create a prioritized channel
//! let (tx, mut rx) = unbounded();
//!
//! // Configure priority based on message value
//! let tx = tx.with_priority(|msg: &i32| *msg as usize);
//!
//! // Send messages - higher values will be processed first
//! tx.send(1).unwrap();
//! tx.send(5).unwrap();
//! tx.send(3).unwrap();
//! # }
//! # }
//! ```

#![cfg_attr(feature = "fail-on-warnings", deny(warnings))]
#![warn(clippy::all, clippy::pedantic, clippy::nursery, clippy::cargo)]
#![allow(clippy::multiple_crate_versions)]

#[cfg(feature = "futures-channel")]
pub mod futures_channel;

/// A trait for sending messages of type `T` with error type `E`.
pub trait MoosicBoxSender<T, E> {
    /// Sends a message through the channel.
    ///
    /// # Errors
    ///
    /// * If the send failed
    fn send(&self, msg: T) -> Result<(), E>;
}
