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

#[cfg(test)]
mod tests {
    use super::*;
    use serial_test::serial;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    // Note: All tests in this module use #[serial] because they interact with the global
    // AUDIO_ZONES_UPDATED_EVENT_LISTENERS state. Running these tests in parallel would cause
    // race conditions where one test's .clear() or listener registration affects another
    // test's expectations. The serial_test crate ensures these tests run one at a time.

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_on_audio_zones_updated_event_registers_listener() {
        // Clear listeners to start fresh
        AUDIO_ZONES_UPDATED_EVENT_LISTENERS.write().await.clear();

        let initial_count = AUDIO_ZONES_UPDATED_EVENT_LISTENERS.read().await.len();
        assert_eq!(initial_count, 0);

        on_audio_zones_updated_event(|| async { Ok(()) }).await;

        let new_count = AUDIO_ZONES_UPDATED_EVENT_LISTENERS.read().await.len();
        assert_eq!(new_count, 1);
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_trigger_audio_zones_updated_event_calls_all_listeners() {
        // Clear listeners to avoid interference from other tests
        AUDIO_ZONES_UPDATED_EVENT_LISTENERS.write().await.clear();

        let counter1 = Arc::new(AtomicUsize::new(0));
        let counter2 = Arc::new(AtomicUsize::new(0));

        let c1 = counter1.clone();
        let c2 = counter2.clone();

        on_audio_zones_updated_event(move || {
            let c = c1.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .await;

        on_audio_zones_updated_event(move || {
            let c = c2.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .await;

        // Trigger the event
        let result = trigger_audio_zones_updated_event().await;
        assert!(result.is_ok(), "Expected Ok result, got {result:?}");

        // Both listeners should have been called
        assert_eq!(counter1.load(Ordering::SeqCst), 1);
        assert_eq!(counter2.load(Ordering::SeqCst), 1);
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_trigger_audio_zones_updated_event_collects_errors() {
        // Clear listeners to start fresh
        AUDIO_ZONES_UPDATED_EVENT_LISTENERS.write().await.clear();

        let success_called = Arc::new(AtomicBool::new(false));
        let error_called = Arc::new(AtomicBool::new(false));

        let sc = success_called.clone();
        let ec = error_called.clone();

        // Register a successful listener
        on_audio_zones_updated_event(move || {
            let c = sc.clone();
            async move {
                c.store(true, Ordering::SeqCst);
                Ok(())
            }
        })
        .await;

        // Register a failing listener
        on_audio_zones_updated_event(move || {
            let c = ec.clone();
            async move {
                c.store(true, Ordering::SeqCst);
                Err(Box::new(std::io::Error::other("test error")) as BoxErrorSend)
            }
        })
        .await;

        // Trigger should collect errors
        let result = trigger_audio_zones_updated_event().await;
        assert!(result.is_err(), "Expected Err result, got {result:?}");

        // Both listeners should have been called despite one failing
        assert!(success_called.load(Ordering::SeqCst));
        assert!(error_called.load(Ordering::SeqCst));

        // Should have collected the error
        if let Err(errors) = result {
            assert_eq!(errors.len(), 1);
        }
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_send_audio_zones_updated_event_with_no_listeners() {
        // Clear listeners and test empty case
        AUDIO_ZONES_UPDATED_EVENT_LISTENERS.write().await.clear();

        let result = send_audio_zones_updated_event().await;
        assert!(result.is_ok(), "Expected Ok result, got {result:?}");
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_multiple_errors_collected() {
        // Clear existing listeners
        AUDIO_ZONES_UPDATED_EVENT_LISTENERS.write().await.clear();

        // Register multiple failing listeners
        on_audio_zones_updated_event(|| async {
            Err(Box::new(std::io::Error::other("error 1")) as BoxErrorSend)
        })
        .await;

        on_audio_zones_updated_event(|| async {
            Err(Box::new(std::io::Error::other("error 2")) as BoxErrorSend)
        })
        .await;

        let result = trigger_audio_zones_updated_event().await;
        assert!(result.is_err(), "Expected Err result, got {result:?}");

        if let Err(errors) = result {
            assert_eq!(errors.len(), 2);
        }
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_send_and_trigger_are_equivalent() {
        // Clear existing listeners
        AUDIO_ZONES_UPDATED_EVENT_LISTENERS.write().await.clear();

        let counter = Arc::new(AtomicUsize::new(0));
        let c = counter.clone();

        on_audio_zones_updated_event(move || {
            let cnt = c.clone();
            async move {
                cnt.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .await;

        // Test send_audio_zones_updated_event directly
        let result1 = send_audio_zones_updated_event().await;
        assert!(result1.is_ok());
        assert_eq!(counter.load(Ordering::SeqCst), 1);

        // Test trigger_audio_zones_updated_event
        let result2 = trigger_audio_zones_updated_event().await;
        assert!(result2.is_ok());
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }
}
