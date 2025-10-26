use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, LazyLock},
};

use tokio::sync::RwLock;

pub type BoxErrorSend = Box<dyn std::error::Error + Send>;

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

/// # Errors
///
/// * If any of the event handlers fail with an error
pub async fn trigger_audio_zones_updated_event()
-> Result<(), Vec<Box<dyn std::error::Error + Send>>> {
    send_audio_zones_updated_event().await
}

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
