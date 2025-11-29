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

use switchy_async::sync::RwLock;

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

#[cfg(test)]
#[allow(clippy::significant_drop_tightening)]
mod tests {
    use super::*;
    use serial_test::serial;

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_on_profiles_updated_event_registers_listener() {
        let call_count = Arc::new(RwLock::new(0u32));
        let call_count_clone = Arc::clone(&call_count);

        on_profiles_updated_event(move |_added, _removed| {
            let count = Arc::clone(&call_count_clone);
            async move {
                *count.write().await += 1;
                Ok(())
            }
        })
        .await;

        let initial_count = *call_count.read().await;

        // Trigger an event
        let result = trigger_profiles_updated_event(vec!["test".to_string()], vec![]).await;
        assert!(result.is_ok());

        // Verify the listener was called
        let final_count = *call_count.read().await;
        assert!(final_count > initial_count);
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_trigger_profiles_updated_event_with_added_profiles() {
        let added_profiles = Arc::new(RwLock::new(Vec::new()));
        let added_clone = Arc::clone(&added_profiles);

        on_profiles_updated_event(move |added, _removed| {
            let profiles = Arc::clone(&added_clone);
            let added_vec = added.to_vec();
            async move {
                *profiles.write().await = added_vec;
                Ok(())
            }
        })
        .await;

        let test_added = vec!["profile1".to_string(), "profile2".to_string()];
        let result = trigger_profiles_updated_event(test_added.clone(), vec![]).await;
        assert!(result.is_ok());

        let received = added_profiles.read().await.clone();
        drop(received);
        assert_eq!(added_profiles.read().await.clone(), test_added);
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_trigger_profiles_updated_event_with_removed_profiles() {
        let removed_profiles = Arc::new(RwLock::new(Vec::new()));
        let removed_clone = Arc::clone(&removed_profiles);

        on_profiles_updated_event(move |_added, removed| {
            let profiles = Arc::clone(&removed_clone);
            let removed_vec = removed.to_vec();
            async move {
                *profiles.write().await = removed_vec;
                Ok(())
            }
        })
        .await;

        let test_removed = vec!["profile1".to_string(), "profile2".to_string()];
        let result = trigger_profiles_updated_event(vec![], test_removed.clone()).await;
        assert!(result.is_ok());

        assert_eq!(removed_profiles.read().await.clone(), test_removed);
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_trigger_profiles_updated_event_with_both_added_and_removed() {
        let added_profiles = Arc::new(RwLock::new(Vec::new()));
        let removed_profiles = Arc::new(RwLock::new(Vec::new()));
        let added_clone = Arc::clone(&added_profiles);
        let removed_clone = Arc::clone(&removed_profiles);

        on_profiles_updated_event(move |added, removed| {
            let a = Arc::clone(&added_clone);
            let r = Arc::clone(&removed_clone);
            let added_vec = added.to_vec();
            let removed_vec = removed.to_vec();
            async move {
                *a.write().await = added_vec;
                *r.write().await = removed_vec;
                Ok(())
            }
        })
        .await;

        let test_added = vec!["new_profile".to_string()];
        let test_removed = vec!["old_profile".to_string()];
        let result = trigger_profiles_updated_event(test_added.clone(), test_removed.clone()).await;
        assert!(result.is_ok());

        assert_eq!(added_profiles.read().await.clone(), test_added);
        assert_eq!(removed_profiles.read().await.clone(), test_removed);
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_multiple_listeners_receive_events() {
        let call_count1 = Arc::new(RwLock::new(0u32));
        let call_count2 = Arc::new(RwLock::new(0u32));
        let count1_clone = Arc::clone(&call_count1);
        let count2_clone = Arc::clone(&call_count2);

        on_profiles_updated_event(move |_added, _removed| {
            let count = Arc::clone(&count1_clone);
            async move {
                *count.write().await += 1;
                Ok(())
            }
        })
        .await;

        on_profiles_updated_event(move |_added, _removed| {
            let count = Arc::clone(&count2_clone);
            async move {
                *count.write().await += 1;
                Ok(())
            }
        })
        .await;

        let initial_count1 = *call_count1.read().await;
        let initial_count2 = *call_count2.read().await;

        let result = trigger_profiles_updated_event(vec!["test".to_string()], vec![]).await;
        assert!(result.is_ok());

        // Both listeners should be called
        let final_count1 = *call_count1.read().await;
        let final_count2 = *call_count2.read().await;
        assert!(final_count1 > initial_count1);
        assert!(final_count2 > initial_count2);
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_send_profiles_updated_event_is_equivalent_to_trigger() {
        let call_count = Arc::new(RwLock::new(0u32));
        let count_clone = Arc::clone(&call_count);

        on_profiles_updated_event(move |_added, _removed| {
            let count = Arc::clone(&count_clone);
            async move {
                *count.write().await += 1;
                Ok(())
            }
        })
        .await;

        let initial_count = *call_count.read().await;

        // Test send_profiles_updated_event directly
        let result = send_profiles_updated_event(vec!["test".to_string()], vec![]).await;
        assert!(result.is_ok());

        let final_count = *call_count.read().await;
        assert!(final_count > initial_count);
    }

    #[test_log::test(switchy_async::test)]
    #[serial]
    async fn test_empty_added_and_removed_lists() {
        let call_count = Arc::new(RwLock::new(0u32));
        let added_received = Arc::new(RwLock::new(Vec::new()));
        let removed_received = Arc::new(RwLock::new(Vec::new()));

        let count_clone = Arc::clone(&call_count);
        let added_clone = Arc::clone(&added_received);
        let removed_clone = Arc::clone(&removed_received);

        on_profiles_updated_event(move |added, removed| {
            let count = Arc::clone(&count_clone);
            let a = Arc::clone(&added_clone);
            let r = Arc::clone(&removed_clone);
            let added_vec = added.to_vec();
            let removed_vec = removed.to_vec();
            async move {
                *a.write().await = added_vec;
                *r.write().await = removed_vec;
                *count.write().await += 1;
                Ok(())
            }
        })
        .await;

        let initial_count = *call_count.read().await;

        let result = trigger_profiles_updated_event(vec![], vec![]).await;
        assert!(result.is_ok());

        let final_count = *call_count.read().await;
        assert!(final_count > initial_count);

        // Verify empty lists were received
        assert!(added_received.read().await.is_empty());
        assert!(removed_received.read().await.is_empty());
    }
}
