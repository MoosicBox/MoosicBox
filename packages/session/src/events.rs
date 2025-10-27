//! Event notification system for player updates.
//!
//! This module provides a simple event system that allows components to register listeners
//! and be notified when players are updated. This is useful for keeping UI components or
//! other subsystems in sync with player state changes.
//!
//! # Examples
//!
//! ```rust,no_run
//! # #[cfg(feature = "events")]
//! # {
//! # use moosicbox_session::events::{on_players_updated_event, trigger_players_updated_event};
//! # async fn example() {
//! // Register a listener
//! on_players_updated_event(|| async {
//!     println!("Players were updated!");
//!     Ok(())
//! }).await;
//!
//! // Trigger the event
//! let _ = trigger_players_updated_event().await;
//! # }
//! # }
//! ```

use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, LazyLock},
};

use tokio::sync::RwLock;

/// Type alias for boxed errors that can be sent across threads.
pub type BoxErrorSend = Box<dyn std::error::Error + Send>;

/// Type alias for player update event listener callbacks.
pub type PlayersUpdatedSubscriptionAction = Box<
    dyn (Fn() -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error + Send>>> + Send>>)
        + Send
        + Sync,
>;
static PLAYERS_UPDATED_EVENT_LISTENERS: LazyLock<
    Arc<RwLock<Vec<PlayersUpdatedSubscriptionAction>>>,
> = LazyLock::new(|| Arc::new(RwLock::new(Vec::new())));

/// Registers a listener to be notified when players are updated.
pub async fn on_players_updated_event<
    F: Send + Future<Output = Result<(), Box<dyn std::error::Error + Send>>> + 'static,
>(
    listener: impl (Fn() -> F) + Send + Sync + 'static,
) {
    PLAYERS_UPDATED_EVENT_LISTENERS
        .write()
        .await
        .push(Box::new(move || Box::pin(listener())));
}

/// # Errors
///
/// * If any of the event handlers produce errors
pub async fn trigger_players_updated_event() -> Result<(), Vec<Box<dyn std::error::Error + Send>>> {
    send_players_updated_event().await
}

/// # Errors
///
/// * If any of the event handlers produce errors
pub async fn send_players_updated_event() -> Result<(), Vec<Box<dyn std::error::Error + Send>>> {
    let mut errors = vec![];
    let listeners = PLAYERS_UPDATED_EVENT_LISTENERS.read().await;
    for listener in listeners.iter() {
        if let Err(e) = listener().await {
            errors.push(e);
        }
    }
    drop(listeners);

    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(())
}
