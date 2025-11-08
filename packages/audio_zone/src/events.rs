//! Event system for audio zone updates.
//!
//! This module provides an event listener system that triggers callbacks when audio zones are
//! created, updated, or deleted. Listeners can be registered to receive notifications when
//! audio zone state changes occur.
//!
//! # Examples
//!
//! ```rust,no_run
//! # use moosicbox_audio_zone::events;
//! # async fn example() {
//! // Register a listener for audio zone updates
//! events::on_audio_zones_updated_event(|| async {
//!     println!("Audio zones updated!");
//!     Ok(())
//! }).await;
//!
//! // Trigger the event when zones are modified
//! if let Err(errors) = events::trigger_audio_zones_updated_event().await {
//!     eprintln!("Event errors: {} error(s) occurred", errors.len());
//! }
//! # }
//! ```

use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, LazyLock},
};

use tokio::sync::RwLock;

/// A boxed error type that implements `Send` for thread-safe error handling.
pub type BoxErrorSend = Box<dyn std::error::Error + Send>;

/// A boxed function that returns a future for handling audio zone update events.
///
/// This type represents an event listener callback that can be invoked when audio zones
/// are updated. The function must be `Send + Sync` to support concurrent execution.
pub type AudioZonesUpdatedSubscriptionAction = Box<
    dyn (Fn() -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error + Send>>> + Send>>)
        + Send
        + Sync,
>;
static AUDIO_ZONES_UPDATED_EVENT_LISTENERS: LazyLock<
    Arc<RwLock<Vec<AudioZonesUpdatedSubscriptionAction>>>,
> = LazyLock::new(|| Arc::new(RwLock::new(Vec::new())));

/// Registers a listener to be called when audio zones are updated.
///
/// The listener will be invoked whenever an audio zone is created, updated, or deleted.
pub async fn on_audio_zones_updated_event<
    F: Send + Future<Output = Result<(), Box<dyn std::error::Error + Send>>> + 'static,
>(
    listener: impl (Fn() -> F) + Send + Sync + 'static,
) {
    AUDIO_ZONES_UPDATED_EVENT_LISTENERS
        .write()
        .await
        .push(Box::new(move || Box::pin(listener())));
}

/// Triggers all registered audio zone update event listeners.
///
/// This function invokes all callbacks registered via [`on_audio_zones_updated_event`]
/// when audio zones are created, updated, or deleted.
///
/// # Errors
///
/// * If any of the event handlers fail with an error
pub async fn trigger_audio_zones_updated_event()
-> Result<(), Vec<Box<dyn std::error::Error + Send>>> {
    send_audio_zones_updated_event().await
}

/// Sends audio zone update notifications to all registered listeners.
///
/// This is the internal implementation that iterates through all registered event listeners
/// and invokes them. Use [`trigger_audio_zones_updated_event`] as the public entry point.
///
/// # Errors
///
/// * If any of the event handlers fail with an error
pub async fn send_audio_zones_updated_event() -> Result<(), Vec<Box<dyn std::error::Error + Send>>>
{
    let mut errors = vec![];

    {
        let listeners = AUDIO_ZONES_UPDATED_EVENT_LISTENERS.read().await;
        for listener in listeners.iter() {
            if let Err(e) = listener().await {
                errors.push(e);
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(())
}
