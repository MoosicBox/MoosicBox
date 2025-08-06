#![allow(clippy::module_name_repetitions)]

use std::{collections::HashMap, fmt::Display, pin::Pin, sync::Arc, time::Duration};

use futures::Future;
use strum_macros::AsRefStr;
use switchy_async::{task::JoinError, util::CancellationToken};
use thiserror::Error;
use tokio::sync::RwLock;

use crate::{MediaInfo, PositionInfo, TransportInfo};

impl From<flume::SendError<usize>> for ListenerError {
    fn from(_value: flume::SendError<usize>) -> Self {
        Self::Send
    }
}

#[derive(Debug, Error)]
pub enum ListenerError {
    #[error(transparent)]
    Join(#[from] JoinError),
    #[error(transparent)]
    Rupnp(#[from] rupnp::Error),
    #[error("Failed to send")]
    Send,
}

#[derive(AsRefStr)]
pub enum UpnpCommand {
    SubscribeMediaInfo {
        interval: Duration,
        instance_id: u32,
        udn: String,
        service_id: String,
        action: MediaInfoSubscriptionAction,
        tx: flume::Sender<usize>,
    },
    SubscribePositionInfo {
        interval: Duration,
        instance_id: u32,
        udn: String,
        service_id: String,
        action: PositionInfoSubscriptionAction,
        tx: flume::Sender<usize>,
    },
    SubscribeTransportInfo {
        interval: Duration,
        instance_id: u32,
        udn: String,
        service_id: String,
        action: TransportInfoSubscriptionAction,
        tx: flume::Sender<usize>,
    },
    Unsubscribe {
        subscription_id: usize,
    },
}

impl Display for UpnpCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

pub struct UpnpContext {
    #[allow(clippy::type_complexity)]
    status_join_handles: HashMap<usize, switchy_async::task::JoinHandle<Result<(), ListenerError>>>,
    status_tokens: HashMap<usize, CancellationToken>,
    token: Option<CancellationToken>,
    subscription_id: usize,
}

impl UpnpContext {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

impl Default for UpnpContext {
    fn default() -> Self {
        Self {
            status_join_handles: HashMap::default(),
            status_tokens: HashMap::default(),
            token: Option::default(),
            subscription_id: 1,
        }
    }
}

moosicbox_async_service::async_service!(UpnpCommand, UpnpContext, ListenerError);

impl Handle {
    /// # Errors
    ///
    /// * If failed to send the command
    /// * If failed to recv the response
    pub async fn subscribe_media_info(
        &self,
        interval: Duration,
        instance_id: u32,
        udn: String,
        service_id: String,
        action: MediaInfoSubscriptionAction,
    ) -> Result<usize, CommanderError> {
        let (tx, rx) = flume::bounded(1);
        self.send_command(UpnpCommand::SubscribeMediaInfo {
            interval,
            instance_id,
            udn,
            service_id,
            action,
            tx,
        })?;
        Ok(rx.recv_async().await?)
    }

    /// # Errors
    ///
    /// * If failed to send the command
    /// * If failed to recv the response
    pub async fn subscribe_position_info(
        &self,
        interval: Duration,
        instance_id: u32,
        udn: String,
        service_id: String,
        action: PositionInfoSubscriptionAction,
    ) -> Result<usize, CommanderError> {
        let (tx, rx) = flume::bounded(1);
        self.send_command(UpnpCommand::SubscribePositionInfo {
            interval,
            instance_id,
            udn,
            service_id,
            action,
            tx,
        })?;
        Ok(rx.recv_async().await?)
    }

    /// # Errors
    ///
    /// * If failed to send the command
    /// * If failed to recv the response
    pub async fn subscribe_transport_info(
        &self,
        interval: Duration,
        instance_id: u32,
        udn: String,
        service_id: String,
        action: TransportInfoSubscriptionAction,
    ) -> Result<usize, CommanderError> {
        let (tx, rx) = flume::bounded(1);
        self.send_command(UpnpCommand::SubscribeTransportInfo {
            interval,
            instance_id,
            udn,
            service_id,
            action,
            tx,
        })?;
        Ok(rx.recv_async().await?)
    }

    /// # Errors
    ///
    /// * If failed to send the command
    pub fn unsubscribe(&self, subscription_id: usize) -> Result<(), CommanderError> {
        self.send_command(UpnpCommand::Unsubscribe { subscription_id })
    }
}

#[moosicbox_async_service::async_trait]
impl Processor for Service {
    type Error = ListenerError;

    async fn on_start(&mut self) -> Result<(), Self::Error> {
        self.ctx.write().await.token.replace(self.token.clone());
        Ok(())
    }

    async fn on_shutdown(ctx: Arc<RwLock<UpnpContext>>) -> Result<(), Self::Error> {
        let mut ctx = ctx.write().await;
        for (_, handle) in ctx.status_join_handles.drain() {
            handle.await??;
        }
        drop(ctx);
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    async fn process_command(
        ctx: Arc<RwLock<UpnpContext>>,
        command: UpnpCommand,
    ) -> Result<(), Self::Error> {
        let cmd_str = command.as_ref().to_owned();
        log::debug!("process_command command={cmd_str}");
        match command {
            UpnpCommand::SubscribeMediaInfo {
                interval,
                instance_id,
                udn,
                service_id,
                action,
                tx,
            } => {
                let action = Arc::new(action);
                tx.send_async(
                    subscribe(
                        &cmd_str,
                        ctx,
                        interval,
                        Box::new(move || {
                            let action = action.clone();
                            let udn = udn.clone();
                            let service_id = service_id.clone();
                            Box::pin(async move {
                                if let Ok(device) = super::get_device(&udn) {
                                    if let Ok(service) = super::get_service(&udn, &service_id) {
                                        match super::get_media_info(
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
                    .await?,
                )
                .await?;
            }
            UpnpCommand::SubscribePositionInfo {
                interval,
                instance_id,
                udn,
                service_id,
                action,
                tx,
            } => {
                let action = Arc::new(action);
                tx.send_async(
                    subscribe(
                        &cmd_str,
                        ctx,
                        interval,
                        Box::new(move || {
                            let action = action.clone();
                            let udn = udn.clone();
                            let service_id = service_id.clone();
                            Box::pin(async move {
                                if let Ok(device) = super::get_device(&udn) {
                                    if let Ok(service) = super::get_service(&udn, &service_id) {
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
                    .await?,
                )
                .await?;
            }
            UpnpCommand::SubscribeTransportInfo {
                interval,
                instance_id,
                udn,
                service_id,
                action,
                tx,
            } => {
                let action = Arc::new(action);
                tx.send_async(
                    subscribe(
                        &cmd_str,
                        ctx,
                        interval,
                        Box::new(move || {
                            let action = action.clone();
                            let udn = udn.clone();
                            let service_id = service_id.clone();
                            Box::pin(async move {
                                if let Ok(device) = super::get_device(&udn) {
                                    if let Ok(service) = super::get_service(&udn, &service_id) {
                                        match super::get_transport_info(
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
                                                log::error!("Failed to get_transport_info: {e:?}");
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
                    .await?,
                )
                .await?;
            }
            UpnpCommand::Unsubscribe { subscription_id } => {
                unsubscribe(ctx, subscription_id).await?;
            }
        }
        Ok(())
    }
}

async fn subscribe(
    command: &str,
    ctx: Arc<RwLock<UpnpContext>>,
    interval: Duration,
    action: SubscriptionAction,
) -> Result<usize, ListenerError> {
    let mut ctx = ctx.write().await;
    let subscription_id = ctx.subscription_id;
    ctx.subscription_id += 1;
    let token = ctx.token.clone().unwrap();
    let status_token = CancellationToken::new();
    ctx.status_tokens
        .insert(subscription_id, status_token.clone());
    ctx.status_join_handles.insert(
        subscription_id,
        moosicbox_task::spawn(&format!("upnp: subscribe {command}"), async move {
            let mut interval = tokio::time::interval(interval);

            while tokio::select!(
                () = token.cancelled() => {
                    log::debug!("UpnpListener was cancelled");
                    Err(std::io::Error::new(std::io::ErrorKind::Interrupted, "Cancelled"))
                }
                () = status_token.cancelled() => {
                    log::debug!("Subscription was cancelled for subscription_id={subscription_id}");
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
    drop(ctx);

    Ok(subscription_id)
}

async fn unsubscribe(
    ctx: Arc<RwLock<UpnpContext>>,
    subscription_id: usize,
) -> Result<(), ListenerError> {
    let mut ctx = ctx.write().await;
    log::debug!("Unsubscribing subscription_id={subscription_id}");
    if let Some(token) = ctx.status_tokens.remove(&subscription_id) {
        token.cancel();
        if let Some(handle) = ctx.status_join_handles.remove(&subscription_id) {
            handle.await??;
        } else {
            log::debug!("No status_join_handle with subscription_id={subscription_id}");
        }
    } else {
        log::debug!("No token with subscription_id={subscription_id}");
    }

    Ok(())
}

type SubscriptionAction = Box<dyn (Fn() -> Pin<Box<dyn Future<Output = ()> + Send>>) + Send>;
pub type MediaInfoSubscriptionAction =
    Box<dyn (Fn(MediaInfo) -> Pin<Box<dyn Future<Output = ()> + Send>>) + Send + Sync>;
pub type PositionInfoSubscriptionAction =
    Box<dyn (Fn(PositionInfo) -> Pin<Box<dyn Future<Output = ()> + Send>>) + Send + Sync>;
pub type TransportInfoSubscriptionAction =
    Box<dyn (Fn(TransportInfo) -> Pin<Box<dyn Future<Output = ()> + Send>>) + Send + Sync>;
