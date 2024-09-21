use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, LazyLock},
};

use tokio::sync::RwLock;

pub type BoxErrorSend = Box<dyn std::error::Error + Send>;

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

pub async fn trigger_profiles_updated_event(
    added: Vec<String>,
    removed: Vec<String>,
) -> Result<(), Vec<Box<dyn std::error::Error + Send>>> {
    send_profiles_updated_event(added, removed).await
}

pub async fn send_profiles_updated_event(
    added: Vec<String>,
    removed: Vec<String>,
) -> Result<(), Vec<Box<dyn std::error::Error + Send>>> {
    let mut errors = vec![];
    for listener in PROFILES_UPDATED_EVENT_LISTENERS.read().await.iter() {
        if let Err(e) = listener(&added, &removed).await {
            errors.push(e);
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(())
}
