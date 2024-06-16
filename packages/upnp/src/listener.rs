use std::{collections::HashMap, fmt::Display, pin::Pin, sync::Arc, time::Duration};

use futures::Future;
use strum_macros::AsRefStr;
use thiserror::Error;
use tokio::task::{JoinError, JoinHandle};
use tokio_util::sync::CancellationToken;

#[derive(Debug, Error)]
pub enum ListenerError {
    #[error(transparent)]
    Join(#[from] JoinError),
    #[error(transparent)]
    Rupnp(#[from] rupnp::Error),
}

#[derive(AsRefStr)]
pub enum UpnpCommand {
    SubscribeMediaInfo {
        interval: Duration,
        instance_id: u32,
        udn: String,
        service_id: String,
        action: MediaInfoSubscriptionAction,
    },
    UnsubscribeMediaInfo {
        instance_id: u32,
        udn: String,
        service_id: String,
    },
    SubscribePositionInfo {
        interval: Duration,
        instance_id: u32,
        udn: String,
        service_id: String,
        action: PositionInfoSubscriptionAction,
    },
    UnsubscribePositionInfo {
        instance_id: u32,
        udn: String,
        service_id: String,
    },
}

impl Display for UpnpCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

#[derive(Default)]
pub struct UpnpContext {
    #[allow(clippy::type_complexity)]
    status_join_handles: HashMap<String, JoinHandle<Result<(), ListenerError>>>,
    status_tokens: HashMap<String, CancellationToken>,
    token: Option<CancellationToken>,
}

impl UpnpContext {
    pub fn new() -> Self {
        Self::default()
    }
}

moosicbox_async_service::async_service!(UpnpCommand, UpnpContext, ListenerError);

#[moosicbox_async_service::async_trait]
impl Processor for Service {
    type Error = ListenerError;

    async fn on_start(ctx: &mut UpnpContext, token: CancellationToken) -> Result<(), Self::Error> {
        ctx.token.replace(token);
        Ok(())
    }

    async fn on_shutdown(ctx: &mut UpnpContext) -> Result<(), Self::Error> {
        for (_, handle) in ctx.status_join_handles.drain() {
            handle.await??;
        }
        Ok(())
    }

    async fn process_command(
        ctx: &mut UpnpContext,
        command: UpnpCommand,
    ) -> Result<(), Self::Error> {
        log::debug!("process_command command={command}");
        match command {
            UpnpCommand::SubscribeMediaInfo {
                interval,
                instance_id,
                udn,
                service_id,
                action,
            } => {
                let action = Arc::new(action);
                let key = format!("MediaInfo:{instance_id}:{udn}:{service_id}");
                subscribe(
                    ctx,
                    interval,
                    key,
                    Box::new(move || {
                        let action = action.clone();
                        let udn = udn.clone();
                        let service_id = service_id.clone();
                        Box::pin(async move {
                            if let Some(device) = super::get_device(&udn) {
                                if let Some(service) = super::get_service(&udn, &service_id) {
                                    match super::get_media_info(&service, device.url(), instance_id)
                                        .await
                                    {
                                        Ok(info) => {
                                            action(info).await;
                                        }
                                        Err(e) => {
                                            log::error!("Failed to get_media_info: {e:?}");
                                        }
                                    }
                                } else {
                                    log::debug!(
                                        "No service with device_udn={udn} service_id={service_id}"
                                    );
                                }
                            } else {
                                log::debug!("No device with udn={udn}");
                            }
                        })
                    }),
                )
                .await?;
            }
            UpnpCommand::UnsubscribeMediaInfo {
                instance_id,
                udn,
                service_id,
            } => {
                let key = format!("MediaInfo:{instance_id}:{udn}:{service_id}");
                unsubscribe(ctx, key).await?;
            }
            UpnpCommand::SubscribePositionInfo {
                interval,
                instance_id,
                udn,
                service_id,
                action,
            } => {
                let action = Arc::new(action);
                let key = format!("PositionInfo:{instance_id}:{udn}:{service_id}");
                subscribe(
                    ctx,
                    interval,
                    key,
                    Box::new(move || {
                        let action = action.clone();
                        let udn = udn.clone();
                        let service_id = service_id.clone();
                        Box::pin(async move {
                            if let Some(device) = super::get_device(&udn) {
                                if let Some(service) = super::get_service(&udn, &service_id) {
                                    match super::get_position_info(
                                        &service,
                                        device.url(),
                                        instance_id,
                                    )
                                    .await
                                    {
                                        Ok(info) => {
                                            action(info).await;
                                        }
                                        Err(e) => {
                                            log::error!("Failed to get_position_info: {e:?}");
                                        }
                                    }
                                } else {
                                    log::debug!(
                                        "No service with device_udn={udn} service_id={service_id}"
                                    );
                                }
                            } else {
                                log::debug!("No device with udn={udn}");
                            }
                        })
                    }),
                )
                .await?;
            }
            UpnpCommand::UnsubscribePositionInfo {
                instance_id,
                udn,
                service_id,
            } => {
                let key = format!("PositionInfo:{instance_id}:{udn}:{service_id}");
                unsubscribe(ctx, key).await?;
            }
        }
        Ok(())
    }
}

async fn subscribe(
    ctx: &mut UpnpContext,
    interval: Duration,
    key: String,
    action: SubscriptionAction,
) -> Result<(), ListenerError> {
    let token = ctx.token.clone().unwrap();
    let status_token = CancellationToken::new();
    ctx.status_tokens.insert(key.clone(), status_token.clone());
    ctx.status_join_handles.insert(
        key.clone(),
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(interval);

            while tokio::select!(
                () = token.cancelled() => {
                    log::debug!("UpnpListener was cancelled");
                    Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "Cancelled"))
                }
                () = status_token.cancelled() => {
                    log::debug!("Subscription was cancelled for key={key}");
                    Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "Cancelled"))
                }
                _ = interval.tick() => { Ok(()) }
            )
            .is_ok()
            {
                log::trace!("Tick");
                action().await;
            }

            Ok(())
        }),
    );

    Ok(())
}

async fn unsubscribe(ctx: &mut UpnpContext, key: String) -> Result<(), ListenerError> {
    log::debug!("Unsubscribing key={key}");
    if let Some(token) = ctx.status_tokens.remove(&key) {
        token.cancel();
        if let Some(handle) = ctx.status_join_handles.remove(&key) {
            handle.await??;
        } else {
            log::debug!("No status_join_handle with key={key}");
        }
    } else {
        log::debug!("No token with key={key}");
    }

    Ok(())
}

type SubscriptionAction = Box<dyn (Fn() -> Pin<Box<dyn Future<Output = ()> + Send>>) + Send>;
pub type MediaInfoSubscriptionAction = Box<
    dyn (Fn(HashMap<String, String>) -> Pin<Box<dyn Future<Output = ()> + Send>>) + Send + Sync,
>;
pub type PositionInfoSubscriptionAction = Box<
    dyn (Fn(HashMap<String, String>) -> Pin<Box<dyn Future<Output = ()> + Send>>) + Send + Sync,
>;
