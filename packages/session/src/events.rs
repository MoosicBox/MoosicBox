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

/// Triggers the players updated event, notifying all registered listeners.
///
/// # Errors
///
/// * If any of the event handlers produce errors
pub async fn trigger_players_updated_event() -> Result<(), Vec<Box<dyn std::error::Error + Send>>> {
    send_players_updated_event().await
}

/// Sends the players updated event to all registered listeners.
///
/// This is the internal implementation that executes all listener callbacks.
///
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test_log::test(switchy_async::test)]
    async fn test_on_players_updated_event_registers_listener() {
        // Note: Due to global state, count will include listeners from other tests
        let initial_count = PLAYERS_UPDATED_EVENT_LISTENERS.read().await.len();

        on_players_updated_event(|| async { Ok(()) }).await;

        let new_count = PLAYERS_UPDATED_EVENT_LISTENERS.read().await.len();
        // Verify at least one more listener was added
        assert!(new_count > initial_count);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_trigger_players_updated_event_calls_all_listeners() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        // Clear listeners to avoid interference from other tests
        PLAYERS_UPDATED_EVENT_LISTENERS.write().await.clear();

        let counter1 = Arc::new(AtomicUsize::new(0));
        let counter2 = Arc::new(AtomicUsize::new(0));

        let c1 = counter1.clone();
        let c2 = counter2.clone();

        on_players_updated_event(move || {
            let c = c1.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .await;

        on_players_updated_event(move || {
            let c = c2.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .await;

        // Trigger the event
        let result = trigger_players_updated_event().await;
        assert!(result.is_ok());

        // Both listeners should have been called
        assert_eq!(counter1.load(Ordering::SeqCst), 1);
        assert_eq!(counter2.load(Ordering::SeqCst), 1);
    }

    #[test_log::test(switchy_async::test)]
    async fn test_trigger_players_updated_event_collects_errors() {
        use std::sync::atomic::{AtomicBool, Ordering};

        // Clear listeners to start fresh
        PLAYERS_UPDATED_EVENT_LISTENERS.write().await.clear();

        let success_called = Arc::new(AtomicBool::new(false));
        let error_called = Arc::new(AtomicBool::new(false));

        let sc = success_called.clone();
        let ec = error_called.clone();

        // Register a successful listener
        on_players_updated_event(move || {
            let c = sc.clone();
            async move {
                c.store(true, Ordering::SeqCst);
                Ok(())
            }
        })
        .await;

        // Register a failing listener
        on_players_updated_event(move || {
            let c = ec.clone();
            async move {
                c.store(true, Ordering::SeqCst);
                Err(Box::new(std::io::Error::other("test error")) as BoxErrorSend)
            }
        })
        .await;

        // Trigger should collect errors
        let result = trigger_players_updated_event().await;
        assert!(result.is_err());

        // Both listeners should have been called despite one failing
        assert!(success_called.load(Ordering::SeqCst));
        assert!(error_called.load(Ordering::SeqCst));

        // Should have collected the error
        if let Err(errors) = result {
            assert_eq!(errors.len(), 1);
        }
    }

    #[test_log::test(switchy_async::test)]
    async fn test_send_players_updated_event_with_no_listeners() {
        // Clear listeners and test empty case
        PLAYERS_UPDATED_EVENT_LISTENERS.write().await.clear();

        let result = send_players_updated_event().await;
        assert!(result.is_ok());
    }

    #[test_log::test(switchy_async::test)]
    async fn test_multiple_errors_collected() {
        // Clear existing listeners
        PLAYERS_UPDATED_EVENT_LISTENERS.write().await.clear();

        // Register multiple failing listeners
        on_players_updated_event(|| async {
            Err(Box::new(std::io::Error::other("error 1")) as BoxErrorSend)
        })
        .await;

        on_players_updated_event(|| async {
            Err(Box::new(std::io::Error::other("error 2")) as BoxErrorSend)
        })
        .await;

        let result = trigger_players_updated_event().await;
        assert!(result.is_err());

        if let Err(errors) = result {
            assert_eq!(errors.len(), 2);
        }
    }
}
