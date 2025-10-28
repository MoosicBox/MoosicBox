//! Event system for profile updates.
//!
//! Provides an event listener system that notifies registered callbacks when profiles
//! are added or removed from the registry.
//!
//! # Example
//!
//! ```rust,no_run
//! # #[cfg(feature = "events")]
//! # {
//! use moosicbox_profiles::events::on_profiles_updated_event;
//!
//! # async {
//! // Register a listener for profile updates
//! on_profiles_updated_event(|added, removed| {
//!     let added = added.to_vec();
//!     let removed = removed.to_vec();
//!     async move {
//!         println!("Added: {:?}, Removed: {:?}", added, removed);
//!         Ok(())
//!     }
//! }).await;
//! # };
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

/// Type alias for profile update event listener callbacks.
pub type ProfilesUpdatedSubscriptionAction = Box<
    dyn (Fn(
            &[String],
            &[String],
        )
            -> Pin<Box<dyn Future<Output = Result<(), Box<dyn std::error::Error + Send>>> + Send>>)
        + Send
        + Sync,
>;
static PROFILES_UPDATED_EVENT_LISTENERS: LazyLock<
    Arc<RwLock<Vec<ProfilesUpdatedSubscriptionAction>>>,
> = LazyLock::new(|| Arc::new(RwLock::new(Vec::new())));

/// Registers a listener for profile update events.
///
/// The listener callback receives two parameters:
/// * `added` - Slice of profile names that were added
/// * `removed` - Slice of profile names that were removed
///
/// The callback must return a future that resolves to `Result<(), BoxErrorSend>`.
pub async fn on_profiles_updated_event<
    F: Send + Future<Output = Result<(), Box<dyn std::error::Error + Send>>> + 'static,
>(
    listener: impl (Fn(&[String], &[String]) -> F) + Send + Sync + 'static,
) {
    PROFILES_UPDATED_EVENT_LISTENERS
        .write()
        .await
        .push(Box::new(move |added, removed| {
            Box::pin(listener(added, removed))
        }));
}

/// Triggers profile update events for all registered listeners.
///
/// # Errors
///
/// * If any of the event listeners fail
pub async fn trigger_profiles_updated_event(
    added: Vec<String>,
    removed: Vec<String>,
) -> Result<(), Vec<Box<dyn std::error::Error + Send>>> {
    send_profiles_updated_event(added, removed).await
}

/// Sends profile update events to all registered listeners.
///
/// # Errors
///
/// * If any of the event listeners fail
pub async fn send_profiles_updated_event(
    added: Vec<String>,
    removed: Vec<String>,
) -> Result<(), Vec<Box<dyn std::error::Error + Send>>> {
    let mut errors = vec![];
    let listeners = PROFILES_UPDATED_EVENT_LISTENERS.read().await;
    for listener in listeners.iter() {
        if let Err(e) = listener(&added, &removed).await {
            errors.push(e);
        }
    }
    drop(listeners);

    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(())
}
