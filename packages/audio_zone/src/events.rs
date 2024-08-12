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

pub async fn trigger_audio_zones_updated_event(
) -> Result<(), Vec<Box<dyn std::error::Error + Send>>> {
    send_audio_zones_updated_event().await
}

pub async fn send_audio_zones_updated_event() -> Result<(), Vec<Box<dyn std::error::Error + Send>>>
{
    let mut errors = vec![];
    for listener in AUDIO_ZONES_UPDATED_EVENT_LISTENERS.read().await.iter() {
        if let Err(e) = listener().await {
            errors.push(e);
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(())
}
